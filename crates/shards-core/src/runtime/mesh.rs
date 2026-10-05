//! The stackful scheduler: one `corosensei` coroutine per instance. Not
//! available on wasm, which has no native stack switching.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::mem::size_of;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::Arc;
use std::task::Waker;

use corosensei::stack::DefaultStack;
use corosensei::{Coroutine, CoroutineResult, Yielder};

use super::{ActivationCtx, MeshShared};
use crate::compose::{CacheStats, CompiledWire, ComposeCache, ComposeEnv, FrameLayout, WireDef};
use crate::error::{Error, Result, panic_message};
use crate::instance::{InstanceCtx, InstanceId, InstanceMemory, Outcome, WakeFlag, WakeMode};
use crate::shard::{Flow, Stackful};
use crate::types::Type;
use crate::var::Var;

/// Coroutine stack size per instance. Release builds match 1.x Release
/// (`SH_BASE_STACK_SIZE`). Debug builds use far larger frames, so they get
/// 1 MB: enough for flows nested up to `compose::MAX_FLOW_DEPTH` through any
/// control shard (the stack is reserved, and committed only as it is used).
#[cfg(not(debug_assertions))]
pub const DEFAULT_STACK_SIZE: usize = 128 * 1024;
#[cfg(debug_assertions)]
pub const DEFAULT_STACK_SIZE: usize = 1024 * 1024;

type InstanceCoroutine = Coroutine<(), (), Outcome, DefaultStack>;

/// Per-instance signals shared between the mesh and the instance's coroutine.
pub(crate) struct Signals {
  pub(crate) cancel: Cell<bool>,
  /// Set by `ActivationCtx::wait`: suspended until the waker fires (in
  /// `WakeMode::OnNotify`), not just until the next tick.
  pub(crate) waiting: Cell<bool>,
  pub(crate) memory: Cell<InstanceMemory>,
  pub(crate) wake: Arc<WakeFlag>,
  pub(crate) waker: Waker,
}

struct Instance {
  id: InstanceId,
  /// Immutable root code, retained for reload compatibility and reports.
  wire: Arc<CompiledWire<Stackful>>,
  coroutine: Option<InstanceCoroutine>,
  signals: Rc<Signals>,
  outcome: Option<Outcome>,
}

pub struct Mesh {
  shared: Rc<RefCell<MeshShared>>,
  wires: HashMap<String, WireDef>,
  cache: ComposeCache<Stackful>,
  prepared_calls: crate::reload::InlineRegistry<Stackful>,
  instances: Vec<Instance>,
  next_id: InstanceId,
  stack_size: usize,
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

  pub fn with_cache(cache: ComposeCache<Stackful>) -> Mesh {
    Mesh {
      shared: Rc::new(RefCell::new(MeshShared {
        inline_calls: Default::default(),
        layout: FrameLayout::default(),
        frame: Vec::new(),
        spawn_queue: Vec::new(),
      })),
      wires: HashMap::new(),
      cache,
      prepared_calls: HashMap::new(),
      instances: Vec::new(),
      next_id: 0,
      stack_size: DEFAULT_STACK_SIZE,
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
    let mut shared = self.shared.borrow_mut();
    assert!(
      shared.layout.lookup(name).is_none(),
      "mesh variable {name} already declared"
    );
    shared.layout.declare(name, value.type_of(), mutable);
    shared.frame.push(value);
  }

  pub fn get_var(&self, name: &str) -> Option<Var> {
    let shared = self.shared.borrow();
    shared
      .layout
      .lookup(name)
      .map(|slot| shared.frame[slot.index].clone())
  }

  pub fn set_var(&mut self, name: &str, value: Var) {
    let mut shared = self.shared.borrow_mut();
    let slot = shared.layout.lookup(name).expect("unknown mesh variable");
    assert!(
      slot.ty.admits(&value),
      "mesh variable {name} is {}; the value does not fit",
      slot.ty
    );
    shared.frame[slot.index] = value;
  }

  /// Compiles a wire for the given input type, through the compose cache.
  pub fn compile(&mut self, name: &str, input: Type) -> Result<Arc<CompiledWire<Stackful>>> {
    let def = self
      .wires
      .get(name)
      .cloned()
      .ok_or_else(|| Error::Compose(format!("unknown wire: {name}")))?;
    let shared = self.shared.borrow();
    let env = ComposeEnv {
      mesh_layout: &shared.layout,
      wires: &self.wires,
    };
    let wire = self
      .cache
      .get_or_compose(&def, input, &env, &mut Vec::new(), 0)?;
    self.prepared_calls.extend(
      wire
        .inline_calls
        .iter()
        .map(|(k, v)| (k.clone(), v.clone())),
    );
    Ok(wire)
  }

  /// A fresh compilation candidate with the same mesh-variable layout and
  /// values. No running instance or compiled cache is copied.
  pub fn revision(&self) -> Self {
    let mut next = Self::new();
    let shared = self.shared.borrow();
    next.shared.borrow_mut().layout = shared.layout.clone();
    next.shared.borrow_mut().frame = shared.frame.clone();
    next.wake_mode = self.wake_mode;
    next.stack_size = self.stack_size;
    next
  }

  /// Whether this candidate can keep an existing instance's root code.
  pub fn can_retain(&self, wire: &CompiledWire<Stackful>) -> bool {
    let shared = self.shared.borrow();
    crate::reload::reusable(
      wire,
      &ComposeEnv {
        mesh_layout: &shared.layout,
        wires: &self.wires,
      },
    )
  }

  /// Checks every live caller before any cancellation or definition change.
  pub fn validate_reload(&self, next: &mut Self, removed: &HashSet<InstanceId>) -> Result<()> {
    // Detached children may have input specializations not reached by the
    // new entry graph. Prepare those too before selecting their next Do call.
    for instance in &self.instances {
      if !removed.contains(&instance.id)
        && instance.outcome.is_none()
        && next.can_retain(&instance.wire)
      {
        next.compile(&instance.wire.name, instance.wire.input)?;
      }
    }
    let shared = self.shared.borrow();
    let new_shared = next.shared.borrow();
    let env = ComposeEnv {
      mesh_layout: &new_shared.layout,
      wires: &next.wires,
    };
    if shared.layout != new_shared.layout {
      return Err(Error::Compose(
        "preserving reload requires the same mesh-variable layout".into(),
      ));
    }
    for instance in &self.instances {
      if !removed.contains(&instance.id) && instance.outcome.is_none() {
        crate::reload::validate(
          &instance.wire,
          &env,
          &shared.inline_calls.calls,
          &next.prepared_calls,
        )?;
      }
    }
    Ok(())
  }

  /// Installs a validated candidate. Retains compatible instances, cancels
  /// changed roots and explicitly removed entries, and preserves mesh values.
  /// The candidate must have no instances and must come from `revision`.
  pub fn install_revision(&mut self, mut next: Self, removed: &HashSet<InstanceId>) {
    self
      .validate_reload(&mut next, removed)
      .expect("revision validated before commit");
    assert!(next.instances.is_empty(), "revision already has instances");
    for instance in &mut self.instances {
      if removed.contains(&instance.id) || !next.can_retain(&instance.wire) {
        cancel(instance);
      }
    }
    let mut calls = std::mem::take(&mut next.prepared_calls);
    crate::reload::reuse_unchanged(&self.prepared_calls, &mut calls);
    {
      let mut shared = self.shared.borrow_mut();
      crate::reload::reuse_unchanged(&shared.inline_calls.calls, &mut calls);
      self.prepared_calls = calls.clone();
      shared.inline_calls.install(calls);
    }
    self.wires = std::mem::take(&mut next.wires);
    self.cache = std::mem::take(&mut next.cache);
  }

  /// How many times Do calls consulted the reload registry. A call site
  /// checks it once per accepted preserving reload, not on every call.
  pub fn reload_lookups(&self) -> u64 {
    self.shared.borrow().inline_calls.lookups.get()
  }

  pub fn cache_stats(&self) -> CacheStats {
    self.cache.stats
  }

  /// Schedules an instance of a compiled wire. The wire may have been
  /// compiled by another mesh, as long as its recorded dependencies hold
  /// here (e.g. the same mesh variable layout).
  pub fn spawn(&mut self, wire: &Arc<CompiledWire<Stackful>>, input: Var) -> Result<InstanceId> {
    {
      let shared = self.shared.borrow();
      let env = ComposeEnv {
        mesh_layout: &shared.layout,
        wires: &self.wires,
      };
      if !wire.deps_valid(&env) {
        return Err(Error::Compose(format!(
          "wire {} was compiled for a different mesh layout",
          wire.name
        )));
      }
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

  fn start(&mut self, wire: Arc<CompiledWire<Stackful>>, input: Var) -> InstanceId {
    let id = self.next_id;
    self.next_id += 1;
    let wake = Arc::new(WakeFlag::default());
    let signals = Rc::new(Signals {
      cancel: Cell::new(false),
      waiting: Cell::new(false),
      memory: Cell::new(InstanceMemory::default()),
      waker: Waker::from(wake.clone()),
      wake,
    });
    let instance_wire = wire.clone();
    let stack = DefaultStack::new(self.stack_size).expect("failed to allocate coroutine stack");
    let coroutine = {
      let shared = self.shared.clone();
      let signals = signals.clone();
      let stack_reserved = self.stack_size;
      Coroutine::with_stack(stack, move |yielder: &Yielder<(), ()>, ()| {
        run_instance(id, &wire, input, &shared, yielder, &signals, stack_reserved)
      })
    };
    self.instances.push(Instance {
      id,
      wire: instance_wire,
      coroutine: Some(coroutine),
      signals,
      outcome: None,
    });
    id
  }

  /// Runs one tick: resumes every running instance once, in schedule order,
  /// then starts instances spawned during the tick. Returns the number of
  /// instances still running.
  pub fn tick(&mut self) -> usize {
    let notify_only = self.wake_mode == WakeMode::OnNotify;
    for instance in &mut self.instances {
      let signals = &instance.signals;
      // Clear the wake flag before resuming, so a wake that happens during
      // or after this activation is seen on the next tick.
      let woken = signals.wake.take();
      if notify_only && signals.waiting.get() && !woken {
        continue;
      }
      signals.waiting.set(false);
      resume(instance);
    }
    let spawned = std::mem::take(&mut self.shared.borrow_mut().spawn_queue);
    for (wire, input) in spawned {
      self.start(wire, input);
    }
    self.running()
  }

  /// Cancels a running instance: its suspended execution unwinds, then its
  /// state is cleaned up exactly once. Further ticks never resume it.
  pub fn cancel(&mut self, id: InstanceId) {
    if let Some(instance) = self.instances.iter_mut().find(|i| i.id == id) {
      cancel(instance);
    }
  }

  /// Cancels all running instances, including spawned children, in one pass.
  /// Outcomes (including cleanup failures) remain available to `take_finished`.
  pub fn cancel_all(&mut self) {
    for instance in &mut self.instances {
      cancel(instance);
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
    self
      .instances
      .iter()
      .find(|i| i.id == id)
      .map(|i| i.signals.memory.get())
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

  /// Ticks until no instance is running, or `max_ticks` is reached.
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
    // Cancel what is still running, so every instance is cleaned up exactly
    // once even when the mesh goes away first.
    self.cancel_all();
  }
}

fn resume(instance: &mut Instance) {
  let Some(coroutine) = instance.coroutine.as_mut() else {
    return;
  };
  // Panics are contained inside the instance (see `run_instance`); this is a
  // last line of defense so a finished coroutine is never resumed again.
  match catch_unwind(AssertUnwindSafe(|| coroutine.resume(()))) {
    Ok(CoroutineResult::Yield(())) => {}
    Ok(CoroutineResult::Return(outcome)) => {
      instance.outcome = Some(outcome);
      instance.coroutine = None;
    }
    Err(payload) => {
      instance.outcome = Some(Outcome::Failed(Error::Activation(format!(
        "panic escaped instance: {}",
        panic_message(&*payload)
      ))));
      instance.coroutine = None;
    }
  }
}

fn cancel(instance: &mut Instance) {
  if instance.coroutine.is_none() {
    return;
  }
  instance.signals.cancel.set(true);
  resume(instance);
  assert!(
    instance.coroutine.is_none(),
    "cancelled instance did not finish"
  );
}

fn run_instance(
  id: InstanceId,
  wire: &CompiledWire<Stackful>,
  input: Var,
  shared: &RefCell<MeshShared>,
  yielder: &Yielder<(), ()>,
  signals: &Signals,
  stack_reserved: usize,
) -> Outcome {
  if signals.cancel.get() {
    return Outcome::Cancelled;
  }
  let mut locals = vec![Var::None; wire.locals.len()];
  let mut instance_ctx = InstanceCtx { instance: id };
  // Flow instantiation already turns panics into errors; this is a last line
  // of defense so a panic never escapes the instance.
  let instantiated = catch_unwind(AssertUnwindSafe(|| {
    wire.flow.instantiate(&mut instance_ctx)
  }));
  let mut state = match instantiated {
    Ok(Ok(state)) => state,
    Ok(Err(err)) => return Outcome::Failed(err),
    Err(payload) => {
      return Outcome::Failed(Error::Activation(format!(
        "panic in instantiate: {}",
        panic_message(&*payload)
      )));
    }
  };
  signals.memory.set(InstanceMemory {
    state_bytes: wire.flow.state_size(&state)
      + locals.len() * size_of::<Var>()
      + size_of::<Vec<Var>>(),
    stack_reserved,
  });

  // The instance is the panic boundary: a panic in activation fails this
  // instance only, and cleanup still runs exactly once (contract §5).
  let mut outcome = catch_unwind(AssertUnwindSafe(|| {
    let suspend = || yielder.suspend(());
    let mut ctx = ActivationCtx {
      instance: id,
      locals: &mut locals,
      mesh: shared,
      yielder: &suspend,
      cancel: &signals.cancel,
      waiting: &signals.waiting,
      waker: &signals.waker,
      iteration: 0,
    };
    run_wire(wire, &mut state, &mut ctx, &input)
  }))
  .unwrap_or_else(|payload| {
    Outcome::Failed(Error::Activation(format!(
      "panic in activation: {}",
      panic_message(&*payload)
    )))
  });

  let cleanup = catch_unwind(AssertUnwindSafe(|| {
    wire
      .flow
      .cleanup(&mut state, &mut instance_ctx.cleanup_ctx())
  }));
  if let Err(payload) = cleanup {
    outcome = Outcome::Failed(Error::Activation(format!(
      "panic in cleanup: {}",
      panic_message(&*payload)
    )));
  }
  outcome
}

fn run_wire(
  wire: &CompiledWire<Stackful>,
  state: &mut crate::flow::FlowState,
  ctx: &mut ActivationCtx<'_>,
  input: &Var,
) -> Outcome {
  loop {
    match wire.flow.activate(state, ctx, input) {
      Ok(Flow::Next(value)) if !wire.looped => return Outcome::Completed(value),
      Ok(Flow::Return(value)) => return Outcome::Completed(value),
      Ok(Flow::Stop) => return Outcome::Stopped,
      Ok(Flow::Next(_)) | Ok(Flow::Restart) => ctx.iteration += 1,
      Err(Error::Cancelled) => return Outcome::Cancelled,
      Err(err) => return Outcome::Failed(err),
    }
    // A loop iteration or Restart keeps the same instance and state; it
    // yields to the scheduler between iterations.
    match ctx.suspend() {
      Ok(()) => {}
      Err(Error::Cancelled) => return Outcome::Cancelled,
      Err(err) => return Outcome::Failed(err),
    }
  }
}
