//! Structural M4 gate: a pending leaf is resumed directly at every depth.
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use shards_core::args::Args;
use shards_core::compose::ComposeCtx;
use shards_core::instance::LeafCtx;
use shards_core::shards::async_shard::{AsyncShard, async_type};
use shards_core::shards::defs::*;
use shards_core::{
  Composed, Error, Mesh, Outcome, Result, ShardDef, ShardDesc, ShardType, Type, Var, WakeMode,
  WireDef,
};

#[derive(Default)]
struct Gate {
  ready: AtomicBool,
  fail: AtomicBool,
  starts: AtomicUsize,
  polls: AtomicUsize,
  drops: AtomicUsize,
  wakes: Mutex<Vec<Waker>>,
}
thread_local! { static GATE: RefCell<Option<Arc<Gate>>> = const { RefCell::new(None) }; }
struct Wait;
struct Pending {
  gate: Arc<Gate>,
  registered: bool,
}
impl Future for Pending {
  type Output = Result<Var>;
  fn poll(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
    self.gate.polls.fetch_add(1, Ordering::Relaxed);
    if !self.registered {
      self.gate.wakes.lock().unwrap().push(ctx.waker().clone());
      self.registered = true;
    }
    if self.gate.ready.load(Ordering::Relaxed) {
      Poll::Ready(if self.gate.fail.load(Ordering::Relaxed) {
        Err(Error::Activation("controlled failure".into()))
      } else {
        Ok(Var::Int(42))
      })
    } else {
      Poll::Pending
    }
  }
}
impl Drop for Pending {
  fn drop(&mut self) {
    self.gate.drops.fetch_add(1, Ordering::Relaxed);
  }
}
impl AsyncShard for Wait {
  type Compiled = ();
  type Op = Pending;
  const DESC: ShardDesc = ShardDesc::undocumented("Test.Wait", 1);
  fn compose(_: &Args, _: &mut ComposeCtx<'_>) -> Result<Composed<()>> {
    Ok(Composed {
      compiled: (),
      output: Type::int(),
    })
  }
  fn start(_: &(), _: &mut impl LeafCtx, _: &Var) -> Result<Pending> {
    let gate = GATE.with(|g| g.borrow().as_ref().unwrap().clone());
    gate.starts.fetch_add(1, Ordering::Relaxed);
    Ok(Pending {
      gate,
      registered: false,
    })
  }
}
static WAIT: ShardType = async_type::<Wait>();
fn gate() -> Arc<Gate> {
  let gate = Arc::new(Gate::default());
  GATE.with(|g| *g.borrow_mut() = Some(gate.clone()));
  gate
}
fn nested(depth: usize) -> Vec<ShardDef> {
  let mut flow = vec![ShardDef::new(&WAIT, vec![])];
  for i in 0..depth {
    flow = vec![match i % 4 {
      0 => if_(
        vec![konst(Var::Bool(true))],
        flow,
        Some(vec![konst(Var::Int(0))]),
      ),
      1 => repeat(flow, val(Var::Int(1))),
      2 => maybe(flow, Some(vec![konst(Var::Int(-1))])),
      _ => sub(flow),
    }];
  }
  flow
}
fn wire(flow: Vec<ShardDef>, looped: bool) -> WireDef {
  WireDef {
    name: "root".into(),
    looped,
    flow,
  }
}

#[test]
fn pending_repolls_dispatch_zero_ancestors_at_every_depth() {
  for depth in [1, 4, 16, 32] {
    let gate = gate();
    let mut mesh = Mesh::new();
    mesh.add_wire(wire(nested(depth), false));
    let code = mesh.compile("root", Type::int()).unwrap();
    let id = mesh.spawn(&code, Var::Int(7)).unwrap();
    mesh.tick();
    let dispatches = mesh.composite_dispatches();
    assert!(dispatches >= depth as u64);
    let owners = Arc::strong_count(&gate);
    for _ in 0..100 {
      mesh.tick();
    }
    assert_eq!(mesh.composite_dispatches(), dispatches, "depth {depth}");
    assert_eq!(gate.starts.load(Ordering::Relaxed), 1);
    assert_eq!(gate.polls.load(Ordering::Relaxed), 101);
    assert_eq!(Arc::strong_count(&gate), owners);
    gate.ready.store(true, Ordering::Relaxed);
    mesh.tick();
    assert!(matches!(mesh.outcome(id), Some(Outcome::Completed(_))));
    assert_eq!(gate.drops.load(Ordering::Relaxed), 1);
    assert_eq!(Arc::strong_count(&gate), owners - 1);
  }
}

#[test]
fn errors_reach_the_nearest_handler_and_loop_continuations_reset() {
  let gate = gate();
  gate.fail.store(true, Ordering::Relaxed);
  let mut mesh = Mesh::new();
  mesh.declare_var("caught", Var::Int(0), true);
  mesh.add_wire(wire(
    vec![maybe(
      vec![repeat(vec![ShardDef::new(&WAIT, vec![])], val(Var::Int(2)))],
      Some(vec![inc("caught")]),
    )],
    true,
  ));
  let code = mesh.compile("root", Type::none()).unwrap();
  let id = mesh.spawn(&code, Var::None).unwrap();
  for expected in 1..=20 {
    gate.ready.store(false, Ordering::Relaxed);
    mesh.tick();
    let dispatches = mesh.composite_dispatches();
    mesh.tick();
    assert_eq!(mesh.composite_dispatches(), dispatches);
    gate.ready.store(true, Ordering::Relaxed);
    mesh.tick();
    assert_eq!(mesh.get_var("caught"), Some(Var::Int(expected)));
    assert_eq!(gate.starts.load(Ordering::Relaxed), expected as usize);
    assert_eq!(gate.drops.load(Ordering::Relaxed), expected as usize);
  }
  mesh.cancel(id);
}

#[test]
fn stale_wakes_do_not_resume_cancelled_or_replacement_instances() {
  let gate = gate();
  let mut mesh = Mesh::new();
  mesh.set_wake_mode(WakeMode::OnNotify);
  mesh.add_wire(wire(nested(16), false));
  let code = mesh.compile("root", Type::int()).unwrap();
  let old = mesh.spawn(&code, Var::Int(1)).unwrap();
  mesh.tick();
  let stale = gate.wakes.lock().unwrap()[0].clone();
  mesh.cancel(old);
  assert_eq!(mesh.take_outcome(old), Some(Outcome::Cancelled));
  let new = mesh.spawn(&code, Var::Int(2)).unwrap();
  mesh.tick();
  let polls = gate.polls.load(Ordering::Relaxed);
  stale.wake_by_ref();
  mesh.tick();
  assert_eq!(gate.polls.load(Ordering::Relaxed), polls);
  assert!(mesh.outcome(new).is_none());
  gate.ready.store(true, Ordering::Relaxed);
  gate.wakes.lock().unwrap()[1].wake_by_ref();
  mesh.tick();
  assert!(matches!(mesh.outcome(new), Some(Outcome::Completed(_))));
  assert_eq!(gate.drops.load(Ordering::Relaxed), 2);
}

#[test]
fn completing_nested_frames_dispatches_each_parent_once() {
  for depth in [1, 4, 16, 32] {
    let gate = gate();
    let mut mesh = Mesh::new();
    mesh.add_wire(wire(nested(depth), true));
    let code = mesh.compile("root", Type::int()).unwrap();
    let id = mesh.spawn(&code, Var::Int(7)).unwrap();
    mesh.tick();
    let before = mesh.composite_dispatches();
    gate.ready.store(true, Ordering::Relaxed);
    mesh.tick();
    assert_eq!(mesh.composite_dispatches() - before, depth as u64);
    mesh.cancel(id);
  }
}

#[cfg(not(target_os = "espidf"))]
mod allocations {
  use std::alloc::{GlobalAlloc, Layout, System};
  use std::cell::Cell;
  thread_local! { static LIVE: Cell<isize> = const { Cell::new(0) }; }
  struct Count;
  // SAFETY: forwards each allocation unchanged to System. The counter is
  // thread-local and allocation-free; test instances never migrate threads.
  unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
      let ptr = unsafe { System.alloc(layout) };
      if !ptr.is_null() {
        let _ = LIVE.try_with(|n| n.set(n.get() + layout.size() as isize));
      }
      ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
      let _ = LIVE.try_with(|n| n.set(n.get() - layout.size() as isize));
      unsafe {
        System.dealloc(ptr, layout);
      }
    }
  }
  #[global_allocator]
  static ALLOC: Count = Count;
  pub fn live() -> isize {
    LIVE.with(Cell::get)
  }
}

#[cfg(not(target_os = "espidf"))]
#[test]
fn repeated_suspend_complete_cycles_do_not_grow_live_allocations_or_owners() {
  let gate = gate();
  let mut mesh = Mesh::new();
  mesh.add_wire(wire(nested(32), true));
  let code = mesh.compile("root", Type::int()).unwrap();
  let id = mesh.spawn(&code, Var::Int(7)).unwrap();
  mesh.tick();
  let live = allocations::live();
  let owners = Arc::strong_count(&gate);
  for _ in 0..1000 {
    for _ in 0..3 {
      mesh.tick();
    }
    gate.ready.store(true, Ordering::Relaxed);
    mesh.tick();
    gate.wakes.lock().unwrap().clear();
    gate.ready.store(false, Ordering::Relaxed);
    mesh.tick();
    assert_eq!(allocations::live(), live);
    assert_eq!(Arc::strong_count(&gate), owners);
  }
  mesh.cancel(id);
}

#[test]
fn call_depth_limit_is_checked_before_entering_the_named_body() {
  let gate = gate();
  let mut mesh = Mesh::new();
  mesh.set_max_call_depth(2);
  mesh.add_wire(WireDef {
    name: "deep".into(),
    looped: false,
    flow: vec![ShardDef::new(&WAIT, vec![])],
  });
  mesh.add_wire(WireDef {
    name: "middle".into(),
    looped: false,
    flow: vec![do_("deep")],
  });
  mesh.add_wire(WireDef {
    name: "outer".into(),
    looped: false,
    flow: vec![do_("middle")],
  });
  mesh.add_wire(wire(vec![do_("outer")], false));
  let code = mesh.compile("root", Type::int()).unwrap();
  let id = mesh.spawn(&code, Var::Int(7)).unwrap();
  mesh.tick();
  let Some(Outcome::Failed(Error::Diagnostic(d))) = mesh.outcome(id) else {
    panic!("expected limit error")
  };
  assert_eq!(d.code, "recursion-limit");
  assert_eq!(gate.starts.load(Ordering::Relaxed), 0);
  assert_eq!(mesh.revision().max_call_depth(), 2);
  mesh.set_max_call_depth(3);
  let id = mesh.spawn(&code, Var::Int(7)).unwrap();
  mesh.tick();
  assert_eq!(gate.starts.load(Ordering::Relaxed), 1);
  mesh.cancel(id);
}
