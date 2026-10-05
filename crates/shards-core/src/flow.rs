//! A compiled flow (a sequence of compiled nodes) and its per-instance state.
//! Wires, and shards with sub-flows (`Once`, `When`, `Do`...), use these.

use std::any::Any;
use std::sync::Arc;

use crate::compose::Backend;
use crate::error::Result;
use crate::lifecycle::{cleanup_each, instantiate_all};
use crate::runtime::{ActivationCtx, CleanupCtx, InstanceCtx};
use crate::shard::{Flow, Stackful};
use crate::types::Type;
use crate::var::Var;

/// A compiled flow: a sequence of compiled nodes of one scheduler's kind.
pub struct CompiledFlow<B: Backend> {
  pub(crate) nodes: Vec<Arc<B::Node>>,
  pub output: Type,
}

/// Per-instance state of a [`CompiledFlow`]: one state per node.
pub struct FlowState {
  states: Vec<Box<dyn Any>>,
  cleaned: bool,
}

impl CompiledFlow<Stackful> {
  /// Instantiates every node in order. If one fails, the already
  /// instantiated nodes are cleaned up (in reverse) before returning the
  /// error, so a failed instantiation releases everything it acquired.
  pub fn instantiate(&self, ctx: &mut InstanceCtx) -> Result<FlowState> {
    let mut cleanup_ctx = ctx.cleanup_ctx();
    let done = instantiate_all(
      self.nodes.iter(),
      |node| node.instantiate(ctx),
      |node, state| node.cleanup(state.as_mut(), &mut cleanup_ctx),
    )?;
    Ok(FlowState {
      states: done.into_iter().map(|(_, state)| state).collect(),
      cleaned: false,
    })
  }

  pub fn activate(
    &self,
    state: &mut FlowState,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Flow> {
    let mut value = input.clone();
    for (node, node_state) in self.nodes.iter().zip(state.states.iter_mut()) {
      match node.activate(node_state.as_mut(), ctx, &value)? {
        Flow::Next(output) => value = output,
        other => return Ok(other),
      }
    }
    Ok(Flow::Next(value))
  }

  /// Cleans up every node's state, in reverse order. The runtime calls this
  /// exactly once per successfully instantiated flow state.
  pub fn cleanup(&self, state: &mut FlowState, ctx: &mut CleanupCtx) {
    assert!(!state.cleaned, "flow state cleaned up twice");
    state.cleaned = true;
    cleanup_each(
      self.nodes.iter().zip(state.states.iter_mut()).rev(),
      |(node, node_state)| node.cleanup(node_state.as_mut(), ctx),
    );
  }

  /// Inline state size of this flow's nodes (boxes included), for measurements.
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
    // Drop releases memory only; logical cleanup must already have run.
    if !std::thread::panicking() {
      debug_assert!(
        self.cleaned || self.states.is_empty(),
        "flow state dropped without cleanup"
      );
    }
  }
}
