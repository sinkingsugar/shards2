//! Control flow added for real scripts: `If`, `Match`, `Maybe`, `All`,
//! `Any`. Descriptions, compose and compiled types live here, shared by
//! both schedulers; execution is in `stackful.rs` and
//! `stackless/shards.rs`. Every compiled type keeps its nested flows in one
//! list ([`ControlFlows::flows`]) so instantiation, cleanup and size
//! accounting are one helper per scheduler.

use super::*;

/// The nested flows of a control shard, in a fixed order.
pub trait ControlFlows<B: Backend> {
  fn flows(&self) -> &[CompiledFlow<B>];
}

fn compose_error(
  shard: &str,
  param: &str,
  index: usize,
  code: &'static str,
  message: String,
) -> Error {
  Error::Diagnostic(Box::new(
    Diagnostic::new(Phase::Compose, "compose-error", code, message)
      .shard(shard)
      .param(param, Some(index)),
  ))
}

/// A predicate flow, which must output Bool.
fn compose_predicate<B: Backend>(
  ctx: &mut ComposeCtx<'_, B>,
  flow: &[ShardDef],
  input: Type,
  conditional: bool,
  shard: &str,
  param: &str,
  index: usize,
) -> Result<CompiledFlow<B>> {
  let compiled = if conditional {
    ctx.compose_flow_conditional(flow, input)?
  } else {
    ctx.compose_flow(flow, input)?
  };
  if !Type::bool().accepts(compiled.output) {
    return Err(Error::Diagnostic(Box::new(
      Diagnostic::new(
        Phase::Compose,
        "compose-error",
        "predicate-not-bool",
        format!("{param} must output Bool, got {}", compiled.output),
      )
      .shard(shard)
      .param(param, Some(index))
      .types(
        Some(TypeRef::of(compiled.output)),
        vec![TypeRef::named(TypeName::Bool)],
      ),
    )));
  }
  Ok(compiled)
}

const FLOW: Forms = Forms::FLOW;

// --- If ---

pub static IF_PARAMS: &[ParamDecl] = &[
  decl(
    "predicate",
    crate::shard_doc!("A flow that receives the input and outputs a Bool."),
    FLOW,
    NONE_TYPES,
    Requirement::Required,
  ),
  decl(
    "then",
    crate::shard_doc!("The flow to run when the predicate is true."),
    FLOW,
    NONE_TYPES,
    Requirement::Required,
  ),
  decl(
    "else",
    crate::shard_doc!("The flow to run when the predicate is false."),
    FLOW,
    NONE_TYPES,
    Requirement::Optional,
  ),
  decl(
    "passthrough",
    crate::shard_doc!("Output the input instead of the branch's output."),
    Forms::LITERAL,
    &[TypeName::Bool],
    Requirement::Default(DefaultValue::Bool(false)),
  ),
];

pub const IF_DESC: ShardDesc = ShardDesc {
  name: "If",
  version: 1,
  summary: crate::shard_doc!("Runs `then` or `else` depending on a predicate."),
  help: crate::shard_doc!(
    "All three flows receive If's input. The output is the branch's output (a union when the branches differ); without `else`, a false predicate outputs the input. With `passthrough`, the input. A Stop, Restart or Return inside a flow propagates."
  ),
  params: Params::Declared(IF_PARAMS),
  input: InputDesc::Any,
  output: OutputDesc::Dynamic(crate::shard_doc!(
    "the branch's output, or the input with Passthrough"
  )),
  targets: Targets::All,
  aliases: &[],
};

/// `flows`: `[predicate, then]` or `[predicate, then, else]`.
pub struct IfCompiled<B: Backend> {
  pub(crate) flows: Vec<CompiledFlow<B>>,
  pub(crate) passthrough: bool,
}

impl<B: Backend> ControlFlows<B> for IfCompiled<B> {
  fn flows(&self) -> &[CompiledFlow<B>] {
    &self.flows
  }
}

pub(crate) fn compose_if<B: Backend>(
  args: &Args,
  ctx: &mut ComposeCtx<'_, B>,
) -> Result<Composed<IfCompiled<B>>> {
  let input = ctx.input();
  let pred = compose_predicate(
    ctx,
    args.flow("predicate").expect("decoded"),
    input,
    false,
    "If",
    "predicate",
    0,
  )?;
  let then = ctx.compose_flow_conditional(args.flow("then").expect("decoded"), input)?;
  let passthrough = args.bool("passthrough").unwrap_or(false);
  let els = match args.flow("else") {
    Some(flow) => Some(ctx.compose_flow_conditional(flow, input)?),
    None => None,
  };
  let output = if passthrough {
    input
  } else {
    Type::union([then.output, els.as_ref().map_or(input, |e| e.output)])
  };
  let mut flows = vec![pred, then];
  flows.extend(els);
  Ok(Composed {
    compiled: IfCompiled { flows, passthrough },
    output,
  })
}

// --- Match ---

pub static MATCH_PARAMS: &[ParamDecl] = &[
  decl(
    "cases",
    crate::shard_doc!(
      "Value-flow pairs, `[value {flow} value {flow}]`; a `none` value matches anything."
    ),
    Forms::CASES,
    NONE_TYPES,
    Requirement::Required,
  ),
  decl(
    "passthrough",
    crate::shard_doc!("Output the input instead of the matched case's output."),
    Forms::LITERAL,
    &[TypeName::Bool],
    Requirement::Default(DefaultValue::Bool(true)),
  ),
];

pub const MATCH_DESC: ShardDesc = ShardDesc {
  name: "Match",
  version: 1,
  summary: crate::shard_doc!("Runs the flow of the first case equal to the input."),
  help: crate::shard_doc!(
    "Cases are tried in order and compare as in Is; a `none` case matches anything. The matched flow receives the input. By default Match passes its input through; with `passthrough: false` it outputs the matched flow's output (the input when nothing matched)."
  ),
  params: Params::Declared(MATCH_PARAMS),
  input: InputDesc::Any,
  output: OutputDesc::Dynamic(crate::shard_doc!("the input, or the matched case's output")),
  targets: Targets::All,
  aliases: &[],
};

pub struct MatchCompiled<B: Backend> {
  /// One value per flow; `None` matches anything.
  pub(crate) values: Vec<Var>,
  pub(crate) flows: Vec<CompiledFlow<B>>,
  pub(crate) passthrough: bool,
}

impl<B: Backend> ControlFlows<B> for MatchCompiled<B> {
  fn flows(&self) -> &[CompiledFlow<B>] {
    &self.flows
  }
}

impl<B: Backend> MatchCompiled<B> {
  /// The index of the first case matching `input`.
  pub(crate) fn find(&self, input: &Var) -> Option<usize> {
    self
      .values
      .iter()
      .position(|v| matches!(v, Var::None) || super::values::values_equal(v, input))
  }
}

pub(crate) fn compose_match<B: Backend>(
  args: &Args,
  ctx: &mut ComposeCtx<'_, B>,
) -> Result<Composed<MatchCompiled<B>>> {
  let input = ctx.input();
  let cases = args.cases("cases").expect("decoded Cases");
  let mut values = Vec::with_capacity(cases.len());
  let mut flows = Vec::with_capacity(cases.len());
  for (value, flow) in cases {
    let ty = value.type_of();
    let can_match = matches!(value, Var::None) || super::values::comparable(input, ty);
    if !can_match {
      return Err(compose_error(
        "Match",
        "cases",
        0,
        "unmatchable-case",
        format!("the case {value} ({ty}) can never match a {input} input"),
      ));
    }
    values.push(value.clone());
    flows.push(ctx.compose_flow_conditional(flow, input)?);
  }
  let passthrough = args.bool("passthrough").unwrap_or(true);
  let output = if passthrough || flows.is_empty() {
    input
  } else {
    let default = values.iter().any(|v| matches!(v, Var::None));
    let mut members: Vec<Type> = flows.iter().map(|f| f.output).collect();
    if !default {
      members.push(input);
    }
    Type::union(members)
  };
  Ok(Composed {
    compiled: MatchCompiled {
      values,
      flows,
      passthrough,
    },
    output,
  })
}

// --- Maybe ---

pub static MAYBE_PARAMS: &[ParamDecl] = &[
  decl(
    "action",
    crate::shard_doc!("The flow to try."),
    FLOW,
    NONE_TYPES,
    Requirement::Required,
  ),
  decl(
    "else",
    crate::shard_doc!("The flow to run, with Maybe's input, if `action` fails."),
    FLOW,
    NONE_TYPES,
    Requirement::Optional,
  ),
  decl(
    "silent",
    crate::shard_doc!("Do not log the error that `action` failed with."),
    Forms::LITERAL,
    &[TypeName::Bool],
    Requirement::Default(DefaultValue::Bool(false)),
  ),
];

pub const MAYBE_DESC: ShardDesc = ShardDesc {
  name: "Maybe",
  version: 1,
  summary: crate::shard_doc!("Runs `action`; if it fails, runs `else` instead."),
  help: crate::shard_doc!(
    "With `else`: on success the output is `action`'s, on an activation error (logged unless `silent`) it is `else`'s. Without `else`, Maybe passes its input through either way, as in 1.x: `action` runs for its effects. Cancellation is not caught. Effects of the part of `action` that ran stay."
  ),
  params: Params::Declared(MAYBE_PARAMS),
  input: InputDesc::Any,
  output: OutputDesc::Dynamic(crate::shard_doc!(
    "Action's or Else's output; the input without Else"
  )),
  targets: Targets::All,
  aliases: &[],
};

/// `flows`: `[action]` or `[action, else]`.
pub struct MaybeCompiled<B: Backend> {
  pub(crate) flows: Vec<CompiledFlow<B>>,
  pub(crate) silent: bool,
}

impl<B: Backend> ControlFlows<B> for MaybeCompiled<B> {
  fn flows(&self) -> &[CompiledFlow<B>] {
    &self.flows
  }
}

pub(crate) fn compose_maybe<B: Backend>(
  args: &Args,
  ctx: &mut ComposeCtx<'_, B>,
) -> Result<Composed<MaybeCompiled<B>>> {
  let input = ctx.input();
  // Action may stop partway, so what it assigns is not definitely assigned.
  let action = ctx.compose_flow_conditional(args.flow("action").expect("decoded"), input)?;
  let els = match args.flow("else") {
    Some(flow) => Some(ctx.compose_flow_conditional(flow, input)?),
    None => None,
  };
  // Without Else, the input passes through (1.x).
  let output = match &els {
    Some(e) => Type::union([action.output, e.output]),
    None => input,
  };
  let mut flows = vec![action];
  flows.extend(els);
  Ok(Composed {
    compiled: MaybeCompiled {
      flows,
      silent: args.bool("silent").unwrap_or(false),
    },
    output,
  })
}

/// What Maybe does with an error from action: `None` to propagate it.
pub(crate) fn maybe_caught(silent: bool, err: Error) -> std::result::Result<(), Error> {
  match err {
    Error::Cancelled => Err(Error::Cancelled),
    other => {
      if !silent {
        crate::log::emit(format!("Maybe: {other}"));
      }
      Ok(())
    }
  }
}

// --- All, Any ---

pub static CONDITIONS_PARAMS: &[ParamDecl] = &[decl(
  "conditions",
  crate::shard_doc!(
    "One or more conditions, checked left to right: Bool literals, Bool variables, or flows that receive the input and output a Bool."
  ),
  Forms::LITERAL.or(Forms::VARIABLE).or(Forms::FLOW),
  &[TypeName::Bool],
  Requirement::Variadic,
)];

pub const ALL_DESC: ShardDesc = ShardDesc {
  name: "All",
  version: 1,
  summary: crate::shard_doc!("Outputs whether every condition is true."),
  help: crate::shard_doc!(
    "Stops at the first false condition (later flows do not run). `If(All(far can-point) ...)` replaces 1.x's `{far | And | can-point}`."
  ),
  params: Params::Declared(CONDITIONS_PARAMS),
  input: InputDesc::Any,
  output: OutputDesc::Fixed(TypeName::Bool),
  targets: Targets::All,
  aliases: &[],
};

pub const ANY_DESC: ShardDesc = ShardDesc {
  name: "Any",
  version: 1,
  summary: crate::shard_doc!("Outputs whether at least one condition is true."),
  help: crate::shard_doc!(
    "Stops at the first true condition (later flows do not run). Replaces 1.x's `Or` in conditions."
  ),
  params: Params::Declared(CONDITIONS_PARAMS),
  input: InputDesc::Any,
  output: OutputDesc::Fixed(TypeName::Bool),
  targets: Targets::All,
  aliases: &[],
};

#[derive(Clone, Copy)]
pub(crate) enum Condition {
  Const(bool),
  Bound(Binding),
  /// An index into the compiled flows.
  Flow(usize),
}

pub struct ConditionsCompiled<B: Backend> {
  pub(crate) conditions: Vec<Condition>,
  pub(crate) flows: Vec<CompiledFlow<B>>,
  /// `All` stops at the first false, `Any` at the first true.
  pub(crate) stop_on: bool,
}

impl<B: Backend> ControlFlows<B> for ConditionsCompiled<B> {
  fn flows(&self) -> &[CompiledFlow<B>] {
    &self.flows
  }
}

pub(crate) fn compose_conditions<B: Backend>(
  args: &Args,
  ctx: &mut ComposeCtx<'_, B>,
  shard: &'static str,
  stop_on: bool,
) -> Result<Composed<ConditionsCompiled<B>>> {
  let input = ctx.input();
  let items = args.variadic("conditions");
  if items.is_empty() {
    return Err(compose_error(
      shard,
      "conditions",
      0,
      "missing-argument",
      format!("{shard} needs at least one condition"),
    ));
  }
  let mut conditions = Vec::with_capacity(items.len());
  let mut flows = Vec::new();
  for (i, item) in items.iter().enumerate() {
    conditions.push(match item {
      ParamValue::Value(Var::Bool(b)) => Condition::Const(*b),
      ParamValue::Var(name) => {
        let info = ctx
          .read_var(name, shard)
          .map_err(|e| e.with_param("conditions", 0))?;
        if info.ty != Type::bool() {
          return Err(compose_error(
            shard,
            "conditions",
            0,
            "wrong-variable-type",
            format!("condition {name} must be a Bool, got {}", info.ty),
          ));
        }
        Condition::Bound(info.binding)
      }
      ParamValue::Flow(flow) => {
        // Only the first condition always runs.
        flows.push(compose_predicate(
          ctx,
          flow,
          input,
          i > 0,
          shard,
          "conditions",
          0,
        )?);
        Condition::Flow(flows.len() - 1)
      }
      other => unreachable!("decoder accepted {other:?} for {shard}.Conditions"),
    });
  }
  Ok(Composed {
    compiled: ConditionsCompiled {
      conditions,
      flows,
      stop_on,
    },
    output: Type::bool(),
  })
}

// --- Repeat: Until and Forever (compose and compiled type in mod.rs) ---

/// Whether the repeat should stop before its next iteration because of
/// `times` (`done` iterations so far).
pub(crate) fn repeat_exhausted(times: Option<i64>, done: i64) -> bool {
  times.is_some_and(|t| done >= t)
}
