//! The prototype's shard subset: enough to express the 1.x benchmark's entity
//! wire and the nested suspension acceptance test (design doc §5).
//!
//! Shards that cannot suspend have one implementation for both schedulers
//! ([`leaf`]). Shards that suspend or run nested flows have a stackful
//! implementation ([`stackful`]) and a stackless one
//! ([`crate::stackless::shards`]); their compose logic and compiled types are
//! still shared, here, generic over the scheduler's [`Backend`].
//!
//! [`defs`] has helpers that build [`ShardDef`]s, standing in for the parser.

pub mod async_shard;
pub mod control;
pub mod data;
pub mod leaf;
pub mod math;
pub mod sim;
pub mod stackful;
pub mod values;

use std::cell::RefCell;
use std::sync::Arc;
use std::time::Duration;

use crate::args::Args;
use crate::compose::{Backend, Binding, CompiledWire, ComposeCtx};
use crate::describe::{
  DefaultValue, Forms, InputDesc, OutputDesc, ParamDecl, Params, Requirement, ShardDesc, Targets,
  TypeName,
};
use crate::diagnostic::{Diagnostic, Phase, TypeRef};
use crate::error::{Error, Result};
use crate::flow::CompiledFlow;
use crate::instance::{Frames, InstanceId};
use crate::shard::{Composed, ParamValue, ShardDef, ShardType, Stackful};
use crate::stackless::shards as sl;
use crate::types::{Type, TypeDesc};
use crate::var::Var;
use leaf::leaf_type;

pub static CONST: ShardType = leaf_type::<leaf::Const>();
pub static SET: ShardType = leaf_type::<leaf::Set>();
pub static REF: ShardType = leaf_type::<leaf::Ref>();
pub static UPDATE: ShardType = leaf_type::<leaf::Update>();
pub static GET: ShardType = leaf_type::<leaf::Get>();
pub static INC: ShardType = leaf_type::<leaf::Inc>();
pub static ADD: ShardType = leaf_type::<leaf::Add>();
pub static IS_LESS: ShardType = leaf_type::<leaf::IsLess>();
pub static IS_MORE_EQUAL: ShardType = leaf_type::<leaf::IsMoreEqual>();
pub static WHEN: ShardType = ShardType::new(WHEN_DESC)
  .with_stackful::<stackful::When>()
  .with_stackless::<sl::When>();
pub static IF: ShardType = ShardType::new(control::IF_DESC)
  .with_stackful::<stackful::If>()
  .with_stackless::<sl::If>();
pub static MATCH: ShardType = ShardType::new(control::MATCH_DESC)
  .with_stackful::<stackful::Match>()
  .with_stackless::<sl::Match>();
pub static MAYBE: ShardType = ShardType::new(control::MAYBE_DESC)
  .with_stackful::<stackful::Maybe>()
  .with_stackless::<sl::Maybe>();
pub static ALL: ShardType = ShardType::new(control::ALL_DESC)
  .with_stackful::<stackful::All>()
  .with_stackless::<sl::All>();
pub static ANY: ShardType = ShardType::new(control::ANY_DESC)
  .with_stackful::<stackful::Any>()
  .with_stackless::<sl::Any>();
pub static SUB: ShardType = ShardType::new(SUB_DESC)
  .with_stackful::<stackful::Sub>()
  .with_stackless::<sl::Sub>();
pub static ONCE: ShardType = ShardType::new(ONCE_DESC)
  .with_stackful::<stackful::Once>()
  .with_stackless::<sl::Once>();
pub static REPEAT: ShardType = ShardType::new(REPEAT_DESC)
  .with_stackful::<stackful::Repeat>()
  .with_stackless::<sl::Repeat>();
pub static WHILE: ShardType = ShardType::new(WHILE_DESC)
  .with_stackful::<stackful::While>()
  .with_stackless::<sl::While>();
pub static DO: ShardType = ShardType::new(DO_DESC)
  .with_stackful::<stackful::Do>()
  .with_stackless::<sl::Do>();
pub static PAUSE: ShardType = ShardType::new(PAUSE_DESC)
  .with_stackful::<stackful::Pause>()
  .with_stackless::<sl::Pause>();
pub static SPAWN: ShardType = ShardType::new(SPAWN_DESC)
  .with_stackful::<stackful::Spawn>()
  .with_stackless::<sl::Spawn>();
pub static PROBE: ShardType = leaf_type::<leaf::Probe>();
pub static REQUEST: ShardType = async_shard::async_type::<sim::Request>();

/// Every shard type in this crate, for building a [`crate::Catalog`].
pub static CATALOG: &[&ShardType] = &[
  &CONST,
  &SET,
  &REF,
  &UPDATE,
  &GET,
  &INC,
  &ADD,
  &IS_LESS,
  &IS_MORE_EQUAL,
  &WHEN,
  &IF,
  &MATCH,
  &MAYBE,
  &ALL,
  &ANY,
  &ONCE,
  &SUB,
  &data::TAKE,
  &data::PUSH,
  &data::SEQ_MAKE,
  &data::TABLE_MAKE,
  &data::STRING_FORMAT,
  &math::SUBTRACT,
  &math::MULTIPLY,
  &math::DIVIDE,
  &math::DEC,
  &math::ABS,
  &math::ROUND,
  &math::FLOOR,
  &math::CEIL,
  &math::LENGTH,
  &values::LOG,
  &values::STOP,
  &values::IS,
  &values::IS_NOT,
  &values::IS_MORE,
  &values::IS_LESS_EQUAL,
  &values::IS_ANY,
  &values::PARSE_INT,
  &values::COUNT,
  &values::NOT,
  &values::IS_NONE,
  &values::IS_NOT_NONE,
  &values::TIME_NOW,
  &values::TO_STRING,
  &values::TO_INT,
  &values::TO_FLOAT,
  &values::TO_HEX,
  &values::PARSE_FLOAT,
  &values::TO_FLOAT2,
  &values::TO_FLOAT3,
  &values::TO_FLOAT4,
  &values::EXPECT_INT,
  &values::EXPECT_FLOAT,
  &values::EXPECT_BOOL,
  &values::EXPECT_STRING,
  &values::EXPECT_SEQ,
  &values::EXPECT_TABLE,
  &values::EXPECT_FLOAT2,
  &values::EXPECT_FLOAT3,
  &values::EXPECT_FLOAT4,
  &REPEAT,
  &WHILE,
  &DO,
  &PAUSE,
  &SPAWN,
  &PROBE,
  &REQUEST,
];

// --- helpers ---

/// A parameter declaration (shorthand for the statics below).
const fn decl(
  name: &'static str,
  help: &'static str,
  forms: Forms,
  types: &'static [TypeName],
  requirement: Requirement,
) -> ParamDecl {
  ParamDecl {
    name,
    help,
    forms,
    types,
    requirement,
  }
}

const OPERAND: Forms = Forms::LITERAL.or(Forms::VARIABLE);
const NONE_TYPES: &[TypeName] = &[];
const COMPARABLE: &[TypeName] = &[TypeName::Int, TypeName::Float];

/// A structured compose error about one of a shard's parameters.
fn param_error(
  args: &Args,
  shard: &str,
  param: &str,
  kind: &'static str,
  code: &'static str,
  message: String,
) -> Error {
  Error::Diagnostic(Box::new(
    Diagnostic::new(Phase::Compose, kind, code, message)
      .shard(shard)
      .param(param, Some(args.param_index(param))),
  ))
}

/// The decoded variable name of a declared variable parameter.
fn variable<'a>(args: &'a Args, param: &str) -> &'a str {
  args.variable(param).expect("decoded variable parameter")
}

/// A constant, or a variable binding read at activation (contract §2).
#[derive(Clone, Debug)]
pub enum Operand {
  Const(Var),
  Bound(Binding),
}

impl Operand {
  /// Composes a declared literal-or-variable parameter (already decoded):
  /// the literal, or the variable's binding (an unknown or possibly
  /// uninitialized variable is a structured error naming `shard` and the
  /// parameter). Returns the operand and its type, for the shard to check.
  pub fn compose_arg<B: Backend>(
    args: &Args,
    name: &str,
    shard: &str,
    ctx: &mut ComposeCtx<'_, B>,
  ) -> Result<(Operand, Type)> {
    match args.get(name).expect("decoded required parameter") {
      ParamValue::Value(v) => Ok((Operand::Const(v.clone()), v.type_of())),
      ParamValue::Var(var) => {
        let info = ctx
          .read_var(var, shard)
          .map_err(|e| e.with_param(name, args.param_index(name)))?;
        Ok((Operand::Bound(info.binding), info.ty))
      }
      other => unreachable!("decoder accepted {other:?} for {shard}.{name}"),
    }
  }

  /// The value at activation: the literal, or the variable's current value.
  pub fn get(&self, frames: &impl Frames) -> Var {
    match self {
      Operand::Const(v) => v.clone(),
      Operand::Bound(b) => frames.get(*b),
    }
  }
}

// --- descriptions, shared compose logic and compiled types ---
//
// Each shard has one description: both backends' implementations, the
// argument decoder and the catalog read it. Forms, literal types and
// defaults are enforced by the decoder; compose checks what depends on
// context (bindings, input types, flow outputs, values).

pub static CONST_PARAMS: &[ParamDecl] = &[decl(
  "Value",
  crate::shard_doc!("The constant value: any literal."),
  Forms::LITERAL,
  NONE_TYPES,
  Requirement::Required,
)];

pub const CONST_DESC: ShardDesc = ShardDesc {
  name: "Const",
  version: 1,
  summary: crate::shard_doc!("Outputs a constant value."),
  help: crate::shard_doc!("Ignores its input and outputs Value on every activation."),
  params: Params::Declared(CONST_PARAMS),
  input: InputDesc::Ignored,
  output: OutputDesc::Dynamic(crate::shard_doc!("the type of Value")),
  targets: Targets::All,
  aliases: &[],
};

pub(crate) fn compose_const(args: &Args) -> Result<Composed<Var>> {
  let value = args.literal("Value").expect("decoded Value").clone();
  Ok(Composed {
    output: value.type_of(),
    compiled: value,
  })
}

pub static SET_PARAMS: &[ParamDecl] = &[decl(
  "Variable",
  crate::shard_doc!(
    "The variable to assign. Declared as a mutable local if it does not exist yet."
  ),
  Forms::VARIABLE,
  NONE_TYPES,
  Requirement::Required,
)];

pub const SET_DESC: ShardDesc = ShardDesc {
  name: "Set",
  version: 1,
  summary: crate::shard_doc!("Assigns the input to a mutable variable."),
  help: crate::shard_doc!(
    "Declares a mutable local variable with the input's type if Variable does not exist; otherwise Variable must be mutable and have the input's type. Passes its input through."
  ),
  params: Params::Declared(SET_PARAMS),
  input: InputDesc::Any,
  output: OutputDesc::Passthrough,
  targets: Targets::All,
  aliases: &[],
};

/// `Set`: assigns the input to a mutable variable, declaring a local if needed.
pub(crate) fn compose_set<B: Backend>(
  args: &Args,
  ctx: &mut ComposeCtx<'_, B>,
) -> Result<Composed<Binding>> {
  let name = variable(args, "Variable");
  let input = ctx.input();
  let binding = match ctx.var(name) {
    Some(info) if info.mutable && info.ty.accepts(input) => {
      ctx.mark_initialized(info.binding);
      info.binding
    }
    Some(info) => {
      let (code, message) = if info.mutable {
        (
          "variable-type-mismatch",
          format!("{name} is {}, cannot assign {input}", info.ty),
        )
      } else {
        ("immutable-variable", format!("{name} is immutable"))
      };
      return Err(param_error(
        args,
        "Set",
        "Variable",
        "compose-error",
        code,
        message,
      ));
    }
    None => ctx.declare_local(name, input, true).binding,
  };
  Ok(Composed {
    compiled: binding,
    output: input,
  })
}

pub static REF_PARAMS: &[ParamDecl] = &[decl(
  "Variable",
  crate::shard_doc!("The immutable local variable to declare."),
  Forms::VARIABLE,
  NONE_TYPES,
  Requirement::Required,
)];

pub const REF_DESC: ShardDesc = ShardDesc {
  name: "Ref",
  version: 1,
  summary: crate::shard_doc!("Declares an immutable variable holding the input."),
  help: crate::shard_doc!(
    "Declares an immutable local variable with the input's type. The variable must not exist yet; it cannot be assigned again with Set or Update. Running the same Ref again (in a loop) gives it the new value. Passes its input through. The `= name` operator is a Ref."
  ),
  params: Params::Declared(REF_PARAMS),
  input: InputDesc::Any,
  output: OutputDesc::Passthrough,
  targets: Targets::All,
  aliases: &[],
};

/// `Ref`: declares an immutable local holding the input.
pub(crate) fn compose_ref<B: Backend>(
  args: &Args,
  ctx: &mut ComposeCtx<'_, B>,
) -> Result<Composed<Binding>> {
  let name = variable(args, "Variable");
  let input = ctx.input();
  // `%` names are frontend temporaries (source cannot name them): each
  // occurrence declares a fresh slot, shadowing an earlier one, so a wire
  // inlined twice by Do with different input types still composes.
  if name.starts_with('%') {
    return Ok(Composed {
      compiled: ctx.declare_local(name, input, false).binding,
      output: input,
    });
  }
  if ctx.var(name).is_some() {
    return Err(param_error(
      args,
      "Ref",
      "Variable",
      "compose-error",
      "variable-exists",
      format!(
        "{name} already exists; Ref declares a new variable (use Set or Update to assign a mutable one)"
      ),
    ));
  }
  Ok(Composed {
    compiled: ctx.declare_local(name, input, false).binding,
    output: input,
  })
}

pub static UPDATE_PARAMS: &[ParamDecl] = &[decl(
  "Variable",
  crate::shard_doc!("The existing mutable variable to assign."),
  Forms::VARIABLE,
  NONE_TYPES,
  Requirement::Required,
)];

pub const UPDATE_DESC: ShardDesc = ShardDesc {
  name: "Update",
  version: 1,
  summary: crate::shard_doc!("Assigns the input to an existing mutable variable."),
  help: crate::shard_doc!(
    "Variable must exist, be mutable and have the input's type. Passes its input through."
  ),
  params: Params::Declared(UPDATE_PARAMS),
  input: InputDesc::Any,
  output: OutputDesc::Passthrough,
  targets: Targets::All,
  aliases: &[],
};

/// `Update`: assigns the input to an existing mutable variable.
pub(crate) fn compose_update<B: Backend>(
  args: &Args,
  ctx: &mut ComposeCtx<'_, B>,
) -> Result<Composed<Binding>> {
  let name = variable(args, "Variable");
  let input = ctx.input();
  let Some(info) = ctx.var(name) else {
    return Err(param_error(
      args,
      "Update",
      "Variable",
      "compose-error",
      "unknown-variable",
      format!("unknown variable {name}"),
    ));
  };
  if !info.mutable {
    return Err(param_error(
      args,
      "Update",
      "Variable",
      "compose-error",
      "immutable-variable",
      format!("{name} is immutable"),
    ));
  }
  if !info.ty.accepts(input) {
    return Err(param_error(
      args,
      "Update",
      "Variable",
      "compose-error",
      "variable-type-mismatch",
      format!("{name} is {}, cannot assign {input}", info.ty),
    ));
  }
  ctx.mark_initialized(info.binding);
  Ok(Composed {
    compiled: info.binding,
    output: input,
  })
}

pub static GET_PARAMS: &[ParamDecl] = &[decl(
  "Variable",
  crate::shard_doc!("The variable to read; it must be definitely assigned here."),
  Forms::VARIABLE,
  NONE_TYPES,
  Requirement::Required,
)];

pub const GET_DESC: ShardDesc = ShardDesc {
  name: "Get",
  version: 1,
  summary: crate::shard_doc!("Outputs a variable's value."),
  help: crate::shard_doc!(
    "Ignores its input. Reading a variable that is only assigned in a branch or loop body that might not run is a compose error."
  ),
  params: Params::Declared(GET_PARAMS),
  input: InputDesc::Ignored,
  output: OutputDesc::Dynamic(crate::shard_doc!("the variable's type")),
  targets: Targets::All,
  aliases: &[],
};

pub(crate) fn compose_get<B: Backend>(
  args: &Args,
  ctx: &mut ComposeCtx<'_, B>,
) -> Result<Composed<Binding>> {
  let name = variable(args, "Variable");
  let info = ctx
    .read_var(name, "Get")
    .map_err(|e| e.with_param("Variable", args.param_index("Variable")))?;
  Ok(Composed {
    compiled: info.binding,
    output: info.ty,
  })
}

pub static INC_PARAMS: &[ParamDecl] = &[decl(
  "Variable",
  crate::shard_doc!("The mutable Int variable to increment."),
  Forms::VARIABLE,
  NONE_TYPES,
  Requirement::Required,
)];

pub const INC_DESC: ShardDesc = ShardDesc {
  name: "Math.Inc",
  version: 1,
  summary: crate::shard_doc!("Increments an Int variable and outputs the new value."),
  help: crate::shard_doc!(
    "Ignores its input. Variable must be a mutable Int; overflow is an activation error."
  ),
  params: Params::Declared(INC_PARAMS),
  input: InputDesc::Ignored,
  output: OutputDesc::Fixed(TypeName::Int),
  targets: Targets::All,
  aliases: &["Inc"],
};

/// `Math.Inc`: increments an Int variable and outputs the new value.
pub(crate) fn compose_inc<B: Backend>(
  args: &Args,
  ctx: &mut ComposeCtx<'_, B>,
) -> Result<Composed<Binding>> {
  compose_counter(args, ctx, INC_DESC.name)
}

/// `Math.Inc` and `Math.Dec`: Variable must be a mutable Int.
pub(crate) fn compose_counter<B: Backend>(
  args: &Args,
  ctx: &mut ComposeCtx<'_, B>,
  shard: &'static str,
) -> Result<Composed<Binding>> {
  let name = variable(args, "Variable");
  let info = ctx
    .read_var(name, shard)
    .map_err(|e| e.with_param("Variable", args.param_index("Variable")))?;
  if !info.mutable || info.ty != Type::int() {
    return Err(param_error(
      args,
      shard,
      "Variable",
      "compose-error",
      "wrong-variable-type",
      format!(
        "{name} must be a mutable Int, got {}{}",
        if info.mutable { "" } else { "immutable " },
        info.ty
      ),
    ));
  }
  Ok(Composed {
    compiled: info.binding,
    output: Type::int(),
  })
}

pub(crate) fn activate_inc(binding: Binding, frames: &mut impl Frames) -> Result<Var> {
  let Var::Int(v) = frames.get(binding) else {
    return Err(Error::Activation("Inc: variable is not an Int".into()));
  };
  let next = v
    .checked_add(1)
    .ok_or_else(|| Error::Activation("Inc: integer overflow".into()))?;
  frames.set(binding, Var::Int(next));
  Ok(Var::Int(next))
}

/// `Add`'s parameters. The decoder enforces these declarations and the
/// catalog documents them.
pub static ADD_PARAMS: &[ParamDecl] = &[ParamDecl {
  name: "Operand",
  help: crate::shard_doc!("The value to add: a literal, or a variable read at activation."),
  forms: Forms::LITERAL.or(Forms::VARIABLE),
  types: NUMERIC,
  requirement: Requirement::Required,
}];

const NUMERIC: &[TypeName] = math::ARITHMETIC;

pub const ADD_DESC: ShardDesc = ShardDesc {
  name: "Math.Add",
  version: 1,
  summary: crate::shard_doc!("Adds the operand to the input."),
  help: crate::shard_doc!(
    "Int, Float and the float vectors mix: Int with Float gives a Float, a vector with a number applies to each component, and vectors of one size work per component. Int with Int stays Int; overflow is an activation error."
  ),
  params: Params::Declared(ADD_PARAMS),
  input: InputDesc::Types(NUMERIC),
  output: OutputDesc::Dynamic(crate::shard_doc!(
    "the mixed type of the input and the operand"
  )),
  targets: Targets::All,
  aliases: &["Add"],
};

pub static IS_LESS_PARAMS: &[ParamDecl] = &[decl(
  "Operand",
  crate::shard_doc!(
    "The value to compare the input with: a literal, or a variable read at activation."
  ),
  OPERAND,
  COMPARABLE,
  Requirement::Required,
)];

pub const IS_LESS_DESC: ShardDesc = ShardDesc {
  name: "IsLess",
  version: 1,
  summary: crate::shard_doc!("Outputs whether the input is less than the operand."),
  help: crate::shard_doc!(
    "The input and the operand are Ints or Floats, mixed freely (compared by value). Comparing NaN is an activation error."
  ),
  params: Params::Declared(IS_LESS_PARAMS),
  input: InputDesc::Types(COMPARABLE),
  output: OutputDesc::Fixed(TypeName::Bool),
  targets: Targets::All,
  aliases: &[],
};

pub static IS_MORE_EQUAL_PARAMS: &[ParamDecl] = &[decl(
  "Operand",
  crate::shard_doc!(
    "The value to compare the input with: a literal, or a variable read at activation."
  ),
  OPERAND,
  COMPARABLE,
  Requirement::Required,
)];

pub const IS_MORE_EQUAL_DESC: ShardDesc = ShardDesc {
  name: "IsMoreEqual",
  version: 1,
  summary: crate::shard_doc!("Outputs whether the input is greater than or equal to the operand."),
  help: crate::shard_doc!(
    "The input and the operand are Ints or Floats, mixed freely (compared by value). Comparing NaN is an activation error."
  ),
  params: Params::Declared(IS_MORE_EQUAL_PARAMS),
  input: InputDesc::Types(COMPARABLE),
  output: OutputDesc::Fixed(TypeName::Bool),
  targets: Targets::All,
  aliases: &[],
};

pub(crate) fn compose_compare<B: Backend>(
  args: &Args,
  ctx: &mut ComposeCtx<'_, B>,
  shard: &str,
) -> Result<Composed<Operand>> {
  let (operand, ty) = Operand::compose_arg(args, "Operand", shard, ctx)?;
  let input = ctx.input();
  // Int and Float compare with each other by value.
  let number = |t: Type| matches!(t.desc(), TypeDesc::Int | TypeDesc::Float);
  if !number(input) || !number(ty) {
    let expected = COMPARABLE.iter().copied().map(TypeRef::named).collect();
    return Err(Error::Diagnostic(Box::new(
      Diagnostic::new(
        Phase::Compose,
        "input-type-mismatch",
        "input-type-mismatch",
        format!("cannot compare {input} with {ty}"),
      )
      .shard(shard)
      .param("Operand", Some(args.param_index("Operand")))
      .types(Some(TypeRef::of(input)), expected),
    )));
  }
  Ok(Composed {
    compiled: operand,
    output: Type::bool(),
  })
}

/// Compares an Int with a Float exactly (no rounding of the Int to f64,
/// which would make 2^53 + 1 equal to 2^53). `None` for NaN.
pub(crate) fn cmp_int_float(i: i64, f: f64) -> Option<std::cmp::Ordering> {
  use std::cmp::Ordering;
  // 2^63 is exactly representable; every Float at or beyond it is outside
  // the Int range.
  const LIMIT: f64 = 9_223_372_036_854_775_808.0;
  if f.is_nan() {
    return None;
  }
  if f >= LIMIT {
    return Some(Ordering::Less);
  }
  if f < -LIMIT {
    return Some(Ordering::Greater);
  }
  let whole = f.trunc();
  // In range, so the conversion is exact.
  Some(i.cmp(&(whole as i64)).then_with(|| {
    if f > whole {
      Ordering::Less
    } else if f < whole {
      Ordering::Greater
    } else {
      Ordering::Equal
    }
  }))
}

pub(crate) fn compare(input: &Var, operand: Var) -> Result<std::cmp::Ordering> {
  match (input, operand) {
    (Var::Int(a), Var::Int(b)) => Ok(a.cmp(&b)),
    (Var::Float(a), Var::Float(b)) => a
      .partial_cmp(&b)
      .ok_or_else(|| Error::Activation("cannot compare NaN".into())),
    (Var::Int(a), Var::Float(b)) => {
      cmp_int_float(*a, b).ok_or_else(|| Error::Activation("cannot compare NaN".into()))
    }
    (Var::Float(a), Var::Int(b)) => cmp_int_float(b, *a)
      .map(std::cmp::Ordering::reverse)
      .ok_or_else(|| Error::Activation("cannot compare NaN".into())),
    _ => Err(Error::Activation("comparison type mismatch".into())),
  }
}

/// Compiled form of `When` and `While`: a predicate flow and a body flow.
pub struct Predicated<B: Backend> {
  pub(crate) pred: CompiledFlow<B>,
  pub(crate) body: CompiledFlow<B>,
}

/// Composes a predicate flow (which must output Bool) and a body flow that
/// might not run. Shared by `When` and `While`.
fn compose_predicate_and_body<B: Backend>(
  pred: &[ShardDef],
  body: &[ShardDef],
  ctx: &mut ComposeCtx<'_, B>,
  shard: &str,
) -> Result<Composed<Predicated<B>>> {
  let input = ctx.input();
  let pred = ctx.compose_flow(pred, input)?;
  if !Type::bool().accepts(pred.output) {
    return Err(Error::Diagnostic(Box::new(
      Diagnostic::new(
        Phase::Compose,
        "compose-error",
        "predicate-not-bool",
        format!("predicate must output Bool, got {}", pred.output),
      )
      .shard(shard)
      .param("Predicate", Some(0))
      .types(
        Some(TypeRef::of(pred.output)),
        vec![TypeRef::named(TypeName::Bool)],
      ),
    )));
  }
  // The Action flow might not run (When) or might run zero times (While).
  let body = ctx.compose_flow_conditional(body, input)?;
  Ok(Composed {
    compiled: Predicated { pred, body },
    output: input,
  })
}

pub static WHILE_PARAMS: &[ParamDecl] = &[
  decl(
    "Predicate",
    crate::shard_doc!(
      "A flow that receives the input and must output a Bool; evaluated before each iteration."
    ),
    Forms::FLOW,
    NONE_TYPES,
    Requirement::Required,
  ),
  decl(
    "Action",
    crate::shard_doc!(
      "The flow to run while the predicate is true. It can run zero times, so variables it assigns are not definitely assigned after While."
    ),
    Forms::FLOW,
    NONE_TYPES,
    Requirement::Required,
  ),
];

pub const WHILE_DESC: ShardDesc = ShardDesc {
  name: "While",
  version: 1,
  summary: crate::shard_doc!("Runs an Action flow while a predicate flow outputs true."),
  help: crate::shard_doc!(
    "Both flows receive While's input. On normal completion While passes its input through. A Stop, Restart or Return inside either flow propagates. The Action flow may suspend; the loop resumes where it left off."
  ),
  params: Params::Declared(WHILE_PARAMS),
  input: InputDesc::Any,
  output: OutputDesc::Passthrough,
  targets: Targets::All,
  aliases: &[],
};

pub(crate) fn compose_while<B: Backend>(
  args: &Args,
  ctx: &mut ComposeCtx<'_, B>,
) -> Result<Composed<Predicated<B>>> {
  let pred = args.flow("Predicate").expect("decoded Predicate flow");
  let body = args.flow("Action").expect("decoded Action flow");
  compose_predicate_and_body(pred, body, ctx, "While")
}

/// `When`'s parameters, shared by its stackful and stackless
/// implementations, the decoder and the catalog.
pub static WHEN_PARAMS: &[ParamDecl] = &[
  ParamDecl {
    name: "Predicate",
    help: crate::shard_doc!("A flow that receives the input and must output a Bool."),
    forms: Forms::FLOW,
    types: &[],
    requirement: Requirement::Required,
  },
  ParamDecl {
    name: "Action",
    help: crate::shard_doc!(
      "The flow to run when the predicate is true. Variables it assigns are not definitely assigned after When."
    ),
    forms: Forms::FLOW,
    types: &[],
    requirement: Requirement::Required,
  },
];

pub const WHEN_DESC: ShardDesc = ShardDesc {
  name: "When",
  version: 1,
  summary: crate::shard_doc!("Runs an Action flow if a predicate flow outputs true."),
  help: crate::shard_doc!(
    "Both flows receive When's input. On normal completion When passes its input through, whether or not the Action flow ran. A Stop, Restart or Return inside either flow propagates."
  ),
  params: Params::Declared(WHEN_PARAMS),
  input: InputDesc::Any,
  output: OutputDesc::Passthrough,
  targets: Targets::All,
  aliases: &[],
};

pub(crate) fn compose_when<B: Backend>(
  args: &Args,
  ctx: &mut ComposeCtx<'_, B>,
) -> Result<Composed<Predicated<B>>> {
  // The decoder guarantees both are present flows.
  let pred = args.flow("Predicate").expect("decoded Predicate flow");
  let body = args.flow("Action").expect("decoded Action flow");
  compose_predicate_and_body(pred, body, ctx, "When")
}

pub static ONCE_PARAMS: &[ParamDecl] = &[decl(
  "Action",
  crate::shard_doc!(
    "The flow to run on the first activation. Variables it assigns are definitely assigned after Once."
  ),
  Forms::FLOW,
  NONE_TYPES,
  Requirement::Required,
)];

pub const ONCE_DESC: ShardDesc = ShardDesc {
  name: "Once",
  version: 1,
  summary: crate::shard_doc!("Runs an Action flow on the first activation only."),
  help: crate::shard_doc!(
    "The Action flow receives Once's input. Once passes its input through. If the Action flow suspends during the first run, it resumes there; it never runs again after completing. If it fails (an error caught by Maybe), it runs again on the next activation. A Stop, Restart or Return inside the Action flow propagates."
  ),
  params: Params::Declared(ONCE_PARAMS),
  input: InputDesc::Any,
  output: OutputDesc::Passthrough,
  targets: Targets::All,
  aliases: &[],
};

pub(crate) fn compose_once<B: Backend>(
  args: &Args,
  ctx: &mut ComposeCtx<'_, B>,
) -> Result<Composed<CompiledFlow<B>>> {
  let input = ctx.input();
  Ok(Composed {
    compiled: ctx.compose_flow(args.flow("Action").expect("decoded Action flow"), input)?,
    output: input,
  })
}

pub static SUBFLOW_PARAMS: &[ParamDecl] = &[decl(
  "Action",
  crate::shard_doc!(
    "The flow to run on every activation. Variables it assigns are definitely assigned after SubFlow."
  ),
  Forms::FLOW,
  NONE_TYPES,
  Requirement::Required,
)];

pub const SUB_DESC: ShardDesc = ShardDesc {
  name: "SubFlow",
  version: 1,
  summary: crate::shard_doc!("Runs an Action flow and passes its input through."),
  help: crate::shard_doc!(
    "The Action flow receives SubFlow's input; its output is discarded (1.x `_SubFlow`; `Sub` is 1.x's alias for Math.Subtract). A Stop, Restart or Return inside the Action flow propagates; a suspension resumes inside it. The frontend computes parameter values and computed elements in a SubFlow before the shard that uses them."
  ),
  params: Params::Declared(SUBFLOW_PARAMS),
  input: InputDesc::Any,
  output: OutputDesc::Passthrough,
  targets: Targets::All,
  aliases: &[],
};

/// `SubFlow` composes like `Once`: the flow always runs, in the caller's frame.
pub(crate) fn compose_sub<B: Backend>(
  args: &Args,
  ctx: &mut ComposeCtx<'_, B>,
) -> Result<Composed<CompiledFlow<B>>> {
  compose_once(args, ctx)
}

pub struct RepeatCompiled<B: Backend> {
  pub(crate) body: CompiledFlow<B>,
  /// `None`: no limit (Forever, or Until alone).
  pub(crate) times: Option<Operand>,
  /// Checked before each iteration; true stops the repeat.
  pub(crate) until: Option<CompiledFlow<B>>,
}

impl<B: Backend> RepeatCompiled<B> {
  /// The iteration limit for this run, read when the repeat starts.
  pub(crate) fn limit(&self, frames: &impl Frames) -> Result<Option<i64>> {
    match &self.times {
      None => Ok(None),
      Some(op) => match op.get(frames) {
        Var::Int(t) => Ok(Some(t.max(0))),
        _ => Err(Error::Activation("Repeat: times is not an Int".into())),
      },
    }
  }
}

pub static REPEAT_PARAMS: &[ParamDecl] = &[
  decl(
    "Action",
    crate::shard_doc!(
      "The flow to repeat. It can run zero times, so variables it assigns are not definitely assigned after Repeat."
    ),
    Forms::FLOW,
    NONE_TYPES,
    Requirement::Required,
  ),
  decl(
    "Times",
    crate::shard_doc!(
      "How many times to run the Action flow at most: an Int literal, or an Int variable read when the repeat starts. Negative counts run it zero times."
    ),
    OPERAND,
    &[TypeName::Int],
    Requirement::Optional,
  ),
  decl(
    "Forever",
    crate::shard_doc!("Repeat without a limit (Times is ignored); Until can still end it."),
    Forms::LITERAL,
    &[TypeName::Bool],
    Requirement::Default(DefaultValue::Bool(false)),
  ),
  decl(
    "Until",
    crate::shard_doc!(
      "A flow that receives the input and outputs a Bool, checked before each iteration: true ends the repeat."
    ),
    Forms::FLOW,
    NONE_TYPES,
    Requirement::Optional,
  ),
];

pub const REPEAT_DESC: ShardDesc = ShardDesc {
  name: "Repeat",
  version: 1,
  summary: crate::shard_doc!(
    "Runs an Action flow a number of times, until a condition, or forever."
  ),
  help: crate::shard_doc!(
    "Needs Times, Until or Forever. Before each iteration, Until (if given) is checked and ends the repeat when true; Times limits the iterations unless Forever. The Action and Until flows receive Repeat's input. Repeat passes its input through. A Stop, Restart or Return inside a flow propagates. The flows may suspend; the repeat resumes where it left off."
  ),
  params: Params::Declared(REPEAT_PARAMS),
  input: InputDesc::Any,
  output: OutputDesc::Passthrough,
  targets: Targets::All,
  aliases: &[],
};

pub(crate) fn compose_repeat<B: Backend>(
  args: &Args,
  ctx: &mut ComposeCtx<'_, B>,
) -> Result<Composed<RepeatCompiled<B>>> {
  let input = ctx.input();
  // Until runs before each iteration, then Action: one region that might
  // not run (Times 0), composed in run order, so a variable Until assigns is
  // usable in Action but not after the Repeat.
  let (until, body) = ctx.conditional_region(|ctx| {
    let until = match args.flow("Until") {
      Some(flow) => {
        let until = ctx.compose_flow(flow, input)?;
        if !Type::bool().accepts(until.output) {
          return Err(param_error(
            args,
            "Repeat",
            "Until",
            "compose-error",
            "predicate-not-bool",
            format!("Until must output Bool, got {}", until.output),
          ));
        }
        Some(until)
      }
      None => None,
    };
    let body = ctx.compose_flow(args.flow("Action").expect("decoded Action flow"), input)?;
    Ok((until, body))
  })?;
  let forever = args.bool("Forever").unwrap_or(false);
  let times = match args.get("Times") {
    Some(_) if !forever => {
      let (times, ty) = Operand::compose_arg(args, "Times", "Repeat", ctx)?;
      // A literal is checked by the decoder; a variable's binding here.
      if ty != Type::int() {
        return Err(param_error(
          args,
          "Repeat",
          "Times",
          "compose-error",
          "wrong-variable-type",
          format!("Times must be an Int, got {ty}"),
        ));
      }
      Some(times)
    }
    _ => None,
  };
  if times.is_none() && until.is_none() && !forever {
    return Err(param_error(
      args,
      "Repeat",
      "Times",
      "compose-error",
      "missing-argument",
      "Repeat needs Times, Until or Forever".into(),
    ));
  }
  Ok(Composed {
    compiled: RepeatCompiled { body, times, until },
    output: input,
  })
}

pub static DO_PARAMS: &[ParamDecl] = &[decl(
  "Wire",
  crate::shard_doc!("The wire to run inline. It shares the caller's variables."),
  Forms::WIRE,
  NONE_TYPES,
  Requirement::Required,
)];

pub const DO_DESC: ShardDesc = ShardDesc {
  name: "Do",
  version: 1,
  summary: crate::shard_doc!("Runs another wire inline, sharing the caller's variables."),
  help: crate::shard_doc!(
    "The wire receives Do's input and is composed into the caller's frame. A Return inside it ends only that wire, and its value becomes Do's output. A Stop or Restart propagates. A wire cannot run itself, directly or indirectly."
  ),
  params: Params::Declared(DO_PARAMS),
  input: InputDesc::Any,
  output: OutputDesc::Dynamic(crate::shard_doc!("the wire's output")),
  targets: Targets::All,
  aliases: &[],
};

/// Attributes an error from resolving a `Wire` argument to the calling shard
/// and its `Wire` parameter, but only when the error is about the reference
/// itself. Diagnostics from shards inside the called wire keep their owner.
fn wire_ref_error(e: Error, args: &Args, shard: &str) -> Error {
  match e.diagnostic() {
    Some(d) if matches!(d.code, "unknown-wire" | "recursive-wire") => e
      .in_shard(shard)
      .with_param("Wire", args.param_index("Wire")),
    _ => e,
  }
}

pub(crate) fn compose_do<B: Backend>(
  args: &Args,
  ctx: &mut ComposeCtx<'_, B>,
) -> Result<Composed<CompiledFlow<B>>> {
  let name = args.wire("Wire").expect("decoded Wire");
  let input = ctx.input();
  let flow = ctx
    .compose_inline(name, input)
    .map_err(|e| wire_ref_error(e, args, "Do"))?;
  let output = flow.output;
  Ok(Composed {
    compiled: flow,
    output,
  })
}

pub static PAUSE_PARAMS: &[ParamDecl] = &[decl(
  "Seconds",
  crate::shard_doc!(
    "The minimum time to wait, in seconds; 0 waits for the next tick. Must be finite, not negative and representable as a duration."
  ),
  Forms::LITERAL,
  &[TypeName::Float],
  Requirement::Default(DefaultValue::Float(0.0)),
)];

pub const PAUSE_DESC: ShardDesc = ShardDesc {
  name: "Pause",
  version: 1,
  summary: crate::shard_doc!("Suspends until a later tick."),
  help: crate::shard_doc!(
    "Always suspends at least once, then resumes on the first tick at least Seconds after it started waiting. Passes its input through."
  ),
  params: Params::Declared(PAUSE_PARAMS),
  input: InputDesc::Any,
  output: OutputDesc::Passthrough,
  targets: Targets::All,
  aliases: &[],
};

pub(crate) fn compose_pause<B: Backend>(
  args: &Args,
  ctx: &mut ComposeCtx<'_, B>,
) -> Result<Composed<Duration>> {
  let Some(Var::Float(secs)) = args.literal("Seconds") else {
    unreachable!("decoded Seconds")
  };
  // Rejects negative, NaN, infinite and too large values.
  let Ok(duration) = Duration::try_from_secs_f64(*secs) else {
    return Err(param_error(
      args,
      "Pause",
      "Seconds",
      "compose-error",
      "invalid-argument-value",
      format!("Seconds must be finite, not negative and representable as a duration, got {secs}"),
    ));
  };
  Ok(Composed {
    compiled: duration,
    output: ctx.input(),
  })
}

pub static SPAWN_PARAMS: &[ParamDecl] = &[decl(
  "Wire",
  crate::shard_doc!("The wire to start a new instance of."),
  Forms::WIRE,
  NONE_TYPES,
  Requirement::Required,
)];

pub const SPAWN_DESC: ShardDesc = ShardDesc {
  name: "Spawn",
  version: 1,
  summary: crate::shard_doc!("Starts a new instance of a wire."),
  help: crate::shard_doc!(
    "The new instance receives Spawn's input and starts on the next tick, with its own variables. The wire is compiled once, through the compose cache, and every spawned instance shares the compiled wire. Passes its input through."
  ),
  params: Params::Declared(SPAWN_PARAMS),
  input: InputDesc::Any,
  output: OutputDesc::Passthrough,
  targets: Targets::All,
  aliases: &[],
};

pub(crate) fn compose_spawn<B: Backend>(
  args: &Args,
  ctx: &mut ComposeCtx<'_, B>,
) -> Result<Composed<Arc<CompiledWire<B>>>> {
  let name = args.wire("Wire").expect("decoded Wire");
  let input = ctx.input();
  Ok(Composed {
    compiled: ctx
      .compose_wire(name, input)
      .map_err(|e| wire_ref_error(e, args, "Spawn"))?,
    output: input,
  })
}

// --- test instrumentation ---

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeEventKind {
  Instantiate,
  Activate,
  Cleanup,
  /// A simulated request was aborted because its future was dropped.
  Aborted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeEvent {
  pub tag: String,
  pub instance: InstanceId,
  pub kind: ProbeEventKind,
}

thread_local! {
  static PROBE_EVENTS: RefCell<Vec<ProbeEvent>> = const { RefCell::new(Vec::new()) };
}

/// Returns and clears the probe events recorded on this thread.
pub fn take_probe_events() -> Vec<ProbeEvent> {
  PROBE_EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()))
}

pub(crate) fn record(tag: &str, instance: InstanceId, kind: ProbeEventKind) {
  PROBE_EVENTS.with(|events| {
    events.borrow_mut().push(ProbeEvent {
      tag: tag.to_string(),
      instance,
      kind,
    })
  });
}

/// Failure modes for `Probe`, selected by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeMode {
  Normal,
  /// `"fail-instantiate"`: instantiate returns an error.
  FailInstantiate,
  /// `"panic-instantiate"`: instantiate panics.
  PanicInstantiate,
  /// `"panic-activate"`: activate panics.
  PanicActivate,
  /// `"panic-cleanup"`: cleanup panics (after recording its event).
  PanicCleanup,
}

pub struct ProbeCompiled {
  pub(crate) tag: Arc<str>,
  pub(crate) mode: ProbeMode,
}

pub static PROBE_PARAMS: &[ParamDecl] = &[
  decl(
    "Tag",
    crate::shard_doc!("The tag recorded with each event."),
    Forms::LITERAL,
    &[TypeName::String],
    Requirement::Required,
  ),
  decl(
    "Mode",
    crate::shard_doc!(
      "A failure mode: fail-instantiate, panic-instantiate, panic-activate or panic-cleanup. Absent: normal."
    ),
    Forms::LITERAL,
    &[TypeName::String],
    Requirement::Optional,
  ),
];

pub const PROBE_DESC: ShardDesc = ShardDesc {
  name: "Probe",
  version: 1,
  summary: crate::shard_doc!("Test instrumentation: records its lifecycle events."),
  help: crate::shard_doc!(
    "Records instantiate, activate and cleanup events for tests, and can fail in a chosen way. Passes its input through."
  ),
  params: Params::Declared(PROBE_PARAMS),
  input: InputDesc::Any,
  output: OutputDesc::Passthrough,
  targets: Targets::All,
  aliases: &[],
};

/// `Probe`: records its instantiate, activate and cleanup events (for tests).
pub(crate) fn compose_probe<B: Backend>(
  args: &Args,
  ctx: &mut ComposeCtx<'_, B>,
) -> Result<Composed<ProbeCompiled>> {
  let tag: Arc<str> = Arc::from(args.string("Tag").expect("decoded Tag"));
  let mode = match args.string("Mode") {
    None => ProbeMode::Normal,
    Some("fail-instantiate") => ProbeMode::FailInstantiate,
    Some("panic-instantiate") => ProbeMode::PanicInstantiate,
    Some("panic-activate") => ProbeMode::PanicActivate,
    Some("panic-cleanup") => ProbeMode::PanicCleanup,
    Some(other) => {
      return Err(param_error(
        args,
        "Probe",
        "Mode",
        "compose-error",
        "invalid-argument-value",
        format!("unknown mode {other}"),
      ));
    }
  };
  Ok(Composed {
    compiled: ProbeCompiled { tag, mode },
    output: ctx.input(),
  })
}

pub(crate) fn probe_instantiate(c: &ProbeCompiled, instance: InstanceId) -> Result<InstanceId> {
  match c.mode {
    ProbeMode::FailInstantiate => {
      return Err(Error::Activation(format!(
        "probe {} failed to instantiate",
        c.tag
      )));
    }
    ProbeMode::PanicInstantiate => panic!("probe {} panicked in instantiate", c.tag),
    _ => {}
  }
  record(&c.tag, instance, ProbeEventKind::Instantiate);
  Ok(instance)
}

pub(crate) fn probe_activate(c: &ProbeCompiled, instance: InstanceId) {
  record(&c.tag, instance, ProbeEventKind::Activate);
  if c.mode == ProbeMode::PanicActivate {
    panic!("probe {} panicked in activate", c.tag);
  }
}

pub(crate) fn probe_cleanup(c: &ProbeCompiled, instance: InstanceId) {
  record(&c.tag, instance, ProbeEventKind::Cleanup);
  if c.mode == ProbeMode::PanicCleanup {
    panic!("probe {} panicked in cleanup", c.tag);
  }
}

/// Helpers that build shard definitions, standing in for the parser. The
/// same definitions run on either scheduler.
pub mod defs {
  use super::*;
  use crate::args::Arg;

  pub fn val(v: Var) -> ParamValue {
    ParamValue::Value(v)
  }
  pub fn var(name: &str) -> ParamValue {
    ParamValue::Var(name.to_string())
  }

  pub fn konst(v: Var) -> ShardDef {
    ShardDef::new(&CONST, vec![val(v)])
  }
  pub fn ref_(name: &str) -> ShardDef {
    ShardDef::new(&REF, vec![var(name)])
  }
  pub fn set(name: &str) -> ShardDef {
    ShardDef::new(&SET, vec![var(name)])
  }
  pub fn update(name: &str) -> ShardDef {
    ShardDef::new(&UPDATE, vec![var(name)])
  }
  pub fn get(name: &str) -> ShardDef {
    ShardDef::new(&GET, vec![var(name)])
  }
  pub fn inc(name: &str) -> ShardDef {
    ShardDef::new(&INC, vec![var(name)])
  }
  pub fn add(operand: ParamValue) -> ShardDef {
    ShardDef::new(&ADD, vec![operand])
  }
  pub fn is_less(operand: ParamValue) -> ShardDef {
    ShardDef::new(&IS_LESS, vec![operand])
  }
  pub fn is_more_equal(operand: ParamValue) -> ShardDef {
    ShardDef::new(&IS_MORE_EQUAL, vec![operand])
  }
  pub fn when(pred: Vec<ShardDef>, body: Vec<ShardDef>) -> ShardDef {
    ShardDef::new(&WHEN, vec![ParamValue::Flow(pred), ParamValue::Flow(body)])
  }
  pub fn while_(pred: Vec<ShardDef>, body: Vec<ShardDef>) -> ShardDef {
    ShardDef::new(&WHILE, vec![ParamValue::Flow(pred), ParamValue::Flow(body)])
  }
  pub fn if_(pred: Vec<ShardDef>, then: Vec<ShardDef>, els: Option<Vec<ShardDef>>) -> ShardDef {
    let mut params = vec![ParamValue::Flow(pred), ParamValue::Flow(then)];
    params.extend(els.map(ParamValue::Flow));
    ShardDef::new(&IF, params)
  }
  pub fn match_(cases: Vec<(Var, Vec<ShardDef>)>) -> ShardDef {
    ShardDef::new(&MATCH, vec![ParamValue::Cases(cases)])
  }
  pub fn maybe(action: Vec<ShardDef>, els: Option<Vec<ShardDef>>) -> ShardDef {
    let mut args = vec![Arg::pos(ParamValue::Flow(action))];
    args.extend(els.map(|e| Arg::pos(ParamValue::Flow(e))));
    args.push(Arg::named("Silent", val(Var::Bool(true))));
    ShardDef::with_args(&MAYBE, args)
  }
  pub fn all(conditions: Vec<ParamValue>) -> ShardDef {
    ShardDef::new(&ALL, conditions)
  }
  pub fn repeat_until(body: Vec<ShardDef>, until: Vec<ShardDef>) -> ShardDef {
    ShardDef::with_args(
      &REPEAT,
      vec![
        Arg::pos(ParamValue::Flow(body)),
        Arg::named("Until", ParamValue::Flow(until)),
      ],
    )
  }
  pub fn stop() -> ShardDef {
    ShardDef::new(&super::values::STOP, vec![])
  }
  pub fn sub(body: Vec<ShardDef>) -> ShardDef {
    ShardDef::new(&SUB, vec![ParamValue::Flow(body)])
  }
  pub fn take(key: ParamValue) -> ShardDef {
    ShardDef::new(&super::data::TAKE, vec![key])
  }
  pub fn push(name: &str) -> ShardDef {
    ShardDef::new(&super::data::PUSH, vec![var(name)])
  }
  pub fn once(body: Vec<ShardDef>) -> ShardDef {
    ShardDef::new(&ONCE, vec![ParamValue::Flow(body)])
  }
  pub fn repeat(body: Vec<ShardDef>, times: ParamValue) -> ShardDef {
    ShardDef::new(&REPEAT, vec![ParamValue::Flow(body), times])
  }
  pub fn do_(wire: &str) -> ShardDef {
    ShardDef::new(&DO, vec![ParamValue::Wire(wire.to_string())])
  }
  pub fn pause() -> ShardDef {
    ShardDef::new(&PAUSE, vec![])
  }
  pub fn pause_secs(secs: f64) -> ShardDef {
    ShardDef::new(&PAUSE, vec![val(Var::Float(secs))])
  }
  pub fn spawn(wire: &str) -> ShardDef {
    ShardDef::new(&SPAWN, vec![ParamValue::Wire(wire.to_string())])
  }
  pub fn probe(tag: &str) -> ShardDef {
    ShardDef::new(&PROBE, vec![val(Var::string(tag))])
  }
  /// A simulated async request, see [`sim::Request`].
  pub fn request(delay: i64, fail: bool, abortable: bool) -> ShardDef {
    ShardDef::new(
      &REQUEST,
      vec![
        val(Var::Int(delay)),
        val(Var::Bool(fail)),
        val(Var::Bool(abortable)),
      ],
    )
  }
  /// A probe with a failure mode: `"fail-instantiate"`, `"panic-instantiate"`,
  /// `"panic-activate"` or `"panic-cleanup"`.
  pub fn probe_mode(tag: &str, mode: &str) -> ShardDef {
    ShardDef::new(&PROBE, vec![val(Var::string(tag)), val(Var::string(mode))])
  }
}
