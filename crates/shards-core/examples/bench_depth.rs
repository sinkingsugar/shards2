//! M4 gate measurements: true pending re-polls and full completion/unwind.
//! 1000 instances; identical workloads on both schedulers. Mixed-flow throughput
//! is measured separately with bench_instances. See bench/trampoline/run.py.

use std::time::Instant;

use shards_core::args::Args;
use shards_core::compose::{Backend, ComposeCtx};
use shards_core::instance::LeafCtx;
use shards_core::shards::async_shard::{AsyncShard, async_type};
use shards_core::shards::defs::*;
use shards_core::{Composed, Result, ShardDef, ShardDesc, ShardType};
use shards_core::{Type, Var, WireDef};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};

static READY: AtomicBool = AtomicBool::new(false);
struct Wait;
struct Pending(bool);
impl Future for Pending {
  type Output = Result<Var>;
  fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
    if self.0 && READY.load(Ordering::Relaxed) {
      Poll::Ready(Ok(Var::None))
    } else {
      self.0 = true;
      Poll::Pending
    }
  }
}
impl AsyncShard for Wait {
  type Compiled = ();
  type Op = Pending;
  const DESC: ShardDesc = ShardDesc::undocumented("Bench.Wait", 1);
  fn compose<B: Backend>(_: &Args, _: &mut ComposeCtx<'_, B>) -> Result<Composed<()>> {
    Ok(Composed {
      compiled: (),
      output: Type::none(),
    })
  }
  fn start(_: &(), _: &mut impl LeafCtx, _: &Var) -> Result<Pending> {
    Ok(Pending(false))
  }
}
static WAIT: ShardType = async_type::<Wait>();

const INSTANCES: usize = 1000;
const WARMUP: u32 = 10;
const TICKS: u32 = 1000;

fn wires(depth: usize, progress: bool) -> Vec<WireDef> {
  let mut wires = vec![WireDef {
    name: format!("level-{depth}"),
    looped: false,
    flow: if progress {
      // Retain the pre-M4 workload unchanged: each Pause completes on resume,
      // then While performs work and starts a new Pause at the bottom.
      vec![while_(
        vec![konst(Var::Bool(true))],
        vec![inc("resumes"), pause()],
      )]
    } else {
      vec![ShardDef::new(&WAIT, vec![]), inc("resumes")]
    },
  }];
  for level in 0..depth {
    wires.push(WireDef {
      name: format!("level-{level}"),
      looped: false,
      flow: vec![do_(&format!("level-{}", level + 1))],
    });
  }
  wires.push(WireDef {
    name: "root".into(),
    looped: true,
    flow: vec![do_("level-0")],
  });
  wires
}

macro_rules! run_depth {
  ($mesh:expr, $depth:expr, $progress:expr) => {{
    let mut mesh = $mesh;
    mesh.declare_var("resumes", Var::Int(0), true);
    for def in wires($depth, $progress) {
      mesh.add_wire(def);
    }
    let root = mesh.compile("root", Type::none()).expect("compile");
    for _ in 0..INSTANCES {
      mesh.spawn(&root, Var::None).expect("spawn");
    }
    READY.store(false, Ordering::Relaxed);
    for _ in 0..WARMUP {
      mesh.tick();
    }
    if $progress {
      let before = mesh.get_var("resumes").unwrap();
      let t = Instant::now();
      for _ in 0..TICKS {
        mesh.tick();
      }
      let ns = t.elapsed().as_secs_f64() * 1e9 / f64::from(TICKS);
      let Var::Int(before) = before else {
        unreachable!()
      };
      assert_eq!(
        mesh.get_var("resumes"),
        Some(Var::Int(before + INSTANCES as i64 * i64::from(TICKS)))
      );
      (ns, 0.0)
    } else {
      let t = Instant::now();
      for _ in 0..TICKS {
        mesh.tick();
      }
      let pending_ns = t.elapsed().as_secs_f64() * 1e9 / f64::from(TICKS);
      assert_eq!(mesh.get_var("resumes"), Some(Var::Int(0)));
      READY.store(true, Ordering::Relaxed);
      let mut completion = std::time::Duration::ZERO;
      for _ in 0..TICKS {
        let t = Instant::now();
        mesh.tick();
        completion += t.elapsed();
        // Entry/start is outside the completion sample. Each operation's first
        // poll always parks, even when ready, so the timed tick unwinds it fully.
        mesh.tick();
      }
      assert_eq!(
        mesh.get_var("resumes"),
        Some(Var::Int(INSTANCES as i64 * i64::from(TICKS)))
      );
      (
        pending_ns,
        completion.as_secs_f64() * 1e9 / f64::from(TICKS),
      )
    }
  }};
}

fn main() {
  let stackless = std::env::args().any(|a| a == "--stackless");
  let progress = std::env::args().any(|a| a == "--progress");
  let scheduler = if stackless { "stackless" } else { "stackful" };
  // Include the root/callee levels within the current 48-level compose limit.
  for depth in [1, 4, 16, 32] {
    let (pending_ns, completion_ns) = if stackless {
      run_depth!(shards_core::stackless::Mesh::new(), depth, progress)
    } else {
      run_depth!(shards_core::StackfulMesh::new(), depth, progress)
    };
    let phases = if progress {
      vec![("progress", pending_ns)]
    } else {
      vec![("pending", pending_ns), ("completion", completion_ns)]
    };
    for (phase, tick_ns) in phases {
      println!(
        "scheduler={scheduler} phase={phase} depth={depth} instances={INSTANCES} ticks={TICKS} ns_per_instance_tick={:.2}",
        tick_ns / INSTANCES as f64
      );
    }
  }
}
