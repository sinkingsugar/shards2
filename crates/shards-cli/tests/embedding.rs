//! The example from docs/embedding.md, as a test: a host crate defines its
//! own shards with the public API only (as an external crate would), adds
//! them to the catalog, and checks and runs a script on both schedulers.

use std::collections::HashMap;

use shards_core::args::Args;
use shards_core::compose::{Backend, ComposeCtx};
use shards_core::describe::{
  DefaultValue, Forms, InputDesc, OutputDesc, ParamDecl, Params, Requirement, ShardDesc, Targets,
  TypeName,
};
use shards_core::diagnostic::{Diagnostic, Phase};
use shards_core::instance::{InstanceCtx, LeafCtx};
use shards_core::shards::Operand;
use shards_core::shards::async_shard::{AsyncShard, async_type};
use shards_core::shards::leaf::{LeafShard, leaf_type};
use shards_core::{Catalog, Composed, Error, Flow, Outcome, Result, ShardType, Type, Var};
use shards_io::IoTask;
use shards_lang::{Program, Source};

// --- A leaf shard: a fixed record from a literal-or-variable parameter ---

static READING_PARAMS: &[ParamDecl] = &[
  ParamDecl {
    name: "Sensor",
    help: "Which sensor to read: a literal, or an Int variable read at activation.",
    forms: Forms::LITERAL.or(Forms::VARIABLE),
    types: &[TypeName::Int],
    requirement: Requirement::Default(DefaultValue::Int(0)),
  },
  ParamDecl {
    name: "Unit",
    help: "Override the unit (optional): a literal or a String variable.",
    forms: Forms::LITERAL.or(Forms::VARIABLE),
    types: &[TypeName::String],
    requirement: Requirement::Optional,
  },
];

/// Compiled Host.Reading: the sensor, and the unit override if given.
struct ReadingCompiled {
  sensor: Operand,
  unit: Option<Operand>,
}

const READING_DESC: ShardDesc = ShardDesc {
  name: "Host.Reading",
  version: 1,
  summary: "Reads a sensor: {id value unit}.",
  help: "Ignores its input. value is none when the sensor cannot be read.",
  params: Params::Declared(READING_PARAMS),
  input: InputDesc::Ignored,
  output: OutputDesc::Dynamic("{id: Int value: Float | None unit: String}"),
  targets: Targets::All,
  aliases: &[],
};

struct Reading;

impl LeafShard for Reading {
  type Compiled = ReadingCompiled;
  type State = ();
  const DESC: ShardDesc = READING_DESC;

  fn compose<B: Backend>(
    args: &Args,
    ctx: &mut ComposeCtx<'_, B>,
  ) -> Result<Composed<ReadingCompiled>> {
    // Compose reads only its arguments and declared context: never the host.
    let (sensor, ty) = Operand::compose_arg(args, "Sensor", READING_DESC.name, ctx)?;
    if ty != Type::int() {
      return Err(Error::Diagnostic(Box::new(
        Diagnostic::new(
          Phase::Compose,
          "compose-error",
          "wrong-variable-type",
          format!("Sensor must be an Int, got {ty}"),
        )
        .shard(READING_DESC.name)
        .param("Sensor", Some(0)),
      )));
    }
    // An optional parameter: None when the script did not give it.
    let unit =
      Operand::compose_optional_arg(args, "Unit", READING_DESC.name, ctx)?.map(|(op, _)| op);
    Ok(Composed {
      compiled: ReadingCompiled { sensor, unit },
      // A fixed record: `r.value` composes to `Float | None`, `r.typo` fails.
      output: Type::fixed_table([
        ("id", Type::int()),
        ("value", Type::union([Type::float(), Type::none()])),
        ("unit", Type::string()),
      ]),
    })
  }

  fn instantiate(_: &ReadingCompiled, _: &mut InstanceCtx) -> Result<()> {
    Ok(())
  }

  fn activate(c: &ReadingCompiled, _: &mut (), ctx: &mut impl LeafCtx, _: &Var) -> Result<Flow> {
    let Var::Int(id) = c.sensor.get(ctx) else {
      return Err(Error::Activation(
        "Host.Reading: Sensor is not an Int".into(),
      ));
    };
    // The host is read at activation; an unknown value is an explicit none.
    let value = if id >= 0 {
      Var::Float(id as f64 * 1.5)
    } else {
      Var::None
    };
    Ok(Flow::Next(Var::table([
      ("id", Var::Int(id)),
      ("value", value),
      (
        "unit",
        c.unit
          .as_ref()
          .map_or_else(|| Var::string("C"), |u| u.get(ctx)),
      ),
    ])))
  }
}

// --- A blocking shard: slow host work off the mesh thread ---

const SCAN_DESC: ShardDesc = ShardDesc {
  name: "Host.Scan",
  version: 1,
  summary: "Counts to the input on a worker thread.",
  help: "Suspends only its own wire; cancelling the instance stops the scan.",
  params: Params::Declared(&[]),
  input: InputDesc::Types(&[TypeName::Int]),
  output: OutputDesc::Fixed(TypeName::Int),
  targets: Targets::NativeOnly,
  aliases: &[],
};

struct Scan;

impl AsyncShard for Scan {
  type Compiled = ();
  type Op = IoTask;
  const DESC: ShardDesc = SCAN_DESC;

  fn compose<B: Backend>(_: &Args, ctx: &mut ComposeCtx<'_, B>) -> Result<Composed<()>> {
    if ctx.input() != Type::int() {
      return Err(Error::Diagnostic(Box::new(
        Diagnostic::new(
          Phase::Compose,
          "input-type-mismatch",
          "input-type-mismatch",
          format!("Host.Scan needs an Int input, got {}", ctx.input()),
        )
        .shard(SCAN_DESC.name),
      )));
    }
    Ok(Composed {
      compiled: (),
      output: Type::int(),
    })
  }

  fn start(_: &(), _: &mut impl LeafCtx, input: &Var) -> Result<IoTask> {
    // Own the inputs: the work runs on another thread.
    let Var::Int(n) = *input else {
      return Err(Error::Activation("Host.Scan: input is not an Int".into()));
    };
    Ok(shards_io::runtime::spawn_blocking(move |cancel| {
      let mut count = 0;
      for _ in 0..n {
        if cancel.is_cancelled() {
          return Err("cancelled".into());
        }
        count += 1;
      }
      Ok(Var::Int(count))
    }))
  }
}

// --- The host's catalog: its shards next to the core's ---

static READING: ShardType = leaf_type::<Reading>();
static SCAN: ShardType = async_type::<Scan>();
static HOST_CATALOG: &[&ShardType] = &[&READING, &SCAN];

fn catalog() -> Catalog {
  Catalog::new(&[shards_core::shards::CATALOG, HOST_CATALOG]).expect("no duplicate names")
}

const SCRIPT: &str = r#"
@sensor | ParseInt = id
Host.Reading(Sensor: id) = r
If(r.value | IsNone {"unreadable"} {f"{r.id}: {r.value}{r.unit}"}) | Log
1000 | Host.Scan
"#;

#[test]
fn a_host_checks_and_runs_a_script_with_its_own_shards() {
  let mut defines = HashMap::new();
  defines.insert("sensor".to_string(), "4".to_string());
  let report =
    shards_lang::check::<shards_core::Mesh>(Source::new("host.shs", SCRIPT), &catalog(), &defines);
  assert!(report.ok(), "{}", report.to_json());

  let program = Program::load(Source::new("host.shs", SCRIPT), &catalog(), &defines)
    .unwrap_or_else(|(_, d)| panic!("{d:?}"));
  for stackful in [false, true] {
    let (report, lines) = shards_core::log::capture(|| {
      if stackful {
        program.run::<shards_core::StackfulMesh>()
      } else {
        program.run::<shards_core::Mesh>()
      }
    });
    let report = report.unwrap_or_else(|d| panic!("{d:?}"));
    assert_eq!(lines, ["4: 6C"]);
    assert!(matches!(
      &report.outcomes[0].1,
      Some(Outcome::Completed(Var::Int(1000)))
    ));
  }

  // A typo on the host's fixed record is a located compose error.
  let report = shards_lang::check::<shards_core::Mesh>(
    Source::new("host.shs", "Host.Reading = r\nr.valeu"),
    &catalog(),
    &HashMap::new(),
  );
  let d = &report.diagnostics[0];
  assert_eq!(
    (d.code, d.line, d.did_you_mean.clone()),
    ("unknown-key", Some(2), vec!["value".to_string()])
  );
}
