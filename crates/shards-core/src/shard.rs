//! The shard contract (contract §1, §2, §4-§6).
//!
//! A shard kind implements [`Shard`]. Its static [`ShardType`] (one per kind,
//! built with [`ShardType::new`] and [`ShardType::implemented_by`]) is what
//! wire definitions refer to. Compose turns parameters into an immutable
//! `Compiled` value shared by every instance; each instance owns only its
//! `State`. Most shards implement [`crate::shards::leaf::LeafShard`] or
//! [`crate::shards::async_shard::AsyncShard`] instead, and are adapted to
//! [`Shard`] by their `*_type` helpers.

use std::any::Any;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::mem::size_of;
use std::sync::Arc;
use std::task::Waker;

use crate::args::{Arg, Args};
use crate::compose::{Binding, CompiledWire, ComposeCtx};
use crate::describe::{ShardDesc, check_params, str_eq};
use crate::error::Result;
use crate::instance::{CleanupCtx, Frames, InstanceCtx, InstanceId, LeafCtx};
use crate::stackless::Control;
use crate::types::Type;
use crate::var::Var;

/// A parameter as the loader produces it. Immutable, and part of the cache key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ParamValue {
  /// A constant value.
  Value(Var),
  /// A reference to a variable. Compose turns it into a binding, read at
  /// activation, so updating the variable needs no recompile (contract §2).
  Var(String),
  /// A reference to a wire, by name.
  Wire(String),
  /// A nested flow of shards.
  Flow(Vec<ShardDef>),
  /// Value-flow pairs (`Match`); a `None` value matches anything.
  Cases(Vec<(Var, Vec<ShardDef>)>),
}

/// Result of a successful compose.
pub struct Composed<C> {
  pub compiled: C,
  pub output: Type,
}

/// What a non-suspending activation produced (contract §6): the result of a
/// [`crate::shards::leaf::LeafShard`]. [`Step`] adds suspension.
#[derive(Clone, Debug, PartialEq)]
pub enum Flow {
  /// Continue with this value as the next shard's input.
  Next(Var),
  /// End the instance.
  Stop,
  /// End the current iteration and start the wire again. Not termination.
  Restart,
  /// End the current wire (or `Do` sub-wire) with this value.
  Return(Var),
}

/// What an activation produced: [`Flow`]'s outcomes, plus `Suspend`.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
  Next(Var),
  Stop,
  Restart,
  Return(Var),
  /// Not finished: activate again on a later tick, and the shard continues
  /// from the resume point it saved in its state. The input passed on resume
  /// is the original input.
  Suspend,
}

impl From<Flow> for Step {
  fn from(flow: Flow) -> Step {
    match flow {
      Flow::Next(v) => Step::Next(v),
      Flow::Stop => Step::Stop,
      Flow::Restart => Step::Restart,
      Flow::Return(v) => Step::Return(v),
    }
  }
}

/// Context for `activate`. No scheduler operation is needed to suspend: a
/// shard returns [`Step::Suspend`] and is activated again on a later tick.
pub struct ActivationCtx<'a> {
  pub(crate) instance: InstanceId,
  pub(crate) locals: &'a mut Vec<Var>,
  pub(crate) revisions: &'a crate::reload::Revisions,
  /// Every body this mesh compiled, by key: what recursive call sites
  /// resolve through, pinned per outermost invocation.
  pub(crate) table: &'a Arc<crate::reload::FunctionRegistry>,
  pub(crate) mesh_frame: &'a mut Vec<Var>,
  pub(crate) spawn_queue: &'a mut Vec<(Arc<CompiledWire>, Var)>,
  pub(crate) waiting: &'a mut bool,
  pub(crate) waker: &'a Waker,
  /// The loop iteration ([`LeafCtx::iteration`]).
  pub(crate) iteration: u64,
  pub(crate) max_call_depth: usize,
}

impl ActivationCtx<'_> {
  /// The newest accepted body for a function, if a reload installed one.
  pub(crate) fn function_body(
    &self,
    key: &crate::reload::FunctionKey,
  ) -> Option<Arc<crate::function::CompiledFunction>> {
    self.revisions.select(key)
  }

  pub(crate) fn reload_revision(&self) -> u64 {
    self.revisions.revision
  }

  /// The mesh's current function table (the newest accepted bodies).
  pub(crate) fn table(&self) -> Arc<crate::reload::FunctionRegistry> {
    self.table.clone()
  }

  pub fn instance(&self) -> InstanceId {
    self.instance
  }

  pub fn get(&self, binding: Binding) -> Var {
    match binding {
      Binding::Local(i) => self.locals[i].clone(),
      Binding::Mesh(i) => self.mesh_frame[i].clone(),
    }
  }

  pub fn set(&mut self, binding: Binding, value: Var) {
    match binding {
      Binding::Local(i) => self.locals[i] = value,
      Binding::Mesh(i) => self.mesh_frame[i] = value,
    }
  }

  /// Schedules a new instance of `wire`. It starts on the next tick.
  pub fn spawn(&mut self, wire: Arc<CompiledWire>, input: Var) {
    self.spawn_queue.push((wire, input));
  }

  /// Marks the instance as waiting on its waker. A shard calls this right
  /// before returning [`Step::Suspend`] for an async operation; in
  /// `WakeMode::OnNotify` the instance is then resumed only after the waker
  /// fires.
  pub fn set_waiting(&mut self) {
    *self.waiting = true;
  }

  /// The instance's waker. Waking it after the instance finished is harmless.
  pub fn waker(&self) -> &Waker {
    self.waker
  }
}

impl LeafCtx for ActivationCtx<'_> {
  fn instance(&self) -> InstanceId {
    self.instance
  }

  fn iteration(&self) -> u64 {
    self.iteration
  }
}

impl Frames for ActivationCtx<'_> {
  fn get(&self, binding: Binding) -> Var {
    ActivationCtx::get(self, binding)
  }

  fn set(&mut self, binding: Binding, value: Var) {
    ActivationCtx::set(self, binding, value)
  }
}

pub trait Shard: 'static {
  /// Compose output, shared by every instance. Must not change after compose
  /// in any way that alters one instance's behavior.
  type Compiled: Send + Sync + 'static;
  /// Per-instance runtime state.
  type State: 'static;

  const NAME: &'static str;
  /// Bump when compose output changes for the same inputs, so cached
  /// artifacts from an older implementation are not reused.
  const VERSION: u32 = 1;

  /// Must be a deterministic function of `params`, the input type and the
  /// dependencies read through `ctx`. This is a trusted contract: nothing
  /// stops Rust code from reading the clock or the filesystem here, and it
  /// must not (contract §4).
  fn compose(args: &Args, ctx: &mut ComposeCtx<'_>) -> Result<Composed<Self::Compiled>>;

  /// Creates one instance's state. On error, must release anything it
  /// acquired: no `cleanup` follows (contract §5).
  fn instantiate(compiled: &Self::Compiled, ctx: &mut InstanceCtx) -> Result<Self::State>;

  /// Builtin composites describe children to the runner, never activate them.
  #[doc(hidden)]
  fn control(_compiled: &Self::Compiled) -> Option<Control<'_>> {
    None
  }

  #[doc(hidden)]
  fn inline(_compiled: &Self::Compiled) -> Option<crate::inline::InlineOp> {
    None
  }

  /// Starts an activation, or continues one that returned [`Step::Suspend`].
  /// A directly suspending leaf keeps its pending operation in `state` and
  /// resets it on completion/error. Builtin composites instead expose their
  /// control description; the runner owns and resets their continuations.
  fn activate(
    compiled: &Self::Compiled,
    state: &mut Self::State,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Step>;

  /// Called by the runtime exactly once for every successfully instantiated
  /// state, on completion, failure or cancellation.
  fn cleanup(_compiled: &Self::Compiled, _state: &mut Self::State, _ctx: &mut CleanupCtx) {}

  /// For measurements: bytes of per-instance state held outside `State`'s
  /// inline size, e.g. the states of nested flows.
  fn nested_state_size(_compiled: &Self::Compiled, _state: &Self::State) -> usize {
    0
  }
}

/// Type-erased compiled node, as stored in a compiled flow.
pub trait CompiledNode: Send + Sync {
  #[doc(hidden)]
  fn control(&self) -> Option<Control<'_>> {
    None
  }
  #[doc(hidden)]
  fn inline(&self) -> Option<crate::inline::InlineOp> {
    None
  }

  fn name(&self) -> &'static str;
  fn instantiate(&self, ctx: &mut InstanceCtx) -> Result<Box<dyn Any>>;
  fn activate(&self, state: &mut dyn Any, ctx: &mut ActivationCtx<'_>, input: &Var)
  -> Result<Step>;
  fn cleanup(&self, state: &mut dyn Any, ctx: &mut CleanupCtx);
  /// Inline size of this node's state, for measurements.
  fn state_size(&self, state: &dyn Any) -> usize;
}

struct Node<S: Shard>(S::Compiled);

impl<S: Shard> CompiledNode for Node<S> {
  fn control(&self) -> Option<Control<'_>> {
    S::control(&self.0)
  }

  fn inline(&self) -> Option<crate::inline::InlineOp> {
    S::inline(&self.0)
  }

  fn name(&self) -> &'static str {
    S::NAME
  }

  fn instantiate(&self, ctx: &mut InstanceCtx) -> Result<Box<dyn Any>> {
    Ok(Box::new(S::instantiate(&self.0, ctx)?))
  }

  fn activate(
    &self,
    state: &mut dyn Any,
    ctx: &mut ActivationCtx<'_>,
    input: &Var,
  ) -> Result<Step> {
    S::activate(&self.0, downcast::<S>(state), ctx, input)
  }

  fn cleanup(&self, state: &mut dyn Any, ctx: &mut CleanupCtx) {
    S::cleanup(&self.0, downcast::<S>(state), ctx)
  }

  fn state_size(&self, state: &dyn Any) -> usize {
    let state = state
      .downcast_ref::<S::State>()
      .expect("shard state type mismatch");
    size_of::<S::State>() + S::nested_state_size(&self.0, state)
  }
}

fn downcast<S: Shard>(state: &mut dyn Any) -> &mut S::State {
  state
    .downcast_mut::<S::State>()
    .expect("shard state type mismatch")
}

pub(crate) type ComposeFn =
  fn(&Args, &mut ComposeCtx<'_>) -> Result<Composed<Arc<dyn CompiledNode>>>;

/// One shard kind: its static description ([`ShardDesc`]) and its
/// implementation. Wire definitions refer to it.
///
/// The description is the single source of the shard's name, version,
/// documentation and parameter contract; the implementation is attached to
/// it, and attaching one whose name or version differs fails to compile.
pub struct ShardType {
  pub desc: ShardDesc,
  pub(crate) compose: Option<ComposeFn>,
}

impl ShardType {
  /// A shard type with no implementation attached yet. Its parameter
  /// declarations are checked at compile time ([`check_params`]): an
  /// invalid declaration (a default the parameter itself would reject, or a
  /// duplicate name) does not compile:
  ///
  /// ```compile_fail
  /// use shards_core::describe::*;
  /// use shards_core::ShardType;
  /// static BAD: &[ParamDecl] = &[ParamDecl {
  ///   name: "Count",
  ///   help: "",
  ///   forms: Forms::LITERAL,
  ///   types: &[TypeName::Int],
  ///   // A String default for an Int-only parameter.
  ///   requirement: Requirement::Default(DefaultValue::Str("ten")),
  ///   ty: None,
  /// }];
  /// static T: ShardType = ShardType::new(ShardDesc {
  ///   params: Params::Declared(BAD),
  ///   ..ShardDesc::undocumented("Bad", 1)
  /// });
  /// ```
  pub const fn new(desc: ShardDesc) -> ShardType {
    if let crate::describe::Params::Declared(decls) = desc.params {
      assert!(
        check_params(decls).is_ok(),
        "invalid parameter declaration (duplicate name, or a default the parameter does not accept)"
      );
    }
    ShardType {
      desc,
      compose: None,
    }
  }

  /// Attaches the implementation. Its name and version must match the
  /// description (checked at compile time), so an implementation cannot
  /// drift from the description it is attached to:
  ///
  /// ```compile_fail
  /// use shards_core::*;
  /// struct Mislabeled;
  /// impl Shard for Mislabeled {
  ///   type Compiled = ();
  ///   type State = ();
  ///   const NAME: &'static str = "Mislabeled";
  ///   fn compose(_: &Args, ctx: &mut ComposeCtx<'_>) -> Result<Composed<()>> {
  ///     Ok(Composed { compiled: (), output: ctx.input() })
  ///   }
  ///   fn instantiate(_: &(), _: &mut instance::InstanceCtx) -> Result<()> { Ok(()) }
  ///   fn activate(_: &(), _: &mut (), _: &mut ActivationCtx<'_>, v: &Var) -> Result<Step> {
  ///     Ok(Step::Next(v.clone()))
  ///   }
  /// }
  /// // The description says "Other": this does not compile.
  /// static T: ShardType = ShardType::new(ShardDesc::undocumented("Other", 1)).implemented_by::<Mislabeled>();
  /// ```
  pub const fn implemented_by<S: Shard>(self) -> ShardType {
    assert!(
      str_eq(S::NAME, self.desc.name) && S::VERSION == self.desc.version,
      "implementation does not match the shard description"
    );
    ShardType {
      compose: Some(compose_erased::<S>),
      ..self
    }
  }

  pub fn name(&self) -> &'static str {
    self.desc.name
  }

  /// Composes one occurrence of this shard. A described shard with no
  /// implementation attached is a compose error (`not-implemented`).
  pub(crate) fn compose_node(
    &self,
    args: &Args,
    ctx: &mut ComposeCtx<'_>,
  ) -> Result<Composed<Arc<dyn CompiledNode>>> {
    match self.compose {
      Some(compose) => compose(args, ctx),
      None => Err(crate::error::Error::Diagnostic(Box::new(
        crate::diagnostic::Diagnostic::new(
          crate::diagnostic::Phase::Compose,
          "compose-error",
          "not-implemented",
          format!("{} is described but has no implementation", self.name()),
        )
        .shard(self.name()),
      ))),
    }
  }
}

fn compose_erased<S: Shard>(
  args: &Args,
  ctx: &mut ComposeCtx<'_>,
) -> Result<Composed<Arc<dyn CompiledNode>>> {
  let composed = S::compose(args, ctx)?;
  Ok(Composed {
    compiled: Arc::new(Node::<S>(composed.compiled)),
    output: composed.output,
  })
}

/// One shard in a wire definition: its kind and its arguments. A call to
/// a script function (golden path D2: called exactly like a shard) names
/// the function; its `ty` is then the internal `Call` shard.
#[derive(Clone)]
pub struct ShardDef {
  pub ty: &'static ShardType,
  pub args: Vec<Arg>,
  pub function: Option<Arc<str>>,
}

impl ShardDef {
  /// A definition with positional arguments.
  pub fn new(ty: &'static ShardType, params: Vec<ParamValue>) -> ShardDef {
    ShardDef {
      ty,
      args: params.into_iter().map(Arg::pos).collect(),
      function: None,
    }
  }

  /// A definition with positional and/or named arguments.
  pub fn with_args(ty: &'static ShardType, args: Vec<Arg>) -> ShardDef {
    ShardDef {
      ty,
      args,
      function: None,
    }
  }

  /// A call to the script function `name` with literal or variable
  /// arguments (computed arguments are hoisted before the call by the
  /// frontend).
  pub fn call(name: &str, args: Vec<Arg>) -> ShardDef {
    ShardDef {
      ty: &crate::shards::CALL,
      args,
      function: Some(Arc::from(name)),
    }
  }

  /// The name diagnostics and occurrence paths use: the function's for a
  /// call, else the shard's.
  pub fn name(&self) -> &str {
    match &self.function {
      Some(name) => name,
      None => self.ty.desc.name,
    }
  }
}

impl PartialEq for ShardDef {
  fn eq(&self, other: &ShardDef) -> bool {
    self.ty.desc.name == other.ty.desc.name
      && self.ty.desc.version == other.ty.desc.version
      && self.function == other.function
      && self.args == other.args
  }
}

impl Eq for ShardDef {}

impl Hash for ShardDef {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.ty.desc.name.hash(state);
    self.ty.desc.version.hash(state);
    self.function.hash(state);
    self.args.hash(state);
  }
}

impl fmt::Debug for ShardDef {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}{:?}", self.name(), self.args)
  }
}

/// Erases a compiled value into a flow node of shard `S`.
pub(crate) fn erase<S: Shard>(compiled: S::Compiled) -> Arc<dyn CompiledNode> {
  Arc::new(Node::<S>(compiled))
}
