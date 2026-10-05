//! Stackless implementations of the prototype shards.
//!
//! Compose and the compiled types are shared with the stackful versions
//! ([`crate::shards`]). Leaf shards differ only in returning [`Step`]. Shards
//! that run nested flows or suspend keep a resume point in their `State`,
//! and reset it on every exit other than [`Step::Suspend`].

use crate::args::Args;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{ActivationCtx, FlowState, Shard, Stackless, Step};
use crate::compose::{CompiledWire, ComposeCtx};
use crate::error::{Error, Result};
use crate::flow::CompiledFlow;
use crate::instance::{CleanupCtx, InstanceCtx};
use crate::shard::Composed;
use crate::shards::*;
use crate::var::Var;

type Ctx<'a, 'b> = &'a mut ComposeCtx<'b, Stackless>;

/// Resets a shard's resume point unless the step is a suspension: on any
/// other exit (including an error) the next activation starts fresh.
fn settle(step: Result<Step>, reset: impl FnOnce()) -> Result<Step> {
  if !matches!(step, Ok(Step::Suspend)) {
    reset();
  }
  step
}

/// Instantiates several flows; if one fails, cleans up the ones already done.
fn instantiate_flows<const N: usize>(
  flows: [&CompiledFlow<Stackless>; N],
  ctx: &mut InstanceCtx,
) -> Result<[FlowState; N]> {
  let mut cleanup_ctx = ctx.cleanup_ctx();
  let done = crate::lifecycle::instantiate_all(
    flows,
    |flow| flow.instantiate(ctx),
    |flow, state| flow.cleanup(state, &mut cleanup_ctx),
  )?;
  let states: Vec<FlowState> = done.into_iter().map(|(_, state)| state).collect();
  Ok(states.try_into().unwrap_or_else(|_| unreachable!()))
}

macro_rules! leaf_instantiate {
  ($compiled:ty) => {
    fn instantiate(_: &$compiled, _: &mut InstanceCtx) -> Result<()> {
      Ok(())
    }
  };
}

// --- leaf shards: same as stackful, returning Step ---

pub struct Spawn;

impl Shard for Spawn {
  type Compiled = Arc<CompiledWire<Stackless>>;
  type State = ();
  const NAME: &'static str = SPAWN_DESC.name;
  const VERSION: u32 = SPAWN_DESC.version;

  fn compose(args: &Args, ctx: Ctx) -> Result<Composed<Arc<CompiledWire<Stackless>>>> {
    compose_spawn(args, ctx)
  }
  leaf_instantiate!(Arc<CompiledWire<Stackless>>);

  fn activate(
    wire: &Arc<CompiledWire<Stackless>>,
    _: &mut (),
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Step> {
    ctx.spawn(wire.clone(), input.clone());
    Ok(Step::Next(input.clone()))
  }
}

// --- shards that suspend, or run nested flows: they keep a resume point ---

/// Suspends for one tick, or until at least `seconds` have passed. The resume
/// point is when it started waiting.
pub struct Pause;

impl Shard for Pause {
  type Compiled = Duration;
  type State = Option<Instant>;
  const NAME: &'static str = PAUSE_DESC.name;
  const VERSION: u32 = PAUSE_DESC.version;

  fn compose(args: &Args, ctx: Ctx) -> Result<Composed<Duration>> {
    compose_pause(args, ctx)
  }

  fn instantiate(_: &Duration, _: &mut InstanceCtx) -> Result<Option<Instant>> {
    Ok(None)
  }

  fn activate(
    duration: &Duration,
    started: &mut Option<Instant>,
    _: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Step> {
    match started {
      None => {
        *started = Some(Instant::now());
        Ok(Step::Suspend)
      }
      Some(start) if start.elapsed() < *duration => Ok(Step::Suspend),
      Some(_) => {
        *started = None;
        Ok(Step::Next(input.clone()))
      }
    }
  }
}

/// Runs another wire inline. Its flow state carries the resume point, so `Do`
/// needs none of its own.
pub struct Do;

pub struct DoState {
  call: Arc<crate::reload::InlineCall<Stackless>>,
  state: Option<FlowState>,
  active: bool,
  /// The mesh reload revision this call site last checked for a new body.
  revision: u64,
}

impl Shard for Do {
  type Compiled = Arc<crate::reload::InlineCall<Stackless>>;
  type State = DoState;
  const NAME: &'static str = DO_DESC.name;
  const VERSION: u32 = DO_DESC.version;

  fn compose(args: &Args, ctx: &mut ComposeCtx<'_, Stackless>) -> Result<Composed<Self::Compiled>> {
    compose_do(args, ctx)
  }

  fn instantiate(call: &Self::Compiled, ctx: &mut InstanceCtx) -> Result<DoState> {
    Ok(DoState {
      call: call.clone(),
      state: Some(call.flow.instantiate(ctx)?),
      active: false,
      revision: 0,
    })
  }

  fn activate(
    _: &Self::Compiled,
    state: &mut DoState,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Step> {
    // The registry changes only when a reload is installed: check it once
    // per revision, not on every call (its keys hash whole definitions).
    if !state.active && state.revision != ctx.reload_revision() {
      state.revision = ctx.reload_revision();
      if let Some(next) = ctx.inline_call(&state.call.key)
        && next.signature == state.call.signature
        && next.deps != state.call.deps
      {
        // Switch, then take the old state before its cleanup: if cleanup
        // panics, terminal cleanup must not attempt it a second time, and
        // the next call instantiates the new body.
        let old_call = std::mem::replace(&mut state.call, next);
        if let Some(mut old) = state.state.take() {
          old_call.flow.cleanup(
            &mut old,
            &mut CleanupCtx {
              instance: ctx.instance(),
            },
          );
        }
      }
    }
    if !state.active {
      if state.state.is_none() {
        state.state = Some(state.call.flow.instantiate(&mut InstanceCtx {
          instance: ctx.instance(),
        })?);
      }
      state.active = true;
    }
    let result = state
      .call
      .flow
      .activate(state.state.as_mut().expect("Do state"), ctx, input);
    if !matches!(result, Ok(Step::Suspend)) {
      state.active = false;
    }
    match result? {
      Step::Return(value) => Ok(Step::Next(value)),
      other => Ok(other),
    }
  }

  fn cleanup(_: &Self::Compiled, state: &mut DoState, ctx: &mut CleanupCtx) {
    if let Some(mut flow_state) = state.state.take() {
      state.call.flow.cleanup(&mut flow_state, ctx);
    }
  }

  fn nested_state_size(_: &Self::Compiled, state: &DoState) -> usize {
    state
      .state
      .as_ref()
      .map_or(0, |s| state.call.flow.state_size(s))
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
  Idle,
  Pred,
  Body,
}

pub struct PredicatedState {
  pred: FlowState,
  body: FlowState,
  /// Resume point: whether the predicate or the body is suspended.
  phase: Phase,
}

fn instantiate_predicated(
  c: &Predicated<Stackless>,
  ctx: &mut InstanceCtx,
) -> Result<PredicatedState> {
  let [pred, body] = instantiate_flows([&c.pred, &c.body], ctx)?;
  Ok(PredicatedState {
    pred,
    body,
    phase: Phase::Idle,
  })
}

fn cleanup_predicated(
  c: &Predicated<Stackless>,
  state: &mut PredicatedState,
  ctx: &mut CleanupCtx,
) {
  // Every nested flow is cleaned up, even if another one panics.
  crate::lifecycle::cleanup_each(
    [(&c.body, &mut state.body), (&c.pred, &mut state.pred)],
    |(flow, state)| flow.cleanup(state, ctx),
  );
}

/// Evaluates (or resumes) the predicate. `Ok(Ok(b))` is its value;
/// `Ok(Err(step))` means return `step` (a suspension or other control flow).
fn eval_pred(
  c: &Predicated<Stackless>,
  state: &mut PredicatedState,
  ctx: &mut ActivationCtx<'_>,
  input: &Var,
) -> Result<std::result::Result<bool, Step>> {
  state.phase = Phase::Pred;
  match c.pred.activate(&mut state.pred, ctx, input)? {
    Step::Next(Var::Bool(b)) => Ok(Ok(b)),
    Step::Next(_) => Err(Error::Activation("predicate did not output a Bool".into())),
    other => Ok(Err(other)),
  }
}

pub struct When;

impl When {
  fn step(
    c: &Predicated<Stackless>,
    state: &mut PredicatedState,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Step> {
    if state.phase != Phase::Body {
      match eval_pred(c, state, ctx, input)? {
        Ok(true) => state.phase = Phase::Body,
        Ok(false) => return Ok(Step::Next(input.clone())),
        Err(step) => return Ok(step),
      }
    }
    match c.body.activate(&mut state.body, ctx, input)? {
      Step::Next(_) => Ok(Step::Next(input.clone())),
      other => Ok(other),
    }
  }
}

impl Shard for When {
  type Compiled = Predicated<Stackless>;
  type State = PredicatedState;
  const NAME: &'static str = WHEN_DESC.name;
  const VERSION: u32 = WHEN_DESC.version;

  fn compose(args: &Args, ctx: Ctx) -> Result<Composed<Predicated<Stackless>>> {
    compose_when(args, ctx)
  }

  fn instantiate(c: &Predicated<Stackless>, ctx: &mut InstanceCtx) -> Result<PredicatedState> {
    instantiate_predicated(c, ctx)
  }

  fn activate(
    c: &Predicated<Stackless>,
    state: &mut PredicatedState,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Step> {
    let step = When::step(c, state, ctx, input);
    settle(step, || state.phase = Phase::Idle)
  }

  fn cleanup(c: &Predicated<Stackless>, state: &mut PredicatedState, ctx: &mut CleanupCtx) {
    cleanup_predicated(c, state, ctx)
  }

  fn nested_state_size(c: &Predicated<Stackless>, state: &PredicatedState) -> usize {
    c.pred.state_size(&state.pred) + c.body.state_size(&state.body)
  }
}

pub struct While;

impl While {
  fn step(
    c: &Predicated<Stackless>,
    state: &mut PredicatedState,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Step> {
    loop {
      if state.phase != Phase::Body {
        match eval_pred(c, state, ctx, input)? {
          Ok(true) => state.phase = Phase::Body,
          Ok(false) => return Ok(Step::Next(input.clone())),
          Err(step) => return Ok(step),
        }
      }
      match c.body.activate(&mut state.body, ctx, input)? {
        Step::Next(_) => state.phase = Phase::Idle,
        other => return Ok(other),
      }
    }
  }
}

impl Shard for While {
  type Compiled = Predicated<Stackless>;
  type State = PredicatedState;
  const NAME: &'static str = WHILE_DESC.name;
  const VERSION: u32 = WHILE_DESC.version;

  fn compose(args: &Args, ctx: Ctx) -> Result<Composed<Predicated<Stackless>>> {
    compose_while(args, ctx)
  }

  fn instantiate(c: &Predicated<Stackless>, ctx: &mut InstanceCtx) -> Result<PredicatedState> {
    instantiate_predicated(c, ctx)
  }

  fn activate(
    c: &Predicated<Stackless>,
    state: &mut PredicatedState,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Step> {
    let step = While::step(c, state, ctx, input);
    settle(step, || state.phase = Phase::Idle)
  }

  fn cleanup(c: &Predicated<Stackless>, state: &mut PredicatedState, ctx: &mut CleanupCtx) {
    cleanup_predicated(c, state, ctx)
  }

  fn nested_state_size(c: &Predicated<Stackless>, state: &PredicatedState) -> usize {
    c.pred.state_size(&state.pred) + c.body.state_size(&state.body)
  }
}

/// Runs its flow on every activation. The flow's own state holds the
/// resume point of a suspension inside it.
pub struct Sub;

impl Shard for Sub {
  type Compiled = CompiledFlow<Stackless>;
  type State = FlowState;
  const NAME: &'static str = SUB_DESC.name;
  const VERSION: u32 = SUB_DESC.version;

  fn compose(args: &Args, ctx: Ctx) -> Result<Composed<CompiledFlow<Stackless>>> {
    compose_sub(args, ctx)
  }

  fn instantiate(body: &CompiledFlow<Stackless>, ctx: &mut InstanceCtx) -> Result<FlowState> {
    body.instantiate(ctx)
  }

  fn activate(
    body: &CompiledFlow<Stackless>,
    state: &mut FlowState,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Step> {
    Ok(match body.activate(state, ctx, input)? {
      Step::Next(_) => Step::Next(input.clone()),
      other => other,
    })
  }

  fn cleanup(body: &CompiledFlow<Stackless>, state: &mut FlowState, ctx: &mut CleanupCtx) {
    body.cleanup(state, ctx);
  }

  fn nested_state_size(body: &CompiledFlow<Stackless>, state: &FlowState) -> usize {
    body.state_size(state)
  }
}

/// The resume point is whether the body is mid-run; `done` is set once the
/// body's first run ends (in any way other than a suspension).
pub struct OnceState {
  done: bool,
  running: bool,
  body: FlowState,
}

pub struct Once;

impl Once {
  fn step(
    body: &CompiledFlow<Stackless>,
    state: &mut OnceState,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Step> {
    if state.done {
      return Ok(Step::Next(input.clone()));
    }
    state.running = true;
    match body.activate(&mut state.body, ctx, input)? {
      Step::Next(_) => Ok(Step::Next(input.clone())),
      other => Ok(other),
    }
  }
}

impl Shard for Once {
  type Compiled = CompiledFlow<Stackless>;
  type State = OnceState;
  const NAME: &'static str = ONCE_DESC.name;
  const VERSION: u32 = ONCE_DESC.version;

  fn compose(args: &Args, ctx: Ctx) -> Result<Composed<CompiledFlow<Stackless>>> {
    compose_once(args, ctx)
  }

  fn instantiate(body: &CompiledFlow<Stackless>, ctx: &mut InstanceCtx) -> Result<OnceState> {
    Ok(OnceState {
      done: false,
      running: false,
      body: body.instantiate(ctx)?,
    })
  }

  fn activate(
    body: &CompiledFlow<Stackless>,
    state: &mut OnceState,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Step> {
    let step = Once::step(body, state, ctx, input);
    // Done only when a run completes: after a failure (caught by Maybe) the
    // next activation runs it again, so what it assigns is really assigned.
    let completed = matches!(step, Ok(ref s) if !matches!(s, Step::Suspend));
    settle(step, || {
      state.running = false;
      if completed {
        state.done = true;
      }
    })
  }

  fn cleanup(body: &CompiledFlow<Stackless>, state: &mut OnceState, ctx: &mut CleanupCtx) {
    body.cleanup(&mut state.body, ctx);
  }

  fn nested_state_size(body: &CompiledFlow<Stackless>, state: &OnceState) -> usize {
    body.state_size(&state.body)
  }
}

/// The resume point: whether the repeat is running, the iterations done and
/// the limit captured when it started, and whether the body (rather than
/// Until) is mid-run.
pub struct RepeatState {
  body: FlowState,
  until: Option<FlowState>,
  active: bool,
  done: i64,
  times: Option<i64>,
  in_body: bool,
}

pub struct Repeat;

impl Repeat {
  fn step(
    c: &RepeatCompiled<Stackless>,
    state: &mut RepeatState,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Step> {
    if !state.active {
      state.times = c.limit(ctx)?;
      state.active = true;
      state.done = 0;
      state.in_body = false;
    }
    loop {
      if !state.in_body {
        if control::repeat_exhausted(state.times, state.done) {
          return Ok(Step::Next(input.clone()));
        }
        if let (Some(until), Some(until_state)) = (&c.until, &mut state.until) {
          match pred_value(until.activate(until_state, ctx, input)?)? {
            Ok(true) => return Ok(Step::Next(input.clone())),
            Ok(false) => {}
            Err(step) => return Ok(step),
          }
        }
        state.in_body = true;
      }
      match c.body.activate(&mut state.body, ctx, input)? {
        Step::Next(_) => {
          state.done += 1;
          state.in_body = false;
        }
        other => return Ok(other),
      }
    }
  }
}

impl Shard for Repeat {
  type Compiled = RepeatCompiled<Stackless>;
  type State = RepeatState;
  const NAME: &'static str = REPEAT_DESC.name;
  const VERSION: u32 = REPEAT_DESC.version;

  fn compose(args: &Args, ctx: Ctx) -> Result<Composed<RepeatCompiled<Stackless>>> {
    compose_repeat(args, ctx)
  }

  fn instantiate(c: &RepeatCompiled<Stackless>, ctx: &mut InstanceCtx) -> Result<RepeatState> {
    let mut flows = vec![&c.body];
    flows.extend(c.until.as_ref());
    let mut states = instantiate_list(&flows, ctx)?.into_iter();
    Ok(RepeatState {
      body: states.next().expect("body state"),
      until: states.next(),
      active: false,
      done: 0,
      times: None,
      in_body: false,
    })
  }

  fn activate(
    c: &RepeatCompiled<Stackless>,
    state: &mut RepeatState,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Step> {
    let step = Repeat::step(c, state, ctx, input);
    settle(step, || {
      state.active = false;
      state.in_body = false;
    })
  }

  fn cleanup(c: &RepeatCompiled<Stackless>, state: &mut RepeatState, ctx: &mut CleanupCtx) {
    let mut items = vec![(&c.body, &mut state.body)];
    if let (Some(until), Some(s)) = (&c.until, &mut state.until) {
      items.push((until, s));
    }
    crate::lifecycle::cleanup_each(items.into_iter().rev(), |(flow, s)| flow.cleanup(s, ctx));
  }

  fn nested_state_size(c: &RepeatCompiled<Stackless>, state: &RepeatState) -> usize {
    c.body.state_size(&state.body)
      + match (&c.until, &state.until) {
        (Some(f), Some(s)) => f.state_size(s),
        _ => 0,
      }
  }
}

// --- If, Match, Maybe, All, Any ---

use crate::shards::control::{
  self, Condition, ConditionsCompiled, ControlFlows, IfCompiled, MatchCompiled, MaybeCompiled,
};

/// A predicate flow's step as a Bool, or the step to return.
fn pred_value(step: Step) -> Result<std::result::Result<bool, Step>> {
  match step {
    Step::Next(Var::Bool(b)) => Ok(Ok(b)),
    Step::Next(_) => Err(Error::Activation("predicate did not output a Bool".into())),
    other => Ok(Err(other)),
  }
}

fn instantiate_list(
  flows: &[&CompiledFlow<Stackless>],
  ctx: &mut InstanceCtx,
) -> Result<Vec<FlowState>> {
  let mut cleanup_ctx = ctx.cleanup_ctx();
  let done = crate::lifecycle::instantiate_all(
    flows.iter().copied(),
    |flow| flow.instantiate(ctx),
    |flow, state| flow.cleanup(state, &mut cleanup_ctx),
  )?;
  Ok(done.into_iter().map(|(_, state)| state).collect())
}

/// A control shard's state: its flows' states, and the resume point (the
/// flow, or condition, that is suspended).
pub struct ControlState {
  flows: Vec<FlowState>,
  resume: Option<usize>,
}

macro_rules! control_shard {
  ($ty:ident, $compiled:ty, $desc:expr, $compose:expr) => {
    impl Shard for $ty {
      type Compiled = $compiled;
      type State = ControlState;
      const NAME: &'static str = $desc.name;
      const VERSION: u32 = $desc.version;

      fn compose(args: &Args, ctx: Ctx) -> Result<Composed<$compiled>> {
        $compose(args, ctx)
      }

      fn instantiate(c: &$compiled, ctx: &mut InstanceCtx) -> Result<ControlState> {
        let flows: Vec<&CompiledFlow<Stackless>> = c.flows().iter().collect();
        Ok(ControlState {
          flows: instantiate_list(&flows, ctx)?,
          resume: None,
        })
      }

      fn activate(
        c: &$compiled,
        state: &mut ControlState,
        ctx: &mut ActivationCtx<'_>,
        input: &Var,
      ) -> Result<Step> {
        let step = $ty::step(c, state, ctx, input);
        settle(step, || state.resume = None)
      }

      fn cleanup(c: &$compiled, state: &mut ControlState, ctx: &mut CleanupCtx) {
        crate::lifecycle::cleanup_each(
          c.flows().iter().zip(state.flows.iter_mut()).rev(),
          |(flow, s)| flow.cleanup(s, ctx),
        );
      }

      fn nested_state_size(c: &$compiled, state: &ControlState) -> usize {
        c.flows()
          .iter()
          .zip(&state.flows)
          .map(|(f, s)| f.state_size(s))
          .sum()
      }
    }
  };
}

pub struct If;

impl If {
  /// Resume points: 0 is the predicate, 1 and 2 the branches.
  fn step(
    c: &IfCompiled<Stackless>,
    state: &mut ControlState,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Step> {
    let branch = match state.resume.unwrap_or(0) {
      0 => {
        state.resume = Some(0);
        match pred_value(c.flows[0].activate(&mut state.flows[0], ctx, input)?)? {
          Ok(true) => 1,
          Ok(false) if c.flows.len() > 2 => 2,
          Ok(false) => return Ok(Step::Next(input.clone())),
          Err(step) => return Ok(step),
        }
      }
      branch => branch,
    };
    state.resume = Some(branch);
    match c.flows[branch].activate(&mut state.flows[branch], ctx, input)? {
      Step::Next(v) => Ok(Step::Next(if c.passthrough { input.clone() } else { v })),
      other => Ok(other),
    }
  }
}

control_shard!(
  If,
  IfCompiled<Stackless>,
  control::IF_DESC,
  control::compose_if
);

pub struct Match;

impl Match {
  /// The resume point is the matched case.
  fn step(
    c: &MatchCompiled<Stackless>,
    state: &mut ControlState,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Step> {
    let case = match state.resume {
      Some(case) => case,
      None => match c.find(input) {
        Some(case) => case,
        None => return Ok(Step::Next(input.clone())),
      },
    };
    state.resume = Some(case);
    match c.flows[case].activate(&mut state.flows[case], ctx, input)? {
      Step::Next(v) => Ok(Step::Next(if c.passthrough { input.clone() } else { v })),
      other => Ok(other),
    }
  }
}

control_shard!(
  Match,
  MatchCompiled<Stackless>,
  control::MATCH_DESC,
  control::compose_match
);

pub struct Maybe;

impl Maybe {
  /// Resume points: 0 is Action, 1 is Else.
  fn step(
    c: &MaybeCompiled<Stackless>,
    state: &mut ControlState,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Step> {
    if state.resume.unwrap_or(0) == 0 {
      state.resume = Some(0);
      match c.flows[0].activate(&mut state.flows[0], ctx, input) {
        // Without Else, the input passes through (1.x).
        Ok(Step::Next(_)) if c.flows.len() < 2 => return Ok(Step::Next(input.clone())),
        Ok(step) => return Ok(step),
        Err(err) => {
          control::maybe_caught(c.silent, err)?;
          if c.flows.len() < 2 {
            return Ok(Step::Next(input.clone()));
          }
        }
      }
    }
    state.resume = Some(1);
    c.flows[1].activate(&mut state.flows[1], ctx, input)
  }
}

control_shard!(
  Maybe,
  MaybeCompiled<Stackless>,
  control::MAYBE_DESC,
  control::compose_maybe
);

/// Shared by All and Any. The resume point is the condition whose flow is
/// suspended.
fn step_conditions(
  c: &ConditionsCompiled<Stackless>,
  state: &mut ControlState,
  ctx: &mut ActivationCtx<'_>,
  input: &Var,
) -> Result<Step> {
  let start = state.resume.unwrap_or(0);
  for (k, condition) in c.conditions.iter().enumerate().skip(start) {
    let value = match *condition {
      Condition::Const(b) => b,
      Condition::Bound(binding) => matches!(ctx.get(binding), Var::Bool(true)),
      Condition::Flow(i) => {
        state.resume = Some(k);
        match pred_value(c.flows[i].activate(&mut state.flows[i], ctx, input)?)? {
          Ok(b) => b,
          Err(step) => return Ok(step),
        }
      }
    };
    if value == c.stop_on {
      return Ok(Step::Next(Var::Bool(c.stop_on)));
    }
  }
  Ok(Step::Next(Var::Bool(!c.stop_on)))
}

pub struct All;

impl All {
  fn step(
    c: &ConditionsCompiled<Stackless>,
    state: &mut ControlState,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Step> {
    step_conditions(c, state, ctx, input)
  }
}

control_shard!(
  All,
  ConditionsCompiled<Stackless>,
  control::ALL_DESC,
  |args, ctx| { control::compose_conditions(args, ctx, "All", false) }
);

pub struct Any;

impl Any {
  fn step(
    c: &ConditionsCompiled<Stackless>,
    state: &mut ControlState,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Step> {
    step_conditions(c, state, ctx, input)
  }
}

control_shard!(
  Any,
  ConditionsCompiled<Stackless>,
  control::ANY_DESC,
  |args, ctx| { control::compose_conditions(args, ctx, "Any", true) }
);
