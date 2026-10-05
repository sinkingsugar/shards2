//! Shards that cannot suspend, with one implementation for both schedulers.
//!
//! "Leaf" means the shard never suspends, not that it does little work. An
//! I/O wrapper qualifies only if it returns without blocking; one that waits
//! on I/O is an async shard ([`crate::shards::async_shard`]).
//!
//! [`Leaf`] adapts a [`LeafShard`] to each scheduler's shard trait: its
//! result becomes a stackful `Flow` or a stackless `Step` at the boundary.

use crate::args::Args;
use std::marker::PhantomData;

use super::*;
use crate::describe::ShardDesc;
use crate::instance::{CleanupCtx, InstanceCtx, LeafCtx};
use crate::shard::{Flow, Shard};
use crate::stackless::{self, Stackless, Step};

/// A shard that cannot suspend. Prototype API.
pub trait LeafShard: 'static {
  type Compiled: Send + Sync + 'static;
  type State: 'static;

  /// The shard's description: identity, documentation and parameters. The
  /// shared decoder enforces the declared parameters.
  const DESC: ShardDesc;

  fn compose<B: Backend>(
    args: &Args,
    ctx: &mut ComposeCtx<'_, B>,
  ) -> Result<Composed<Self::Compiled>>;

  fn instantiate(compiled: &Self::Compiled, ctx: &mut InstanceCtx) -> Result<Self::State>;

  /// Must not block. Can stop, restart or return, but not suspend.
  fn activate(
    compiled: &Self::Compiled,
    state: &mut Self::State,
    ctx: &mut impl LeafCtx,
    input: &Var,
  ) -> Result<Flow>;

  fn cleanup(_compiled: &Self::Compiled, _state: &mut Self::State, _ctx: &mut CleanupCtx) {}
}

/// Adapts a [`LeafShard`] to both schedulers.
pub struct Leaf<L>(PhantomData<fn() -> L>);

/// The shard type of a leaf shard, with both implementations.
pub const fn leaf_type<L: LeafShard>() -> ShardType {
  ShardType::new(L::DESC)
    .with_stackful::<Leaf<L>>()
    .with_stackless::<Leaf<L>>()
}

impl<L: LeafShard> Shard for Leaf<L> {
  type Compiled = L::Compiled;
  type State = L::State;
  const NAME: &'static str = L::DESC.name;
  const VERSION: u32 = L::DESC.version;

  fn compose(args: &Args, ctx: &mut ComposeCtx<'_, Stackful>) -> Result<Composed<L::Compiled>> {
    L::compose(args, ctx)
  }

  fn instantiate(c: &L::Compiled, ctx: &mut InstanceCtx) -> Result<L::State> {
    L::instantiate(c, ctx)
  }

  fn activate(
    c: &L::Compiled,
    s: &mut L::State,
    ctx: &mut crate::runtime::ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Flow> {
    L::activate(c, s, ctx, input)
  }

  fn cleanup(c: &L::Compiled, s: &mut L::State, ctx: &mut CleanupCtx) {
    L::cleanup(c, s, ctx)
  }
}

impl<L: LeafShard> stackless::Shard for Leaf<L> {
  type Compiled = L::Compiled;
  type State = L::State;
  const NAME: &'static str = L::DESC.name;
  const VERSION: u32 = L::DESC.version;

  fn compose(args: &Args, ctx: &mut ComposeCtx<'_, Stackless>) -> Result<Composed<L::Compiled>> {
    L::compose(args, ctx)
  }

  fn instantiate(c: &L::Compiled, ctx: &mut InstanceCtx) -> Result<L::State> {
    L::instantiate(c, ctx)
  }

  fn activate(
    c: &L::Compiled,
    s: &mut L::State,
    ctx: &mut stackless::ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Step> {
    Ok(match L::activate(c, s, ctx, input)? {
      Flow::Next(v) => Step::Next(v),
      Flow::Stop => Step::Stop,
      Flow::Restart => Step::Restart,
      Flow::Return(v) => Step::Return(v),
    })
  }

  fn cleanup(c: &L::Compiled, s: &mut L::State, ctx: &mut CleanupCtx) {
    L::cleanup(c, s, ctx)
  }
}

macro_rules! no_state {
  ($compiled:ty) => {
    type State = ();
    fn instantiate(_: &$compiled, _: &mut InstanceCtx) -> Result<()> {
      Ok(())
    }
  };
}

pub struct Const;

impl LeafShard for Const {
  type Compiled = Var;
  no_state!(Var);
  const DESC: ShardDesc = CONST_DESC;

  fn compose<B: Backend>(args: &Args, _: &mut ComposeCtx<'_, B>) -> Result<Composed<Var>> {
    compose_const(args)
  }

  fn activate(value: &Var, _: &mut (), _: &mut impl LeafCtx, _: &Var) -> Result<Flow> {
    Ok(Flow::Next(value.clone()))
  }
}

/// Assigns the input to a mutable variable, declaring a local if needed.
pub struct Set;

impl LeafShard for Set {
  type Compiled = Binding;
  no_state!(Binding);
  const DESC: ShardDesc = SET_DESC;

  fn compose<B: Backend>(args: &Args, ctx: &mut ComposeCtx<'_, B>) -> Result<Composed<Binding>> {
    compose_set(args, ctx)
  }

  fn activate(b: &Binding, _: &mut (), ctx: &mut impl LeafCtx, input: &Var) -> Result<Flow> {
    ctx.set(*b, input.clone());
    Ok(Flow::Next(input.clone()))
  }
}

/// Declares an immutable local holding the input.
pub struct Ref;

impl LeafShard for Ref {
  type Compiled = Binding;
  no_state!(Binding);
  const DESC: ShardDesc = REF_DESC;

  fn compose<B: Backend>(args: &Args, ctx: &mut ComposeCtx<'_, B>) -> Result<Composed<Binding>> {
    compose_ref(args, ctx)
  }

  fn activate(b: &Binding, _: &mut (), ctx: &mut impl LeafCtx, input: &Var) -> Result<Flow> {
    ctx.set(*b, input.clone());
    Ok(Flow::Next(input.clone()))
  }
}

/// Assigns the input to an existing mutable variable.
pub struct Update;

impl LeafShard for Update {
  type Compiled = Binding;
  no_state!(Binding);
  const DESC: ShardDesc = UPDATE_DESC;

  fn compose<B: Backend>(args: &Args, ctx: &mut ComposeCtx<'_, B>) -> Result<Composed<Binding>> {
    compose_update(args, ctx)
  }

  fn activate(b: &Binding, _: &mut (), ctx: &mut impl LeafCtx, input: &Var) -> Result<Flow> {
    ctx.set(*b, input.clone());
    Ok(Flow::Next(input.clone()))
  }
}

pub struct Get;

impl LeafShard for Get {
  type Compiled = Binding;
  no_state!(Binding);
  const DESC: ShardDesc = GET_DESC;

  fn compose<B: Backend>(args: &Args, ctx: &mut ComposeCtx<'_, B>) -> Result<Composed<Binding>> {
    compose_get(args, ctx)
  }

  fn activate(b: &Binding, _: &mut (), ctx: &mut impl LeafCtx, _: &Var) -> Result<Flow> {
    Ok(Flow::Next(ctx.get(*b)))
  }
}

/// Increments an Int variable and outputs the new value.
pub struct Inc;

impl LeafShard for Inc {
  type Compiled = Binding;
  no_state!(Binding);
  const DESC: ShardDesc = INC_DESC;

  fn compose<B: Backend>(args: &Args, ctx: &mut ComposeCtx<'_, B>) -> Result<Composed<Binding>> {
    compose_inc(args, ctx)
  }

  fn activate(b: &Binding, _: &mut (), ctx: &mut impl LeafCtx, _: &Var) -> Result<Flow> {
    Ok(Flow::Next(activate_inc(*b, ctx)?))
  }
}

pub struct Add;

impl LeafShard for Add {
  type Compiled = Operand;
  no_state!(Operand);
  const DESC: ShardDesc = ADD_DESC;

  fn compose<B: Backend>(args: &Args, ctx: &mut ComposeCtx<'_, B>) -> Result<Composed<Operand>> {
    math::compose_binary(args, ctx, ADD_DESC.name, math::BinOp::Add)
  }

  fn activate(op: &Operand, _: &mut (), ctx: &mut impl LeafCtx, input: &Var) -> Result<Flow> {
    Ok(Flow::Next(math::arith(
      math::BinOp::Add,
      ADD_DESC.name,
      input,
      &op.get(ctx),
    )?))
  }
}

pub struct IsLess;

impl LeafShard for IsLess {
  type Compiled = Operand;
  no_state!(Operand);
  const DESC: ShardDesc = IS_LESS_DESC;

  fn compose<B: Backend>(args: &Args, ctx: &mut ComposeCtx<'_, B>) -> Result<Composed<Operand>> {
    compose_compare(args, ctx, Self::DESC.name)
  }

  fn activate(op: &Operand, _: &mut (), ctx: &mut impl LeafCtx, input: &Var) -> Result<Flow> {
    Ok(Flow::Next(Var::Bool(compare(input, op.get(ctx))?.is_lt())))
  }
}

pub struct IsMoreEqual;

impl LeafShard for IsMoreEqual {
  type Compiled = Operand;
  no_state!(Operand);
  const DESC: ShardDesc = IS_MORE_EQUAL_DESC;

  fn compose<B: Backend>(args: &Args, ctx: &mut ComposeCtx<'_, B>) -> Result<Composed<Operand>> {
    compose_compare(args, ctx, Self::DESC.name)
  }

  fn activate(op: &Operand, _: &mut (), ctx: &mut impl LeafCtx, input: &Var) -> Result<Flow> {
    Ok(Flow::Next(Var::Bool(compare(input, op.get(ctx))?.is_ge())))
  }
}

/// Test instrumentation, see [`ProbeMode`].
pub struct Probe;

impl LeafShard for Probe {
  type Compiled = ProbeCompiled;
  type State = InstanceId;
  const DESC: ShardDesc = PROBE_DESC;

  fn compose<B: Backend>(
    args: &Args,
    ctx: &mut ComposeCtx<'_, B>,
  ) -> Result<Composed<ProbeCompiled>> {
    compose_probe(args, ctx)
  }

  fn instantiate(c: &ProbeCompiled, ctx: &mut InstanceCtx) -> Result<InstanceId> {
    probe_instantiate(c, ctx.instance)
  }

  fn activate(
    c: &ProbeCompiled,
    instance: &mut InstanceId,
    _: &mut impl LeafCtx,
    input: &Var,
  ) -> Result<Flow> {
    probe_activate(c, *instance);
    Ok(Flow::Next(input.clone()))
  }

  fn cleanup(c: &ProbeCompiled, instance: &mut InstanceId, _: &mut CleanupCtx) {
    probe_cleanup(c, *instance);
  }
}
