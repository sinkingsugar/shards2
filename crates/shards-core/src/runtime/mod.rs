//! The runtime: a mesh schedules instances of compiled wires (contract §5-§8).
//!
//! Prototype scheduler: single-threaded, stackful (one `corosensei` coroutine
//! per instance). Each instance owns its local frame and its flow state; the
//! mesh owns the mesh frame. Mutable access to the mesh frame is borrowed only
//! for the duration of one read or write, never across a suspension.

use std::cell::{Cell, RefCell};
use std::sync::Arc;
use std::task::Waker;

use crate::compose::{Binding, CompiledWire, FrameLayout};
use crate::error::{Error, Result};
use crate::shard::Stackful;
use crate::var::Var;

pub use crate::instance::{
  CleanupCtx, Frames, InstanceCtx, InstanceId, InstanceMemory, LeafCtx, Outcome, WakeMode,
};

#[cfg(stackful)]
mod mesh;
#[cfg(stackful)]
pub use mesh::{DEFAULT_STACK_SIZE, Mesh};

// Only the stackful mesh, which wasm and ESP-IDF builds lack, reads every field.
#[cfg_attr(not(stackful), allow(dead_code))]
pub(crate) struct MeshShared {
  pub(crate) inline_calls: crate::reload::InlineRegistry<Stackful>,
  pub(crate) layout: FrameLayout,
  pub(crate) frame: Vec<Var>,
  pub(crate) spawn_queue: Vec<(Arc<CompiledWire<Stackful>>, Var)>,
}

/// Context for `activate`: short-lived access to the frames and to scheduler
/// operations (contract §6).
pub struct ActivationCtx<'a> {
  pub(crate) instance: InstanceId,
  pub(crate) locals: &'a mut Vec<Var>,
  pub(crate) mesh: &'a RefCell<MeshShared>,
  /// Yields to the scheduler; resumes on a later tick.
  pub(crate) yielder: &'a dyn Fn(),
  pub(crate) cancel: &'a Cell<bool>,
  pub(crate) waiting: &'a Cell<bool>,
  pub(crate) waker: &'a Waker,
  /// The loop iteration ([`LeafCtx::iteration`]).
  pub(crate) iteration: u64,
}

impl ActivationCtx<'_> {
  pub(crate) fn inline_call(
    &self,
    key: &crate::reload::InlineKey,
  ) -> Option<Arc<crate::reload::InlineCall<Stackful>>> {
    self.mesh.borrow().inline_calls.get(key).cloned()
  }

  pub fn instance(&self) -> InstanceId {
    self.instance
  }

  pub fn get(&self, binding: Binding) -> Var {
    match binding {
      Binding::Local(i) => self.locals[i].clone(),
      Binding::Mesh(i) => self.mesh.borrow().frame[i].clone(),
    }
  }

  pub fn set(&mut self, binding: Binding, value: Var) {
    match binding {
      Binding::Local(i) => self.locals[i] = value,
      Binding::Mesh(i) => self.mesh.borrow_mut().frame[i] = value,
    }
  }

  /// Suspends until the next tick. Returns `Err(Cancelled)` if the instance
  /// was cancelled; shards propagate it so the nested execution unwinds.
  pub fn suspend(&mut self) -> Result<()> {
    if self.cancel.get() {
      return Err(Error::Cancelled);
    }
    (self.yielder)();
    if self.cancel.get() {
      return Err(Error::Cancelled);
    }
    Ok(())
  }

  /// Suspends until the instance's waker fires (in `WakeMode::OnNotify`; in
  /// `WakeMode::PollEveryTick`, until the next tick). For shards waiting on
  /// an async operation: register [`ActivationCtx::waker`] with it first.
  pub fn wait(&mut self) -> Result<()> {
    self.waiting.set(true);
    self.suspend()
  }

  /// The instance's waker. Waking it after the instance finished is harmless.
  pub fn waker(&self) -> &Waker {
    self.waker
  }

  /// Schedules a new instance of `wire`. It starts on the next tick.
  pub fn spawn(&mut self, wire: Arc<CompiledWire<Stackful>>, input: Var) {
    self.mesh.borrow_mut().spawn_queue.push((wire, input));
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

impl LeafCtx for ActivationCtx<'_> {
  fn instance(&self) -> InstanceId {
    self.instance
  }

  fn iteration(&self) -> u64 {
    self.iteration
  }
}
