//! Composite adapters expose immutable child code to the iterative runner.
use super::{ActivationCtx, Control, Shard, Stackless, Step};
use crate::args::Args;
use crate::compose::{CompiledWire, ComposeCtx};
use crate::error::Result;
use crate::flow::CompiledFlow;
use crate::instance::InstanceCtx;
use crate::shard::Composed;
use crate::shards::control;
use crate::shards::*;
use crate::var::Var;
use std::sync::Arc;
use std::time::{Duration, Instant};
type Ctx<'a, 'b> = &'a mut ComposeCtx<'b, Stackless>;
macro_rules! leaf_instantiate {
  ($compiled:ty) => {
    fn instantiate(_: &$compiled, _: &mut InstanceCtx) -> Result<()> {
      Ok(())
    }
  };
}
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

macro_rules! composite {
  ($name:ident, $compiled:ty, $desc:expr, $compose:expr, $variant:ident) => {
    pub struct $name;
    impl Shard for $name {
      type Compiled = $compiled;
      type State = ();
      const NAME: &'static str = $desc.name;
      const VERSION: u32 = $desc.version;
      fn compose(args: &Args, ctx: Ctx) -> Result<Composed<Self::Compiled>> {
        $compose(args, ctx)
      }
      fn instantiate(_: &Self::Compiled, _: &mut InstanceCtx) -> Result<()> {
        Ok(())
      }
      fn control(c: &Self::Compiled) -> Option<Control<'_>> {
        Some(Control::$variant(c))
      }
      fn activate(
        _: &Self::Compiled,
        _: &mut (),
        _: &mut ActivationCtx<'_>,
        _: &Var,
      ) -> Result<Step> {
        unreachable!("composites are dispatched by the runner")
      }
    }
  };
}
composite!(
  Do,
  Arc<crate::reload::InlineCall<Stackless>>,
  DO_DESC,
  compose_do,
  Do
);
composite!(When, Predicated<Stackless>, WHEN_DESC, compose_when, When);
composite!(
  While,
  Predicated<Stackless>,
  WHILE_DESC,
  compose_while,
  While
);
composite!(Sub, CompiledFlow<Stackless>, SUB_DESC, compose_sub, Sub);
composite!(Once, CompiledFlow<Stackless>, ONCE_DESC, compose_once, Once);
composite!(
  Repeat,
  RepeatCompiled<Stackless>,
  REPEAT_DESC,
  compose_repeat,
  Repeat
);
composite!(
  If,
  control::IfCompiled<Stackless>,
  control::IF_DESC,
  control::compose_if,
  If
);
composite!(
  Match,
  control::MatchCompiled<Stackless>,
  control::MATCH_DESC,
  control::compose_match,
  Match
);
composite!(
  Maybe,
  control::MaybeCompiled<Stackless>,
  control::MAYBE_DESC,
  control::compose_maybe,
  Maybe
);
composite!(
  All,
  control::ConditionsCompiled<Stackless>,
  control::ALL_DESC,
  |args, ctx| control::compose_conditions(args, ctx, "All", false),
  Conditions
);
composite!(
  Any,
  control::ConditionsCompiled<Stackless>,
  control::ANY_DESC,
  |args, ctx| control::compose_conditions(args, ctx, "Any", true),
  Conditions
);
