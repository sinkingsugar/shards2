//! Resume cost against nesting depth. The stackless scheduler re-enters every
//! nesting level on resume; the stackful one switches straight back. This
//! measures how that scales.
//!
//! Each instance goes `depth` levels deep through nested `Do`s and then
//! pauses forever at the bottom, so each measured tick is a pure resume. After
//! 10 warm-up ticks, 1000 ticks are timed as a batch, with 1000 instances.
//! Each resume increments a counter, to verify the work (expected:
//! instances * ticks).
//!
//! Usage: cargo run --release --example bench_depth -- [--stackless]

use std::time::Instant;

use shards_core::shards::defs::*;
use shards_core::{Type, Var, WireDef};

const INSTANCES: usize = 1000;
const WARMUP: u32 = 10;
const TICKS: u32 = 1000;

fn wires(depth: usize) -> Vec<WireDef> {
  let mut wires = vec![WireDef {
    name: format!("level-{depth}"),
    looped: false,
    // Suspends forever without returning, so every tick is a pure resume:
    // the stackful scheduler switches straight back into the bottom level,
    // the stackless one re-enters every level to get there.
    flow: vec![while_(
      vec![konst(Var::Bool(true))],
      vec![inc("resumes"), pause()],
    )],
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
  ($mesh:expr, $depth:expr) => {{
    let mut mesh = $mesh;
    mesh.declare_var("resumes", Var::Int(0), true);
    for def in wires($depth) {
      mesh.add_wire(def);
    }
    let root = mesh.compile("root", Type::none()).expect("compile");
    for _ in 0..INSTANCES {
      mesh.spawn(&root, Var::None).expect("spawn");
    }
    // Warm up: instantiate everything and reach steady state.
    for _ in 0..WARMUP {
      mesh.tick();
    }
    let Some(Var::Int(r0)) = mesh.get_var("resumes") else {
      unreachable!()
    };
    let t = Instant::now();
    for _ in 0..TICKS {
      mesh.tick();
    }
    let tick_ns = t.elapsed().as_secs_f64() * 1e9 / f64::from(TICKS);
    let Some(Var::Int(r1)) = mesh.get_var("resumes") else {
      unreachable!()
    };
    (tick_ns, r1 - r0)
  }};
}

fn main() {
  let stackless = std::env::args().any(|a| a == "--stackless");
  let scheduler = if stackless { "stackless" } else { "stackful" };
  // Include the root/callee levels within the current 48-level compose limit.
  for depth in [1, 4, 16, 32] {
    let (tick_ns, resumes) = if stackless {
      run_depth!(shards_core::stackless::Mesh::new(), depth)
    } else {
      run_depth!(shards_core::StackfulMesh::new(), depth)
    };
    println!(
      "scheduler={scheduler} depth={depth} tick_us={:.2} ns_per_instance_tick={:.1} resumes={resumes} expected={}",
      tick_ns / 1000.0,
      tick_ns / INSTANCES as f64,
      INSTANCES as u32 * TICKS
    );
  }
}
