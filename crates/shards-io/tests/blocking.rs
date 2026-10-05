//! Blocking work on the shared runtime's blocking pool, through one
//! `AsyncShard`: it completes, it does not block the mesh, and cancelling
//! the instance cancels the work's token. Both schedulers.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use shards_core::args::Args;
use shards_core::compose::{Backend, ComposeCtx};
use shards_core::describe::{InputDesc, OutputDesc, Params, ShardDesc, Targets, TypeName};
use shards_core::instance::LeafCtx;
use shards_core::shards::async_shard::{AsyncShard, async_type};
use shards_core::{Composed, Result, ShardType, Type, Var};
use shards_io::IoTask;
use shards_io::runtime::spawn_blocking;

/// Per test slot (one per backend, so parallel tests cannot see each
/// other's signals): whether the blocking task started, and whether it saw
/// its cancellation token fire.
static STARTED: [AtomicBool; 3] = [const { AtomicBool::new(false) }; 3];
static SAW_CANCEL: [AtomicBool; 3] = [const { AtomicBool::new(false) }; 3];

/// Input 0: returns 42 after a short sleep. Input 1 or 2 (a test slot):
/// blocks until cancelled, checking its token as blocking host code must.
struct Block;

const BLOCK_DESC: ShardDesc = ShardDesc {
  name: "Test.Block",
  version: 1,
  summary: "",
  help: "",
  params: Params::Declared(&[]),
  input: InputDesc::Types(&[TypeName::Int]),
  output: OutputDesc::Fixed(TypeName::Int),
  targets: Targets::NativeOnly,
  aliases: &[],
};

impl AsyncShard for Block {
  type Compiled = ();
  type Op = IoTask;
  const DESC: ShardDesc = BLOCK_DESC;

  fn compose<B: Backend>(_: &Args, _: &mut ComposeCtx<'_, B>) -> Result<Composed<()>> {
    Ok(Composed {
      compiled: (),
      output: Type::int(),
    })
  }

  fn start(_: &(), _: &mut impl LeafCtx, input: &Var) -> Result<IoTask> {
    let slot = match input {
      Var::Int(s @ 1..=2) => Some(*s as usize),
      _ => None,
    };
    Ok(spawn_blocking(move |token| match slot {
      Some(slot) => {
        STARTED[slot].store(true, Ordering::SeqCst);
        while !token.is_cancelled() {
          std::thread::sleep(Duration::from_millis(1));
        }
        SAW_CANCEL[slot].store(true, Ordering::SeqCst);
        Err("cancelled".into())
      }
      None => {
        std::thread::sleep(Duration::from_millis(20));
        Ok(Var::Int(42))
      }
    }))
  }
}

static BLOCK: ShardType = async_type::<Block>();

macro_rules! blocking_tests {
  ($mesh:ty, $slot:expr) => {
    use super::*;
    use shards_core::{Outcome, ShardDef, WireDef};

    fn wire(input: i64) -> WireDef {
      WireDef {
        name: "w".into(),
        looped: false,
        flow: vec![
          shards_core::shards::defs::konst(Var::Int(input)),
          ShardDef::new(&BLOCK, vec![]),
        ],
      }
    }

    #[test]
    fn blocking_work_completes_without_blocking_the_mesh() {
      let mut mesh = <$mesh>::new();
      mesh.add_wire(wire(0));
      let w = mesh.compile("w", Type::none()).unwrap();
      let id = mesh.spawn(&w, Var::None).unwrap();
      let start = Instant::now();
      mesh.tick();
      // The 20 ms sleep runs on the blocking pool, not in the tick.
      assert!(start.elapsed() < Duration::from_millis(15), "tick blocked");
      while mesh.outcome(id).is_none() {
        assert!(start.elapsed() < Duration::from_secs(5), "timed out");
        mesh.tick();
        std::thread::sleep(Duration::from_millis(1));
      }
      assert_eq!(mesh.outcome(id), Some(&Outcome::Completed(Var::Int(42))));
    }

    #[test]
    fn cancelling_the_instance_cancels_the_blocking_work() {
      let mut mesh = <$mesh>::new();
      mesh.add_wire(wire($slot));
      let w = mesh.compile("w", Type::none()).unwrap();
      let id = mesh.spawn(&w, Var::None).unwrap();
      let start = Instant::now();
      while !STARTED[$slot].load(Ordering::SeqCst) {
        assert!(start.elapsed() < Duration::from_secs(5), "never started");
        mesh.tick();
        std::thread::sleep(Duration::from_millis(1));
      }
      mesh.cancel(id);
      assert_eq!(mesh.outcome(id), Some(&Outcome::Cancelled));
      while !SAW_CANCEL[$slot].load(Ordering::SeqCst) {
        assert!(
          start.elapsed() < Duration::from_secs(5),
          "token never fired"
        );
        std::thread::sleep(Duration::from_millis(1));
      }
    }
  };
}

mod stackful {
  blocking_tests!(shards_core::StackfulMesh, 1);
}

mod stackless {
  blocking_tests!(shards_core::Mesh, 2);
}
