//! Iterative ownership, dispatch and teardown of child flows.
use std::any::Any;
use std::sync::Arc;

use super::arena::{Arena, Handle};
use super::{ActivationCtx, CompiledNode, Stackless, Step};
use crate::Var;
use crate::compose::CompiledWire;
use crate::error::{Error, Result};
use crate::flow::CompiledFlow;
use crate::instance::{CleanupCtx, InstanceCtx};
use crate::reload::InlineCall;
use crate::shards::control::{
  self, Condition, ConditionsCompiled, IfCompiled, MatchCompiled, MaybeCompiled,
};
use crate::shards::{Predicated, RepeatCompiled};

/// Immutable builtin control descriptions. Children are borrowed only during
/// dispatch; frames retain their compiled owner, never pointers into it.
#[doc(hidden)]
pub enum Control<'a> {
  Do(&'a Arc<InlineCall<Stackless>>),
  When(&'a Predicated<Stackless>),
  While(&'a Predicated<Stackless>),
  Sub(&'a CompiledFlow<Stackless>),
  Once(&'a CompiledFlow<Stackless>),
  Repeat(&'a RepeatCompiled<Stackless>),
  If(&'a IfCompiled<Stackless>),
  Match(&'a MatchCompiled<Stackless>),
  Maybe(&'a MaybeCompiled<Stackless>),
  Conditions(&'a ConditionsCompiled<Stackless>),
}

impl Control<'_> {
  fn len(&self) -> usize {
    match self {
      Self::Do(_) | Self::Sub(_) | Self::Once(_) => 1,
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
  Root(Arc<CompiledWire<Stackless>>),
  Child(Arc<dyn CompiledNode>, usize),
  Call(Arc<InlineCall<Stackless>>),
}
impl Code {
  fn flow(&self) -> &CompiledFlow<Stackless> {
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
        Control::Do(_) => unreachable!("Do retains its selected call"),
      },
      Self::Call(c) => &c.flow,
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
  call: Option<Arc<InlineCall<Stackless>>>,
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
    }
  }
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
  pub fn instantiate(wire: Arc<CompiledWire<Stackless>>, ctx: &mut InstanceCtx) -> Result<Self> {
    let mut engine = Self {
      frames: Arena::default(),
      root: None,
      current: None,
      dispatches: 0,
    };
    let root = engine.build(Code::Root(wire), ctx)?;
    engine.root = Some(root);
    Ok(engine)
  }

  /// Depth-first initialization, preserving source order without Rust recursion.
  fn build(&mut self, code: Code, ctx: &mut InstanceCtx) -> Result<Handle> {
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
            pending.push(match &control {
              Control::Do(call) => Code::Call((*call).clone()),
              _ => Code::Child(node.clone(), i),
            });
          }
        }
      }
    }
    drop(pending);
    self.frames.reserve(count);
    let root = self.frames.insert(Frame::new(code));
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
          if let Control::Do(call) = &control {
            c.call = Some((*call).clone());
          }
          for i in 0..control.len() {
            let code = match &control {
              Control::Do(call) => Code::Call((*call).clone()),
              _ => Code::Child(node.clone(), i),
            };
            c.children.push(self.frames.insert(Frame::new(code)));
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

  fn prepare_call(&mut self, h: Handle, ctx: &ActivationCtx<'_>) -> Result<()> {
    let c = self.continuation(h);
    if c.phase != 0 {
      return Ok(());
    }
    let call = c.call.as_ref().expect("Do call");
    let mut old = Vec::new();
    if c.revision != ctx.reload_revision() {
      c.revision = ctx.reload_revision();
      if let Some(next) = ctx.inline_call(&call.key)
        && next.signature == call.signature
        && next.deps != call.deps
      {
        c.call = Some(next);
        old = std::mem::take(&mut c.children);
      }
    }
    for child in old {
      self.cleanup_tree(
        child,
        &mut CleanupCtx {
          instance: ctx.instance(),
        },
      );
    }
    let c = self.continuation(h);
    if c.children.is_empty() {
      let code = Code::Call(c.call.as_ref().expect("call").clone());
      let child = self.build(
        code,
        &mut InstanceCtx {
          instance: ctx.instance(),
        },
      )?;
      self.continuation(h).children.push(child);
    }
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
      // No registry lookup or compiled-handle cloning on the usual hot path.
      let prepared = if ctx.reload_revision() != 0 && completed.is_none() {
        let f = self.frames.get(h).expect("frame");
        if matches!(f.states.get(f.pc), Some(State::Control(c)) if c.call.is_some()) {
          self.prepare_call(h, ctx)
        } else {
          Ok(())
        }
      } else {
        Ok(())
      };
      let f = self.frames.get_mut(h).expect("live frame");
      let flow = f.code.flow();
      let index = f.pc;
      let result = if index == flow.nodes.len() {
        Request::Complete(Ok(Step::Next(std::mem::replace(&mut f.value, Var::None))))
      } else if let State::Control(c) = &mut f.states[index] {
        self.dispatches += 1;
        let control = flow.nodes[index].control().expect("composite");
        let request = prepared.and_then(|()| {
          if let Control::Do(call) = &control
            && completed.is_none()
            && f.call_depth >= ctx.max_call_depth
          {
            use crate::diagnostic::{Diagnostic, PathStep, Phase};
            let mut d = Diagnostic::new(
              Phase::Activate,
              "activation-error",
              "recursion-limit",
              format!(
                "{} exceeds max_call_depth {} at depth {}",
                call.name, ctx.max_call_depth, f.call_depth
              ),
            );
            d.path.push(PathStep::Wire(call.name.clone()));
            return Err(Error::Diagnostic(Box::new(d.shard("Do"))));
          }
          dispatch(control, c, ctx, &f.value, completed.take())
        });
        match request {
          Ok(Request::Enter(child)) => Request::Enter(child),
          Ok(Request::Complete(result)) => {
            c.reset();
            Request::Complete(result)
          }
          Err(err) => {
            c.reset();
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
              continue;
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
      match result {
        Request::Enter(child) => {
          let input = f.value.clone();
          let depth = f.call_depth
            + usize::from(matches!(&f.states[index], State::Control(c) if c.call.is_some()));
          let child_frame = self.frames.get_mut(child).expect("live child");
          debug_assert!(child_frame.parent.is_none());
          child_frame.parent = Some(h);
          child_frame.call_depth = depth;
          child_frame.value = input;
          self.current = Some(child);
        }
        Request::Complete(Ok(Step::Suspend)) => return Ok(Step::Suspend),
        Request::Complete(Ok(Step::Next(value))) if index < flow.nodes.len() => {
          f.pc += 1;
          f.value = value;
        }
        Request::Complete(result) => {
          f.pc = 0;
          f.value = Var::None;
          self.current = f.parent.take();
          if self.current.is_none() {
            return result;
          }
          completed = Some(result);
        }
      }
    }
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
        Step::Return(v) if matches!(control, Control::Do(_)) => return next(v),
        other => return Ok(Request::Complete(Ok(other))),
      }
    }
  };
  match control {
    Control::Do(_) | Control::Match(_) if value.is_some() => next(value.unwrap()),
    Control::Do(_) => enter(c, 0, 1),
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
