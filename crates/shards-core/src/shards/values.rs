//! Shards over single values: `Log`, `Stop`, `Count`, `Not`, comparisons
//! (`Is`, `IsNot`, `IsMore`, `IsLessEqual`, `IsAny`, `IsNone`,
//! `IsNotNone`), conversions (`ToString`, `ToInt`, `ToFloat`, `ToHex`,
//! `ToFloat2/3/4`, `ParseInt`, `ParseFloat`), type narrowing (`Expect*`)
//! and `Time.Now`. All leaf shards.

use std::sync::{Arc, OnceLock};
use std::time::Instant;

use super::leaf::{LeafShard, leaf_type};
use super::*;
use crate::instance::{InstanceCtx, LeafCtx};
use crate::shard::Flow;

pub static LOG: ShardType = leaf_type::<Log>();
pub static STOP: ShardType = leaf_type::<Stop>();
pub static IS: ShardType = leaf_type::<Equality<IsSpec>>();
pub static IS_NOT: ShardType = leaf_type::<Equality<IsNotSpec>>();
pub static IS_MORE: ShardType = leaf_type::<Ordered<IsMoreSpec>>();
pub static IS_LESS_EQUAL: ShardType = leaf_type::<Ordered<IsLessEqualSpec>>();
pub static IS_ANY: ShardType = leaf_type::<IsAny>();
pub static PARSE_INT: ShardType = leaf_type::<ParseInt>();
pub static COUNT: ShardType = leaf_type::<Pure<CountOp>>();
pub static NOT: ShardType = leaf_type::<Pure<NotOp>>();
pub static IS_NONE: ShardType = leaf_type::<Pure<IsNoneOp>>();
pub static IS_NOT_NONE: ShardType = leaf_type::<Pure<IsNotNoneOp>>();
pub static TIME_NOW: ShardType = leaf_type::<Pure<TimeNowOp>>();
pub static TO_STRING: ShardType = leaf_type::<Pure<ToStringOp>>();
pub static TO_INT: ShardType = leaf_type::<Pure<ToIntOp>>();
pub static TO_FLOAT: ShardType = leaf_type::<Pure<ToFloatOp>>();
pub static TO_HEX: ShardType = leaf_type::<Pure<ToHexOp>>();
pub static PARSE_FLOAT: ShardType = leaf_type::<Pure<ParseFloatOp>>();
pub static TO_FLOAT2: ShardType = leaf_type::<Pure<ToVector<2>>>();
pub static TO_FLOAT3: ShardType = leaf_type::<Pure<ToVector<3>>>();
pub static TO_FLOAT4: ShardType = leaf_type::<Pure<ToVector<4>>>();
pub static EXPECT_INT: ShardType = leaf_type::<Pure<Expect<ExpectIntK>>>();
pub static EXPECT_FLOAT: ShardType = leaf_type::<Pure<Expect<ExpectFloatK>>>();
pub static EXPECT_BOOL: ShardType = leaf_type::<Pure<Expect<ExpectBoolK>>>();
pub static EXPECT_STRING: ShardType = leaf_type::<Pure<Expect<ExpectStringK>>>();
pub static EXPECT_SEQ: ShardType = leaf_type::<Pure<Expect<ExpectSeqK>>>();
pub static EXPECT_TABLE: ShardType = leaf_type::<Pure<Expect<ExpectTableK>>>();
pub static EXPECT_FLOAT2: ShardType = leaf_type::<Pure<Expect<ExpectFloat2K>>>();
pub static EXPECT_FLOAT3: ShardType = leaf_type::<Pure<Expect<ExpectFloat3K>>>();
pub static EXPECT_FLOAT4: ShardType = leaf_type::<Pure<Expect<ExpectFloat4K>>>();

fn mismatch(name: &str, input: Type, expected: &[TypeName]) -> Error {
  let names: Vec<&str> = expected.iter().map(|t| t.name()).collect();
  Error::Diagnostic(Box::new(
    Diagnostic::new(
      Phase::Compose,
      "input-type-mismatch",
      "input-type-mismatch",
      format!("{name} needs {} input, got {input}", names.join(" or ")),
    )
    .shard(name)
    .types(
      Some(TypeRef::of(input)),
      expected.iter().copied().map(TypeRef::named).collect(),
    ),
  ))
}

/// The language's equality (`Is`): floats compare by value (unlike the
/// bitwise identity `Var` implements for cache keys), containers deeply.
pub fn values_equal(a: &Var, b: &Var) -> bool {
  match (a, b) {
    (Var::Float(x), Var::Float(y)) => x == y,
    // Numbers compare by value across Int and Float.
    (Var::Int(x), Var::Float(y)) | (Var::Float(y), Var::Int(x)) => {
      cmp_int_float(*x, *y) == Some(std::cmp::Ordering::Equal)
    }
    (Var::Float2(x), Var::Float2(y)) => x == y,
    (Var::Float3(x), Var::Float3(y)) => x == y,
    (Var::Float4(x), Var::Float4(y)) => x == y,
    (Var::Seq(x), Var::Seq(y)) => {
      x.len() == y.len() && x.iter().zip(y.iter()).all(|(a, b)| values_equal(a, b))
    }
    (Var::Table(x), Var::Table(y)) => {
      x.len() == y.len()
        && x
          .iter()
          .all(|(k, v)| y.get(k).is_some_and(|w| values_equal(v, w)))
    }
    _ => a == b,
  }
}

/// Whether values of these types can ever be equal under [`values_equal`]:
/// numbers mix, and sequences, tables and unions are compared member by
/// member (`[Int]` can equal `[Float]`).
pub(crate) fn comparable(a: Type, b: Type) -> bool {
  if a == b {
    return true;
  }
  let number = |d: &TypeDesc| matches!(d, TypeDesc::Int | TypeDesc::Float);
  match (a.desc(), b.desc()) {
    (TypeDesc::Any, _) | (_, TypeDesc::Any) => true,
    (TypeDesc::Union(members), _) => members.iter().any(|m| comparable(*m, b)),
    (_, TypeDesc::Union(members)) => members.iter().any(|m| comparable(a, *m)),
    (x, y) if number(x) && number(y) => true,
    (TypeDesc::Seq(x), TypeDesc::Seq(y)) => comparable(*x, *y),
    (TypeDesc::Table(x), TypeDesc::Table(y)) => {
      let common = x.keys.iter().all(|(k, t)| match y.get(k) {
        Some(u) => comparable(*t, u),
        // A fixed table lacking the key can never equal one that has it.
        None => y.rest.is_some(),
      });
      let other = y.keys.iter().all(|(k, _)| x.get(k).is_some());
      common && other
    }
    _ => false,
  }
}

// --- Log ---

pub static LOG_PARAMS: &[ParamDecl] = &[decl(
  "Prefix",
  crate::shard_doc!("Text written before the value, as `prefix: value`."),
  Forms::LITERAL,
  &[TypeName::String],
  Requirement::Optional,
)];

pub const LOG_DESC: ShardDesc = ShardDesc {
  name: "Log",
  version: 1,
  summary: crate::shard_doc!("Writes the input to the log and passes it through."),
  help: crate::shard_doc!(
    "Values print as in 1.x: strings as they are, whole floats without `.0`. With a Prefix the line is `prefix: value`."
  ),
  params: Params::Declared(LOG_PARAMS),
  input: InputDesc::Any,
  output: OutputDesc::Passthrough,
  targets: Targets::All,
  aliases: &[],
};

pub struct Log;

impl LeafShard for Log {
  type Compiled = Option<Arc<str>>;
  type State = ();
  const DESC: ShardDesc = LOG_DESC;

  fn compose<B: Backend>(
    args: &Args,
    ctx: &mut ComposeCtx<'_, B>,
  ) -> Result<Composed<Self::Compiled>> {
    Ok(Composed {
      compiled: args.string("Prefix").map(Arc::from),
      output: ctx.input(),
    })
  }

  fn instantiate(_: &Self::Compiled, _: &mut InstanceCtx) -> Result<()> {
    Ok(())
  }

  fn activate(
    prefix: &Self::Compiled,
    _: &mut (),
    _: &mut impl LeafCtx,
    input: &Var,
  ) -> Result<Flow> {
    let value = input.text();
    crate::log::emit(match prefix {
      Some(p) => format!("{p}: {value}"),
      None => value,
    });
    Ok(Flow::Next(input.clone()))
  }
}

// --- Stop ---

pub const STOP_DESC: ShardDesc = ShardDesc {
  name: "Stop",
  version: 1,
  summary: crate::shard_doc!("Ends the instance."),
  help: crate::shard_doc!(
    "Stops the wire's instance (a looped wire does not restart); the instance's outcome is Stopped. Nothing after it runs."
  ),
  params: Params::Declared(&[]),
  input: InputDesc::Any,
  output: OutputDesc::Dynamic(crate::shard_doc!(
    "none: nothing after it runs (type Never)"
  )),
  targets: Targets::All,
  aliases: &[],
};

pub struct Stop;

impl LeafShard for Stop {
  type Compiled = ();
  type State = ();
  const DESC: ShardDesc = STOP_DESC;

  fn compose<B: Backend>(_: &Args, _: &mut ComposeCtx<'_, B>) -> Result<Composed<()>> {
    // Never produces a value: a flow ending in Stop fits any expected type.
    Ok(Composed {
      compiled: (),
      output: Type::never(),
    })
  }

  fn instantiate(_: &(), _: &mut InstanceCtx) -> Result<()> {
    Ok(())
  }

  fn activate(_: &(), _: &mut (), _: &mut impl LeafCtx, _: &Var) -> Result<Flow> {
    Ok(Flow::Stop)
  }
}

// --- Is, IsNot ---

pub static EQUALITY_PARAMS: &[ParamDecl] = &[decl(
  "Operand",
  crate::shard_doc!("The value to compare with: a literal, or a variable read at activation."),
  OPERAND,
  &[],
  Requirement::Required,
)];

pub trait EqualitySpec: 'static {
  const DESC: ShardDesc;
  const EQUAL: bool;
}

pub struct IsSpec;
pub struct IsNotSpec;

impl EqualitySpec for IsSpec {
  const DESC: ShardDesc = ShardDesc {
    name: "Is",
    version: 1,
    summary: crate::shard_doc!("Outputs whether the input equals the operand."),
    help: crate::shard_doc!(
      "Numbers compare by value (1 Is 1.0), sequences and tables element by element. The types must be able to match."
    ),
    params: Params::Declared(EQUALITY_PARAMS),
    input: InputDesc::Any,
    output: OutputDesc::Fixed(TypeName::Bool),
    targets: Targets::All,
    aliases: &[],
  };
  const EQUAL: bool = true;
}

impl EqualitySpec for IsNotSpec {
  const DESC: ShardDesc = ShardDesc {
    name: "IsNot",
    version: 1,
    summary: crate::shard_doc!("Outputs whether the input differs from the operand."),
    help: crate::shard_doc!("The negation of Is, with the same type rules."),
    params: Params::Declared(EQUALITY_PARAMS),
    input: InputDesc::Any,
    output: OutputDesc::Fixed(TypeName::Bool),
    targets: Targets::All,
    aliases: &[],
  };
  const EQUAL: bool = false;
}

pub struct Equality<S>(std::marker::PhantomData<S>);

impl<S: EqualitySpec> LeafShard for Equality<S> {
  type Compiled = Operand;
  type State = ();
  const DESC: ShardDesc = S::DESC;

  fn compose<B: Backend>(args: &Args, ctx: &mut ComposeCtx<'_, B>) -> Result<Composed<Operand>> {
    let (operand, ty) = Operand::compose_arg(args, "Operand", S::DESC.name, ctx)?;
    let input = ctx.input();
    if !comparable(input, ty) {
      return Err(Error::Diagnostic(Box::new(
        Diagnostic::new(
          Phase::Compose,
          "input-type-mismatch",
          "input-type-mismatch",
          format!("{input} and {ty} can never be equal"),
        )
        .shard(S::DESC.name)
        .param("Operand", Some(0))
        .types(Some(TypeRef::of(input)), vec![TypeRef::of(ty)]),
      )));
    }
    Ok(Composed {
      compiled: operand,
      output: Type::bool(),
    })
  }

  fn instantiate(_: &Operand, _: &mut InstanceCtx) -> Result<()> {
    Ok(())
  }

  fn activate(op: &Operand, _: &mut (), ctx: &mut impl LeafCtx, input: &Var) -> Result<Flow> {
    Ok(Flow::Next(Var::Bool(
      values_equal(input, &op.get(ctx)) == S::EQUAL,
    )))
  }
}

// --- IsMore, IsLessEqual ---

pub static ORDERED_PARAMS: &[ParamDecl] = &[decl(
  "Operand",
  crate::shard_doc!(
    "The value to compare the input with: a literal, or a variable read at activation."
  ),
  OPERAND,
  COMPARABLE,
  Requirement::Required,
)];

pub trait OrderedSpec: 'static {
  const DESC: ShardDesc;
  fn holds(o: std::cmp::Ordering) -> bool;
}

pub struct IsMoreSpec;
pub struct IsLessEqualSpec;

const fn ordered_desc(name: &'static str, summary: &'static str) -> ShardDesc {
  ShardDesc {
    name,
    version: 1,
    summary,
    help: crate::shard_doc!(
      "The input and the operand are Ints or Floats, mixed freely (compared by value). Comparing NaN is an activation error."
    ),
    params: Params::Declared(ORDERED_PARAMS),
    input: InputDesc::Types(COMPARABLE),
    output: OutputDesc::Fixed(TypeName::Bool),
    targets: Targets::All,
    aliases: &[],
  }
}

impl OrderedSpec for IsMoreSpec {
  const DESC: ShardDesc = ordered_desc(
    "IsMore",
    crate::shard_doc!("Outputs whether the input is greater than the operand."),
  );
  fn holds(o: std::cmp::Ordering) -> bool {
    o.is_gt()
  }
}

impl OrderedSpec for IsLessEqualSpec {
  const DESC: ShardDesc = ordered_desc(
    "IsLessEqual",
    crate::shard_doc!("Outputs whether the input is less than or equal to the operand."),
  );
  fn holds(o: std::cmp::Ordering) -> bool {
    o.is_le()
  }
}

pub struct Ordered<S>(std::marker::PhantomData<S>);

impl<S: OrderedSpec> LeafShard for Ordered<S> {
  type Compiled = Operand;
  type State = ();
  const DESC: ShardDesc = S::DESC;

  fn compose<B: Backend>(args: &Args, ctx: &mut ComposeCtx<'_, B>) -> Result<Composed<Operand>> {
    compose_compare(args, ctx, S::DESC.name)
  }

  fn instantiate(_: &Operand, _: &mut InstanceCtx) -> Result<()> {
    Ok(())
  }

  fn activate(op: &Operand, _: &mut (), ctx: &mut impl LeafCtx, input: &Var) -> Result<Flow> {
    Ok(Flow::Next(Var::Bool(S::holds(compare(
      input,
      op.get(ctx),
    )?))))
  }
}

// --- IsAny ---

pub static IS_ANY_PARAMS: &[ParamDecl] = &[decl(
  "Values",
  crate::shard_doc!("The sequence to look in: a literal, or a variable read at activation."),
  OPERAND,
  &[TypeName::Seq],
  Requirement::Required,
)];

pub const IS_ANY_DESC: ShardDesc = ShardDesc {
  name: "IsAny",
  version: 1,
  summary: crate::shard_doc!("Outputs whether the input equals any element of a sequence."),
  help: crate::shard_doc!("Elements compare as in Is."),
  params: Params::Declared(IS_ANY_PARAMS),
  input: InputDesc::Any,
  output: OutputDesc::Fixed(TypeName::Bool),
  targets: Targets::All,
  aliases: &[],
};

pub struct IsAny;

impl LeafShard for IsAny {
  type Compiled = Operand;
  type State = ();
  const DESC: ShardDesc = IS_ANY_DESC;

  fn compose<B: Backend>(args: &Args, ctx: &mut ComposeCtx<'_, B>) -> Result<Composed<Operand>> {
    let (operand, ty) = Operand::compose_arg(args, "Values", "IsAny", ctx)?;
    let input = ctx.input();
    let element = match ty.desc() {
      TypeDesc::Seq(e) => *e,
      _ => {
        return Err(param_error(
          args,
          "IsAny",
          "Values",
          "compose-error",
          "wrong-variable-type",
          format!("Values must be a sequence, got {ty}"),
        ));
      }
    };
    if !comparable(input, element) {
      return Err(param_error(
        args,
        "IsAny",
        "Values",
        "input-type-mismatch",
        "input-type-mismatch",
        format!("{input} can never equal an element of {ty}"),
      ));
    }
    Ok(Composed {
      compiled: operand,
      output: Type::bool(),
    })
  }

  fn instantiate(_: &Operand, _: &mut InstanceCtx) -> Result<()> {
    Ok(())
  }

  fn activate(op: &Operand, _: &mut (), ctx: &mut impl LeafCtx, input: &Var) -> Result<Flow> {
    let Var::Seq(items) = op.get(ctx) else {
      return Err(Error::Activation("IsAny: Values is not a sequence".into()));
    };
    Ok(Flow::Next(Var::Bool(
      items.iter().any(|v| values_equal(input, v)),
    )))
  }
}

// --- ParseInt ---

pub static PARSE_INT_PARAMS: &[ParamDecl] = &[decl(
  "Base",
  crate::shard_doc!("The base, 2 to 36."),
  Forms::LITERAL,
  &[TypeName::Int],
  Requirement::Default(DefaultValue::Int(10)),
)];

pub const PARSE_INT_DESC: ShardDesc = ShardDesc {
  name: "ParseInt",
  version: 1,
  summary: crate::shard_doc!("Parses a string as an Int."),
  help: crate::shard_doc!(
    "Surrounding whitespace is ignored. Text that is not a number in Base is an activation error."
  ),
  params: Params::Declared(PARSE_INT_PARAMS),
  input: InputDesc::Types(&[TypeName::String]),
  output: OutputDesc::Fixed(TypeName::Int),
  targets: Targets::All,
  aliases: &[],
};

pub struct ParseInt;

impl LeafShard for ParseInt {
  type Compiled = u32;
  type State = ();
  const DESC: ShardDesc = PARSE_INT_DESC;

  fn compose<B: Backend>(args: &Args, ctx: &mut ComposeCtx<'_, B>) -> Result<Composed<u32>> {
    let input = ctx.input();
    if input != Type::string() {
      return Err(mismatch("ParseInt", input, &[TypeName::String]));
    }
    let base = args.int("Base").unwrap_or(10);
    if !(2..=36).contains(&base) {
      return Err(param_error(
        args,
        "ParseInt",
        "Base",
        "compose-error",
        "invalid-value",
        format!("Base must be between 2 and 36, got {base}"),
      ));
    }
    Ok(Composed {
      compiled: base as u32,
      output: Type::int(),
    })
  }

  fn instantiate(_: &u32, _: &mut InstanceCtx) -> Result<()> {
    Ok(())
  }

  fn activate(base: &u32, _: &mut (), _: &mut impl LeafCtx, input: &Var) -> Result<Flow> {
    let Var::String(s) = input else {
      return Err(Error::Activation("ParseInt: input is not a string".into()));
    };
    i64::from_str_radix(s.trim(), *base)
      .map(|i| Flow::Next(Var::Int(i)))
      .map_err(|_| Error::Activation(format!("ParseInt: {s:?} is not an Int in base {base}")))
  }
}

// --- pure input-to-output shards ---

/// A shard without parameters whose output depends only on its input.
pub trait PureOp: 'static {
  const DESC: ShardDesc;
  /// The output type for an input type, or the accepted input types.
  fn output(input: Type) -> std::result::Result<Type, &'static [TypeName]>;
  fn apply(input: &Var) -> Result<Var>;
}

pub struct Pure<P>(std::marker::PhantomData<P>);

impl<P: PureOp> LeafShard for Pure<P> {
  type Compiled = ();
  type State = ();
  const DESC: ShardDesc = P::DESC;

  fn compose<B: Backend>(_: &Args, ctx: &mut ComposeCtx<'_, B>) -> Result<Composed<()>> {
    let input = ctx.input();
    match P::output(input) {
      Ok(output) => Ok(Composed {
        compiled: (),
        output,
      }),
      Err(expected) => Err(mismatch(P::DESC.name, input, expected)),
    }
  }

  fn instantiate(_: &(), _: &mut InstanceCtx) -> Result<()> {
    Ok(())
  }

  fn activate(_: &(), _: &mut (), _: &mut impl LeafCtx, input: &Var) -> Result<Flow> {
    P::apply(input).map(Flow::Next)
  }
}

const fn pure_desc(
  name: &'static str,
  summary: &'static str,
  help: &'static str,
  input: InputDesc,
  output: OutputDesc,
) -> ShardDesc {
  ShardDesc {
    name,
    version: 1,
    summary,
    help,
    params: Params::Declared(&[]),
    input,
    output,
    targets: Targets::All,
    aliases: &[],
  }
}

/// `Ok(out)` when `input` is one of `accepted`.
fn one_of(
  input: Type,
  accepted: &'static [TypeName],
  out: Type,
) -> std::result::Result<Type, &'static [TypeName]> {
  if accepted.iter().any(|t| t.matches(input)) && input != Type::any() {
    Ok(out)
  } else {
    Err(accepted)
  }
}

fn fail(name: &str, what: &str) -> Error {
  Error::Activation(format!("{name}: {what}"))
}

pub struct CountOp;
impl PureOp for CountOp {
  const DESC: ShardDesc = pure_desc(
    "Count",
    crate::shard_doc!(
      "Outputs how many elements a sequence or table has, or characters a string has."
    ),
    "",
    InputDesc::Types(&[TypeName::Seq, TypeName::Table, TypeName::String]),
    OutputDesc::Fixed(TypeName::Int),
  );
  fn output(input: Type) -> std::result::Result<Type, &'static [TypeName]> {
    one_of(
      input,
      &[TypeName::Seq, TypeName::Table, TypeName::String],
      Type::int(),
    )
  }
  fn apply(input: &Var) -> Result<Var> {
    Ok(Var::Int(match input {
      Var::Seq(s) => s.len() as i64,
      Var::Table(t) => t.len() as i64,
      Var::String(s) => s.chars().count() as i64,
      _ => return Err(fail("Count", "input type mismatch")),
    }))
  }
}

pub struct NotOp;
impl PureOp for NotOp {
  const DESC: ShardDesc = pure_desc(
    "Not",
    crate::shard_doc!("Negates a Bool."),
    "",
    InputDesc::Types(&[TypeName::Bool]),
    OutputDesc::Fixed(TypeName::Bool),
  );
  fn output(input: Type) -> std::result::Result<Type, &'static [TypeName]> {
    one_of(input, &[TypeName::Bool], Type::bool())
  }
  fn apply(input: &Var) -> Result<Var> {
    match input {
      Var::Bool(b) => Ok(Var::Bool(!b)),
      _ => Err(fail("Not", "input is not a Bool")),
    }
  }
}

pub struct IsNoneOp;
impl PureOp for IsNoneOp {
  const DESC: ShardDesc = pure_desc(
    "IsNone",
    crate::shard_doc!("Outputs whether the input is none."),
    "",
    InputDesc::Any,
    OutputDesc::Fixed(TypeName::Bool),
  );
  fn output(_: Type) -> std::result::Result<Type, &'static [TypeName]> {
    Ok(Type::bool())
  }
  fn apply(input: &Var) -> Result<Var> {
    Ok(Var::Bool(matches!(input, Var::None)))
  }
}

pub struct IsNotNoneOp;
impl PureOp for IsNotNoneOp {
  const DESC: ShardDesc = pure_desc(
    "IsNotNone",
    crate::shard_doc!("Outputs whether the input is not none."),
    "",
    InputDesc::Any,
    OutputDesc::Fixed(TypeName::Bool),
  );
  fn output(_: Type) -> std::result::Result<Type, &'static [TypeName]> {
    Ok(Type::bool())
  }
  fn apply(input: &Var) -> Result<Var> {
    Ok(Var::Bool(!matches!(input, Var::None)))
  }
}

pub struct TimeNowOp;
impl PureOp for TimeNowOp {
  const DESC: ShardDesc = pure_desc(
    "Time.Now",
    crate::shard_doc!("Outputs the time in seconds, from a monotonic clock."),
    crate::shard_doc!(
      "Ignores its input. The value counts from an arbitrary start (the first use in the process): use it for differences, not dates."
    ),
    InputDesc::Ignored,
    OutputDesc::Fixed(TypeName::Float),
  );
  fn output(_: Type) -> std::result::Result<Type, &'static [TypeName]> {
    Ok(Type::float())
  }
  fn apply(_: &Var) -> Result<Var> {
    static START: OnceLock<Instant> = OnceLock::new();
    Ok(Var::Float(
      START.get_or_init(Instant::now).elapsed().as_secs_f64(),
    ))
  }
}

pub struct ToStringOp;
impl PureOp for ToStringOp {
  const DESC: ShardDesc = pure_desc(
    "ToString",
    crate::shard_doc!("Converts the input to a string."),
    crate::shard_doc!(
      "A string stays as it is; other values as 1.x prints them (whole floats without `.0`, strings inside sequences as they are)."
    ),
    InputDesc::Any,
    OutputDesc::Fixed(TypeName::String),
  );
  fn output(_: Type) -> std::result::Result<Type, &'static [TypeName]> {
    Ok(Type::string())
  }
  fn apply(input: &Var) -> Result<Var> {
    Ok(match input {
      Var::String(_) => input.clone(),
      other => Var::string(&other.text()),
    })
  }
}

const SCALARS: &[TypeName] = &[
  TypeName::Int,
  TypeName::Float,
  TypeName::Bool,
  TypeName::String,
];

pub struct ToIntOp;
impl PureOp for ToIntOp {
  const DESC: ShardDesc = pure_desc(
    "ToInt",
    crate::shard_doc!("Converts a number, Bool or string to an Int."),
    crate::shard_doc!(
      "A Float is truncated toward zero (NaN or out of range is an activation error); a string is parsed."
    ),
    InputDesc::Types(SCALARS),
    OutputDesc::Fixed(TypeName::Int),
  );
  fn output(input: Type) -> std::result::Result<Type, &'static [TypeName]> {
    one_of(input, SCALARS, Type::int())
  }
  fn apply(input: &Var) -> Result<Var> {
    Ok(Var::Int(match input {
      Var::Int(i) => *i,
      // The exact Int range: [-2^63, 2^63). NaN fails both comparisons.
      Var::Float(f) if *f >= -9_223_372_036_854_775_808.0 && *f < 9_223_372_036_854_775_808.0 => {
        *f as i64
      }
      Var::Float(f) => {
        return Err(fail(
          "ToInt",
          &format!("{f} is not representable as an Int"),
        ));
      }
      Var::Bool(b) => i64::from(*b),
      Var::String(s) => s
        .trim()
        .parse()
        .map_err(|_| fail("ToInt", &format!("{s:?} is not an Int")))?,
      _ => return Err(fail("ToInt", "input type mismatch")),
    }))
  }
}

pub struct ToFloatOp;
impl PureOp for ToFloatOp {
  const DESC: ShardDesc = pure_desc(
    "ToFloat",
    crate::shard_doc!("Converts a number, Bool or string to a Float."),
    crate::shard_doc!("A string is parsed."),
    InputDesc::Types(SCALARS),
    OutputDesc::Fixed(TypeName::Float),
  );
  fn output(input: Type) -> std::result::Result<Type, &'static [TypeName]> {
    one_of(input, SCALARS, Type::float())
  }
  fn apply(input: &Var) -> Result<Var> {
    Ok(Var::Float(match input {
      Var::Int(i) => *i as f64,
      Var::Float(f) => *f,
      Var::Bool(b) => f64::from(u8::from(*b)),
      Var::String(s) => s
        .trim()
        .parse()
        .map_err(|_| fail("ToFloat", &format!("{s:?} is not a Float")))?,
      _ => return Err(fail("ToFloat", "input type mismatch")),
    }))
  }
}

pub struct ParseFloatOp;
impl PureOp for ParseFloatOp {
  const DESC: ShardDesc = pure_desc(
    "ParseFloat",
    crate::shard_doc!("Parses a string as a Float."),
    crate::shard_doc!(
      "Surrounding whitespace is ignored. Text that is not a number is an activation error."
    ),
    InputDesc::Types(&[TypeName::String]),
    OutputDesc::Fixed(TypeName::Float),
  );
  fn output(input: Type) -> std::result::Result<Type, &'static [TypeName]> {
    one_of(input, &[TypeName::String], Type::float())
  }
  fn apply(input: &Var) -> Result<Var> {
    match input {
      Var::String(s) => s
        .trim()
        .parse()
        .map(Var::Float)
        .map_err(|_| fail("ParseFloat", &format!("{s:?} is not a Float"))),
      _ => Err(fail("ParseFloat", "input is not a string")),
    }
  }
}

pub struct ToHexOp;
impl PureOp for ToHexOp {
  const DESC: ShardDesc = pure_desc(
    "ToHex",
    crate::shard_doc!("Writes an Int in hexadecimal, or a string's bytes as hex."),
    crate::shard_doc!(
      "An Int becomes `0x` and its bits as an unsigned 64-bit number (addresses); a string becomes two hex digits per byte."
    ),
    InputDesc::Types(&[TypeName::Int, TypeName::String]),
    OutputDesc::Fixed(TypeName::String),
  );
  fn output(input: Type) -> std::result::Result<Type, &'static [TypeName]> {
    one_of(input, &[TypeName::Int, TypeName::String], Type::string())
  }
  fn apply(input: &Var) -> Result<Var> {
    Ok(Var::string(&match input {
      Var::Int(i) => format!("0x{:x}", *i as u64),
      Var::String(s) => s.bytes().map(|b| format!("{b:02x}")).collect(),
      _ => return Err(fail("ToHex", "input type mismatch")),
    }))
  }
}

/// `ToFloat2`, `ToFloat3`, `ToFloat4`: from another vector (dropping or
/// zero-filling components) or a sequence of numbers.
pub struct ToVector<const N: usize>;

const VECTOR_SOURCES: &[TypeName] = &[
  TypeName::Float2,
  TypeName::Float3,
  TypeName::Float4,
  TypeName::Seq,
];

impl<const N: usize> ToVector<N> {
  const NAME: &'static str = match N {
    2 => "ToFloat2",
    3 => "ToFloat3",
    _ => "ToFloat4",
  };
}

impl<const N: usize> PureOp for ToVector<N> {
  const DESC: ShardDesc = pure_desc(
    Self::NAME,
    crate::shard_doc!("Converts a vector or a sequence of numbers to a float vector."),
    crate::shard_doc!(
      "From a vector, extra components are dropped and missing ones are 0. From a sequence, it must hold exactly that many Ints or Floats (else an activation error)."
    ),
    InputDesc::Types(VECTOR_SOURCES),
    OutputDesc::Dynamic(crate::shard_doc!("the float vector")),
  );
  fn output(input: Type) -> std::result::Result<Type, &'static [TypeName]> {
    let out = match N {
      2 => Type::float2(),
      3 => Type::float3(),
      _ => Type::float4(),
    };
    one_of(input, VECTOR_SOURCES, out)
  }
  fn apply(input: &Var) -> Result<Var> {
    let mut c = [0.0f64; 4];
    match input {
      Var::Float2(v) => c[..2].copy_from_slice(v),
      Var::Float3(v) => v.iter().enumerate().for_each(|(i, x)| c[i] = f64::from(*x)),
      Var::Float4(v) => v.iter().enumerate().for_each(|(i, x)| c[i] = f64::from(*x)),
      Var::Seq(items) if items.len() == N => {
        for (i, item) in items.iter().enumerate() {
          c[i] = match item {
            Var::Int(x) => *x as f64,
            Var::Float(x) => *x,
            _ => return Err(fail(Self::NAME, "sequence elements must be numbers")),
          };
        }
      }
      Var::Seq(items) => {
        return Err(fail(
          Self::NAME,
          &format!("needs {N} elements, got {}", items.len()),
        ));
      }
      _ => return Err(fail(Self::NAME, "input type mismatch")),
    }
    Ok(match N {
      2 => Var::Float2([c[0], c[1]]),
      3 => Var::Float3([c[0] as f32, c[1] as f32, c[2] as f32]),
      _ => Var::Float4([c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32]),
    })
  }
}

// --- Expect* ---

/// The type an `Expect*` shard narrows to.
pub trait ExpectKind: 'static {
  const NAME: &'static str;
  const TARGET: TypeName;
  /// `[TARGET]`, for diagnostics.
  const TARGETS: &'static [TypeName];
}

macro_rules! expect_kind {
  ($kind:ident, $name:literal, $target:ident) => {
    pub struct $kind;
    impl ExpectKind for $kind {
      const NAME: &'static str = $name;
      const TARGET: TypeName = TypeName::$target;
      const TARGETS: &'static [TypeName] = &[TypeName::$target];
    }
  };
}

expect_kind!(ExpectIntK, "ExpectInt", Int);
expect_kind!(ExpectFloatK, "ExpectFloat", Float);
expect_kind!(ExpectBoolK, "ExpectBool", Bool);
expect_kind!(ExpectStringK, "ExpectString", String);
expect_kind!(ExpectSeqK, "ExpectSeq", Seq);
expect_kind!(ExpectTableK, "ExpectTable", Table);
expect_kind!(ExpectFloat2K, "ExpectFloat2", Float2);
expect_kind!(ExpectFloat3K, "ExpectFloat3", Float3);
expect_kind!(ExpectFloat4K, "ExpectFloat4", Float4);

pub struct Expect<K>(std::marker::PhantomData<K>);

impl<K: ExpectKind> PureOp for Expect<K> {
  const DESC: ShardDesc = pure_desc(
    K::NAME,
    crate::shard_doc!("Narrows the input to one type, failing at activation if it has another."),
    crate::shard_doc!(
      "The input's type must be able to hold the expected type: Any, or a union with a member of it. The output has the narrowed type; a value of another type is an activation error. Shards that output fixed types need no Expect."
    ),
    InputDesc::Any,
    OutputDesc::Dynamic(crate::shard_doc!("the expected type")),
  );
  fn output(input: Type) -> std::result::Result<Type, &'static [TypeName]> {
    let target = K::TARGET.to_type();
    if target.accepts(input) {
      return Ok(input);
    }
    match input.desc() {
      TypeDesc::Any => Ok(target),
      TypeDesc::Union(members) => {
        let narrowed: Vec<Type> = members
          .iter()
          .copied()
          .filter(|m| target.accepts(*m))
          .collect();
        if narrowed.is_empty() {
          Err(K::TARGETS)
        } else {
          Ok(Type::union(narrowed))
        }
      }
      _ => Err(K::TARGETS),
    }
  }
  fn apply(input: &Var) -> Result<Var> {
    // Checked on the value: interning its type on every activation would
    // grow the type registry with every new shape.
    if K::TARGET.to_type().admits(input) {
      Ok(input.clone())
    } else {
      Err(fail(
        K::NAME,
        &format!("expected {}, got {}", K::TARGET.name(), input.type_of()),
      ))
    }
  }
}
