//! Iterative ownership, dispatch and teardown of child flows.
use std::any::Any;
use std::sync::Arc;

use super::arena::{Arena, Handle};
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
  Child(Arc<dyn CompiledNode>, usize),
  /// A function invocation: the frame owns the invocation's locals.
  Function(Arc<CompiledFunction>),
}
impl Code {
  fn flow(&self) -> &CompiledFlow {
    match self {
      Self::Root(w) => &w.flow,
      // Resolve from the retained owner without borrowing the frame arena.
      Self::Child(n, i) => match n.control().expect("composite") {
        Control::When(c) | Control::While(c) => {
          if *i == 0 {
            &c.pred
          } else {
            &c.body
          }
        }
        Control::Sub(c) | Control::Once(c) => c,
        Control::Repeat(c) => {
          if *i == 0 {
            &c.body
          } else {
            c.until.as_ref().expect("until")
          }
        }
        Control::If(c) => &c.flows[*i],
        Control::Match(c) => &c.flows[*i],
        Control::Maybe(c) => &c.flows[*i],
        Control::Conditions(c) => &c.flows[*i],
        Control::Call(_) => unreachable!("a call's frame holds its function"),
      },
      Self::Function(f) => &f.flow,
    }
  }
}

#[derive(Default)]
struct Continuation {
  children: Vec<Handle>,
  phase: usize,
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
struct Frame {
  code: Code,
  states: Vec<State>,
  pc: usize,
  value: Var,
  parent: Option<Handle>,
  call_depth: usize,
  /// The invocation's locals, for a `Code::Function` frame; empty otherwise.
  locals: Vec<Var>,
  /// The function frame whose locals this frame's code addresses; `None`
  /// for the instance's own locals.
  scope: Option<Handle>,
  /// For a function frame: the table its recursive calls resolve through,
  /// pinned at the outermost entry of the group.
  table: Option<Arc<FunctionRegistry>>,
}
impl Frame {
  fn new(code: Code) -> Self {
    Self {
      states: Vec::with_capacity(code.flow().nodes.len()),
      code,
      pc: 0,
      value: Var::None,
      parent: None,
      call_depth: 0,
      locals: Vec::new(),
      scope: None,
      table: None,
    }
  }
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
  pub dispatches: u64,
}

impl Engine {
  pub fn instantiate(wire: Arc<CompiledWire>, ctx: &mut InstanceCtx) -> Result<Self> {
    let mut engine = Self {
      frames: Arena::default(),
      root: None,
      current: None,
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
            pending.push(Code::Child(node.clone(), i));
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
        if index == frame.code.flow().nodes.len() {
          work.pop();
          continue;
        }
        let node = frame.code.flow().nodes[index].clone();
        let state = if let Some(control) = node.control() {
          let mut c = Continuation {
            children: Vec::with_capacity(control.len()),
            ..Continuation::default()
          };
          for i in 0..control.len() {
            let mut child = Frame::new(Code::Child(node.clone(), i));
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
          for (node, state) in frame.code.flow().nodes.iter().cloned().zip(frame.states) {
            work.push(Work::Node(node, state));
          }
        }
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
            + f.locals.capacity() * std::mem::size_of::<Var>()
            + f
              .states
              .iter()
              .enumerate()
              .map(|(i, s)| match s {
                State::Leaf(s) => f.code.flow().nodes[i].state_size(s.as_ref()),
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
    match &mut f.states[f.pc] {
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
    let (depth, input, scope) = {
      let f = self.frames.get(h).expect("frame");
      let input = if call.def().ignores_input() {
        Var::None
      } else {
        f.value.clone()
      };
      (f.call_depth, input, f.scope)
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
          .and_then(|s| self.frames.get(s).expect("scope").table.clone())
          .unwrap_or_else(|| ctx.table());
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
        let c = self.continuation(h);
        let mut body = c.function.clone().unwrap_or_else(|| direct.clone());
        if c.revision != ctx.reload_revision() {
          c.revision = ctx.reload_revision();
          if let Some(next) = ctx.function_body(&FunctionKey::of(&body))
            && !Arc::ptr_eq(&next, &body)
          {
            let old = std::mem::take(&mut c.children);
            c.function = Some(next.clone());
            if let (true, Some(&old_child)) = (call.stateful(), old.first()) {
              self.migrate_component(h, old_child, &body, &next, ctx)?;
            }
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
        (body, ctx.table())
      }
    };
    // Arguments are immutable snapshots for the whole invocation.
    let values: Vec<Var> = call.args.iter().map(|op| op.get(ctx)).collect();
    let body = &body;
    let child = match self.continuation(h).children.first().copied() {
      Some(child) => {
        let frame = self.frames.get_mut(child).expect("stateful call frame");
        frame.pc = 0;
        for (slot, value) in frame.locals.iter_mut().enumerate() {
          if !body.persistent(slot) {
            *value = Var::None;
          }
        }
        child
      }
      None => {
        let child = self.build(
          Code::Function(body.clone()),
          None,
          &mut InstanceCtx {
            instance: ctx.instance(),
          },
        )?;
        let frame = self.frames.get_mut(child).expect("new call frame");
        frame.locals = body.fresh_locals();
        frame.table = Some(table.clone());
        self.continuation(h).children.push(child);
        child
      }
    };
    let frame = self.frames.get_mut(child).expect("call frame");
    for (slot, value) in body.param_slots.iter().zip(values) {
      frame.locals[*slot] = value;
    }
    frame.locals[body.input_slot] = input;
    Ok(())
  }

  /// A stateful call site adopting a new body (golden path §11): the new
  /// frame is built, `Keep` slots that match by name and type carry their
  /// values over (and count as applied), everything else starts fresh. The
  /// old frame is cleaned up by the caller afterwards.
  fn migrate_component(
    &mut self,
    h: Handle,
    old_child: Handle,
    old: &CompiledFunction,
    new: &Arc<CompiledFunction>,
    ctx: &ActivationCtx<'_>,
  ) -> Result<()> {
    let plan = crate::reload::keep_plan(old, new);
    let child = self.build(
      Code::Function(new.clone()),
      None,
      &mut InstanceCtx {
        instance: ctx.instance(),
      },
    )?;
    let mut locals = new.fresh_locals();
    let old_locals = &self.frames.get(old_child).expect("old call frame").locals;
    for (from, to, _) in &plan.moves {
      locals[*to] = old_locals[*from].clone();
    }
    let frame = self.frames.get_mut(child).expect("new call frame");
    frame.locals = locals;
    frame.table = Some(ctx.table());
    for (_, _, node) in &plan.moves {
      if let State::Leaf(state) = &mut frame.states[*node]
        && let Some(applied) = state.downcast_mut::<bool>()
      {
        *applied = true;
      }
    }
    self.continuation(h).children.push(child);
    Ok(())
  }

  pub fn activate(&mut self, ctx: &mut ActivationCtx<'_>, input: &Var) -> Result<Step> {
    if self.current.is_none() {
      let root = self.root.expect("live engine");
      self.frames.get_mut(root).expect("root").value = input.clone();
      self.current = Some(root);
    }
    let mut completed = None;
    loop {
      let h = self.current.expect("active frame");
      // A frame inside a function invocation addresses that invocation's
      // locals: swap them in for this step, back out afterwards.
      let scope = self.frames.get(h).expect("frame").scope;
      let mut saved = None;
      if let Some(s) = scope {
        let mut mine = std::mem::take(&mut self.frames.get_mut(s).expect("scope").locals);
        std::mem::swap(ctx.locals, &mut mine);
        saved = Some((s, mine));
      }
      let next = self.step(h, ctx, &mut completed);
      if let Some((s, mut mine)) = saved {
        std::mem::swap(ctx.locals, &mut mine);
        if let Some(frame) = self.frames.get_mut(s) {
          frame.locals = mine;
        }
      }
      match next {
        Next::Continue => {}
        Next::Finish(result) => return result,
      }
    }
  }

  /// One step of the current frame: dispatches its current node, or
  /// delivers a child's completion to it.
  fn step(
    &mut self,
    h: Handle,
    ctx: &mut ActivationCtx<'_>,
    completed: &mut Option<Result<Step>>,
  ) -> Next {
    // No registry lookup or compiled-handle cloning on the usual hot path.
    let mut prepared = Ok(());
    if completed.is_none() {
      let f = self.frames.get(h).expect("frame");
      let entering = match f.states.get(f.pc) {
        Some(State::Control(c)) if c.phase == 0 => Some(f.code.flow().nodes[f.pc].clone()),
        _ => None,
      };
      if let Some(node) = entering
        && let Some(Control::Call(call)) = node.control()
      {
        prepared = self.prepare_function_call(h, call, ctx);
      }
    }
    let f = self.frames.get_mut(h).expect("live frame");
    let flow = f.code.flow();
    let index = f.pc;
    // A stateless invocation's frame, released once its call completes.
    let mut release = Vec::new();
    let result = if index == flow.nodes.len() {
      Request::Complete(Ok(Step::Next(std::mem::replace(&mut f.value, Var::None))))
    } else if let State::Control(c) = &mut f.states[index] {
      self.dispatches += 1;
      let control = flow.nodes[index].control().expect("composite");
      let stateless_call = matches!(&control, Control::Call(call) if !call.stateful());
      let request = prepared.and_then(|()| dispatch(control, c, ctx, &f.value, completed.take()));
      match request {
        Ok(Request::Enter(child)) => Request::Enter(child),
        Ok(Request::Complete(result)) => {
          c.reset();
          if stateless_call {
            release = std::mem::take(&mut c.children);
          }
          Request::Complete(result)
        }
        Err(err) => {
          c.reset();
          if stateless_call {
            release = std::mem::take(&mut c.children);
          }
          Request::Complete(Err(err))
        }
      }
    } else {
      debug_assert!(completed.is_none());
      if flow.code[index].is_constructor()
        || !matches!(flow.code[index].op, crate::inline::Op::Fallback)
      {
        let value = std::mem::replace(&mut f.value, Var::None);
        let result = if flow.code[index].is_constructor() {
          crate::inline::construct(&flow.code, index, value, ctx.locals, ctx.mesh_frame)
        } else {
          crate::inline::run(&flow.code, index, value, ctx.locals, ctx.mesh_frame)
        };
        match result {
          Ok((pc, value)) => {
            f.pc = pc;
            f.value = value;
            return Next::Continue;
          }
          Err(e) => Request::Complete(Err(e)),
        }
      } else {
        let State::Leaf(state) = &mut f.states[index] else {
          unreachable!()
        };
        Request::Complete(flow.nodes[index].activate(state.as_mut(), ctx, &f.value))
      }
    };
    let next = match result {
      Request::Enter(child) => {
        let input = match flow.nodes[index].control() {
          Some(Control::Call(call)) if call.def().ignores_input() => Var::None,
          _ => f.value.clone(),
        };
        let depth = f.call_depth
          + usize::from(matches!(
            flow.nodes[index].control(),
            Some(Control::Call(_))
          ));
        let child_frame = self.frames.get_mut(child).expect("live child");
        debug_assert!(child_frame.parent.is_none());
        child_frame.parent = Some(h);
        child_frame.call_depth = depth;
        child_frame.value = input;
        self.current = Some(child);
        Next::Continue
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
        self.current = f.parent.take();
        if self.current.is_none() {
          Next::Finish(result)
        } else {
          *completed = Some(result);
          Next::Continue
        }
      }
    };
    // Native state inside a stateless invocation lives for the invocation
    // (golden path §3.4): cleaned up on return, failure, Stop or Restart.
    for child in release {
      self.cleanup_tree(
        child,
        &mut CleanupCtx {
          instance: ctx.instance(),
        },
      );
    }
    next
  }
}

// A child receives the parent's original input. Returning errors and signals
// through the same completion channel unwinds each continuation exactly once.
enum Request {
  Enter(Handle),
  Complete(Result<Step>),
}
fn next(value: Var) -> Result<Request> {
  Ok(Request::Complete(Ok(Step::Next(value))))
}
fn enter(c: &mut Continuation, child: usize, phase: usize) -> Result<Request> {
  c.phase = phase;
  Ok(Request::Enter(c.children[child]))
}
fn boolean(v: Var) -> Result<bool> {
  match v {
    Var::Bool(b) => Ok(b),
    _ => Err(Error::Activation("predicate did not output a Bool".into())),
  }
}

fn dispatch(
  control: Control<'_>,
  c: &mut Continuation,
  ctx: &mut ActivationCtx<'_>,
  input: &Var,
  completion: Option<Result<Step>>,
) -> Result<Request> {
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
        other => return Ok(Request::Complete(Ok(other))),
      }
    }
  };
  match control {
    Control::Call(_) | Control::Match(_) if value.is_some() => next(value.unwrap()),
    Control::Call(_) => enter(c, 0, 1),
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
      let mut index = c.phase;
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
          Condition::Flow(i) => return enter(c, i, index + 1),
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
