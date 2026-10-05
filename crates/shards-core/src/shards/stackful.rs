//! Stackful implementations of the prototype shards (the reference).
//! Suspension happens inside `ActivationCtx::suspend`, wherever a shard is.

use crate::args::Args;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::*;
use crate::flow::FlowState;
use crate::instance::{CleanupCtx, InstanceCtx};
use crate::runtime::ActivationCtx;
use crate::shard::{Flow, Shard};

/// Instantiates several flows; if one fails, cleans up the ones already done.
fn instantiate_flows<const N: usize>(
  flows: [&CompiledFlow<Stackful>; N],
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

fn eval_pred(
  pred: &CompiledFlow<Stackful>,
  state: &mut FlowState,
  ctx: &mut ActivationCtx<'_>,
  input: &Var,
) -> Result<std::result::Result<bool, Flow>> {
  match pred.activate(state, ctx, input)? {
    Flow::Next(Var::Bool(b)) => Ok(Ok(b)),
    Flow::Next(_) => Err(Error::Activation("predicate did not output a Bool".into())),
    other => Ok(Err(other)),
  }
}

/// Runs `body` if `pred` is true. Passes its input through.
pub struct When;

impl Shard for When {
  type Compiled = Predicated<Stackful>;
  type State = [FlowState; 2];
  const NAME: &'static str = WHEN_DESC.name;
  const VERSION: u32 = WHEN_DESC.version;

  fn compose(
    args: &Args,
    ctx: &mut ComposeCtx<'_, Stackful>,
  ) -> Result<Composed<Predicated<Stackful>>> {
    compose_when(args, ctx)
  }

  fn instantiate(c: &Predicated<Stackful>, ctx: &mut InstanceCtx) -> Result<[FlowState; 2]> {
    instantiate_flows([&c.pred, &c.body], ctx)
  }

  fn activate(
    c: &Predicated<Stackful>,
    [pred, body]: &mut [FlowState; 2],
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Flow> {
    match eval_pred(&c.pred, pred, ctx, input)? {
      Ok(true) => match c.body.activate(body, ctx, input)? {
        Flow::Next(_) => Ok(Flow::Next(input.clone())),
        other => Ok(other),
      },
      Ok(false) => Ok(Flow::Next(input.clone())),
      Err(flow) => Ok(flow),
    }
  }

  fn cleanup(c: &Predicated<Stackful>, [pred, body]: &mut [FlowState; 2], ctx: &mut CleanupCtx) {
    // Every nested flow is cleaned up, even if another one panics.
    crate::lifecycle::cleanup_each([(&c.body, body), (&c.pred, pred)], |(flow, state)| {
      flow.cleanup(state, ctx)
    });
  }

  fn nested_state_size(c: &Predicated<Stackful>, [pred, body]: &[FlowState; 2]) -> usize {
    c.pred.state_size(pred) + c.body.state_size(body)
  }
}

/// Runs `body` while `pred` is true. Passes its input through.
pub struct While;

impl Shard for While {
  type Compiled = Predicated<Stackful>;
  type State = [FlowState; 2];
  const NAME: &'static str = WHILE_DESC.name;
  const VERSION: u32 = WHILE_DESC.version;

  fn compose(
    args: &Args,
    ctx: &mut ComposeCtx<'_, Stackful>,
  ) -> Result<Composed<Predicated<Stackful>>> {
    compose_while(args, ctx)
  }

  fn instantiate(c: &Predicated<Stackful>, ctx: &mut InstanceCtx) -> Result<[FlowState; 2]> {
    instantiate_flows([&c.pred, &c.body], ctx)
  }

  fn activate(
    c: &Predicated<Stackful>,
    [pred, body]: &mut [FlowState; 2],
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Flow> {
    loop {
      match eval_pred(&c.pred, pred, ctx, input)? {
        Ok(true) => match c.body.activate(body, ctx, input)? {
          Flow::Next(_) => {}
          other => return Ok(other),
        },
        Ok(false) => return Ok(Flow::Next(input.clone())),
        Err(flow) => return Ok(flow),
      }
    }
  }

  fn cleanup(c: &Predicated<Stackful>, [pred, body]: &mut [FlowState; 2], ctx: &mut CleanupCtx) {
    // Every nested flow is cleaned up, even if another one panics.
    crate::lifecycle::cleanup_each([(&c.body, body), (&c.pred, pred)], |(flow, state)| {
      flow.cleanup(state, ctx)
    });
  }

  fn nested_state_size(c: &Predicated<Stackful>, [pred, body]: &[FlowState; 2]) -> usize {
    c.pred.state_size(pred) + c.body.state_size(body)
  }
}

/// Runs `body` on every activation. Passes its input through.
pub struct Sub;

impl Shard for Sub {
  type Compiled = CompiledFlow<Stackful>;
  type State = FlowState;
  const NAME: &'static str = SUB_DESC.name;
  const VERSION: u32 = SUB_DESC.version;

  fn compose(
    args: &Args,
    ctx: &mut ComposeCtx<'_, Stackful>,
  ) -> Result<Composed<CompiledFlow<Stackful>>> {
    compose_sub(args, ctx)
  }

  fn instantiate(body: &CompiledFlow<Stackful>, ctx: &mut InstanceCtx) -> Result<FlowState> {
    body.instantiate(ctx)
  }

  fn activate(
    body: &CompiledFlow<Stackful>,
    state: &mut FlowState,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Flow> {
    match body.activate(state, ctx, input)? {
      Flow::Next(_) => Ok(Flow::Next(input.clone())),
      other => Ok(other),
    }
  }

  fn cleanup(body: &CompiledFlow<Stackful>, state: &mut FlowState, ctx: &mut CleanupCtx) {
    body.cleanup(state, ctx);
  }

  fn nested_state_size(body: &CompiledFlow<Stackful>, state: &FlowState) -> usize {
    body.state_size(state)
  }
}

/// Runs `body` on the first activation only. Passes its input through.
pub struct Once;

pub struct OnceState {
  done: bool,
  body: FlowState,
}

impl Shard for Once {
  type Compiled = CompiledFlow<Stackful>;
  type State = OnceState;
  const NAME: &'static str = ONCE_DESC.name;
  const VERSION: u32 = ONCE_DESC.version;

  fn compose(
    args: &Args,
    ctx: &mut ComposeCtx<'_, Stackful>,
  ) -> Result<Composed<CompiledFlow<Stackful>>> {
    compose_once(args, ctx)
  }

  fn instantiate(body: &CompiledFlow<Stackful>, ctx: &mut InstanceCtx) -> Result<OnceState> {
    Ok(OnceState {
      done: false,
      body: body.instantiate(ctx)?,
    })
  }

  fn activate(
    body: &CompiledFlow<Stackful>,
    state: &mut OnceState,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Flow> {
    if !state.done {
      // Done only when a run completes: an error leaves it to run again.
      let flow = body.activate(&mut state.body, ctx, input)?;
      state.done = true;
      if let flow @ (Flow::Stop | Flow::Restart | Flow::Return(_)) = flow {
        return Ok(flow);
      }
    }
    Ok(Flow::Next(input.clone()))
  }

  fn cleanup(body: &CompiledFlow<Stackful>, state: &mut OnceState, ctx: &mut CleanupCtx) {
    body.cleanup(&mut state.body, ctx);
  }

  fn nested_state_size(body: &CompiledFlow<Stackful>, state: &OnceState) -> usize {
    body.state_size(&state.body)
  }
}

/// Runs `body` a number of times, until `until` is true, or forever.
/// Passes its input through.
pub struct Repeat;

pub struct RepeatState {
  body: FlowState,
  until: Option<FlowState>,
}

impl Shard for Repeat {
  type Compiled = RepeatCompiled<Stackful>;
  type State = RepeatState;
  const NAME: &'static str = REPEAT_DESC.name;
  const VERSION: u32 = REPEAT_DESC.version;

  fn compose(
    args: &Args,
    ctx: &mut ComposeCtx<'_, Stackful>,
  ) -> Result<Composed<RepeatCompiled<Stackful>>> {
    compose_repeat(args, ctx)
  }

  fn instantiate(c: &RepeatCompiled<Stackful>, ctx: &mut InstanceCtx) -> Result<RepeatState> {
    let mut flows = vec![&c.body];
    flows.extend(c.until.as_ref());
    let mut states = instantiate_list(&flows, ctx)?.into_iter();
    Ok(RepeatState {
      body: states.next().expect("body state"),
      until: states.next(),
    })
  }

  fn activate(
    c: &RepeatCompiled<Stackful>,
    state: &mut RepeatState,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Flow> {
    let limit = c.limit(ctx)?;
    let mut done = 0;
    while !control::repeat_exhausted(limit, done) {
      if let (Some(until), Some(until_state)) = (&c.until, &mut state.until) {
        match eval_pred(until, until_state, ctx, input)? {
          Ok(true) => break,
          Ok(false) => {}
          Err(flow) => return Ok(flow),
        }
      }
      if let flow @ (Flow::Stop | Flow::Restart | Flow::Return(_)) =
        c.body.activate(&mut state.body, ctx, input)?
      {
        return Ok(flow);
      }
      done += 1;
    }
    Ok(Flow::Next(input.clone()))
  }

  fn cleanup(c: &RepeatCompiled<Stackful>, state: &mut RepeatState, ctx: &mut CleanupCtx) {
    let mut items = vec![(&c.body, &mut state.body)];
    if let (Some(until), Some(s)) = (&c.until, &mut state.until) {
      items.push((until, s));
    }
    crate::lifecycle::cleanup_each(items.into_iter().rev(), |(flow, s)| flow.cleanup(s, ctx));
  }

  fn nested_state_size(c: &RepeatCompiled<Stackful>, state: &RepeatState) -> usize {
    c.body.state_size(&state.body)
      + match (&c.until, &state.until) {
        (Some(f), Some(s)) => f.state_size(s),
        _ => 0,
      }
  }
}

/// Runs another wire inline, sharing the caller's local frame. A `Return`
/// inside it ends only the sub-wire.
pub struct Do;

impl Shard for Do {
  type Compiled = CompiledFlow<Stackful>;
  type State = FlowState;
  const NAME: &'static str = DO_DESC.name;
  const VERSION: u32 = DO_DESC.version;

  fn compose(
    args: &Args,
    ctx: &mut ComposeCtx<'_, Stackful>,
  ) -> Result<Composed<CompiledFlow<Stackful>>> {
    compose_do(args, ctx)
  }

  fn instantiate(flow: &CompiledFlow<Stackful>, ctx: &mut InstanceCtx) -> Result<FlowState> {
    flow.instantiate(ctx)
  }

  fn activate(
    flow: &CompiledFlow<Stackful>,
    state: &mut FlowState,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Flow> {
    match flow.activate(state, ctx, input)? {
      Flow::Return(value) => Ok(Flow::Next(value)),
      other => Ok(other),
    }
  }

  fn cleanup(flow: &CompiledFlow<Stackful>, state: &mut FlowState, ctx: &mut CleanupCtx) {
    flow.cleanup(state, ctx);
  }

  fn nested_state_size(flow: &CompiledFlow<Stackful>, state: &FlowState) -> usize {
    flow.state_size(state)
  }
}

/// Suspends until the next tick, or until at least `seconds` have passed.
/// Passes its input through.
pub struct Pause;

impl Shard for Pause {
  type Compiled = Duration;
  type State = ();
  const NAME: &'static str = PAUSE_DESC.name;
  const VERSION: u32 = PAUSE_DESC.version;

  fn compose(args: &Args, ctx: &mut ComposeCtx<'_, Stackful>) -> Result<Composed<Duration>> {
    compose_pause(args, ctx)
  }
  leaf_instantiate!(Duration);

  fn activate(
    duration: &Duration,
    _: &mut (),
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Flow> {
    let start = Instant::now();
    ctx.suspend()?;
    while start.elapsed() < *duration {
      ctx.suspend()?;
    }
    Ok(Flow::Next(input.clone()))
  }
}

/// Schedules a new instance of a wire, with this shard's input as its input.
/// The wire is compiled once, through the compose cache; every spawned
/// instance shares it. Passes its input through.
pub struct Spawn;

impl Shard for Spawn {
  type Compiled = Arc<CompiledWire<Stackful>>;
  type State = ();
  const NAME: &'static str = SPAWN_DESC.name;
  const VERSION: u32 = SPAWN_DESC.version;

  fn compose(
    args: &Args,
    ctx: &mut ComposeCtx<'_, Stackful>,
  ) -> Result<Composed<Arc<CompiledWire<Stackful>>>> {
    compose_spawn(args, ctx)
  }
  leaf_instantiate!(Arc<CompiledWire<Stackful>>);

  fn activate(
    wire: &Arc<CompiledWire<Stackful>>,
    _: &mut (),
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Flow> {
    ctx.spawn(wire.clone(), input.clone());
    Ok(Flow::Next(input.clone()))
  }
}

// --- If, Match, Maybe, All, Any ---

use super::control::{
  self, Condition, ConditionsCompiled, ControlFlows, IfCompiled, MatchCompiled, MaybeCompiled,
};

/// Instantiates a list of flows; if one fails, cleans up the ones done.
fn instantiate_list(
  flows: &[&CompiledFlow<Stackful>],
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

fn instantiate_control<C: ControlFlows<Stackful>>(
  c: &C,
  ctx: &mut InstanceCtx,
) -> Result<Vec<FlowState>> {
  let flows: Vec<&CompiledFlow<Stackful>> = c.flows().iter().collect();
  instantiate_list(&flows, ctx)
}

fn cleanup_control<C: ControlFlows<Stackful>>(
  c: &C,
  states: &mut [FlowState],
  ctx: &mut CleanupCtx,
) {
  crate::lifecycle::cleanup_each(
    c.flows().iter().zip(states.iter_mut()).rev(),
    |(flow, state)| flow.cleanup(state, ctx),
  );
}

fn control_size<C: ControlFlows<Stackful>>(c: &C, states: &[FlowState]) -> usize {
  c.flows()
    .iter()
    .zip(states)
    .map(|(f, s)| f.state_size(s))
    .sum()
}

/// The shard trait boilerplate shared by the control shards.
macro_rules! control_shard {
  ($ty:ident, $compiled:ty, $desc:expr, $compose:expr) => {
    impl Shard for $ty {
      type Compiled = $compiled;
      type State = Vec<FlowState>;
      const NAME: &'static str = $desc.name;
      const VERSION: u32 = $desc.version;

      fn compose(args: &Args, ctx: &mut ComposeCtx<'_, Stackful>) -> Result<Composed<$compiled>> {
        $compose(args, ctx)
      }

      fn instantiate(c: &$compiled, ctx: &mut InstanceCtx) -> Result<Vec<FlowState>> {
        instantiate_control(c, ctx)
      }

      fn activate(
        c: &$compiled,
        states: &mut Vec<FlowState>,
        ctx: &mut ActivationCtx<'_>,
        input: &Var,
      ) -> Result<Flow> {
        $ty::run(c, states, ctx, input)
      }

      fn cleanup(c: &$compiled, states: &mut Vec<FlowState>, ctx: &mut CleanupCtx) {
        cleanup_control(c, states, ctx)
      }

      fn nested_state_size(c: &$compiled, states: &Vec<FlowState>) -> usize {
        control_size(c, states)
      }
    }
  };
}

pub struct If;

impl If {
  fn run(
    c: &IfCompiled<Stackful>,
    states: &mut [FlowState],
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Flow> {
    let branch = match eval_pred(&c.flows[0], &mut states[0], ctx, input)? {
      Ok(true) => 1,
      Ok(false) if c.flows.len() > 2 => 2,
      Ok(false) => return Ok(Flow::Next(input.clone())),
      Err(flow) => return Ok(flow),
    };
    match c.flows[branch].activate(&mut states[branch], ctx, input)? {
      Flow::Next(v) => Ok(Flow::Next(if c.passthrough { input.clone() } else { v })),
      other => Ok(other),
    }
  }
}

control_shard!(
  If,
  IfCompiled<Stackful>,
  control::IF_DESC,
  control::compose_if
);

pub struct Match;

impl Match {
  fn run(
    c: &MatchCompiled<Stackful>,
    states: &mut [FlowState],
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Flow> {
    let Some(i) = c.find(input) else {
      return Ok(Flow::Next(input.clone()));
    };
    match c.flows[i].activate(&mut states[i], ctx, input)? {
      Flow::Next(v) => Ok(Flow::Next(if c.passthrough { input.clone() } else { v })),
      other => Ok(other),
    }
  }
}

control_shard!(
  Match,
  MatchCompiled<Stackful>,
  control::MATCH_DESC,
  control::compose_match
);

pub struct Maybe;

impl Maybe {
  fn run(
    c: &MaybeCompiled<Stackful>,
    states: &mut [FlowState],
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Flow> {
    match c.flows[0].activate(&mut states[0], ctx, input) {
      // Without Else, the input passes through (1.x).
      Ok(Flow::Next(_)) if c.flows.len() < 2 => Ok(Flow::Next(input.clone())),
      Ok(flow) => Ok(flow),
      Err(err) => {
        control::maybe_caught(c.silent, err)?;
        if c.flows.len() > 1 {
          c.flows[1].activate(&mut states[1], ctx, input)
        } else {
          Ok(Flow::Next(input.clone()))
        }
      }
    }
  }
}

control_shard!(
  Maybe,
  MaybeCompiled<Stackful>,
  control::MAYBE_DESC,
  control::compose_maybe
);

/// Shared by All and Any: stops at the first condition equal to `stop_on`.
fn run_conditions(
  c: &ConditionsCompiled<Stackful>,
  states: &mut [FlowState],
  ctx: &mut ActivationCtx<'_>,
  input: &Var,
) -> Result<Flow> {
  for condition in &c.conditions {
    let value = match *condition {
      Condition::Const(b) => b,
      Condition::Bound(binding) => matches!(ctx.get(binding), Var::Bool(true)),
      Condition::Flow(i) => match eval_pred(&c.flows[i], &mut states[i], ctx, input)? {
        Ok(b) => b,
        Err(flow) => return Ok(flow),
      },
    };
    if value == c.stop_on {
      return Ok(Flow::Next(Var::Bool(c.stop_on)));
    }
  }
  Ok(Flow::Next(Var::Bool(!c.stop_on)))
}

pub struct All;

impl All {
  fn run(
    c: &ConditionsCompiled<Stackful>,
    states: &mut [FlowState],
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Flow> {
    run_conditions(c, states, ctx, input)
  }
}

control_shard!(
  All,
  ConditionsCompiled<Stackful>,
  control::ALL_DESC,
  |args, ctx| { control::compose_conditions(args, ctx, "All", false) }
);

pub struct Any;

impl Any {
  fn run(
    c: &ConditionsCompiled<Stackful>,
    states: &mut [FlowState],
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Flow> {
    run_conditions(c, states, ctx, input)
  }
}

control_shard!(
  Any,
  ConditionsCompiled<Stackful>,
  control::ANY_DESC,
  |args, ctx| { control::compose_conditions(args, ctx, "Any", true) }
);
