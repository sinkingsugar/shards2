//! Experimental stackless scheduler (design doc §3.4).
//!
//! Same compiled/state model, same compose (cache, dependency recording,
//! definite initialization) as the stackful reference. The difference is how
//! suspension works: there are no coroutines. An activation returns
//! [`Step::Suspend`], and whatever must survive until the next tick lives in
//! the instance's state:
//!
//! - a [`FlowState`] records which node suspended and that node's input, and
//!   resumes there;
//! - a shard that runs nested flows keeps its own resume point (its phase, a
//!   loop counter) in its `State`.
//!
//! On the next tick the scheduler activates the instance's root flow again, and
//! each level continues from its saved point. Cancelling is just cleanup: there
//! is no suspended stack to unwind.

pub mod runtime;
pub mod shards;

use std::any::Any;
use std::mem::size_of;
use std::sync::Arc;
use std::task::Waker;

use crate::args::Args;
use crate::compose::{Backend, Binding, CompiledWire, ComposeCtx};
use crate::error::Result;
use crate::flow::CompiledFlow;
use crate::instance::{CleanupCtx, Frames, InstanceCtx, InstanceId, LeafCtx};
use crate::lifecycle::{cleanup_each, instantiate_all};
use crate::shard::{Composed, ShardType};
use crate::var::Var;

pub use runtime::Mesh;

/// The stackless scheduler's backend.
pub struct Stackless;

impl Backend for Stackless {
  type Node = dyn CompiledNode;

  fn inline(node: &Self::Node) -> Option<crate::inline::InlineOp> {
    node.inline()
  }

  fn compose_shard(
    ty: &ShardType,
    args: &Args,
    ctx: &mut ComposeCtx<'_, Stackless>,
  ) -> Result<Composed<Arc<dyn CompiledNode>>> {
    match ty.stackless {
      Some(compose) => compose(args, ctx),
      None => crate::shard::missing_backend(ty, "stackless"),
    }
  }
}

/// What a stackless activation produced. Like the stackful `Flow`, plus
/// `Suspend`.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
  Next(Var),
  Stop,
  Restart,
  Return(Var),
  /// Not finished: activate again on a later tick, and the shard continues
  /// from the resume point it saved in its state. The input passed on resume
  /// is the original input.
  Suspend,
}

/// Context for a stackless `activate`. No scheduler operations are needed for
/// suspension: a shard returns [`Step::Suspend`].
pub struct ActivationCtx<'a> {
  pub(crate) instance: InstanceId,
  pub(crate) locals: &'a mut Vec<Var>,
  pub(crate) inline_calls: &'a crate::reload::Revisions<Stackless>,
  pub(crate) mesh_frame: &'a mut Vec<Var>,
  pub(crate) spawn_queue: &'a mut Vec<(Arc<CompiledWire<Stackless>>, Var)>,
  pub(crate) waiting: &'a mut bool,
  pub(crate) waker: &'a Waker,
  /// The loop iteration ([`LeafCtx::iteration`]).
  pub(crate) iteration: u64,
}

impl ActivationCtx<'_> {
  pub(crate) fn inline_call(
    &self,
    key: &crate::reload::InlineKey,
  ) -> Option<Arc<crate::reload::InlineCall<Stackless>>> {
    self.inline_calls.select(key)
  }

  pub(crate) fn reload_revision(&self) -> u64 {
    self.inline_calls.revision
  }

  pub fn instance(&self) -> InstanceId {
    self.instance
  }

  pub fn get(&self, binding: Binding) -> Var {
    match binding {
      Binding::Local(i) => self.locals[i].clone(),
      Binding::Mesh(i) => self.mesh_frame[i].clone(),
    }
  }

  pub fn set(&mut self, binding: Binding, value: Var) {
    match binding {
      Binding::Local(i) => self.locals[i] = value,
      Binding::Mesh(i) => self.mesh_frame[i] = value,
    }
  }

  /// Schedules a new instance of `wire`. It starts on the next tick.
  pub fn spawn(&mut self, wire: Arc<CompiledWire<Stackless>>, input: Var) {
    self.spawn_queue.push((wire, input));
  }

  /// Marks the instance as waiting on its waker. A shard calls this right
  /// before returning [`Step::Suspend`] for an async operation; in
  /// `WakeMode::OnNotify` the instance is then resumed only after the waker
  /// fires.
  pub fn set_waiting(&mut self) {
    *self.waiting = true;
  }

  /// The instance's waker. Waking it after the instance finished is harmless.
  pub fn waker(&self) -> &Waker {
    self.waker
  }
}

impl LeafCtx for ActivationCtx<'_> {
  fn instance(&self) -> InstanceId {
    self.instance
  }

  fn iteration(&self) -> u64 {
    self.iteration
  }
}

impl Frames for ActivationCtx<'_> {
  fn get(&self, binding: Binding) -> Var {
    ActivationCtx::get(self, binding)
  }

  fn set(&mut self, binding: Binding, value: Var) {
    ActivationCtx::set(self, binding, value)
  }
}

/// A stackless shard. Compose, instantiate and cleanup are the same as in the
/// stackful [`crate::Shard`]; activation differs.
pub trait Shard: 'static {
  type Compiled: Send + Sync + 'static;
  type State: 'static;

  const NAME: &'static str;
  const VERSION: u32 = 1;

  fn compose(args: &Args, ctx: &mut ComposeCtx<'_, Stackless>) -> Result<Composed<Self::Compiled>>;

  fn instantiate(compiled: &Self::Compiled, ctx: &mut InstanceCtx) -> Result<Self::State>;

  #[doc(hidden)]
  fn inline(_compiled: &Self::Compiled) -> Option<crate::inline::InlineOp> {
    None
  }

  /// Starts an activation, or continues one that returned [`Step::Suspend`].
  /// A shard that can suspend (directly or through a nested flow) must keep
  /// its resume point in `state`, and must reset it whenever it returns
  /// anything other than `Suspend`, including errors.
  fn activate(
    compiled: &Self::Compiled,
    state: &mut Self::State,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Step>;

  fn cleanup(_compiled: &Self::Compiled, _state: &mut Self::State, _ctx: &mut CleanupCtx) {}

  fn nested_state_size(_compiled: &Self::Compiled, _state: &Self::State) -> usize {
    0
  }
}

/// Type-erased stackless compiled node.
pub trait CompiledNode: Send + Sync {
  #[doc(hidden)]
  fn inline(&self) -> Option<crate::inline::InlineOp> {
    None
  }

  fn name(&self) -> &'static str;
  fn instantiate(&self, ctx: &mut InstanceCtx) -> Result<Box<dyn Any>>;
  fn activate(&self, state: &mut dyn Any, ctx: &mut ActivationCtx<'_>, input: &Var)
  -> Result<Step>;
  fn cleanup(&self, state: &mut dyn Any, ctx: &mut CleanupCtx);
  fn state_size(&self, state: &dyn Any) -> usize;
}

struct Node<S: Shard>(S::Compiled);

impl<S: Shard> CompiledNode for Node<S> {
  fn inline(&self) -> Option<crate::inline::InlineOp> {
    S::inline(&self.0)
  }

  fn name(&self) -> &'static str {
    S::NAME
  }

  fn instantiate(&self, ctx: &mut InstanceCtx) -> Result<Box<dyn Any>> {
    Ok(Box::new(S::instantiate(&self.0, ctx)?))
  }

  fn activate(
    &self,
    state: &mut dyn Any,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Step> {
    S::activate(&self.0, downcast::<S>(state), ctx, input)
  }

  fn cleanup(&self, state: &mut dyn Any, ctx: &mut CleanupCtx) {
    S::cleanup(&self.0, downcast::<S>(state), ctx)
  }

  fn state_size(&self, state: &dyn Any) -> usize {
    let state = state
      .downcast_ref::<S::State>()
      .expect("shard state type mismatch");
    size_of::<S::State>() + S::nested_state_size(&self.0, state)
  }
}

fn downcast<S: Shard>(state: &mut dyn Any) -> &mut S::State {
  state
    .downcast_mut::<S::State>()
    .expect("shard state type mismatch")
}

pub(crate) fn compose_erased<S: Shard>(
  args: &Args,
  ctx: &mut ComposeCtx<'_, Stackless>,
) -> Result<Composed<Arc<dyn CompiledNode>>> {
  let composed = S::compose(args, ctx)?;
  Ok(Composed {
    compiled: Arc::new(Node::<S>(composed.compiled)),
    output: composed.output,
  })
}

/// Per-instance state of a stackless [`CompiledFlow`]: one state per node,
/// plus the resume point if the flow is suspended.
pub struct FlowState {
  states: Vec<Box<dyn Any>>,
  /// The node that suspended, and the input it was activated with.
  resume: Option<(usize, Var)>,
  cleaned: bool,
}

impl FlowState {
  /// Whether the flow is suspended partway (it will resume, not restart).
  pub fn is_suspended(&self) -> bool {
    self.resume.is_some()
  }
}

impl CompiledFlow<Stackless> {
  /// Same contract as the stackful version: on failure (or panic), the nodes
  /// already instantiated are cleaned up before the error is returned.
  pub fn instantiate(&self, ctx: &mut InstanceCtx) -> Result<FlowState> {
    let mut cleanup_ctx = ctx.cleanup_ctx();
    let done = instantiate_all(
      self.nodes.iter(),
      |node| node.instantiate(ctx),
      |node, state| node.cleanup(state.as_mut(), &mut cleanup_ctx),
    )?;
    Ok(FlowState {
      states: done.into_iter().map(|(_, state)| state).collect(),
      resume: None,
      cleaned: false,
    })
  }

  /// Runs the flow, or continues it from its resume point if it suspended.
  /// On resume, `input` is ignored: the suspended node gets its saved input.
  pub fn activate(
    &self,
    state: &mut FlowState,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Step> {
    let (start, mut value) = match state.resume.take() {
      Some((index, value)) => (index, value),
      None => (0, input.clone()),
    };
    let mut index = start;
    while index < self.nodes.len() {
      if self.code[index].is_constructor() {
        (index, value) =
          crate::inline::construct(&self.code, index, value, ctx.locals, ctx.mesh_frame)?;
        continue;
      }
      if !matches!(self.code[index].op, crate::inline::Op::Fallback) {
        (index, value) = crate::inline::run(&self.code, index, &value, ctx.locals, ctx.mesh_frame)?;
        continue;
      }
      match self.nodes[index].activate(state.states[index].as_mut(), ctx, &value)? {
        Step::Next(output) => value = output,
        Step::Suspend => {
          state.resume = Some((index, value));
          return Ok(Step::Suspend);
        }
        other => return Ok(other),
      }
      index += 1;
    }
    Ok(Step::Next(value))
  }

  /// Same contract as the stackful version: every node's cleanup runs once,
  /// even if one panics; the first panic is re-raised afterwards.
  pub fn cleanup(&self, state: &mut FlowState, ctx: &mut CleanupCtx) {
    assert!(!state.cleaned, "flow state cleaned up twice");
    state.cleaned = true;
    state.resume = None;
    cleanup_each(
      self.nodes.iter().zip(state.states.iter_mut()).rev(),
      |(node, node_state)| node.cleanup(node_state.as_mut(), ctx),
    );
  }

  pub fn state_size(&self, state: &FlowState) -> usize {
    self
      .nodes
      .iter()
      .zip(state.states.iter())
      .map(|(node, s)| node.state_size(s.as_ref()) + size_of::<Box<dyn Any>>())
      .sum::<usize>()
      + size_of::<FlowState>()
  }
}

impl Drop for FlowState {
  fn drop(&mut self) {
    if !std::thread::panicking() {
      debug_assert!(
        self.cleaned || self.states.is_empty(),
        "flow state dropped without cleanup"
      );
    }
  }
}
