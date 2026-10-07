//! Iterative ownership, dispatch and teardown of child flows.
use std::any::Any;
use std::sync::Arc;

use super::arena::{Arena, Generational, Handle};
use crate::Var;
use crate::compose::CompiledWire;
use crate::error::{Error, Result};
use crate::flow::CompiledFlow;
use crate::function::{CallCompiled, CallTarget, CompiledFunction};
use crate::instance::{CleanupCtx, InstanceCtx};
use crate::reload::{FunctionKey, FunctionRegistry};
use crate::shard::{ActivationCtx, CompiledNode, Step};
use crate::shards::control::{
  self, Condition, ConditionsCompiled, IfCompiled, MatchCompiled, MaybeCompiled,
};
use crate::shards::{Predicated, RepeatCompiled};

/// Immutable builtin control descriptions. Children are borrowed only during
/// dispatch; frames retain their compiled owner, never pointers into it.
#[doc(hidden)]
#[derive(Clone, Copy)]
pub enum Control<'a> {
  /// A function call: its invocation frame is built at entry (golden path
  /// §5), so it has no eager children.
  Call(&'a CallCompiled),
  When(&'a Predicated),
  While(&'a Predicated),
  Sub(&'a CompiledFlow),
  Once(&'a CompiledFlow),
  Repeat(&'a RepeatCompiled),
  If(&'a IfCompiled),
  Match(&'a MatchCompiled),
  Maybe(&'a MaybeCompiled),
  Conditions(&'a ConditionsCompiled),
}

impl Control<'_> {
  fn len(&self) -> usize {
    match self {
      Self::Call(_) => 0,
      Self::Sub(_) | Self::Once(_) => 1,
      Self::When(_) | Self::While(_) => 2,
      Self::Repeat(c) => 1 + usize::from(c.until.is_some()),
      Self::If(c) => c.flows.len(),
      Self::Match(c) => c.flows.len(),
      Self::Maybe(c) => c.flows.len(),
      Self::Conditions(c) => c.flows.len(),
    }
  }
}

#[derive(Clone)]
enum Code {
  Root(Arc<CompiledWire>),
  /// A composite's child flow by index (`u32`, so the tag fits beside it).
  Child(Arc<dyn CompiledNode>, u32),
  /// A function invocation: the frame owns the invocation's locals.
  Function(Arc<CompiledFunction>),
}
impl Code {
  fn flow(&self) -> &CompiledFlow {
    match self {
      Self::Root(w) => &w.flow,
      // Resolve from the retained owner without borrowing the frame arena.
      Self::Child(n, i) => child_flow(&n.control().expect("composite"), *i as usize),
      Self::Function(f) => &f.flow,
    }
  }
}

/// Child flow `i` of a composite (the frame `Code::Child(_, i)` runs).
fn child_flow<'a>(control: &Control<'a>, i: usize) -> &'a CompiledFlow {
  match control {
    Control::When(c) | Control::While(c) => {
      if i == 0 {
        &c.pred
      } else {
        &c.body
      }
    }
    Control::Sub(c) | Control::Once(c) => c,
    Control::Repeat(c) => {
      if i == 0 {
        &c.body
      } else {
        c.until.as_ref().expect("until")
      }
    }
    Control::If(c) => &c.flows[i],
    Control::Match(c) => &c.flows[i],
    Control::Maybe(c) => &c.flows[i],
    Control::Conditions(c) => &c.flows[i],
    Control::Call(_) => unreachable!("a call's frame holds its function"),
  }
}

#[derive(Default)]
struct Continuation {
  children: Vec<Handle>,
  /// A stateless call whose kept frames hold native state to reset at exit
  /// (set at entry from the body entered, so the exit asks no frame).
  resets: bool,
  /// The node is a call (set at build): its entry may run inside this step.
  is_call: bool,
  /// For a stateless, non-recursive call site whose kept frame holds a
  /// straight-line body: the site, so the VM runs it on the kept frame's
  /// locals (`EngineCalls::callee`). Set when the frame path last entered
  /// the site, cleared when the children are replaced. The site lives in
  /// the flow this frame runs, which outlives the frame.
  vm: Option<std::ptr::NonNull<CallCompiled>>,
  phase: u32,
  count: i64,
  limit: Option<i64>,
  once_done: bool,
  /// The body a call site selected at its last entry (golden path §11);
  /// `None` until a reload offered one.
  function: Option<Arc<CompiledFunction>>,
  revision: u64,
}
impl Continuation {
  fn reset(&mut self) {
    self.phase = 0;
    self.count = 0;
    self.limit = None;
  }
}
enum State {
  Leaf(Box<dyn Any>),
  Control(Box<Continuation>),
}

/// The state of a leaf in a cached invocation frame between invocations:
/// cleaned up at exit, instantiated again at the next entry. A zero-sized
/// sentinel in the leaf's box (no allocation), so `State` stays two words.
struct Pending;

impl State {
  fn pending() -> State {
    State::Leaf(Box::new(Pending))
  }

  fn is_pending(&self) -> bool {
    matches!(self, State::Leaf(s) if s.is::<Pending>())
  }
}
/// A frame: 112 bytes, so the arena packs seven per kilobyte (the layout is
/// pinned in `tests/trampoline.rs`). Function-only data is boxed.
struct Frame {
  code: Code,
  /// `code.flow()`, resolved once: the flow lives inside the handle `code`
  /// holds, so its address is stable for the frame's lifetime (see `flow`).
  flow: *const CompiledFlow,
  states: Vec<State>,
  value: Var,
  parent: Option<Handle>,
  /// The function frame whose locals this frame's code addresses; `None`
  /// for the instance's own locals.
  scope: Option<Handle>,
  /// For a `Code::Function` frame: the invocation's locals and table.
  invocation: Option<Box<Invocation>>,
  pc: u32,
  call_depth: u32,
  /// The arena slot's generation (see `arena::Generational`).
  generation: std::num::NonZeroU32,
}

impl Generational for Frame {
  fn generation(&self) -> std::num::NonZeroU32 {
    self.generation
  }
  fn set_generation(&mut self, generation: std::num::NonZeroU32) {
    self.generation = generation;
  }
}

/// What a function frame owns beyond its code: the invocation's locals and
/// the table its recursive calls resolve through, pinned at the outermost
/// entry of the group.
struct Invocation {
  locals: Vec<Var>,
  table: Arc<FunctionRegistry>,
}

impl Frame {
  fn new(code: Code) -> Self {
    let flow: *const CompiledFlow = code.flow();
    Self {
      states: Vec::with_capacity(code.flow().nodes.len()),
      flow,
      code,
      value: Var::None,
      parent: None,
      scope: None,
      invocation: None,
      pc: 0,
      call_depth: 0,
      generation: std::num::NonZeroU32::MIN,
    }
  }

  fn pc(&self) -> usize {
    self.pc as usize
  }

  /// The frame's flow, without resolving it through the handle each step.
  /// The reference is not tied to the frame borrow, so a step can read the
  /// flow while writing the frame's other fields; it must not outlive the
  /// frame (every use is within one step or one cleanup of the frame).
  fn flow<'a>(&self) -> &'a CompiledFlow {
    // SAFETY: `self.flow` was taken from `self.code`, which the frame owns
    // for as long as it exists; the flow is inside that handle's allocation
    // (a wire's or function's field, or a child flow of a compiled node) and
    // never moves. The frame never changes `code` after construction, and
    // callers keep the frame alive while they hold the reference.
    unsafe { &*self.flow }
  }

  fn locals(&self) -> &[Var] {
    self.invocation.as_ref().map_or(&[], |i| &i.locals)
  }

  fn locals_mut(&mut self) -> &mut Vec<Var> {
    &mut self.invocation.as_mut().expect("function frame").locals
  }
}

/// The VM form of a call site with a kept frame, when it has one: a
/// stateless, non-recursive site whose body is straight-line code.
fn vm_site(
  call: &CallCompiled,
  body: &Arc<CompiledFunction>,
) -> Option<std::ptr::NonNull<CallCompiled>> {
  (!call.stateful() && body.vm_only && matches!(call.target, CallTarget::Direct(_)))
    .then(|| std::ptr::NonNull::from(call))
}

/// Binds a call's arguments and input into an invocation's locals: the
/// arguments are snapshots taken from the caller's scope (which `ctx`
/// still addresses at this point), immutable for the whole invocation.
fn bind_arguments(
  locals: &mut [Var],
  body: &CompiledFunction,
  call: &CallCompiled,
  input: Var,
  ctx: &ActivationCtx<'_>,
) {
  for (slot, op) in body.param_slots.iter().zip(&call.args) {
    locals[*slot] = op.get(ctx);
  }
  locals[body.input_slot] = input;
}

/// What one frame step asks the loop to do next.
enum Next {
  Continue,
  Finish(Result<Step>),
}

/// An instance's flat frame tree and current innermost activation. Child
/// handles survive suspension; no ancestor is visited to find the active leaf.
pub(crate) struct Engine {
  frames: Arena<Frame>,
  root: Option<Handle>,
  current: Option<Handle>,
  /// The function frame whose locals `current` addresses (its `scope`),
  /// kept beside it so a step needs no lookup to know it.
  current_scope: Option<Handle>,
  /// The instance's own locals during an activation (`ctx.locals` as
  /// handed in), to point back at when the active scope leaves every
  /// invocation. Null outside an activation.
  own_locals: *mut [Var],
  pub dispatches: u64,
}

impl Engine {
  pub fn instantiate(wire: Arc<CompiledWire>, ctx: &mut InstanceCtx) -> Result<Self> {
    let mut engine = Self {
      frames: Arena::default(),
      root: None,
      current: None,
      current_scope: None,
      own_locals: std::ptr::slice_from_raw_parts_mut(std::ptr::null_mut(), 0),
      dispatches: 0,
    };
    let root = engine.build(Code::Root(wire), None, ctx)?;
    engine.root = Some(root);
    Ok(engine)
  }

  /// Depth-first initialization, preserving source order without Rust
  /// recursion. `scope` is the function frame whose locals the new frames
  /// address (a `Code::Function` root addresses its own).
  fn build(&mut self, code: Code, scope: Option<Handle>, ctx: &mut InstanceCtx) -> Result<Handle> {
    // The immutable graph gives the exact eagerly instantiated frame count.
    // Reserve once: Vec growth at a deep branch otherwise briefly needs both
    // the old and doubled arena, and wastes scarce device heap after growth.
    let mut count = 0;
    let mut pending = vec![code.clone()];
    while let Some(code) = pending.pop() {
      count += 1;
      for node in &code.flow().nodes {
        if let Some(control) = node.control() {
          for i in 0..control.len() {
            pending.push(Code::Child(node.clone(), i as u32));
          }
        }
      }
    }
    drop(pending);
    self.frames.reserve(count);
    let function = matches!(code, Code::Function(_));
    let root = self.frames.insert(Frame::new(code));
    let scope = if function { Some(root) } else { scope };
    self.frames.get_mut(root).expect("root").scope = scope;
    let mut work = vec![root];
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
      while let Some(&handle) = work.last() {
        let frame = self.frames.get(handle).expect("frame");
        let index = frame.states.len();
        if index == frame.flow().nodes.len() {
          work.pop();
          continue;
        }
        let node = frame.flow().nodes[index].clone();
        let state = if let Some(control) = node.control() {
          let mut c = Continuation {
            children: Vec::with_capacity(control.len()),
            is_call: matches!(control, Control::Call(_)),
            ..Continuation::default()
          };
          for i in 0..control.len() {
            let mut child = Frame::new(Code::Child(node.clone(), i as u32));
            child.scope = scope;
            c.children.push(self.frames.insert(child));
          }
          work.extend(c.children.iter().rev().copied());
          State::Control(Box::new(c))
        } else {
          // Shared panic/rollback policy, including host instantiation panics.
          let mut cleanup = ctx.cleanup_ctx();
          let mut initialized = crate::lifecycle::instantiate_all(
            [node],
            |n| n.instantiate(ctx),
            |n, s| n.cleanup(s.as_mut(), &mut cleanup),
          )?;
          State::Leaf(initialized.pop().expect("one node").1)
        };
        self
          .frames
          .get_mut(handle)
          .expect("frame")
          .states
          .push(state);
      }
      Ok(())
    }))
    .unwrap_or_else(|p| {
      Err(Error::Activation(format!(
        "panic in instantiate: {}",
        crate::error::panic_message(&*p)
      )))
    });
    if let Err(err) = result {
      let cleanup = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        self.cleanup_tree(root, &mut ctx.cleanup_ctx());
      }));
      return Err(match cleanup {
        Ok(()) => err,
        Err(p) => Error::Activation(format!(
          "{err}; and a cleanup panicked during rollback: {}",
          crate::error::panic_message(&*p)
        )),
      });
    }
    Ok(root)
  }

  /// Remove ownership first, then attempt every logical cleanup in reverse
  /// initialization order. A panicking cleanup cannot strand an owned frame.
  fn cleanup_tree(&mut self, root: Handle, ctx: &mut CleanupCtx) {
    enum Work {
      Frame(Handle),
      Node(Arc<dyn CompiledNode>, State),
    }
    let mut work = vec![Work::Frame(root)];
    let mut leaves = Vec::new();
    while let Some(item) = work.pop() {
      match item {
        Work::Frame(h) => {
          let frame = self.frames.remove(h).expect("live cleanup frame");
          for (node, state) in frame.flow().nodes.iter().cloned().zip(frame.states) {
            work.push(Work::Node(node, state));
          }
        }
        Work::Node(_, State::Leaf(state)) if state.is::<Pending>() => {}
        Work::Node(node, State::Leaf(state)) => leaves.push((node, state)),
        Work::Node(_, State::Control(c)) => work.extend(c.children.into_iter().map(Work::Frame)),
      }
    }
    crate::lifecycle::cleanup_each(leaves, |(node, mut state)| {
      node.cleanup(state.as_mut(), ctx)
    });
  }

  pub fn cleanup(&mut self, ctx: &mut CleanupCtx) {
    self.current = None;
    self.current_scope = None;
    if let Some(root) = self.root.take() {
      self.cleanup_tree(root, ctx);
    }
  }

  pub fn state_size(&self) -> usize {
    self.frames.capacity_bytes()
      + self
        .frames
        .values()
        .map(|f| {
          f.states.capacity() * std::mem::size_of::<State>()
            + f.invocation.as_ref().map_or(0, |i| {
              std::mem::size_of::<Invocation>() + i.locals.capacity() * std::mem::size_of::<Var>()
            })
            + f
              .states
              .iter()
              .enumerate()
              .map(|(i, s)| match s {
                State::Leaf(s) if s.is::<Pending>() => 0,
                State::Leaf(s) => f.flow().nodes[i].state_size(s.as_ref()),
                State::Control(c) => {
                  std::mem::size_of::<Continuation>()
                    + c.children.capacity() * std::mem::size_of::<Handle>()
                }
              })
              .sum::<usize>()
        })
        .sum::<usize>()
  }

  fn continuation(&mut self, h: Handle) -> &mut Continuation {
    let f = self.frames.get_mut(h).expect("frame");
    let pc = f.pc();
    match &mut f.states[pc] {
      State::Control(c) => c,
      _ => unreachable!(),
    }
  }

  /// Entering a function call (golden path §3.3, §5, §11): the call site
  /// selects the newest accepted body once per reload, the arguments are
  /// read once, in the caller's frame, then the invocation frame is built
  /// (stateless: fresh each time) or reused (stateful: its `Keep` slots and
  /// native state survive, ordinary locals start fresh), and receives the
  /// parameters and `input`.
  fn prepare_function_call(
    &mut self,
    h: Handle,
    call: &CallCompiled,
    ctx: &mut ActivationCtx<'_>,
  ) -> Result<()> {
    let (depth, input, scope, revision, kept) = {
      let f = self.frames.get_mut(h).expect("frame");
      let input = if call.def().ignores_input() {
        Var::None
      } else {
        f.value.clone()
      };
      let pc = f.pc();
      let State::Control(c) = &mut f.states[pc] else {
        unreachable!("a call site is a composite")
      };
      (
        f.call_depth as usize,
        input,
        f.scope,
        c.revision,
        c.children.first().copied(),
      )
    };
    if depth >= ctx.max_call_depth {
      use crate::diagnostic::{Diagnostic, PathStep, Phase};
      let name = &call.def().name;
      let mut d = Diagnostic::new(
        Phase::Activate,
        "activation-error",
        "recursion-limit",
        format!(
          "{name} exceeds max_call_depth {} at depth {depth}",
          ctx.max_call_depth
        ),
      );
      d.path.push(PathStep::Function(name.clone()));
      return Err(Error::Diagnostic(Box::new(d.shard(name))));
    }
    // The body, and the table it pins for recursive calls underneath it.
    let (body, table) = match &call.target {
      // A recursive call resolves through the table its group entered
      // with, so the group runs as a unit across reloads (§11).
      CallTarget::Lazy { key, def } => {
        let table = scope
          .and_then(|s| self.frames.get(s).expect("scope").invocation.as_ref())
          .map_or_else(|| ctx.table().clone(), |i| i.table.clone());
        let body = table.get(key).cloned().ok_or_else(|| {
          Error::Activation(format!(
            "{} has no compiled body for input {} in this revision",
            def.name, key.input
          ))
        })?;
        (body, table)
      }
      CallTarget::Direct(direct) => {
        // Body selection: once per accepted reload, at an inactive boundary.
        // Between reloads a kept frame is entered without touching any
        // handle: its code is the selected body.
        if revision == ctx.reload_revision()
          && let Some(child) = kept
        {
          return self.enter_kept_frame(child, call, input, ctx);
        }
        let c = self.continuation(h);
        let mut body = c.function.clone().unwrap_or_else(|| direct.clone());
        if c.revision != ctx.reload_revision() {
          c.revision = ctx.reload_revision();
          if let Some(next) = ctx.function_body(&FunctionKey::of(&body))
            && !Arc::ptr_eq(&next, &body)
          {
            // The old frames stay owned by the site until the replacement
            // exists: a failed migration leaves the site as it was (its
            // old body still selected, retried at the next entry) and
            // nothing becomes unreachable for cleanup.
            let migrated = match (call.stateful(), c.children.first().copied()) {
              (true, Some(old_child)) => match self.migrate_component(old_child, &body, &next, ctx)
              {
                Ok(child) => Some(child),
                Err(err) => {
                  self.continuation(h).revision = 0;
                  return Err(err);
                }
              },
              _ => None,
            };
            let c = self.continuation(h);
            let old = std::mem::replace(&mut c.children, migrated.into_iter().collect());
            c.function = Some(next.clone());
            c.vm = None;
            for child in old {
              self.cleanup_tree(
                child,
                &mut CleanupCtx {
                  instance: ctx.instance(),
                },
              );
            }
            body = next;
          }
        }
        (body, ctx.table().clone())
      }
    };
    let body = &body;
    if let Some(child) = self.continuation(h).children.first().copied() {
      return self.enter_kept_frame(child, call, input, ctx);
    }
    let child = self.build(
      Code::Function(body.clone()),
      None,
      &mut InstanceCtx {
        instance: ctx.instance(),
      },
    )?;
    let frame = self.frames.get_mut(child).expect("new call frame");
    let mut locals = body.fresh_locals();
    bind_arguments(&mut locals, body, call, input, ctx);
    frame.invocation = Some(Box::new(Invocation {
      locals,
      table: table.clone(),
    }));
    let vm = vm_site(call, body);
    let c = self.continuation(h);
    c.children.push(child);
    c.resets = !call.stateful() && body.native_state;
    c.vm = vm;
    Ok(())
  }

  /// Enters a call site's kept frame (a stateful component, or a stateless
  /// site's cached invocation): resets its program counter and the locals
  /// that do not persist, re-pins the current table (a recursive group
  /// below runs on the revision current now, not at the first call), and
  /// instantiates the leaves `reset_invocation` cleaned up. No handle is
  /// cloned unless the table changed.
  fn enter_kept_frame(
    &mut self,
    child: Handle,
    call: &CallCompiled,
    input: Var,
    ctx: &ActivationCtx<'_>,
  ) -> Result<()> {
    let frame = self.frames.get_mut(child).expect("kept call frame");
    let Code::Function(body) = &frame.code else {
      unreachable!("a call's frame holds its function")
    };
    frame.pc = 0;
    let invocation = frame.invocation.as_mut().expect("function frame");
    if !Arc::ptr_eq(&invocation.table, ctx.table()) {
      invocation.table = ctx.table().clone();
    }
    if body.keeps.is_empty() {
      // A stateless body: parameters and the input are bound below, only
      // the other locals need clearing.
      for slot in &body.scratch_slots {
        invocation.locals[*slot] = Var::None;
      }
    } else {
      for (slot, value) in invocation.locals.iter_mut().enumerate() {
        if !body.persistent(slot) {
          *value = Var::None;
        }
      }
    }
    bind_arguments(&mut invocation.locals, body, call, input, ctx);
    if !call.stateful() && body.native_state {
      self.revive_invocation(child, ctx)?;
    }
    Ok(())
  }

  /// A stateful call site adopting a new body (golden path §11): the new
  /// frame is built, `Keep` slots that match by name and type carry their
  /// values over (and count as applied), everything else starts fresh. The
  /// old frame is cleaned up by the caller afterwards.
  fn migrate_component(
    &mut self,
    old_child: Handle,
    old: &CompiledFunction,
    new: &Arc<CompiledFunction>,
    ctx: &ActivationCtx<'_>,
  ) -> Result<Handle> {
    let plan = crate::reload::keep_plan(old, new);
    let child = self.build(
      Code::Function(new.clone()),
      None,
      &mut InstanceCtx {
        instance: ctx.instance(),
      },
    )?;
    let mut locals = new.fresh_locals();
    let old_locals = self.frames.get(old_child).expect("old call frame").locals();
    for (from, to) in &plan.moves {
      locals[*to] = old_locals[*from].clone();
    }
    let frame = self.frames.get_mut(child).expect("new call frame");
    frame.invocation = Some(Box::new(Invocation {
      locals,
      table: ctx.table().clone(),
    }));
    Ok(child)
  }

  /// The function bodies this engine holds frames for: running or cached
  /// invocations, and stateful call sites' components. A reload validates
  /// against these, not against the bodies the wire was composed with,
  /// since a site may have adopted newer ones (golden path §11).
  pub fn function_bodies(&self) -> impl Iterator<Item = &Arc<CompiledFunction>> {
    self.frames.values().filter_map(|f| match &f.code {
      Code::Function(body) => Some(body),
      _ => None,
    })
  }

  /// Every frame of the invocation rooted at `child`, parents first.
  fn invocation_frames(&self, child: Handle) -> Vec<Handle> {
    let mut all = Vec::new();
    let mut work = vec![child];
    while let Some(h) = work.pop() {
      all.push(h);
      for state in &self.frames.get(h).expect("invocation frame").states {
        if let State::Control(c) = state {
          work.extend(c.children.iter().copied());
        }
      }
    }
    all
  }

  /// A stateless invocation's exit (golden path §3.4): the frames stay for
  /// the next call, but native state inside them lives for the invocation,
  /// so every leaf that is not stateless is cleaned up now and instantiated
  /// again at the next entry.
  fn reset_invocation(&mut self, child: Handle, ctx: &mut CleanupCtx) {
    let mut leaves = Vec::new();
    for h in self.invocation_frames(child) {
      let frame = self.frames.get_mut(h).expect("invocation frame");
      let nodes = frame.flow().nodes.clone();
      for (node, state) in nodes.iter().zip(frame.states.iter_mut()) {
        if matches!(state, State::Leaf(_))
          && !state.is_pending()
          && node.lifetime() != crate::signature::Lifetime::Stateless
          && let State::Leaf(s) = std::mem::replace(state, State::pending())
        {
          leaves.push((node.clone(), s));
        }
      }
    }
    crate::lifecycle::cleanup_each(leaves, |(node, mut state)| {
      node.cleanup(state.as_mut(), ctx)
    });
  }

  /// The entry of a cached stateless invocation: instantiates the leaves
  /// `reset_invocation` cleaned up. An error leaves the rest pending for
  /// the next attempt; nothing instantiated is lost, the frames own it.
  fn revive_invocation(&mut self, child: Handle, ctx: &ActivationCtx<'_>) -> Result<()> {
    let mut ictx = InstanceCtx {
      instance: ctx.instance(),
    };
    for h in self.invocation_frames(child) {
      let frame = self.frames.get_mut(h).expect("invocation frame");
      let nodes = frame.flow().nodes.clone();
      for (node, state) in nodes.iter().zip(frame.states.iter_mut()) {
        if state.is_pending() {
          *state = State::Leaf(node.instantiate(&mut ictx)?);
        }
      }
    }
    Ok(())
  }

  pub fn activate(&mut self, ctx: &mut ActivationCtx<'_>, input: &Var) -> Result<Step> {
    if self.current.is_none() {
      let root = self.root.expect("live engine");
      self.frames.get_mut(root).expect("root").value = input.clone();
      self.current = Some(root);
      self.current_scope = None;
    }
    let mut completed = None;
    self.own_locals = &mut *ctx.locals;
    // Resuming inside an invocation: its locals are the ones addressed.
    let scope = self.current_scope;
    self.switch_scope(scope, ctx);
    loop {
      let h = self.current.expect("active frame");
      if let Next::Finish(result) = self.step(h, ctx, &mut completed) {
        // The activation ends with the instance's own locals addressed.
        self.switch_scope(None, ctx);
        self.own_locals = std::ptr::slice_from_raw_parts_mut(std::ptr::null_mut(), 0);
        return result;
      }
    }
  }

  /// Points `ctx.locals` at the locals of `scope`: a function frame's
  /// invocation buffer, or the instance's own for `None`.
  ///
  /// SAFETY contract: `ctx.locals` is handed in for the activation as the
  /// instance's own locals, which outlive it; an invocation's buffer is
  /// owned by a frame that is an ancestor of (or is) the current frame,
  /// and such a frame is never removed while it is current (frames are
  /// removed only below a completed call site, or when a site replaces its
  /// own children before entering them); an invocation's locals never
  /// resize. So the pointer stays valid for as long as it is addressed,
  /// and the next switch or the end of the activation replaces it.
  fn switch_scope(&mut self, scope: Option<Handle>, ctx: &mut ActivationCtx<'_>) {
    let locals: *mut [Var] = match scope {
      None => self.own_locals,
      Some(s) => self
        .frames
        .get_mut(s)
        .expect("scope")
        .locals_mut()
        .as_mut_slice(),
    };
    // SAFETY: see the contract above.
    ctx.locals = unsafe { &mut *locals };
  }

  /// Makes `child` the current frame with `input`, at call depth `depth`.
  #[inline(always)]
  fn enter_child(
    &mut self,
    h: Handle,
    child: Handle,
    input: Var,
    depth: u32,
    ctx: &mut ActivationCtx<'_>,
  ) -> Next {
    let child_frame = self.frames.get_mut(child).expect("live child");
    debug_assert!(child_frame.parent.is_none());
    child_frame.parent = Some(h);
    child_frame.call_depth = depth;
    child_frame.value = input;
    let scope = child_frame.scope;
    self.current = Some(child);
    if scope != self.current_scope {
      self.current_scope = scope;
      self.switch_scope(scope, ctx);
    }
    Next::Continue
  }

  /// One step of the current frame: dispatches its current node, or
  /// delivers a child's completion to it. Inlined into the activation loop
  /// (its only caller): a call per step is measurable.
  #[inline(always)]
  fn step(
    &mut self,
    h: Handle,
    ctx: &mut ActivationCtx<'_>,
    completed: &mut Option<Result<Step>>,
  ) -> Next {
    // No registry lookup or compiled-handle cloning on the usual hot path:
    // one frame lookup per step. A call site asks to be prepared at phase 0
    // (`Request::Prepare`), which takes a step of its own; a call to a
    // straight-line body, and a `Repeat` of one, run inside the VM segment.
    let f = self.frames.get_mut(h).expect("live frame");
    let index = f.pc();
    let flow = f.flow();
    // The VM runs every instruction it can, leaf or composite, as one
    // segment; it stops before a node that activates through its shard, a
    // composite without a VM form, or a call site that is not ready, and
    // those take the paths below. A segment that made no progress (a call
    // site not ready) falls through to the composite path of that node.
    let mut vm_error = None;
    let vm = completed.is_none()
      && index < flow.nodes.len()
      && !matches!(flow.code[index].op, crate::inline::Op::Fallback);
    let f = if vm {
      let value = std::mem::replace(&mut f.value, Var::None);
      let depth = f.call_depth as usize;
      let calls = EngineCalls {
        frames: &mut self.frames,
        h,
        revision: ctx.reload_revision(),
        max_depth: ctx.max_call_depth,
        depth,
      };
      // One runner call per step, as before: a segment stops at a
      // constructor or after one, and the next step continues it.
      let result = if flow.code[index].is_constructor() {
        crate::inline::construct(&flow.code, index, value, ctx.locals, ctx.mesh_frame)
      } else {
        crate::inline::run(&flow.code, index, value, ctx.locals, ctx.mesh_frame, &calls)
      };
      let f = self.frames.get_mut(h).expect("live frame");
      match result {
        Ok((pc, value)) => {
          f.value = value;
          if pc != index {
            f.pc = pc as u32;
            return Next::Continue;
          }
        }
        Err(err) => vm_error = Some(err),
      }
      f
    } else {
      f
    };
    // A completed stateless call: a recursive site releases its frames (a
    // group's depth varies per call); any other site keeps them for the
    // next call and resets what lives for one invocation.
    let mut release = Vec::new();
    let mut reset = None;
    // For a call being entered: whether the callee ignores its input (so the
    // enter path asks the node nothing).
    let mut call_entry: Option<bool> = None;
    let result = if index == flow.nodes.len() {
      Request::Complete(Ok(Step::Next(std::mem::replace(&mut f.value, Var::None))))
    } else if let State::Control(c) = &mut f.states[index] {
      self.dispatches += 1;
      let control = flow.nodes[index].control().expect("composite");
      let stateless_call = match &control {
        Control::Call(call) if !call.stateful() => {
          Some(matches!(call.target, CallTarget::Lazy { .. }))
        }
        _ => None,
      };
      if let Control::Call(call) = &control {
        call_entry = Some(call.def().ignores_input());
      }
      let result = match vm_error.take() {
        Some(err) => Request::Complete(Err(err)),
        None => match dispatch(control, c, ctx, &f.value, completed.take()) {
          Ok(Dispatch::Enter(i)) => Request::Enter(c.children[i]),
          Ok(Dispatch::Complete(result)) => Request::Complete(result),
          Ok(Dispatch::Prepare) => Request::Prepare,
          Err(err) => Request::Complete(Err(err)),
        },
      };
      if let Request::Complete(_) = &result {
        c.reset();
        match stateless_call {
          Some(true) => release = std::mem::take(&mut c.children),
          Some(false) if c.resets => reset = c.children.first().copied(),
          _ => {}
        }
      }
      result
    } else if let Some(err) = vm_error.take() {
      Request::Complete(Err(err))
    } else {
      debug_assert!(completed.is_none());
      let State::Leaf(state) = &mut f.states[index] else {
        unreachable!()
      };
      Request::Complete(flow.nodes[index].activate(state.as_mut(), ctx, &f.value))
    };
    let next = match result {
      Request::Prepare => {
        // The call site is read through the frame's flow, whose reference
        // is not tied to the frame borrow (the frame stays current).
        let Some(Control::Call(call)) = flow.nodes[index].control() else {
          unreachable!("only a call asks to be prepared")
        };
        match self.prepare_function_call(h, call, ctx) {
          Ok(()) => {
            // Prepared: entered in this step, not the next (`dispatch`
            // would answer `enter(c, 0, 2)`).
            let f = self.frames.get_mut(h).expect("live frame");
            let State::Control(c) = &mut f.states[index] else {
              unreachable!("a call site is a composite")
            };
            c.phase = 2;
            let child = c.children[0];
            let input = if call_entry == Some(true) {
              Var::None
            } else {
              f.value.clone()
            };
            let depth = f.call_depth + 1;
            self.enter_child(h, child, input, depth, ctx)
          }
          Err(err) => {
            let f = self.frames.get_mut(h).expect("live frame");
            if let State::Control(c) = &mut f.states[index] {
              c.reset();
            }
            f.pc = 0;
            f.value = Var::None;
            self.current = f.parent.take();
            match self.current {
              None => {
                self.current_scope = None;
                Next::Finish(Err(err))
              }
              Some(parent) => {
                let scope = self.frames.get(parent).expect("parent frame").scope;
                self.current_scope = scope;
                self.switch_scope(scope, ctx);
                *completed = Some(Err(err));
                Next::Continue
              }
            }
          }
        }
      }
      Request::Enter(child) => {
        let input = if call_entry == Some(true) {
          Var::None
        } else {
          f.value.clone()
        };
        let depth = f.call_depth + u32::from(call_entry.is_some());
        self.enter_child(h, child, input, depth, ctx)
      }
      Request::Complete(Ok(Step::Suspend)) => Next::Finish(Ok(Step::Suspend)),
      Request::Complete(Ok(Step::Next(value))) if index < flow.nodes.len() => {
        f.pc += 1;
        f.value = value;
        Next::Continue
      }
      Request::Complete(result) => {
        f.pc = 0;
        f.value = Var::None;
        let leaving_function = matches!(f.code, Code::Function(_));
        let own_scope = f.scope;
        self.current = f.parent.take();
        match self.current {
          None => {
            self.current_scope = None;
            Next::Finish(result)
          }
          Some(parent) => {
            // A child shares its parent's scope unless it is an invocation
            // frame, whose parent is the caller.
            if leaving_function {
              let scope = self.frames.get(parent).expect("parent frame").scope;
              self.current_scope = scope;
              self.switch_scope(scope, ctx);
            } else {
              debug_assert_eq!(own_scope, self.current_scope);
            }
            *completed = Some(result);
            Next::Continue
          }
        }
      }
    };
    // Native state inside a stateless invocation lives for the invocation
    // (golden path §3.4): cleaned up on return, failure, Stop or Restart.
    let mut cleanup = CleanupCtx {
      instance: ctx.instance(),
    };
    for child in release {
      self.cleanup_tree(child, &mut cleanup);
    }
    if let Some(child) = reset {
      self.reset_invocation(child, &mut cleanup);
    }
    next
  }
}

// A child receives the parent's original input. Returning errors and signals
// through the same completion channel unwinds each continuation exactly once.
enum Request {
  Enter(Handle),
  Complete(Result<Step>),
  /// A call site at phase 0: the step prepares the invocation (which
  /// needs the engine, not the continuation) and dispatches again.
  Prepare,
}
/// What a composite's dispatch asks of its step: `Request` with the child
/// by index.
enum Dispatch {
  Enter(usize),
  Complete(Result<Step>),
  Prepare,
}
fn next(value: Var) -> Result<Dispatch> {
  Ok(Dispatch::Complete(Ok(Step::Next(value))))
}
fn enter(c: &mut Continuation, child: usize, phase: u32) -> Result<Dispatch> {
  c.phase = phase;
  Ok(Dispatch::Enter(child))
}

/// Dispatches the composite, running every child flow it enters that is
/// leaf code (`CompiledFlow::leaf`) right here, on the current locals, and
/// dispatching its completion in the same step: a predicate, a `Sub`,
/// `Once` or `Maybe` body, a branch costs no frame. A child that must
/// suspend, holds a site, loop, branch or constructor, or a call (its own
/// scope) is entered as a frame.
#[inline(always)]
fn dispatch(
  control: Control<'_>,
  c: &mut Continuation,
  ctx: &mut ActivationCtx<'_>,
  input: &Var,
  mut completion: Option<Result<Step>>,
) -> Result<Dispatch> {
  loop {
    let dispatched = dispatch_once(control, c, ctx, input, completion.take())?;
    let Dispatch::Enter(i) = dispatched else {
      return Ok(dispatched);
    };
    if matches!(control, Control::Call(_)) {
      return Ok(dispatched);
    }
    let flow = child_flow(&control, i);
    if !flow.leaf {
      return Ok(dispatched);
    }
    let result = crate::inline::run(
      &flow.code,
      0,
      input.clone(),
      ctx.locals,
      ctx.mesh_frame,
      &crate::inline::NoCalls,
    );
    completion = Some(result.map(|(_, value)| Step::Next(value)));
  }
}
fn boolean(v: Var) -> Result<bool> {
  match v {
    Var::Bool(b) => Ok(b),
    _ => Err(Error::Activation("predicate did not output a Bool".into())),
  }
}

/// Out of line: the match is large and inlining it into the step costs
/// the step more than the call (measured on the entity workload).
#[inline(never)]
fn dispatch_once(
  control: Control<'_>,
  c: &mut Continuation,
  ctx: &mut ActivationCtx<'_>,
  input: &Var,
  completion: Option<Result<Step>>,
) -> Result<Dispatch> {
  let value = match completion {
    None => None,
    Some(result) => {
      if matches!(control, Control::Once(_)) && result.is_ok() {
        c.once_done = true;
      }
      let step = match result {
        Err(err) if matches!(control, Control::Maybe(_)) && c.phase == 1 => {
          let Control::Maybe(m) = &control else {
            unreachable!()
          };
          control::maybe_caught(m.silent, err)?;
          return if m.flows.len() > 1 {
            enter(c, 1, 2)
          } else {
            next(input.clone())
          };
        }
        other => other?,
      };
      match step {
        Step::Next(v) => Some(v),
        // Return exits the nearest named invocation with its value.
        Step::Return(v) if matches!(control, Control::Call(_)) => return next(v),
        other => return Ok(Dispatch::Complete(Ok(other))),
      }
    }
  };
  match control {
    Control::Call(_) | Control::Match(_) if value.is_some() => next(value.unwrap()),
    Control::Call(_) if c.phase == 0 => {
      c.phase = 1;
      Ok(Dispatch::Prepare)
    }
    Control::Call(_) => enter(c, 0, 2),
    Control::Match(m) => enter(c, m.find(input)?, 1),
    Control::Sub(_) => {
      if value.is_some() {
        next(input.clone())
      } else {
        enter(c, 0, 1)
      }
    }
    Control::Once(_) => {
      if c.once_done {
        next(input.clone())
      } else {
        enter(c, 0, 1)
      }
    }
    Control::When(_) | Control::While(_) => {
      let looping = matches!(control, Control::While(_));
      match c.phase {
        0 => enter(c, 0, 1),
        1 => {
          if boolean(value.expect("predicate"))? {
            enter(c, 1, 2)
          } else {
            next(input.clone())
          }
        }
        _ => {
          if looping {
            enter(c, 0, 1)
          } else {
            next(input.clone())
          }
        }
      }
    }
    Control::If(i) => match c.phase {
      0 => enter(c, 0, 1),
      1 => {
        if boolean(value.expect("predicate"))? {
          enter(c, 1, 2)
        } else if i.flows.len() > 2 {
          enter(c, 2, 2)
        } else {
          next(input.clone())
        }
      }
      _ => next(if i.passthrough {
        input.clone()
      } else {
        value.expect("branch")
      }),
    },
    Control::Maybe(m) => {
      if let Some(value) = value {
        next(if m.flows.len() < 2 {
          input.clone()
        } else {
          value
        })
      } else {
        enter(c, 0, 1)
      }
    }
    Control::Repeat(r) => {
      match c.phase {
        0 => {
          c.limit = r.limit(ctx)?;
          c.count = 0;
        }
        1 => {
          if boolean(value.expect("until"))? {
            return next(input.clone());
          }
          return enter(c, 0, 2);
        }
        _ => c.count += 1,
      }
      if control::repeat_exhausted(c.limit, c.count) {
        next(input.clone())
      } else if r.until.is_some() {
        enter(c, 1, 1)
      } else {
        enter(c, 0, 2)
      }
    }
    Control::Conditions(conditions) => {
      let mut index = c.phase as usize;
      if let Some(value) = value
        && boolean(value)? == conditions.stop_on
      {
        return next(Var::Bool(conditions.stop_on));
      }
      // phase is the next condition, saved before entering a predicate.
      while index < conditions.conditions.len() {
        let value = match conditions.conditions[index] {
          Condition::Const(b) => b,
          Condition::Bound(binding) => matches!(ctx.get(binding), Var::Bool(true)),
          Condition::Flow(i) => return enter(c, i, (index + 1) as u32),
        };
        if value == conditions.stop_on {
          return next(Var::Bool(conditions.stop_on));
        }
        index += 1;
      }
      next(Var::Bool(!conditions.stop_on))
    }
  }
}

/// The engine's side of the VM's call sites and loops (`inline::VmCalls`)
/// for one frame. Copyable: a nested run gets its own for the frame it
/// runs in (a callee's kept frame, a loop's child frame).
#[derive(Clone, Copy)]
struct EngineCalls {
  /// The arena, outliving every run it serves (the step holds the engine).
  frames: *mut Arena<Frame>,
  h: Handle,
  revision: u64,
  max_depth: usize,
  depth: usize,
}

impl EngineCalls {
  // A raw-pointer view by design (see the safety note); the lint wants
  // `&mut self`, which a copyable value lent to a run cannot offer.
  #[allow(clippy::mut_from_ref)]
  fn arena(&self) -> &mut Arena<Frame> {
    // SAFETY: the step that created this value holds `&mut Engine` for the
    // whole run; the run addresses frames only through these calls, one at
    // a time, and the returned borrows never outlive the run (callee code
    // and locals are handed out as raw or `'static` references with the
    // contract documented at `VmCallee`).
    unsafe { &mut *self.frames }
  }

  /// The kept callee frame of the call site at `site`, with the site
  /// itself: `None` unless the site is stateless, non-recursive, at phase
  /// 0, at the current revision, with a kept frame whose body is
  /// straight-line, and within the call depth.
  fn callee(
    &self,
    site: usize,
  ) -> Option<(
    &'static CallCompiled,
    Handle,
    &'static CompiledFunction,
    *mut [Var],
  )> {
    let frames = self.arena();
    let f = frames.get(self.h)?;
    let State::Control(c) = f.states.get(site)? else {
      return None;
    };
    if !c.is_call || c.phase != 0 || c.revision != self.revision || self.depth >= self.max_depth {
      return None;
    }
    let call = c.vm?;
    let child = *c.children.first()?;
    let frame = frames.get_mut(child)?;
    let Code::Function(body) = &frame.code else {
      return None;
    };
    let body: *const CompiledFunction = Arc::as_ptr(body);
    let locals: *mut [Var] = frame.invocation.as_mut()?.locals.as_mut_slice();
    // SAFETY (lifetimes): the site lives in the flow of frame `h` and the
    // body in the kept frame, both outliving the run this serves (it ends
    // with the step; see `Continuation::vm`).
    let (call, body) = unsafe { (&*call.as_ptr(), &*body) };
    Some((call, child, body, locals))
  }
}

impl crate::inline::VmCalls for EngineCalls {
  fn enter(&self, site: usize) -> Option<crate::inline::VmCallee<Self>> {
    let (call, child, body, locals) = self.callee(site)?;
    let calls = EngineCalls {
      frames: self.frames,
      h: child,
      revision: self.revision,
      max_depth: self.max_depth,
      depth: self.depth + 1,
    };
    if !body.vm_leaf && !crate::inline::vm_ready(&body.flow.code, &calls) {
      return None;
    }
    Some(crate::inline::VmCallee {
      code: &body.flow.code,
      constructors: body
        .flow
        .code
        .iter()
        .any(crate::inline::Instruction::is_constructor),
      locals,
      param_slots: &body.param_slots,
      scratch: &body.scratch_slots,
      input_slot: body.input_slot,
      args: &call.args,
      ignores_input: call.def().ignores_input(),
      calls,
    })
  }

  fn site_ready(&self, site: usize) -> bool {
    let Some((_, child, body, _)) = self.callee(site) else {
      return false;
    };
    if body.vm_leaf {
      return true;
    }
    let calls = EngineCalls {
      frames: self.frames,
      h: child,
      revision: self.revision,
      max_depth: self.max_depth,
      depth: self.depth + 1,
    };
    crate::inline::vm_ready(&body.flow.code, &calls)
  }

  fn child(&self, site: usize, i: usize) -> Option<Self> {
    let f = self.arena().get(self.h)?;
    let State::Control(c) = f.states.get(site)? else {
      return None;
    };
    let child = *c.children.get(i)?;
    Some(EngineCalls {
      frames: self.frames,
      h: child,
      revision: self.revision,
      max_depth: self.max_depth,
      depth: self.depth,
    })
  }
}

#[cfg(test)]
mod layout {
  #[test]
  fn a_frame_is_128_bytes_on_64_bit_targets() {
    if cfg!(target_pointer_width = "64") {
      assert_eq!(std::mem::size_of::<super::Frame>(), 128);
      assert_eq!(std::mem::size_of::<Option<super::Frame>>(), 128);
      assert_eq!(std::mem::size_of::<super::State>(), 16);
      assert_eq!(std::mem::size_of::<super::Continuation>(), 80);
    }
  }
}
