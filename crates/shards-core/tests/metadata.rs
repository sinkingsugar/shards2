//! Shard metadata, argument decoding and the catalog
//! (docs/shard-metadata-and-compose.md §7). Runs on both schedulers.

use shards_core::args::decode;
use shards_core::describe::{Params, TypeName};
use shards_core::shards::defs::*;
use shards_core::shards::{ADD, ADD_DESC, CATALOG, WHEN, WHEN_PARAMS};
use shards_core::{Arg, Catalog, ParamValue, ShardDef, Var};

fn add_with(args: Vec<Arg>) -> ShardDef {
  ShardDef::with_args(&ADD, args)
}

macro_rules! metadata_tests {
  ($mesh:ty) => {
    use super::*;
    use shards_core::{Diagnostic, Error, Outcome, Type, WireDef};

    type Mesh = $mesh;

    fn wire(flow: Vec<ShardDef>) -> WireDef {
      WireDef {
        name: "test".into(),
        looped: false,
        flow,
      }
    }

    fn compile_error(flow: Vec<ShardDef>, input: Type) -> Diagnostic {
      let mut mesh = Mesh::new();
      mesh.add_wire(wire(flow));
      match mesh.compile("test", input) {
        Err(Error::Diagnostic(d)) => *d,
        Err(other) => panic!("expected a structured diagnostic, got {other:?}"),
        Ok(_) => panic!("expected a compose failure"),
      }
    }

    fn run(flow: Vec<ShardDef>, input: Var) -> Outcome {
      let mut mesh = Mesh::new();
      mesh.add_wire(wire(flow));
      let compiled = mesh.compile("test", input.type_of()).unwrap();
      let id = mesh.spawn(&compiled, input).unwrap();
      mesh.run(10);
      mesh.outcome(id).cloned().unwrap()
    }

    #[test]
    fn argument_decoding_diagnostics() {
      let cases: Vec<(Vec<Arg>, &str, Option<&str>, Option<i32>)> = vec![
        (vec![], "missing-argument", Some("Operand"), Some(0)),
        (
          vec![Arg::named("Value", val(Var::Int(1)))],
          "unknown-argument",
          Some("Value"),
          None,
        ),
        (
          vec![
            Arg::pos(val(Var::Int(1))),
            Arg::named("Operand", val(Var::Int(2))),
          ],
          "duplicate-argument",
          Some("Operand"),
          Some(0),
        ),
        (
          vec![Arg::pos(ParamValue::Wire("w".into()))],
          "wrong-argument-form",
          Some("Operand"),
          Some(0),
        ),
        (
          vec![Arg::pos(val(Var::string("one")))],
          "wrong-argument-type",
          Some("Operand"),
          Some(0),
        ),
        (
          vec![
            Arg::named("Operand", val(Var::Int(1))),
            Arg::pos(val(Var::Int(2))),
          ],
          "positional-after-named",
          None,
          None,
        ),
        (
          vec![Arg::pos(val(Var::Int(1))), Arg::pos(val(Var::Int(2)))],
          "too-many-arguments",
          None,
          None,
        ),
      ];
      for (args, code, param, index) in cases {
        let d = compile_error(vec![konst(Var::Int(1)), add_with(args)], Type::none());
        assert_eq!(d.code, code, "{d:?}");
        assert_eq!(d.phase.name(), "construct");
        assert_eq!(d.shard.as_deref(), Some("Math.Add"));
        assert_eq!(d.param.as_deref(), param, "{code}");
        assert_eq!(d.param_index, index, "{code}");
      }

      // The wrong-type case carries structured types with 1.x basic types.
      let d = compile_error(
        vec![
          konst(Var::Int(1)),
          add_with(vec![Arg::pos(val(Var::string("one")))]),
        ],
        Type::none(),
      );
      assert_eq!(d.actual.as_ref().unwrap().name, "String");
      assert_eq!(d.actual.as_ref().unwrap().basic_type, 52);
      let expected: Vec<&str> = d.expected.iter().map(|t| t.name.as_str()).collect();
      assert_eq!(expected, ["Int", "Float", "Float2", "Float3", "Float4"]);

      // A shard that does not describe its parameters rejects named arguments.
      let err = decode(
        &shards_core::ShardDesc::undocumented("Undescribed", 1),
        &[Arg::named("Variable", var("x"))],
      )
      .err()
      .unwrap();
      assert_eq!(
        err.diagnostic().unwrap().code,
        "named-arguments-unsupported"
      );
    }

    #[test]
    fn compose_checks_context_dependent_rules() {
      // Add publishes its numeric types; compose decides whether the pair
      // mixes (vectors of different sizes do not).
      let d = compile_error(
        vec![
          konst(Var::Float3([0.0; 3])),
          add(val(Var::Float2([0.0; 2]))),
        ],
        Type::none(),
      );
      assert_eq!(
        (d.phase.name(), d.kind, d.code),
        ("compose", "input-type-mismatch", "input-type-mismatch")
      );
      assert_eq!(d.actual.as_ref().unwrap().name, "Float3");
      assert_eq!(d.expected[0].name, "Int");

      // When's predicate must output Bool.
      let d = compile_error(vec![when(vec![konst(Var::Int(1))], vec![])], Type::none());
      assert_eq!(
        (d.code, d.shard.as_deref(), d.param.as_deref()),
        ("predicate-not-bool", Some("When"), Some("Predicate"))
      );
    }

    #[test]
    fn named_and_positional_arguments_are_equivalent() {
      let positional = run(vec![add(val(Var::Int(2)))], Var::Int(40));
      let named = run(
        vec![add_with(vec![Arg::named("Operand", val(Var::Int(2)))])],
        Var::Int(40),
      );
      assert_eq!(positional, Outcome::Completed(Var::Int(42)));
      assert_eq!(named, positional);

      // When with named arguments in reverse order.
      let named_when = ShardDef::with_args(
        &WHEN,
        vec![
          Arg::named(
            "Action",
            ParamValue::Flow(vec![konst(Var::Int(0)), update("x")]),
          ),
          Arg::named("Predicate", ParamValue::Flow(vec![konst(Var::Bool(true))])),
        ],
      );
      let flow = vec![konst(Var::Int(7)), set("x"), named_when, get("x")];
      assert_eq!(run(flow, Var::None), Outcome::Completed(Var::Int(0)));

      // A variable operand is a binding read at activation.
      let flow = vec![
        konst(Var::Int(5)),
        set("k"),
        konst(Var::Int(1)),
        add_with(vec![Arg::named("Operand", var("k"))]),
      ];
      assert_eq!(run(flow, Var::None), Outcome::Completed(Var::Int(6)));
    }

    #[test]
    fn variable_resolution_errors_are_structured() {
      // Unknown variable for a declared parameter: code, shard and parameter.
      let d = compile_error(
        vec![
          konst(Var::Int(1)),
          add_with(vec![Arg::named("Operand", var("missing"))]),
        ],
        Type::none(),
      );
      assert_eq!(
        (
          d.code,
          d.shard.as_deref(),
          d.param.as_deref(),
          d.param_index
        ),
        (
          "unknown-variable",
          Some("Math.Add"),
          Some("Operand"),
          Some(0)
        )
      );
      // A possibly-uninitialized variable, through an undescribed shard
      // (Get): structured too, without a parameter.
      let d = compile_error(
        vec![
          when(
            vec![konst(Var::Bool(false))],
            vec![konst(Var::Int(1)), set("x")],
          ),
          get("x"),
        ],
        Type::none(),
      );
      assert_eq!(
        (d.code, d.shard.as_deref()),
        ("possibly-uninitialized", Some("Get"))
      );
    }

    /// Context-dependent checks stay in compose, as structured diagnostics.
    #[test]
    fn compose_checks_of_every_shard() {
      use shards_core::shards::{DO, PROBE, REPEAT, SPAWN};
      let named =
        |ty: &'static shards_core::ShardType, args: Vec<Arg>| ShardDef::with_args(ty, args);
      let cases: Vec<(&str, Vec<ShardDef>, &str, &str, Option<&str>)> = vec![
        (
          "set-type",
          vec![
            konst(Var::Int(1)),
            set("x"),
            konst(Var::Float(1.0)),
            set("x"),
          ],
          "variable-type-mismatch",
          "Set",
          Some("Variable"),
        ),
        (
          "update-unknown",
          vec![konst(Var::Int(1)), update("nope")],
          "unknown-variable",
          "Update",
          Some("Variable"),
        ),
        (
          "update-type",
          vec![
            konst(Var::Int(1)),
            set("x"),
            konst(Var::Float(1.0)),
            update("x"),
          ],
          "variable-type-mismatch",
          "Update",
          Some("Variable"),
        ),
        (
          "inc-type",
          vec![konst(Var::Float(1.0)), set("f"), inc("f")],
          "wrong-variable-type",
          "Math.Inc",
          Some("Variable"),
        ),
        (
          "get-unknown",
          vec![get("nope")],
          "unknown-variable",
          "Get",
          Some("Variable"),
        ),
        (
          "repeat-times-var",
          vec![
            konst(Var::Float(2.0)),
            set("t"),
            named(
              &REPEAT,
              vec![
                Arg::pos(ParamValue::Flow(vec![])),
                Arg::named("Times", var("t")),
              ],
            ),
          ],
          "wrong-variable-type",
          "Repeat",
          Some("Times"),
        ),
        (
          "pause-negative",
          vec![pause_secs(-1.0)],
          "invalid-argument-value",
          "Pause",
          Some("Seconds"),
        ),
        (
          "pause-nan",
          vec![pause_secs(f64::NAN)],
          "invalid-argument-value",
          "Pause",
          Some("Seconds"),
        ),
        (
          "pause-infinite",
          vec![pause_secs(f64::INFINITY)],
          "invalid-argument-value",
          "Pause",
          Some("Seconds"),
        ),
        (
          "pause-too-large",
          vec![pause_secs(1e100)],
          "invalid-argument-value",
          "Pause",
          Some("Seconds"),
        ),
        (
          "do-unknown",
          vec![named(
            &DO,
            vec![Arg::pos(ParamValue::Wire("nowhere".into()))],
          )],
          "unknown-wire",
          "Do",
          Some("Wire"),
        ),
        (
          "spawn-unknown",
          vec![named(
            &SPAWN,
            vec![Arg::pos(ParamValue::Wire("nowhere".into()))],
          )],
          "unknown-wire",
          "Spawn",
          Some("Wire"),
        ),
        (
          "probe-mode",
          vec![named(
            &PROBE,
            vec![
              Arg::pos(val(Var::string("p"))),
              Arg::named("Mode", val(Var::string("explode"))),
            ],
          )],
          "invalid-argument-value",
          "Probe",
          Some("Mode"),
        ),
        (
          "request-delay",
          vec![request(-1, false, true)],
          "invalid-argument-value",
          "Request",
          Some("Delay"),
        ),
      ];
      for (label, flow, code, shard, param) in cases {
        let d = compile_error(flow, Type::none());
        assert_eq!(
          (d.code, d.shard.as_deref(), d.param.as_deref()),
          (code, Some(shard), param),
          "{label}: {d:?}"
        );
        assert_eq!(d.phase.name(), "compose", "{label}");
      }

      // A wire that runs itself through Do: recursive.
      let mut mesh = Mesh::new();
      mesh.add_wire(WireDef {
        name: "loop".into(),
        looped: false,
        flow: vec![named(&DO, vec![Arg::pos(ParamValue::Wire("loop".into()))])],
      });
      let d = match mesh.compile("loop", Type::none()) {
        Err(Error::Diagnostic(d)) => *d,
        other => panic!("{:?}", other.map(|_| ())),
      };
      assert_eq!((d.code, d.shard.as_deref()), ("recursive-wire", Some("Do")));
    }

    #[test]
    fn a_called_wire_keeps_its_own_diagnostics() {
      use shards_core::shards::{DO, SPAWN};
      // An error inside the called wire belongs to the shard that raised it,
      // not to Do's or Spawn's Wire parameter.
      for caller in [&DO, &SPAWN] {
        let mut mesh = Mesh::new();
        mesh.add_wire(WireDef {
          name: "child".into(),
          looped: false,
          flow: vec![ShardDef::new(
            &ADD,
            vec![
              ParamValue::Value(Var::Int(1)),
              ParamValue::Value(Var::Int(2)),
            ],
          )],
        });
        mesh.add_wire(wire(vec![ShardDef::with_args(
          caller,
          vec![Arg::pos(ParamValue::Wire("child".into()))],
        )]));
        let d = match mesh.compile("test", Type::int()) {
          Err(Error::Diagnostic(d)) => *d,
          other => panic!("{:?}", other.map(|_| ())),
        };
        let name = caller.desc.name;
        assert_eq!(d.code, "too-many-arguments", "{name}: {d:?}");
        assert_eq!(d.shard.as_deref(), Some("Math.Add"), "{name}: {d:?}");
        assert_eq!(
          (d.param.as_deref(), d.param_index),
          (None, None),
          "{name}: {d:?}"
        );
      }
    }

    #[test]
    fn defaults_behave_like_the_documented_behavior() {
      // Pause's default (0 seconds) suspends exactly once: completes on the
      // second tick, like an explicit Pause(Seconds: 0.0).
      for flow in [
        vec![pause(), konst(Var::Int(1))],
        vec![pause_secs(0.0), konst(Var::Int(1))],
      ] {
        let mut mesh = Mesh::new();
        mesh.add_wire(wire(flow));
        let compiled = mesh.compile("test", Type::none()).unwrap();
        let id = mesh.spawn(&compiled, Var::None).unwrap();
        assert_eq!(mesh.tick(), 1);
        assert_eq!(mesh.tick(), 0);
        assert_eq!(mesh.outcome(id), Some(&Outcome::Completed(Var::Int(1))));
      }
      // Repeat with a negative count runs its body zero times, as documented.
      let flow = vec![
        konst(Var::Int(0)),
        set("n"),
        repeat(vec![inc("n")], val(Var::Int(-3))),
        get("n"),
      ];
      assert_eq!(run(flow, Var::None), Outcome::Completed(Var::Int(0)));
    }

    #[test]
    fn wrong_form_for_a_composite_parameter_cites_its_declaration() {
      let d = compile_error(
        vec![ShardDef::with_args(
          &WHEN,
          vec![
            Arg::pos(ParamValue::Flow(vec![konst(Var::Bool(true))])),
            Arg::pos(val(Var::Int(1))),
          ],
        )],
        Type::none(),
      );
      assert_eq!(
        (d.code, d.param.as_deref(), d.param_index),
        ("wrong-argument-form", Some("Action"), Some(1))
      );
    }
  };
}

#[cfg(not(target_family = "wasm"))]
mod stackful {
  metadata_tests!(shards_core::StackfulMesh);

  #[test]
  fn a_shard_without_a_backend_implementation_reports_it() {
    // Probe-like shard with only a stackful implementation.
    struct OnlyStackful;
    impl shards_core::Shard for OnlyStackful {
      type Compiled = ();
      type State = ();
      const NAME: &'static str = "OnlyStackful";
      fn compose(
        _: &shards_core::Args,
        ctx: &mut shards_core::ComposeCtx<'_, shards_core::Stackful>,
      ) -> shards_core::Result<shards_core::Composed<()>> {
        Ok(shards_core::Composed {
          compiled: (),
          output: ctx.input(),
        })
      }
      fn instantiate(
        _: &(),
        _: &mut shards_core::instance::InstanceCtx,
      ) -> shards_core::Result<()> {
        Ok(())
      }
      fn activate(
        _: &(),
        _: &mut (),
        _: &mut shards_core::runtime::ActivationCtx<'_>,
        input: &Var,
      ) -> shards_core::Result<shards_core::Flow> {
        Ok(shards_core::Flow::Next(input.clone()))
      }
    }
    static ONLY_STACKFUL: shards_core::ShardType = shards_core::shard_type::<OnlyStackful>();

    assert_eq!(ONLY_STACKFUL.backends(), ["stackful"]);
    let catalog = Catalog::new(&[&[&ONLY_STACKFUL]]).unwrap();
    assert!(
      catalog
        .describe_json("OnlyStackful")
        .unwrap()
        .contains("\"backends\":[\"stackful\"]")
    );

    let mut stackless = shards_core::stackless::Mesh::new();
    stackless.add_wire(WireDef {
      name: "w".into(),
      looped: false,
      flow: vec![ShardDef::new(&ONLY_STACKFUL, vec![])],
    });
    let err = stackless.compile("w", Type::none()).err().unwrap();
    let d = err.diagnostic().unwrap();
    assert_eq!(d.code, "backend-unavailable");
    assert!(d.message.contains("available: stackful"), "{}", d.message);

    let mut stackful = shards_core::StackfulMesh::new();
    stackful.add_wire(WireDef {
      name: "w".into(),
      looped: false,
      flow: vec![ShardDef::new(&ONLY_STACKFUL, vec![])],
    });
    assert!(stackful.compile("w", Type::none()).is_ok());
  }
}

mod stackless {
  metadata_tests!(shards_core::stackless::Mesh);
}

#[test]
fn when_description_and_decoder_use_the_same_declarations() {
  // Structural: there is one copy of the declarations (a static), the
  // ShardType's description holds it, and both the decoder and the catalog
  // read that description.
  let Params::Declared(declared) = WHEN.desc.params else {
    panic!("When must declare its parameters");
  };
  assert!(std::ptr::eq(declared, WHEN_PARAMS));
  let decoded = decode(
    &WHEN.desc,
    &[
      Arg::named("Predicate", ParamValue::Flow(vec![])),
      Arg::named("Action", ParamValue::Flow(vec![])),
    ],
  )
  .unwrap();
  assert!(decoded.flow("Predicate").is_some() && decoded.flow("Action").is_some());

  let catalog = Catalog::new(&[CATALOG]).unwrap();
  let json = catalog.describe_json("When").unwrap();
  for decl in WHEN_PARAMS {
    assert!(
      json.contains(&format!("\"name\":\"{}\"", decl.name)),
      "{json}"
    );
  }
  assert!(json.contains("\"forms\":[\"flow\"]"));
  assert!(json.contains("\"output\":{\"kind\":\"passthrough\"}"));
  assert!(json.contains("\"backends\":[\"stackful\",\"stackless\"]"));
}

#[test]
fn catalog_index_detail_and_search() {
  let catalog = Catalog::new(&[CATALOG]).unwrap();
  assert!(
    Catalog::new(&[CATALOG, CATALOG]).is_err(),
    "duplicates are rejected"
  );

  let index = catalog.index_json();
  assert!(index.starts_with("{\"schema\":\"shards2-catalog/1\""));
  assert!(index.contains("{\"name\":\"Math.Add\",\"aliases\":[\"Add\"],\"summary\":"));
  assert!(index.contains("\"documented\":true"));
  // Every shard shipped in the core crate is described.
  assert!(!index.contains("\"documented\":false"), "{index}");
  for ty in CATALOG {
    assert!(ty.desc.is_documented(), "{} is not described", ty.name());
    assert_eq!(ty.backends(), ["stackful", "stackless"], "{}", ty.name());
  }

  let add = catalog.describe_json("Add").unwrap();
  assert!(add.contains("\"name\":\"Operand\",\"index\":0"));
  assert!(add.contains("\"forms\":[\"literal\",\"variable\"]"));
  assert!(add.contains("{\"name\":\"Int\",\"basic_type\":4}"));
  assert!(add.contains("\"required\":true"));
  assert!(add.contains("\"output\":{\"kind\":\"dynamic\""), "{add}");
  assert!(catalog.describe_json("NoSuchShard").is_none());

  let found: Vec<&str> = catalog.search("add").iter().map(|s| s.name()).collect();
  assert_eq!(found, ["Math.Add"]);
  // Aliases resolve to the canonical shard.
  assert_eq!(catalog.get("Add").map(|s| s.name()), Some("Math.Add"));
}

#[test]
fn diagnostic_json_keeps_1x_field_names() {
  let err = decode(&ADD_DESC, &[]).err().unwrap();
  let json = err.diagnostic().unwrap().to_json();
  for field in [
    "\"phase\":\"construct\"",
    "\"severity\":\"error\"",
    "\"kind\":\"generic\"",
    "\"code\":\"missing-argument\"",
    "\"shard\":\"Math.Add\"",
    "\"param_index\":0",
    "\"param\":\"Operand\"",
  ] {
    assert!(json.contains(field), "{field} missing in {json}");
  }
  // Not computed yet: omitted when empty, as in 1.x.
  assert!(!json.contains("did_you_mean") && !json.contains("candidates"));
  // No source location for wires built in Rust.
  assert!(!json.contains("\"file\""));
}

#[test]
fn documentation_prose_follows_the_docs_feature() {
  // The argument contract is always there; the prose only with `docs`.
  assert!(
    matches!(ADD_DESC.params, Params::Declared(decls) if decls[0].types.contains(&TypeName::Float3))
  );
  if cfg!(feature = "docs") {
    assert!(!ADD_DESC.summary.is_empty());
  } else {
    assert!(ADD_DESC.summary.is_empty() && ADD_DESC.help.is_empty());
    assert!(WHEN_PARAMS.iter().all(|p| p.help.is_empty()));
  }
}

#[test]
fn sequence_types_use_the_1x_code() {
  use shards_core::diagnostic::TypeRef;
  assert_eq!(
    TypeRef::of(shards_core::Type::seq(shards_core::Type::int())).basic_type,
    56
  );
  assert_eq!(TypeRef::of(shards_core::Type::string()).basic_type, 52);
}

#[test]
fn defaults_follow_the_declared_contract() {
  use shards_core::describe::{DefaultValue, Forms, ParamDecl, Requirement, check_params};
  // An Int-only parameter with a String default: rejected by the shared
  // check (which ShardType::new runs at compile time) and by the decoder.
  static INVALID: &[ParamDecl] = &[ParamDecl {
    name: "Operand",
    help: "",
    forms: Forms::LITERAL,
    types: &[TypeName::Int],
    requirement: Requirement::Default(DefaultValue::Str("bad")),
    ty: None,
  }];
  assert_eq!(check_params(INVALID), Err(0));
  let desc = shards_core::ShardDesc {
    params: Params::Declared(INVALID),
    ..ADD_DESC
  };
  let err = decode(&desc, &[]).err().unwrap();
  assert_eq!(err.diagnostic().unwrap().code, "invalid-declaration");

  // A default on a parameter that does not accept literals, and a
  // duplicate name, are invalid too.
  static NOT_LITERAL: &[ParamDecl] = &[ParamDecl {
    name: "V",
    help: "",
    forms: Forms::VARIABLE,
    types: &[],
    requirement: Requirement::Default(DefaultValue::Int(1)),
    ty: None,
  }];
  assert_eq!(check_params(NOT_LITERAL), Err(0));
  static DUPLICATE: &[ParamDecl] = &[
    ParamDecl {
      name: "A",
      help: "",
      forms: Forms::LITERAL,
      types: &[],
      requirement: Requirement::Required,
      ty: None,
    },
    ParamDecl {
      name: "A",
      help: "",
      forms: Forms::LITERAL,
      types: &[],
      requirement: Requirement::Required,
      ty: None,
    },
  ];
  assert_eq!(check_params(DUPLICATE), Err(1));

  // Every shipped declaration is valid.
  for ty in CATALOG {
    if let Params::Declared(decls) = ty.desc.params {
      assert_eq!(check_params(decls), Ok(()), "{}", ty.name());
    }
  }
}

/// For every shard in the catalog and every declared parameter, each form the
/// declaration does not accept is rejected by the decoder, and each form it
/// accepts decodes. The decoder enforces exactly what is documented.
#[test]
fn every_declared_parameter_accepts_exactly_its_documented_forms() {
  use shards_core::describe::{Forms, ParamDecl, Requirement};
  fn sample(form: Forms, decl: &ParamDecl) -> ParamValue {
    if form == Forms::LITERAL {
      let v = match decl.types.first() {
        Some(TypeName::Bool) => Var::Bool(true),
        Some(TypeName::Int) | None => Var::Int(1),
        Some(TypeName::Float) => Var::Float(1.0),
        Some(TypeName::Float2) => Var::Float2([1.0, 1.0]),
        Some(TypeName::Float3) => Var::Float3([1.0, 1.0, 1.0]),
        Some(TypeName::Float4) => Var::Float4([1.0, 1.0, 1.0, 1.0]),
        Some(TypeName::String) => Var::string("s"),
        Some(TypeName::Seq) => Var::Seq(Default::default()),
        Some(TypeName::Table) => Var::table(Vec::<(&str, Var)>::new()),
        Some(TypeName::None) | Some(TypeName::Any) => Var::None,
      };
      ParamValue::Value(v)
    } else if form == Forms::VARIABLE {
      ParamValue::Var("v".into())
    } else if form == Forms::WIRE {
      ParamValue::Wire("w".into())
    } else if form == Forms::CASES {
      ParamValue::Cases(vec![])
    } else {
      ParamValue::Flow(vec![])
    }
  }
  let forms = [
    Forms::LITERAL,
    Forms::VARIABLE,
    Forms::WIRE,
    Forms::FLOW,
    Forms::CASES,
  ];
  let mut checked = 0;
  for ty in CATALOG {
    let Params::Declared(decls) = ty.desc.params else {
      continue;
    };
    for (index, decl) in decls.iter().enumerate() {
      // Arguments for every other required parameter, in an accepted form.
      let base: Vec<Arg> = decls
        .iter()
        .filter(|d| d.name != decl.name && d.requirement == Requirement::Required)
        .map(|d| {
          Arg::named(
            d.name,
            sample(forms.into_iter().find(|f| d.forms.contains(*f)).unwrap(), d),
          )
        })
        .collect();
      let variadic = decl.requirement == Requirement::Variadic;
      for form in forms {
        let args = if variadic {
          // Variadic arguments are positional, after the others in order.
          let mut args: Vec<Arg> = base.iter().map(|a| Arg::pos(a.value.clone())).collect();
          args.push(Arg::pos(sample(form, decl)));
          args
        } else {
          let mut args = base.clone();
          args.push(Arg::named(decl.name, sample(form, decl)));
          args
        };
        let result = decode(&ty.desc, &args);
        if decl.forms.contains(form) {
          assert!(
            result.is_ok(),
            "{}.{}: {:?} should decode",
            ty.name(),
            decl.name,
            form.names()
          );
        } else {
          let err = result.err().unwrap();
          let d = err.diagnostic().unwrap();
          assert_eq!(
            (d.code, d.param_index),
            ("wrong-argument-form", Some(index as i32)),
            "{}.{}",
            ty.name(),
            decl.name
          );
        }
        checked += 1;
      }
    }
  }
  assert!(checked >= 4 * 20, "checked only {checked} combinations");
}

/// Table, sequence and float-vector parameters decode through the same
/// acceptance rule, and mismatches report 1.x type codes.
#[test]
fn table_seq_and_vector_parameters_decode_by_acceptance() {
  use shards_core::describe::{Forms, ParamDecl, Requirement, ShardDesc};
  static PARAMS: &[ParamDecl] = &[
    ParamDecl {
      name: "Shape",
      help: "",
      forms: Forms::LITERAL,
      types: &[TypeName::Table],
      requirement: Requirement::Optional,
      ty: None,
    },
    ParamDecl {
      name: "Items",
      help: "",
      forms: Forms::LITERAL,
      types: &[TypeName::Seq],
      requirement: Requirement::Optional,
      ty: None,
    },
    ParamDecl {
      name: "At",
      help: "",
      forms: Forms::LITERAL,
      types: &[TypeName::Float2, TypeName::Float4],
      requirement: Requirement::Optional,
      ty: None,
    },
  ];
  let desc = ShardDesc {
    params: Params::Declared(PARAMS),
    ..ShardDesc::undocumented("Shapes", 1)
  };
  let table = Var::table([("a", Var::Int(1)), ("b", Var::string("s"))]);
  let mixed = Var::Seq(std::sync::Arc::new(vec![Var::Int(1), Var::Float(2.0)]));
  let decoded = decode(
    &desc,
    &[
      Arg::named("Shape", ParamValue::Value(table.clone())),
      Arg::named("Items", ParamValue::Value(mixed)),
      Arg::named("At", ParamValue::Value(Var::Float2([1.0, 2.0]))),
    ],
  )
  .unwrap();
  assert_eq!(decoded.literal("Shape"), Some(&table));

  let err = decode(
    &desc,
    &[Arg::named("Shape", ParamValue::Value(Var::Int(1)))],
  )
  .err()
  .unwrap();
  let d = err.diagnostic().unwrap();
  assert_eq!(d.code, "wrong-argument-type");
  assert_eq!(d.actual.as_ref().map(|t| t.basic_type), Some(4));
  assert_eq!(
    d.expected
      .iter()
      .map(|t| (t.name.as_str(), t.basic_type))
      .collect::<Vec<_>>(),
    [("Table", 57)]
  );

  let err = decode(
    &desc,
    &[Arg::named("At", ParamValue::Value(Var::Float3([0.0; 3])))],
  )
  .err()
  .unwrap();
  let d = err.diagnostic().unwrap();
  assert_eq!(
    d.expected.iter().map(|t| t.basic_type).collect::<Vec<_>>(),
    [11, 13]
  );
}

/// Type codes of the full descriptions: tables 57, sets -1 with their
/// printed form.
#[test]
fn diagnostic_type_refs_cover_tables_and_sets() {
  use shards_core::Type;
  use shards_core::diagnostic::TypeRef;
  let t = TypeRef::of(Type::fixed_table([("x", Type::float())]));
  assert_eq!((t.name.as_str(), t.basic_type), ("{x: Float}", 57));
  let s = TypeRef::of(Type::union([Type::float4(), Type::none()]));
  assert_eq!((s.name.as_str(), s.basic_type), ("Float4 | None", -1));
  assert_eq!(TypeRef::of(Type::float2()).basic_type, 11);
}

/// A variadic parameter collects the remaining positional arguments and
/// cannot be named; only the last parameter may be variadic.
#[test]
fn variadic_parameters_take_the_remaining_positional_arguments() {
  use shards_core::describe::{Forms, ParamDecl, Requirement, check_params};
  use shards_core::shards::data::SEQ_MAKE;
  let decoded = decode(
    &SEQ_MAKE.desc,
    &[
      Arg::pos(ParamValue::Value(Var::Int(1))),
      Arg::pos(ParamValue::Var("x".into())),
    ],
  )
  .unwrap();
  assert_eq!(
    decoded.variadic("Items"),
    [ParamValue::Value(Var::Int(1)), ParamValue::Var("x".into())]
  );
  assert!(
    decode(&SEQ_MAKE.desc, &[])
      .unwrap()
      .variadic("Items")
      .is_empty()
  );

  let err = decode(
    &SEQ_MAKE.desc,
    &[Arg::named("Items", ParamValue::Value(Var::Int(1)))],
  )
  .err()
  .unwrap();
  assert_eq!(err.diagnostic().unwrap().code, "variadic-by-name");

  static NOT_LAST: &[ParamDecl] = &[
    ParamDecl {
      name: "A",
      help: "",
      forms: Forms::LITERAL,
      types: &[],
      requirement: Requirement::Variadic,
      ty: None,
    },
    ParamDecl {
      name: "B",
      help: "",
      forms: Forms::LITERAL,
      types: &[],
      requirement: Requirement::Optional,
      ty: None,
    },
  ];
  assert_eq!(check_params(NOT_LAST), Err(0));
}

/// A parameter can declare a full type (`[Int]`, a table with keys): the
/// decoder checks literals against it and the catalog documents it.
#[test]
fn full_parameter_types_check_literals_and_are_documented() {
  use shards_core::describe::{Forms, ParamDecl, Requirement, ShardDesc};
  use shards_core::{Catalog, ShardType, Type};
  static PARAMS: &[ParamDecl] = &[
    ParamDecl::new(
      "Offsets",
      "",
      Forms::LITERAL.or(Forms::VARIABLE),
      &[TypeName::Seq],
      Requirement::Optional,
    )
    .typed(|| Type::seq(Type::int())),
    ParamDecl::new(
      "Ref",
      "",
      Forms::LITERAL,
      &[TypeName::Table],
      Requirement::Optional,
    )
    .typed(|| Type::fixed_table([("addr", Type::int()), ("guid", Type::seq(Type::int()))])),
  ];
  static TYPED: ShardType = ShardType::new(ShardDesc {
    params: Params::Declared(PARAMS),
    ..ShardDesc::undocumented("Typed", 1)
  });
  let ints = Var::Seq(std::sync::Arc::new(vec![Var::Int(1), Var::Int(2)]));
  assert!(
    decode(
      &TYPED.desc,
      &[Arg::named("Offsets", ParamValue::Value(ints))]
    )
    .is_ok()
  );

  let strings = Var::Seq(std::sync::Arc::new(vec![Var::string("a")]));
  let err = decode(
    &TYPED.desc,
    &[Arg::named("Offsets", ParamValue::Value(strings))],
  )
  .err()
  .unwrap();
  let d = err.diagnostic().unwrap();
  assert_eq!(d.code, "wrong-argument-type");
  assert!(
    d.message.contains("Offsets must be [Int], got [String]"),
    "{}",
    d.message
  );
  assert_eq!(d.expected[0].name, "[Int]");

  let record = Var::table([
    ("addr", Var::Int(1)),
    ("guid", Var::Seq(std::sync::Arc::new(vec![Var::Int(2)]))),
  ]);
  assert!(decode(&TYPED.desc, &[Arg::named("Ref", ParamValue::Value(record))]).is_ok());
  let missing = Var::table([("addr", Var::Int(1))]);
  assert!(
    decode(
      &TYPED.desc,
      &[Arg::named("Ref", ParamValue::Value(missing))]
    )
    .is_err()
  );

  let catalog = Catalog::new(&[&[&TYPED]]).unwrap();
  let json = catalog.describe_json("Typed").unwrap();
  assert!(json.contains("\"type\":\"[Int]\""), "{json}");
  assert!(
    json.contains("\"type\":\"{addr: Int guid: [Int]}\""),
    "{json}"
  );
}
