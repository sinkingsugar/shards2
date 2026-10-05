//! Shards over values: reading (`Take`), building (`Seq.Make`,
//! `Table.Make`, `String.Format`) and appending (`Push`). All are leaf
//! shards (one implementation for both schedulers). The frontend lowers
//! `t.key` / `s.0`, computed sequence and table elements, f-strings and
//! `>>` onto them.

use std::sync::Arc;

use super::leaf::{LeafShard, leaf_type};
use super::*;
use crate::diagnostic::closest;
use crate::instance::{InstanceCtx, LeafCtx};
use crate::shard::Flow;

pub static TAKE: ShardType = leaf_type::<Take>();
pub static PUSH: ShardType = leaf_type::<Push>();
pub static SEQ_MAKE: ShardType = leaf_type::<SeqMake>();
pub static TABLE_MAKE: ShardType = leaf_type::<TableMake>();
pub static STRING_FORMAT: ShardType = leaf_type::<StringFormat>();

const OPERAND: Forms = Forms::LITERAL.or(Forms::VARIABLE);

fn compose_error(shard: &str, kind: &'static str, code: &'static str, message: String) -> Error {
  Error::Diagnostic(Box::new(
    Diagnostic::new(Phase::Compose, kind, code, message).shard(shard),
  ))
}

// --- Take ---

pub static TAKE_PARAMS: &[ParamDecl] = &[decl(
  "Key",
  crate::shard_doc!(
    "An Int index into a sequence or vector, or a String key into a table: a literal, or a variable read at activation."
  ),
  OPERAND,
  &[TypeName::Int, TypeName::String],
  Requirement::Required,
)];

pub const TAKE_DESC: ShardDesc = ShardDesc {
  name: "Take",
  version: 1,
  summary: crate::shard_doc!("Outputs one element of a sequence, table or vector."),
  help: crate::shard_doc!(
    "On a fixed table, a literal key must be one of its keys (a compose error lists them) and the output has that key's type. On an open table, a missing key outputs none. An index out of range is an activation error. `t.key` and `s.0` are Take."
  ),
  params: Params::Declared(TAKE_PARAMS),
  input: InputDesc::Types(&[
    TypeName::Seq,
    TypeName::Table,
    TypeName::Float2,
    TypeName::Float3,
    TypeName::Float4,
  ]),
  output: OutputDesc::Dynamic(crate::shard_doc!("the element's type")),
  targets: Targets::All,
  aliases: &[],
};

pub struct Take;

impl LeafShard for Take {
  type Compiled = Operand;
  type State = ();
  const DESC: ShardDesc = TAKE_DESC;

  fn compose<B: Backend>(args: &Args, ctx: &mut ComposeCtx<'_, B>) -> Result<Composed<Operand>> {
    let (key, key_ty) = Operand::compose_arg(args, "Key", "Take", ctx)?;
    let input = ctx.input();
    let literal = match &key {
      Operand::Const(v) => Some(v.clone()),
      Operand::Bound(_) => None,
    };
    let key_error = |expected: TypeName| {
      Err(
        compose_error(
          "Take",
          "compose-error",
          "wrong-key-type",
          format!(
            "Take on {input} needs a {} key, got {key_ty}",
            expected.name()
          ),
        )
        .with_param("Key", 0),
      )
    };
    let output = match input.desc() {
      TypeDesc::Seq(element) => {
        if key_ty != Type::int() {
          return key_error(TypeName::Int);
        }
        *element
      }
      TypeDesc::Float2 | TypeDesc::Float3 | TypeDesc::Float4 => {
        if key_ty != Type::int() {
          return key_error(TypeName::Int);
        }
        Type::float()
      }
      TypeDesc::Table(table) => {
        if key_ty != Type::string() {
          return key_error(TypeName::String);
        }
        match literal {
          Some(Var::String(k)) => match table.keys.iter().find(|(name, _)| **name == *k) {
            Some((_, ty)) => *ty,
            None => match table.rest {
              // An open table may not have the key at runtime.
              Some(rest) => Type::union([rest, Type::none()]),
              None => {
                let keys: Vec<String> = table.keys.iter().map(|(n, _)| n.to_string()).collect();
                let mut d = Diagnostic::new(
                  Phase::Compose,
                  "compose-error",
                  "unknown-key",
                  format!("{input} has no key `{k}` (keys: {})", keys.join(", ")),
                )
                .shard("Take")
                .param("Key", Some(0));
                d.did_you_mean = closest(&k, keys, 3);
                return Err(Error::Diagnostic(Box::new(d)));
              }
            },
          },
          // A key read at activation: any value type, or none if missing.
          _ => Type::union(
            table
              .keys
              .iter()
              .map(|(_, t)| *t)
              .chain(table.rest)
              .chain([Type::none()]),
          ),
        }
      }
      _ => {
        return Err(Error::Diagnostic(Box::new(
          Diagnostic::new(
            Phase::Compose,
            "input-type-mismatch",
            "input-type-mismatch",
            format!("Take needs a sequence, table or vector input, got {input}"),
          )
          .shard("Take")
          .types(
            Some(TypeRef::of(input)),
            [TypeName::Seq, TypeName::Table, TypeName::Float3]
              .into_iter()
              .map(TypeRef::named)
              .collect(),
          ),
        )));
      }
    };
    Ok(Composed {
      compiled: key,
      output,
    })
  }

  fn instantiate(_: &Operand, _: &mut InstanceCtx) -> Result<()> {
    Ok(())
  }

  fn activate(key: &Operand, _: &mut (), ctx: &mut impl LeafCtx, input: &Var) -> Result<Flow> {
    let key = key.get(ctx);
    take_value(input, &key).map(Flow::Next)
  }
}

#[inline]
pub(crate) fn take_value(input: &Var, key: &Var) -> Result<Var> {
  let index = |len: usize| match key {
    Var::Int(i) if *i >= 0 && (*i as usize) < len => Ok(*i as usize),
    Var::Int(i) => Err(Error::Activation(format!(
      "Take: index {i} is out of range (length {len})"
    ))),
    _ => Err(Error::Activation("Take: the key must be an Int".into())),
  };
  let value = match input {
    Var::Seq(items) => items[index(items.len())?].clone(),
    Var::Float2(v) => Var::Float(v[index(2)?]),
    Var::Float3(v) => Var::Float(f64::from(v[index(3)?])),
    Var::Float4(v) => Var::Float(f64::from(v[index(4)?])),
    Var::Table(entries) => match &key {
      Var::String(k) => entries.get(&**k).cloned().unwrap_or(Var::None),
      _ => return Err(Error::Activation("Take: the key must be a String".into())),
    },
    _ => return Err(Error::Activation("Take: input type mismatch".into())),
  };
  Ok(value)
}

// --- Push ---

pub static PUSH_PARAMS: &[ParamDecl] = &[
  decl(
    "Variable",
    crate::shard_doc!(
      "The mutable sequence variable to append to. Declared as an empty sequence of the input's type if it does not exist yet."
    ),
    Forms::VARIABLE,
    NONE_TYPES,
    Requirement::Required,
  ),
  decl(
    "Clear",
    crate::shard_doc!(
      "When this Push declares the variable: its first run in each iteration of a looped wire starts the sequence over, so later pushes in the same iteration (in a Repeat, say) grow it. 1.x clears on every run of the declaring Push instead. A Push that does not run in an iteration (inside Once, a branch) leaves the sequence as it is."
    ),
    Forms::LITERAL,
    &[TypeName::Bool],
    Requirement::Default(DefaultValue::Bool(true)),
  ),
];

pub const PUSH_DESC: ShardDesc = ShardDesc {
  name: "Push",
  version: 1,
  summary: crate::shard_doc!("Appends the input to a sequence variable."),
  help: crate::shard_doc!(
    "Variable must be a mutable sequence whose element type accepts the input, or not exist yet (then it is declared, and with Clear its first run in each loop iteration starts the sequence over). Passes its input through. The `>> name` operator is a Push."
  ),
  params: Params::Declared(PUSH_PARAMS),
  input: InputDesc::Any,
  output: OutputDesc::Passthrough,
  targets: Targets::All,
  aliases: &[],
};

pub struct Push;

pub struct PushCompiled {
  pub(crate) binding: Binding,
  /// This Push declared the variable and clears it each iteration.
  pub(crate) clear: bool,
}

impl LeafShard for Push {
  type Compiled = PushCompiled;
  /// The iteration this Push last cleared in.
  type State = Option<u64>;
  const DESC: ShardDesc = PUSH_DESC;

  fn compose<B: Backend>(
    args: &Args,
    ctx: &mut ComposeCtx<'_, B>,
  ) -> Result<Composed<PushCompiled>> {
    let name = variable(args, "Variable");
    let input = ctx.input();
    // Clearing applies only to the Push that declares the variable.
    let mut clear = false;
    let binding = match ctx.var(name) {
      None => {
        clear = args.bool("Clear").unwrap_or(true);
        ctx.declare_local(name, Type::seq(input), true).binding
      }
      Some(info) => {
        let element = match info.ty.desc() {
          TypeDesc::Seq(e) => Some(*e),
          _ => None,
        };
        let problem = if !info.mutable {
          Some(("immutable-variable", format!("{name} is immutable")))
        } else {
          match element {
            Some(e) if e.accepts(input) => None,
            Some(_) => Some((
              "variable-type-mismatch",
              format!("{name} is {}, cannot push {input}", info.ty),
            )),
            None => Some((
              "variable-type-mismatch",
              format!("{name} is {}, not a sequence", info.ty),
            )),
          }
        };
        if let Some((code, message)) = problem {
          return Err(param_error(
            args,
            "Push",
            "Variable",
            "compose-error",
            code,
            message,
          ));
        }
        ctx.mark_initialized(info.binding);
        info.binding
      }
    };
    Ok(Composed {
      compiled: PushCompiled { binding, clear },
      output: input,
    })
  }

  fn instantiate(_: &PushCompiled, _: &mut InstanceCtx) -> Result<Option<u64>> {
    Ok(None)
  }

  fn activate(
    c: &PushCompiled,
    cleared_in: &mut Option<u64>,
    ctx: &mut impl LeafCtx,
    input: &Var,
  ) -> Result<Flow> {
    let b = &c.binding;
    // With Clear, the first run in each loop iteration starts the sequence
    // over (1.x clears on every run; a listed deviation). A Push that does
    // not run (inside Once, or a branch) leaves the sequence as it is, so
    // it always holds a sequence.
    let fresh = c.clear && *cleared_in != Some(ctx.iteration());
    if c.clear {
      *cleared_in = Some(ctx.iteration());
    }
    // Take the value out of its slot so the sequence is unshared and grows
    // in place instead of being copied.
    let current = if fresh { Var::None } else { ctx.get(*b) };
    ctx.set(*b, Var::None);
    let seq = match current {
      Var::Seq(mut items) => {
        Arc::make_mut(&mut items).push(input.clone());
        Var::Seq(items)
      }
      Var::None => Var::Seq(Arc::new(vec![input.clone()])),
      other => {
        ctx.set(*b, other);
        return Err(Error::Activation(
          "Push: the variable is not a sequence".into(),
        ));
      }
    };
    ctx.set(*b, seq);
    Ok(Flow::Next(input.clone()))
  }
}

// --- Seq.Make and Table.Make ---

pub static SEQ_MAKE_PARAMS: &[ParamDecl] = &[decl(
  "Items",
  crate::shard_doc!("The elements: literals, or variables read at activation."),
  OPERAND,
  &[],
  Requirement::Variadic,
)];

pub const SEQ_MAKE_DESC: ShardDesc = ShardDesc {
  name: "Seq.Make",
  version: 1,
  summary: crate::shard_doc!("Builds a sequence from literals and variables."),
  help: crate::shard_doc!(
    "Ignores its input. The element type is the union of the items' types. The frontend lowers sequence literals with computed elements to Seq.Make."
  ),
  params: Params::Declared(SEQ_MAKE_PARAMS),
  input: InputDesc::Ignored,
  output: OutputDesc::Dynamic(crate::shard_doc!("a sequence of the items' types")),
  targets: Targets::All,
  aliases: &[],
};

/// Composes variadic literal-or-variable operands.
fn compose_operands<B: Backend>(
  args: &Args,
  param: &str,
  shard: &str,
  ctx: &mut ComposeCtx<'_, B>,
) -> Result<Vec<(Operand, Type)>> {
  let index = args.param_index(param);
  args
    .variadic(param)
    .iter()
    .map(|value| match value {
      ParamValue::Value(v) => Ok((Operand::Const(v.clone()), v.type_of())),
      ParamValue::Var(name) => {
        let info = ctx
          .read_var(name, shard)
          .map_err(|e| e.with_param(param, index))?;
        Ok((Operand::Bound(info.binding), info.ty))
      }
      other => unreachable!("decoder accepted {other:?} for {shard}.{param}"),
    })
    .collect()
}

pub struct SeqMake;

impl LeafShard for SeqMake {
  type Compiled = Vec<Operand>;
  type State = ();
  const DESC: ShardDesc = SEQ_MAKE_DESC;

  fn compose<B: Backend>(
    args: &Args,
    ctx: &mut ComposeCtx<'_, B>,
  ) -> Result<Composed<Vec<Operand>>> {
    let items = compose_operands(args, "Items", "Seq.Make", ctx)?;
    let element = if items.is_empty() {
      Type::any()
    } else {
      Type::union(items.iter().map(|(_, t)| *t))
    };
    Ok(Composed {
      compiled: items.into_iter().map(|(o, _)| o).collect(),
      output: Type::seq(element),
    })
  }

  fn instantiate(_: &Vec<Operand>, _: &mut InstanceCtx) -> Result<()> {
    Ok(())
  }

  fn activate(items: &Vec<Operand>, _: &mut (), ctx: &mut impl LeafCtx, _: &Var) -> Result<Flow> {
    Ok(Flow::Next(Var::Seq(Arc::new(
      items.iter().map(|o| o.get(ctx)).collect(),
    ))))
  }
}

pub static TABLE_MAKE_PARAMS: &[ParamDecl] = &[
  decl(
    "Keys",
    crate::shard_doc!("The keys, as a literal sequence of distinct strings."),
    Forms::LITERAL,
    &[TypeName::Seq],
    Requirement::Required,
  ),
  decl(
    "Values",
    crate::shard_doc!("One value per key, in order: literals, or variables read at activation."),
    OPERAND,
    &[],
    Requirement::Variadic,
  ),
];

pub const TABLE_MAKE_DESC: ShardDesc = ShardDesc {
  name: "Table.Make",
  version: 1,
  summary: crate::shard_doc!("Builds a fixed table from keys and values."),
  help: crate::shard_doc!(
    "Ignores its input. The output is a fixed table with exactly these keys. The frontend lowers table literals with computed values to Table.Make."
  ),
  params: Params::Declared(TABLE_MAKE_PARAMS),
  input: InputDesc::Ignored,
  output: OutputDesc::Dynamic(crate::shard_doc!(
    "a fixed table of the keys and the values' types"
  )),
  targets: Targets::All,
  aliases: &[],
};

pub struct TableMake;

impl LeafShard for TableMake {
  type Compiled = Vec<(Arc<str>, Operand)>;
  type State = ();
  const DESC: ShardDesc = TABLE_MAKE_DESC;

  fn compose<B: Backend>(
    args: &Args,
    ctx: &mut ComposeCtx<'_, B>,
  ) -> Result<Composed<Vec<(Arc<str>, Operand)>>> {
    let bad_keys = || {
      Err(param_error(
        args,
        "Table.Make",
        "Keys",
        "compose-error",
        "invalid-keys",
        "Keys must be a sequence of distinct strings".into(),
      ))
    };
    let keys: Vec<Arc<str>> = match args.literal("Keys") {
      Some(Var::Seq(items)) => {
        let mut keys = Vec::with_capacity(items.len());
        for item in items.iter() {
          match item {
            Var::String(s) if !keys.contains(s) => keys.push(s.clone()),
            _ => return bad_keys(),
          }
        }
        keys
      }
      _ => return bad_keys(),
    };
    let values = compose_operands(args, "Values", "Table.Make", ctx)?;
    if values.len() != keys.len() {
      return Err(param_error(
        args,
        "Table.Make",
        "Values",
        "compose-error",
        "value-count",
        format!("{} keys but {} values", keys.len(), values.len()),
      ));
    }
    let output = Type::fixed_table(keys.iter().cloned().zip(values.iter().map(|(_, t)| *t)));
    Ok(Composed {
      compiled: keys
        .into_iter()
        .zip(values.into_iter().map(|(o, _)| o))
        .collect(),
      output,
    })
  }

  fn instantiate(_: &Vec<(Arc<str>, Operand)>, _: &mut InstanceCtx) -> Result<()> {
    Ok(())
  }

  fn activate(
    entries: &Vec<(Arc<str>, Operand)>,
    _: &mut (),
    ctx: &mut impl LeafCtx,
    _: &Var,
  ) -> Result<Flow> {
    Ok(Flow::Next(Var::Table(Arc::new(
      entries
        .iter()
        .map(|(k, o)| (k.clone(), o.get(ctx)))
        .collect(),
    ))))
  }
}

// --- String.Format ---

pub const STRING_FORMAT_DESC: ShardDesc = ShardDesc {
  name: "String.Format",
  version: 1,
  summary: crate::shard_doc!("Joins a sequence's elements into one string."),
  help: crate::shard_doc!(
    "Elements print as text: strings as they are, whole floats without `.0`, other floats exact (`12`, `2.5`, `[1 2]`, `{a: 1}`, `none`). f-strings lower to Seq.Make followed by String.Format."
  ),
  params: Params::Declared(&[]),
  input: InputDesc::Types(&[TypeName::Seq]),
  output: OutputDesc::Fixed(TypeName::String),
  targets: Targets::All,
  aliases: &[],
};

pub struct StringFormat;

impl LeafShard for StringFormat {
  type Compiled = ();
  type State = ();
  const DESC: ShardDesc = STRING_FORMAT_DESC;

  fn compose<B: Backend>(_: &Args, ctx: &mut ComposeCtx<'_, B>) -> Result<Composed<()>> {
    let input = ctx.input();
    if !matches!(input.desc(), TypeDesc::Seq(_)) {
      return Err(Error::Diagnostic(Box::new(
        Diagnostic::new(
          Phase::Compose,
          "input-type-mismatch",
          "input-type-mismatch",
          format!("String.Format needs a sequence input, got {input}"),
        )
        .shard("String.Format")
        .types(
          Some(TypeRef::of(input)),
          vec![TypeRef::named(TypeName::Seq)],
        ),
      )));
    }
    Ok(Composed {
      compiled: (),
      output: Type::string(),
    })
  }

  fn instantiate(_: &(), _: &mut InstanceCtx) -> Result<()> {
    Ok(())
  }

  fn activate(_: &(), _: &mut (), _: &mut impl LeafCtx, input: &Var) -> Result<Flow> {
    let Var::Seq(items) = input else {
      return Err(Error::Activation(
        "String.Format: input is not a sequence".into(),
      ));
    };
    let mut out = String::new();
    for item in items.iter() {
      out.push_str(&item.text());
    }
    Ok(Flow::Next(Var::String(Arc::from(out))))
  }
}
