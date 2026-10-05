//! The stackless scheduler: no coroutines. Each tick activates every running
//! instance's root flow once; a `Suspend` means "activate again next tick",
//! and the instance's state holds the resume points. Same API and contract as
//! the stackful [`crate::Mesh`].

use std::collections::HashMap;
use std::mem::size_of;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::task::Waker;

use super::{ActivationCtx, FlowState, Stackless, Step};
use crate::compose::{CacheStats, CompiledWire, ComposeCache, ComposeEnv, FrameLayout, WireDef};
use crate::error::{Error, Result, panic_message};
use crate::instance::{InstanceCtx, InstanceId, InstanceMemory, Outcome, WakeFlag, WakeMode};
use crate::types::Type;
use crate::var::Var;

struct Instance {
  id: InstanceId,
  wire: Arc<CompiledWire<Stackless>>,
  input: Var,
  locals: Vec<Var>,
  /// `None` until instantiated (on the instance's first tick), and after
  /// cleanup.
  state: Option<FlowState>,
  started: bool,
  memory: InstanceMemory,
  outcome: Option<Outcome>,
  /// Waiting on its waker (set by `ActivationCtx::set_waiting`).
  waiting: bool,
  wake: Arc<WakeFlag>,
  waker: Waker,
  /// The loop iteration ([`crate::instance::LeafCtx::iteration`]).
  iteration: u64,
}

pub struct Mesh {
  layout: FrameLayout,
  frame: Vec<Var>,
  spawn_queue: Vec<(Arc<CompiledWire<Stackless>>, Var)>,
  wires: HashMap<String, WireDef>,
  cache: ComposeCache<Stackless>,
  instances: Vec<Instance>,
  next_id: InstanceId,
  wake_mode: WakeMode,
}

impl Default for Mesh {
  fn default() -> Mesh {
    Mesh::new()
  }
}

impl Mesh {
  pub fn new() -> Mesh {
    Mesh::with_cache(ComposeCache::default())
  }

  pub fn with_cache(cache: ComposeCache<Stackless>) -> Mesh {
    Mesh {
      layout: FrameLayout::default(),
      frame: Vec::new(),
      spawn_queue: Vec::new(),
      wires: HashMap::new(),
      cache,
      instances: Vec::new(),
      next_id: 0,
      wake_mode: WakeMode::default(),
    }
  }

  pub fn set_wake_mode(&mut self, mode: WakeMode) {
    self.wake_mode = mode;
  }

  pub fn add_wire(&mut self, def: WireDef) {
    self.wires.insert(def.name.clone(), def);
  }

  /// Declares a mesh variable, typed by its initial value.
  pub fn declare_var(&mut self, name: &str, value: Var, mutable: bool) {
    assert!(
      self.layout.lookup(name).is_none(),
      "mesh variable {name} already declared"
    );
    self.layout.declare(name, value.type_of(), mutable);
    self.frame.push(value);
  }

  pub fn get_var(&self, name: &str) -> Option<Var> {
    self
      .layout
      .lookup(name)
      .map(|slot| self.frame[slot.index].clone())
  }

  pub fn set_var(&mut self, name: &str, value: Var) {
    let slot = self.layout.lookup(name).expect("unknown mesh variable");
    assert!(
      slot.ty.admits(&value),
      "mesh variable {name} is {}; the value does not fit",
      slot.ty
    );
    self.frame[slot.index] = value;
  }

  pub fn compile(&mut self, name: &str, input: Type) -> Result<Arc<CompiledWire<Stackless>>> {
    let def = self
      .wires
      .get(name)
      .cloned()
      .ok_or_else(|| Error::Compose(format!("unknown wire: {name}")))?;
    let env = ComposeEnv {
      mesh_layout: &self.layout,
      wires: &self.wires,
    };
    self
      .cache
      .get_or_compose(&def, input, &env, &mut Vec::new(), 0)
  }

  pub fn cache_stats(&self) -> CacheStats {
    self.cache.stats
  }

  pub fn spawn(&mut self, wire: &Arc<CompiledWire<Stackless>>, input: Var) -> Result<InstanceId> {
    let env = ComposeEnv {
      mesh_layout: &self.layout,
      wires: &self.wires,
    };
    if !wire.deps_valid(&env) {
      return Err(Error::Compose(format!(
        "wire {} was compiled for a different mesh layout",
        wire.name
      )));
    }
    // Checked on the value: a union or open-table input accepts any value it
    // admits, and nothing is interned.
    if !wire.input.admits(&input) {
      return Err(Error::Compose(format!(
        "wire {} expects {} input, got {}",
        wire.name,
        wire.input,
        input.type_of()
      )));
    }
    Ok(self.start(wire.clone(), input))
  }

  fn start(&mut self, wire: Arc<CompiledWire<Stackless>>, input: Var) -> InstanceId {
    let id = self.next_id;
    self.next_id += 1;
    let wake = Arc::new(WakeFlag::default());
    self.instances.push(Instance {
      waiting: false,
      waker: Waker::from(wake.clone()),
      wake,
      iteration: 0,
      id,
      locals: vec![Var::None; wire.locals.len()],
      wire,
      input,
      state: None,
      started: false,
      memory: InstanceMemory::default(),
      outcome: None,
    });
    id
  }

  /// Runs one tick: activates every running instance once, in schedule order,
  /// then starts instances spawned during the tick. Returns the number of
  /// instances still running.
  pub fn tick(&mut self) -> usize {
    let notify_only = self.wake_mode == WakeMode::OnNotify;
    let Mesh {
      instances,
      frame,
      spawn_queue,
      ..
    } = self;
    for instance in instances.iter_mut() {
      if instance.outcome.is_some() {
        continue;
      }
      // Same rule as the stackful scheduler.
      let woken = instance.wake.take();
      if notify_only && instance.waiting && !woken {
        continue;
      }
      instance.waiting = false;
      step(instance, frame, spawn_queue);
    }
    for (wire, input) in std::mem::take(&mut self.spawn_queue) {
      self.start(wire, input);
    }
    self.running()
  }

  /// Cancels a running instance: its state is cleaned up exactly once, and it
  /// is never activated again. Nothing needs unwinding: a suspended stackless
  /// instance is only data.
  pub fn cancel(&mut self, id: InstanceId) {
    if let Some(instance) = self
      .instances
      .iter_mut()
      .find(|i| i.id == id && i.outcome.is_none())
    {
      finish(instance, Outcome::Cancelled);
    }
  }

  /// Returns the outcome of a finished instance and retires its record. A
  /// long-running mesh should take outcomes it no longer needs, so records
  /// of finished instances do not accumulate. Returns `None` for an unknown
  /// or still running instance.
  pub fn take_outcome(&mut self, id: InstanceId) -> Option<Outcome> {
    let index = self
      .instances
      .iter()
      .position(|i| i.id == id && i.outcome.is_some())?;
    self.instances.remove(index).outcome
  }

  pub fn outcome(&self, id: InstanceId) -> Option<&Outcome> {
    self
      .instances
      .iter()
      .find(|i| i.id == id)
      .and_then(|i| i.outcome.as_ref())
  }

  pub fn memory(&self, id: InstanceId) -> Option<InstanceMemory> {
    self.instances.iter().find(|i| i.id == id).map(|i| i.memory)
  }

  /// Removes the records of every finished instance in one pass, returning
  /// each one's id, wire name and outcome. The cost is linear in the
  /// records; a host that needs some outcomes later keeps them itself.
  pub fn take_finished(&mut self) -> Vec<(InstanceId, String, Outcome)> {
    let mut finished = Vec::new();
    self.instances.retain_mut(|i| {
      let Some(outcome) = i.outcome.take() else {
        return true;
      };
      finished.push((i.id, i.wire.name.clone(), outcome));
      false
    });
    finished
  }

  /// The name of the wire an instance runs, while its record exists.
  pub fn instance_wire(&self, id: InstanceId) -> Option<&str> {
    self
      .instances
      .iter()
      .find(|i| i.id == id)
      .map(|i| i.wire.name.as_str())
  }

  pub fn instance_ids(&self) -> Vec<InstanceId> {
    self.instances.iter().map(|i| i.id).collect()
  }

  pub fn running(&self) -> usize {
    self
      .instances
      .iter()
      .filter(|i| i.outcome.is_none())
      .count()
  }

  pub fn run(&mut self, max_ticks: usize) -> usize {
    let mut ticks = 0;
    while ticks < max_ticks && self.tick() > 0 {
      ticks += 1;
    }
    ticks
  }
}

impl Drop for Mesh {
  fn drop(&mut self) {
    for instance in &mut self.instances {
      if instance.outcome.is_none() {
        finish(instance, Outcome::Cancelled);
      }
    }
  }
}

/// One activation of one instance.
fn step(
  instance: &mut Instance,
  frame: &mut Vec<Var>,
  spawn_queue: &mut Vec<(Arc<CompiledWire<Stackless>>, Var)>,
) {
  if !instance.started {
    instance.started = true;
    let mut ictx = InstanceCtx {
      instance: instance.id,
    };
    let wire = &instance.wire;
    // Flow instantiation already turns panics into errors; this is a last
    // line of defense so a panic never escapes `tick`.
    let instantiated = catch_unwind(AssertUnwindSafe(|| wire.flow.instantiate(&mut ictx)))
      .unwrap_or_else(|payload| {
        Err(Error::Activation(format!(
          "panic in instantiate: {}",
          panic_message(&*payload)
        )))
      });
    match instantiated {
      Ok(state) => {
        instance.memory = InstanceMemory {
          state_bytes: instance.wire.flow.state_size(&state)
            + instance.locals.len() * size_of::<Var>()
            + size_of::<Vec<Var>>(),
          stack_reserved: 0,
        };
        instance.state = Some(state);
      }
      Err(err) => {
        finish(instance, Outcome::Failed(err));
        return;
      }
    }
  }

  // The instance is the panic boundary, as in the stackful scheduler.
  let result = {
    let Instance {
      id,
      wire,
      input,
      locals,
      state,
      waiting,
      waker,
      iteration,
      ..
    } = instance;
    let state = state.as_mut().expect("running instance without state");
    catch_unwind(AssertUnwindSafe(|| {
      let mut ctx = ActivationCtx {
        instance: *id,
        locals,
        mesh_frame: frame,
        spawn_queue,
        waiting,
        waker,
        iteration: *iteration,
      };
      wire.flow.activate(state, &mut ctx, input)
    }))
  };

  let outcome = match result {
    Ok(Ok(Step::Suspend)) => return,
    Ok(Ok(Step::Next(value))) if !instance.wire.looped => Outcome::Completed(value),
    // A loop iteration or Restart keeps the instance and its state.
    Ok(Ok(Step::Next(_) | Step::Restart)) => {
      instance.iteration += 1;
      return;
    }
    Ok(Ok(Step::Return(value))) => Outcome::Completed(value),
    Ok(Ok(Step::Stop)) => Outcome::Stopped,
    Ok(Err(err)) => Outcome::Failed(err),
    Err(payload) => Outcome::Failed(Error::Activation(format!(
      "panic in activation: {}",
      panic_message(&*payload)
    ))),
  };
  finish(instance, outcome);
}

/// Ends an instance: cleans up its state exactly once (if it was
/// instantiated) and records the outcome.
fn finish(instance: &mut Instance, mut outcome: Outcome) {
  if let Some(mut state) = instance.state.take() {
    let mut ctx = InstanceCtx {
      instance: instance.id,
    }
    .cleanup_ctx();
    let wire = &instance.wire;
    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| wire.flow.cleanup(&mut state, &mut ctx)))
    {
      outcome = Outcome::Failed(Error::Activation(format!(
        "panic in cleanup: {}",
        panic_message(&*payload)
      )));
    }
  }
  // Release execution data on every terminal path: only the outcome stays
  // (until `take_outcome`).
  instance.locals = Vec::new();
  instance.input = Var::None;
  instance.outcome = Some(outcome);
}
