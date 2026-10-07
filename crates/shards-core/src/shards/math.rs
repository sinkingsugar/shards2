//! Arithmetic: `Math.Add`, `Math.Subtract`, `Math.Multiply`, `Math.Divide`
//! (one compose and evaluation path), `Math.Inc`/`Math.Dec`, and unary
//! shards (`Math.Abs`, `Math.Round`, `Math.Floor`, `Math.Ceil`,
//! `Math.Length`). All leaf shards. Names and aliases follow 1.x.

use super::leaf::{LeafShard, leaf_type};
use super::*;
use crate::instance::{InstanceCtx, LeafCtx};
use crate::shard::Flow;

pub static SUBTRACT: ShardType = leaf_type::<Binary<SubOp>>();
pub static MULTIPLY: ShardType = leaf_type::<Binary<MulOp>>();
pub static DIVIDE: ShardType = leaf_type::<Binary<DivOp>>();
pub static DEC: ShardType = leaf_type::<Dec>();
pub static ABS: ShardType = leaf_type::<Unary<AbsOp>>();
pub static ROUND: ShardType = leaf_type::<Unary<RoundOp>>();
pub static FLOOR: ShardType = leaf_type::<Unary<FloorOp>>();
pub static CEIL: ShardType = leaf_type::<Unary<CeilOp>>();
pub static LENGTH: ShardType = leaf_type::<Length>();

/// Types arithmetic works on (same type on both sides).
pub const ARITHMETIC: &[TypeName] = &[
  TypeName::Int,
  TypeName::Float,
  TypeName::Float2,
  TypeName::Float3,
  TypeName::Float4,
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
  Add,
  Subtract,
  Multiply,
  Divide,
}

impl BinOp {
  /// "cannot add Float to Int", "cannot divide Int by Float".
  fn mismatch(self, operand: Type, input: Type) -> String {
    match self {
      BinOp::Add => format!("cannot add {operand} to {input}"),
      BinOp::Subtract => format!("cannot subtract {operand} from {input}"),
      BinOp::Multiply => format!("cannot multiply {input} by {operand}"),
      BinOp::Divide => format!("cannot divide {input} by {operand}"),
    }
  }

  fn int(self, a: i64, b: i64) -> Option<i64> {
    match self {
      BinOp::Add => a.checked_add(b),
      BinOp::Subtract => a.checked_sub(b),
      BinOp::Multiply => a.checked_mul(b),
      BinOp::Divide => a.checked_div(b),
    }
  }

  fn f64(self, a: f64, b: f64) -> f64 {
    match self {
      BinOp::Add => a + b,
      BinOp::Subtract => a - b,
      BinOp::Multiply => a * b,
      BinOp::Divide => a / b,
    }
  }
}

/// A number's shape: Int, Float, or a float vector of a size.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
  Int,
  Float,
  Vector(usize),
}

impl Shape {
  fn of_type(t: Type) -> Option<Shape> {
    Some(match t.desc() {
      TypeDesc::Int => Shape::Int,
      TypeDesc::Float => Shape::Float,
      TypeDesc::Float2 => Shape::Vector(2),
      TypeDesc::Float3 => Shape::Vector(3),
      TypeDesc::Float4 => Shape::Vector(4),
      _ => return None,
    })
  }

  fn to_type(self) -> Type {
    match self {
      Shape::Int => Type::int(),
      Shape::Float => Type::float(),
      Shape::Vector(2) => Type::float2(),
      Shape::Vector(3) => Type::float3(),
      Shape::Vector(_) => Type::float4(),
    }
  }

  /// The result of mixing two shapes (docs/values-and-types.md §6): Int
  /// with Int stays Int, Int with Float is Float, a vector with a number
  /// applies per component, vectors must have one size.
  fn mix(a: Shape, b: Shape) -> Option<Shape> {
    match (a, b) {
      (Shape::Int, Shape::Int) => Some(Shape::Int),
      (Shape::Int | Shape::Float, Shape::Int | Shape::Float) => Some(Shape::Float),
      (Shape::Vector(n), Shape::Int | Shape::Float)
      | (Shape::Int | Shape::Float, Shape::Vector(n)) => Some(Shape::Vector(n)),
      (Shape::Vector(n), Shape::Vector(m)) if n == m => Some(Shape::Vector(n)),
      _ => None,
    }
  }
}

/// A value's components as f64 (one for a number), and its vector size.
fn components(v: &Var) -> Option<([f64; 4], Option<usize>)> {
  let mut c = [0.0; 4];
  let size = match v {
    Var::Int(i) => {
      c[0] = *i as f64;
      None
    }
    Var::Float(f) => {
      c[0] = *f;
      None
    }
    Var::Float2(x) => {
      x.iter().enumerate().for_each(|(i, v)| c[i] = f64::from(*v));
      Some(2)
    }
    Var::Float3(x) => {
      x.iter().enumerate().for_each(|(i, v)| c[i] = f64::from(*v));
      Some(3)
    }
    Var::Float4(x) => {
      x.iter().enumerate().for_each(|(i, v)| c[i] = f64::from(*v));
      Some(4)
    }
    _ => return None,
  };
  Some((c, size))
}

/// `input op operand` with numeric promotion: Int with Int stays Int (overflow
/// and division by zero are activation errors; division truncates); Int with
/// Float is Float; a vector with a number applies per component; floats
/// follow IEEE (division by zero is infinite).
pub fn arith(op: BinOp, name: &str, input: &Var, operand: &Var) -> Result<Var> {
  if let (Var::Int(a), Var::Int(b)) = (input, operand) {
    return op.int(*a, *b).map(Var::Int).ok_or_else(|| {
      Error::Activation(if *b == 0 && op == BinOp::Divide {
        format!("{name}: division by zero")
      } else {
        format!("{name}: integer overflow")
      })
    });
  }
  let mismatch = || Error::Activation(format!("{name}: operand type mismatch"));
  let (a, na) = components(input).ok_or_else(mismatch)?;
  let (b, nb) = components(operand).ok_or_else(mismatch)?;
  let size = match (na, nb) {
    (Some(n), Some(m)) if n != m => return Err(mismatch()),
    (Some(n), _) | (_, Some(n)) => n,
    (None, None) => return Ok(Var::Float(op.f64(a[0], b[0]))),
  };
  // A number on either side applies to every component.
  let at = |c: &[f64; 4], n: Option<usize>, i: usize| if n.is_some() { c[i] } else { c[0] };
  let r: [f64; 4] = std::array::from_fn(|i| op.f64(at(&a, na, i), at(&b, nb, i)));
  // Components are computed in f64 and rounded once to f32.
  let r = r.map(|x| x as f32);
  Ok(match size {
    2 => Var::float2(r[0], r[1]),
    3 => Var::float3(r[0], r[1], r[2]),
    _ => Var::float4(r[0], r[1], r[2], r[3]),
  })
}

/// Shared compose of the binary shards: the input and the operand (literal
/// or variable) are numbers or float vectors that mix ([`Shape::mix`]).
pub(crate) fn compose_binary(
  args: &Args,
  ctx: &mut ComposeCtx<'_>,
  name: &'static str,
  op: BinOp,
) -> Result<Composed<Operand>> {
  let (operand, ty) = Operand::compose_arg(args, "operand", name, ctx)?;
  let input = ctx.input();
  let mixed = match (Shape::of_type(input), Shape::of_type(ty)) {
    (Some(a), Some(b)) => Shape::mix(a, b),
    _ => None,
  };
  let Some(shape) = mixed else {
    return Err(Error::Diagnostic(Box::new(
      Diagnostic::new(
        Phase::Compose,
        "input-type-mismatch",
        "input-type-mismatch",
        op.mismatch(ty, input),
      )
      .shard(name)
      .param("operand", Some(args.param_index("operand")))
      .types(
        Some(TypeRef::of(input)),
        ARITHMETIC.iter().copied().map(TypeRef::named).collect(),
      ),
    )));
  };
  Ok(Composed {
    compiled: operand,
    output: shape.to_type(),
  })
}

pub static BINARY_PARAMS: &[ParamDecl] = &[decl(
  "operand",
  crate::shard_doc!(
    "The right-hand value: a number or float vector that mixes with the input, as a literal or a variable read at activation."
  ),
  OPERAND,
  ARITHMETIC,
  Requirement::Required,
)];

/// One binary operation: its description and operator.
pub trait BinarySpec: 'static {
  const DESC: ShardDesc;
  const OP: BinOp;
}

pub struct SubOp;
pub struct MulOp;
pub struct DivOp;

const fn binary_desc(
  name: &'static str,
  summary: &'static str,
  aliases: &'static [&'static str],
) -> ShardDesc {
  ShardDesc {
    name,
    version: 1,
    summary,
    help: crate::shard_doc!(
      "Int, Float and the float vectors mix: Int with Float gives a Float, a vector with a number applies to each component, vectors of one size work per component. Int with Int stays Int (division truncates); overflow and Int division by zero are activation errors."
    ),
    params: Params::Declared(BINARY_PARAMS),
    input: InputDesc::Types(ARITHMETIC),
    output: OutputDesc::Dynamic(crate::shard_doc!(
      "the mixed type of the input and the operand"
    )),
    targets: Targets::All,
    aliases,
    effects: crate::signature::Effects::NONE,
    lifetime: crate::signature::Lifetime::Stateless,
  }
}

impl BinarySpec for SubOp {
  const DESC: ShardDesc = binary_desc(
    "Math.Subtract",
    crate::shard_doc!("Subtracts the operand from the input."),
    &["Sub"],
  );
  const OP: BinOp = BinOp::Subtract;
}

impl BinarySpec for MulOp {
  const DESC: ShardDesc = binary_desc(
    "Math.Multiply",
    crate::shard_doc!("Multiplies the input by the operand."),
    &["Mul"],
  );
  const OP: BinOp = BinOp::Multiply;
}

impl BinarySpec for DivOp {
  const DESC: ShardDesc = binary_desc(
    "Math.Divide",
    crate::shard_doc!("Divides the input by the operand."),
    &["Div"],
  );
  const OP: BinOp = BinOp::Divide;
}

pub struct Binary<S>(std::marker::PhantomData<S>);

impl<S: BinarySpec> LeafShard for Binary<S> {
  type Compiled = Operand;
  type State = ();
  const DESC: ShardDesc = S::DESC;

  fn compose(args: &Args, ctx: &mut ComposeCtx<'_>) -> Result<Composed<Operand>> {
    compose_binary(args, ctx, S::DESC.name, S::OP)
  }

  fn instantiate(_: &Operand, _: &mut InstanceCtx) -> Result<()> {
    Ok(())
  }

  fn activate(op: &Operand, _: &mut (), ctx: &mut impl LeafCtx, input: &Var) -> Result<Flow> {
    Ok(Flow::Next(arith(S::OP, S::DESC.name, input, &op.get(ctx))?))
  }
}

// --- Math.Dec ---

pub static DEC_PARAMS: &[ParamDecl] = &[decl(
  "variable",
  crate::shard_doc!("The mutable Int variable to decrement."),
  Forms::VARIABLE,
  NONE_TYPES,
  Requirement::Required,
)];

pub const DEC_DESC: ShardDesc = ShardDesc {
  name: "Math.Dec",
  version: 1,
  summary: crate::shard_doc!("Decrements an Int variable and outputs the new value."),
  help: crate::shard_doc!(
    "Ignores its input. `variable` must be a mutable Int; overflow is an activation error."
  ),
  params: Params::Declared(DEC_PARAMS),
  input: InputDesc::Ignored,
  output: OutputDesc::Fixed(TypeName::Int),
  targets: Targets::All,
  aliases: &["Dec"],
  effects: crate::signature::Effects::NONE,
  lifetime: crate::signature::Lifetime::Stateless,
};

pub struct Dec;

impl LeafShard for Dec {
  type Compiled = Binding;
  type State = ();
  const DESC: ShardDesc = DEC_DESC;

  fn compose(args: &Args, ctx: &mut ComposeCtx<'_>) -> Result<Composed<Binding>> {
    compose_counter(args, ctx, DEC_DESC.name)
  }

  fn instantiate(_: &Binding, _: &mut InstanceCtx) -> Result<()> {
    Ok(())
  }

  fn activate(b: &Binding, _: &mut (), ctx: &mut impl LeafCtx, _: &Var) -> Result<Flow> {
    let Var::Int(v) = ctx.get(*b) else {
      return Err(Error::Activation("Math.Dec: variable is not an Int".into()));
    };
    let next = v
      .checked_sub(1)
      .ok_or_else(|| Error::Activation("Math.Dec: integer overflow".into()))?;
    ctx.set(*b, Var::Int(next));
    Ok(Flow::Next(Var::Int(next)))
  }
}

// --- unary ---

/// One unary operation over Int, Float and the vectors.
pub trait UnarySpec: 'static {
  const DESC: ShardDesc;
  fn int(v: i64) -> Option<i64>;
  fn float(v: f64) -> f64;
}

pub struct AbsOp;
pub struct RoundOp;
pub struct FloorOp;
pub struct CeilOp;

const fn unary_desc(
  name: &'static str,
  summary: &'static str,
  aliases: &'static [&'static str],
) -> ShardDesc {
  ShardDesc {
    name,
    version: 1,
    summary,
    help: crate::shard_doc!(
      "Works on Int, Float, Float2, Float3 and Float4 (vectors per component); the output has the input's type."
    ),
    params: Params::Declared(&[]),
    input: InputDesc::Types(ARITHMETIC),
    output: OutputDesc::SameAsInput,
    targets: Targets::All,
    aliases,
    effects: crate::signature::Effects::NONE,
    lifetime: crate::signature::Lifetime::Stateless,
  }
}

impl UnarySpec for AbsOp {
  const DESC: ShardDesc = unary_desc(
    "Math.Abs",
    crate::shard_doc!("Outputs the absolute value of the input."),
    &["Abs"],
  );
  fn int(v: i64) -> Option<i64> {
    v.checked_abs()
  }
  fn float(v: f64) -> f64 {
    v.abs()
  }
}

impl UnarySpec for RoundOp {
  const DESC: ShardDesc = unary_desc(
    "Math.Round",
    crate::shard_doc!("Rounds the input to the nearest integer value (halves away from zero)."),
    &["Round"],
  );
  fn int(v: i64) -> Option<i64> {
    Some(v)
  }
  fn float(v: f64) -> f64 {
    v.round()
  }
}

impl UnarySpec for FloorOp {
  const DESC: ShardDesc = unary_desc(
    "Math.Floor",
    crate::shard_doc!("Rounds the input down."),
    &["Floor"],
  );
  fn int(v: i64) -> Option<i64> {
    Some(v)
  }
  fn float(v: f64) -> f64 {
    v.floor()
  }
}

impl UnarySpec for CeilOp {
  const DESC: ShardDesc = unary_desc(
    "Math.Ceil",
    crate::shard_doc!("Rounds the input up."),
    &["Ceil"],
  );
  fn int(v: i64) -> Option<i64> {
    Some(v)
  }
  fn float(v: f64) -> f64 {
    v.ceil()
  }
}

/// Checks that the input type is one of `accepted`.
fn require_input(ctx: &ComposeCtx<'_>, name: &str, accepted: &[TypeName]) -> Result<Type> {
  let input = ctx.input();
  if accepted.iter().any(|t| t.to_type() == input) {
    return Ok(input);
  }
  Err(Error::Diagnostic(Box::new(
    Diagnostic::new(
      Phase::Compose,
      "input-type-mismatch",
      "input-type-mismatch",
      format!(
        "{name} needs {} input, got {input}",
        accepted
          .iter()
          .map(|t| t.name())
          .collect::<Vec<_>>()
          .join(", ")
      ),
    )
    .shard(name)
    .types(
      Some(TypeRef::of(input)),
      accepted.iter().copied().map(TypeRef::named).collect(),
    ),
  )))
}

pub struct Unary<S>(std::marker::PhantomData<S>);

impl<S: UnarySpec> LeafShard for Unary<S> {
  type Compiled = ();
  type State = ();
  const DESC: ShardDesc = S::DESC;

  fn compose(_: &Args, ctx: &mut ComposeCtx<'_>) -> Result<Composed<()>> {
    let output = require_input(ctx, S::DESC.name, ARITHMETIC)?;
    Ok(Composed {
      compiled: (),
      output,
    })
  }

  fn instantiate(_: &(), _: &mut InstanceCtx) -> Result<()> {
    Ok(())
  }

  fn activate(_: &(), _: &mut (), _: &mut impl LeafCtx, input: &Var) -> Result<Flow> {
    let f32op = |x: f32| S::float(f64::from(x)) as f32;
    Ok(Flow::Next(match input {
      Var::Int(v) => Var::Int(
        S::int(*v)
          .ok_or_else(|| Error::Activation(format!("{}: integer overflow", S::DESC.name)))?,
      ),
      Var::Float(v) => Var::Float(S::float(*v)),
      Var::Float2(v) => Var::from(v.map(f32op)),
      Var::Float3(v) => Var::from(v.map(f32op)),
      Var::Float4(v) => Var::from(v.map(f32op)),
      _ => {
        return Err(Error::Activation(format!(
          "{}: input type mismatch",
          S::DESC.name
        )));
      }
    }))
  }
}

// --- Math.Length ---

const VECTORS: &[TypeName] = &[TypeName::Float2, TypeName::Float3, TypeName::Float4];

pub const LENGTH_DESC: ShardDesc = ShardDesc {
  name: "Math.Length",
  version: 1,
  summary: crate::shard_doc!("Outputs the length (Euclidean norm) of a vector."),
  help: crate::shard_doc!("The input is a Float2, Float3 or Float4; the output is a Float."),
  params: Params::Declared(&[]),
  input: InputDesc::Types(VECTORS),
  output: OutputDesc::Fixed(TypeName::Float),
  targets: Targets::All,
  aliases: &[],
  effects: crate::signature::Effects::NONE,
  lifetime: crate::signature::Lifetime::Stateless,
};

pub struct Length;

impl LeafShard for Length {
  type Compiled = ();
  type State = ();
  const DESC: ShardDesc = LENGTH_DESC;

  fn compose(_: &Args, ctx: &mut ComposeCtx<'_>) -> Result<Composed<()>> {
    require_input(ctx, LENGTH_DESC.name, VECTORS)?;
    Ok(Composed {
      compiled: (),
      output: Type::float(),
    })
  }

  fn instantiate(_: &(), _: &mut InstanceCtx) -> Result<()> {
    Ok(())
  }

  fn activate(_: &(), _: &mut (), _: &mut impl LeafCtx, input: &Var) -> Result<Flow> {
    let squares: f64 = match input {
      Var::Float2(v) => v.iter().map(|x| f64::from(*x).powi(2)).sum(),
      Var::Float3(v) => v.iter().map(|x| f64::from(*x).powi(2)).sum(),
      Var::Float4(v) => v.iter().map(|x| f64::from(*x).powi(2)).sum(),
      _ => {
        return Err(Error::Activation(
          "Math.Length: input is not a vector".into(),
        ));
      }
    };
    Ok(Flow::Next(Var::Float(squares.sqrt())))
  }
}
