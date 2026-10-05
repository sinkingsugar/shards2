//! What core enforces for host shards (defined here with the public API
//! only, as an external crate would): declared input types, and in debug
//! builds (or with the `output-checks` feature) that produced values fit
//! the shard's compose output type. Both schedulers.

use shards_core::args::Args;
use shards_core::compose::{Backend, ComposeCtx};
use shards_core::describe::{InputDesc, OutputDesc, Params, ShardDesc, Targets, TypeName};
use shards_core::instance::{InstanceCtx, LeafCtx};
use shards_core::shards::leaf::{LeafShard, leaf_type};
use shards_core::{Composed, Flow, Result, ShardDef, ShardType, Type, Var, WireDef};

/// Declares a fixed record with `name: String` but produces an Int there:
/// a shard that drifted from its declared type.
struct Drifted;

const DRIFTED_DESC: ShardDesc = ShardDesc {
  name: "Host.Drifted",
  version: 1,
  summary: "",
  help: "",
  params: Params::Declared(&[]),
  input: InputDesc::Types(&[TypeName::Int]),
  output: OutputDesc::Dynamic("{name: String}"),
  targets: Targets::All,
  aliases: &[],
};

impl LeafShard for Drifted {
  type Compiled = ();
  type State = ();
  const DESC: ShardDesc = DRIFTED_DESC;

  fn compose<B: Backend>(_: &Args, _: &mut ComposeCtx<'_, B>) -> Result<Composed<()>> {
    Ok(Composed {
      compiled: (),
      output: Type::fixed_table([("name", Type::string())]),
    })
  }

  fn instantiate(_: &(), _: &mut InstanceCtx) -> Result<()> {
    Ok(())
  }

  fn activate(_: &(), _: &mut (), _: &mut impl LeafCtx, input: &Var) -> Result<Flow> {
    Ok(Flow::Next(Var::table([("name", input.clone())])))
  }
}

static DRIFTED: ShardType = leaf_type::<Drifted>();

fn wire(input: Var) -> WireDef {
  WireDef {
    name: "w".into(),
    looped: false,
    flow: vec![
      shards_core::shards::defs::konst(input),
      ShardDef::new(&DRIFTED, vec![]),
    ],
  }
}

macro_rules! host_contract_tests {
  ($mesh:ty) => {
    use super::*;

    #[test]
    fn declared_input_types_are_enforced() {
      let mut mesh = <$mesh>::new();
      mesh.add_wire(wire(Var::string("not an Int")));
      let err = mesh
        .compile("w", Type::none())
        .err()
        .expect("a compose error");
      let d = err.diagnostic().expect("structured");
      assert_eq!(
        (d.code, d.shard.as_deref()),
        ("input-type-mismatch", Some("Host.Drifted"))
      );
    }

    #[cfg(any(debug_assertions, feature = "output-checks"))]
    #[test]
    fn a_value_outside_the_declared_output_type_fails_the_instance() {
      use shards_core::{Error, Outcome};
      let mut mesh = <$mesh>::new();
      mesh.add_wire(wire(Var::Int(1)));
      let w = mesh.compile("w", Type::none()).expect("composes");
      let id = mesh.spawn(&w, Var::None).expect("spawns");
      mesh.run(2);
      match mesh.outcome(id) {
        Some(Outcome::Failed(Error::Activation(message))) => {
          assert!(
            message.contains("Host.Drifted produced {name: 1}"),
            "{message}"
          );
          assert!(
            message.contains("{name: String} does not admit"),
            "{message}"
          );
        }
        other => panic!("expected a failure, got {other:?}"),
      }
    }
  };
}

#[cfg(not(target_family = "wasm"))]
mod stackful {
  host_contract_tests!(shards_core::StackfulMesh);
}

mod stackless {
  host_contract_tests!(shards_core::Mesh);
}
