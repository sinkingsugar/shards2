//! Compose: wire definitions in, shared compiled wires out (contract §4, §8).
//!
//! [`ComposeCtx`] records every dependency a compose reads, including failed
//! lookups. [`ComposeCache`] finds candidates by primary key (wire definition
//! and input type), then revalidates their recorded dependencies; it composes
//! only on a miss.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::args::{Args, decode};
use crate::diagnostic::PathStep;
use crate::error::Result;
use crate::flow::CompiledFlow;
use crate::reload::{InlineCall, InlineKey, InlineRegistry, InlineSignature, SiteStep};
use crate::shard::{Composed, ShardDef, ShardType};
use crate::signature::{Analysis, Occurrence};
use crate::types::Type;

/// A scheduler's kind of compiled node. Compose is shared by both schedulers:
/// the same wire definitions, cache, dependency recording and checks. Only the
/// compiled nodes (and how they activate) differ.
pub trait Backend: Sized + 'static {
  type Node: ?Sized + Send + Sync + 'static;

  #[doc(hidden)]
  fn inline(_node: &Self::Node) -> Option<crate::inline::InlineOp> {
    None
  }

  fn compose_shard(
    ty: &ShardType,
    args: &Args,
    ctx: &mut ComposeCtx<'_, Self>,
  ) -> Result<Composed<Arc<Self::Node>>>;
}

/// A wire as the loader produces it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WireDef {
  pub name: String,
  pub looped: bool,
  pub flow: Vec<ShardDef>,
}

/// Where a variable lives: the instance's frame or the mesh's frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Binding {
  Local(usize),
  Mesh(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VarInfo {
  pub binding: Binding,
  pub ty: Type,
  pub mutable: bool,
  /// Whether the variable is definitely assigned on every path that reaches
  /// the shard being composed. Mesh variables always are.
  pub initialized: bool,
}

/// One slot of a frame layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Slot {
  pub index: usize,
  pub ty: Type,
  pub mutable: bool,
}

/// Names, types and mutability of a frame's slots. Names exist only here,
/// at compose time; activation uses slot indices.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameLayout {
  slots: Vec<(String, Slot)>,
  index: HashMap<String, usize>,
}

impl FrameLayout {
  pub fn lookup(&self, name: &str) -> Option<Slot> {
    self.index.get(name).map(|i| self.slots[*i].1)
  }

  pub(crate) fn declare(&mut self, name: &str, ty: Type, mutable: bool) -> Slot {
    let slot = Slot {
      index: self.slots.len(),
      ty,
      mutable,
    };
    self.index.insert(name.to_string(), slot.index);
    self.slots.push((name.to_string(), slot));
    slot
  }

  pub(crate) fn slots(&self) -> &[(String, Slot)] {
    &self.slots
  }

  /// The names visible now, for restoring at the end of a block.
  pub(crate) fn visible(&self) -> HashMap<String, usize> {
    self.index.clone()
  }

  /// Ends a block: names declared inside it stop being visible (their slots
  /// stay in the frame). Frontend temporaries (`%` names) stay visible,
  /// since lowering reads a hoisted value after the SubFlow computing it.
  pub(crate) fn restore_visible(&mut self, mut saved: HashMap<String, usize>) {
    for (name, index) in &self.index {
      if name.starts_with('%') {
        saved.insert(name.clone(), *index);
      }
    }
    self.index = saved;
  }

  /// Visible variable names, sorted (for suggestions).
  pub fn names(&self) -> Vec<String> {
    let mut names: Vec<String> = self
      .index
      .keys()
      .filter(|n| !n.starts_with('%'))
      .cloned()
      .collect();
    names.sort();
    names
  }

  pub fn len(&self) -> usize {
    self.slots.len()
  }

  pub fn is_empty(&self) -> bool {
    self.slots.is_empty()
  }
}

/// A dependency read during compose. Revalidated on every cache lookup.
#[derive(Clone, Debug, PartialEq)]
pub enum Dep {
  /// A mesh variable, or its absence (`found: None`).
  MeshVar { name: String, found: Option<Slot> },
  /// A wire definition referenced by name (`Do`, `Spawn`), or its absence.
  Wire {
    name: String,
    def: Option<Arc<WireDef>>,
  },
}

/// What compose may read besides parameters and input type.
pub struct ComposeEnv<'a> {
  pub mesh_layout: &'a FrameLayout,
  pub wires: &'a HashMap<String, WireDef>,
}

impl Dep {
  pub(crate) fn still_valid(&self, env: &ComposeEnv<'_>) -> bool {
    match self {
      Dep::MeshVar { name, found } => env.mesh_layout.lookup(name) == *found,
      Dep::Wire { name, def } => env.wires.get(name) == def.as_deref(),
    }
  }
}

/// A compiled wire: shared, immutable, and free of per-instance data.
pub struct CompiledWire<B: Backend> {
  pub name: String,
  pub looped: bool,
  pub input: Type,
  pub flow: CompiledFlow<B>,
  /// Layout of each instance's local frame.
  pub locals: FrameLayout,
  /// Everything this compose read, for revalidation (e.g. by another mesh).
  pub deps: Vec<Dep>,
  pub(crate) definition: Arc<WireDef>,
  pub(crate) restart_deps: Vec<Dep>,
  pub(crate) inline_calls: InlineRegistry<B>,
}

impl<B: Backend> CompiledWire<B> {
  /// A process signature with exact composed types and inferred mesh access.
  pub fn signature(&self) -> crate::signature::Signature<'_> {
    use crate::signature::{Lifetime, Signature, SignatureInput, SignatureOutput};
    Signature {
      name: self.name.as_str().into(),
      revision: 1,
      input: SignatureInput::Type(self.input),
      output: SignatureOutput::Type(self.flow.output),
      params: Some(Vec::new()),
      lifetime: Lifetime::Stateful,
      effects: self.flow.analysis.effects,
      uses: self.flow.analysis.uses.clone(),
      mutates: self.flow.analysis.mutates.clone(),
      source: None,
      summary: "".into(),
      help: "".into(),
    }
  }

  pub fn deps_valid(&self, env: &ComposeEnv<'_>) -> bool {
    self.deps.iter().all(|d| d.still_valid(env))
  }
}

/// A nested flow or referenced wire whose compose failed: recorded when the
/// error leaves it, so the step for the shard that composed it can name the
/// parameter holding it (diagnostic occurrence paths).
pub(crate) enum Child {
  /// A flow, by identity (the slice passed to `compose_flow`).
  Flow(*const ShardDef),
  Wire(String),
}

/// The context a shard's `compose` receives.
pub struct ComposeCtx<'a, B: Backend> {
  analysis: Analysis,
  input: Type,
  locals: FrameLayout,
  env: &'a ComposeEnv<'a>,
  cache: &'a mut ComposeCache<B>,
  composing: &'a mut Vec<String>,
  deps: Vec<Dep>,
  restart_deps: Vec<Dep>,
  inline_calls: InlineRegistry<B>,
  site: InlineKey,
  next_flow: usize,
  // Structural declaration origins for reload diagnostics, never source spans.
  diagnostic_path: Vec<PathStep>,
  local_paths: Vec<Vec<PathStep>>,
  current_args: Option<Arc<Args>>,
  /// Definite initialization of each local slot at the current compose point.
  initialized: Vec<bool>,
  /// The child flow or wire the last error came out of.
  failed_child: Option<Child>,
  /// How many flows enclose the one being composed, counting wires
  /// inlined by `Do` and composed for `Spawn` ([`MAX_FLOW_DEPTH`]).
  depth: usize,
  /// How many blocks (nested flows of shards) enclose the shard being
  /// composed within its wire body; 0 at a wire's top level.
  blocks: usize,
}

/// How deeply flows may nest, counting wires inlined by `Do` (they run in
/// the caller's flow) and composed for `Spawn`. Activation recurses once
/// per level, on the stackful scheduler on a fixed coroutine stack, and
/// compose recurses too: past this, compose reports `too-deep` instead of
/// letting the stack overflow. Real scripts stay far below.
pub const MAX_FLOW_DEPTH: usize = 48;

impl<B: Backend> ComposeCtx<'_, B> {
  /// The input type of the shard being composed.
  pub fn input(&self) -> Type {
    self.input
  }

  /// Resolves a variable: the local frame first, then the mesh frame. Mesh
  /// lookups are recorded as dependencies, including absences.
  pub fn var(&mut self, name: &str) -> Option<VarInfo> {
    if let Some(slot) = self.locals.lookup(name) {
      return Some(VarInfo {
        binding: Binding::Local(slot.index),
        ty: slot.ty,
        mutable: slot.mutable,
        initialized: self.initialized[slot.index],
      });
    }
    let found = self.env.mesh_layout.lookup(name);
    let dep = Dep::MeshVar {
      name: name.to_string(),
      found,
    };
    self.deps.push(dep.clone());
    self.restart_deps.push(dep);
    found.map(|slot| VarInfo {
      binding: Binding::Mesh(slot.index),
      ty: slot.ty,
      mutable: slot.mutable,
      initialized: true,
    })
  }

  /// Variable names visible here, locals and mesh variables, sorted. For
  /// suggestions; it records no dependencies.
  pub fn visible_names(&self) -> Vec<String> {
    let mut names = self.locals.names();
    for name in self.env.mesh_layout.names() {
      if !names.contains(&name) {
        names.push(name);
      }
    }
    names.sort();
    names
  }

  /// Whether the shard being composed is at its wire body's top level,
  /// outside every branch and loop.
  pub fn at_top_level(&self) -> bool {
    self.blocks == 0
  }

  /// Where a local was declared: the occurrence path of the declaring
  /// shard, from its wire.
  pub fn declaration_path(&self, binding: Binding) -> Option<Vec<PathStep>> {
    match binding {
      Binding::Local(i) => self.local_paths.get(i).cloned(),
      Binding::Mesh(_) => None,
    }
  }

  /// Resolves a variable that is about to be read. Fails if it is unknown, or
  /// not definitely initialized here (e.g. only assigned in a `When` body or
  /// a loop body that might not run).
  /// Errors are structured diagnostics (`unknown-variable`,
  /// `possibly-uninitialized`); callers that read a parameter add it with
  /// [`crate::Error::with_param`].
  pub fn read_var(&mut self, name: &str, shard: &str) -> Result<VarInfo> {
    let variable_error = |code: &'static str, message: String| {
      Err(crate::Error::Diagnostic(Box::new(
        crate::diagnostic::Diagnostic::new(
          crate::diagnostic::Phase::Compose,
          "compose-error",
          code,
          message,
        )
        .shard(shard),
      )))
    };
    match self.var(name) {
      None => {
        let near = crate::diagnostic::closest(name, self.visible_names(), 3);
        variable_error("unknown-variable", format!("unknown variable {name}")).map_err(
          |e: crate::Error| match e {
            crate::Error::Diagnostic(mut d) => {
              d.did_you_mean = near;
              crate::Error::Diagnostic(d)
            }
            other => other,
          },
        )
      }
      Some(info) if !info.initialized => variable_error(
        "possibly-uninitialized",
        format!(
          "{name} may be uninitialized here (it is only assigned in a branch or loop body that might not run)"
        ),
      ),
      Some(info) => {
        if matches!(info.binding, Binding::Mesh(_)) {
          self.analysis.access(name, info.ty, false);
        }
        Ok(info)
      }
    }
  }

  /// Declares a new local variable in the wire's frame. The declaring shard
  /// assigns it, so it is initialized from this point on.
  pub fn declare_local(&mut self, name: &str, ty: Type, mutable: bool) -> VarInfo {
    let slot = self.locals.declare(name, ty, mutable);
    self.initialized.push(true);
    self.local_paths.push(self.diagnostic_path.clone());
    VarInfo {
      binding: Binding::Local(slot.index),
      ty,
      mutable,
      initialized: true,
    }
  }

  /// Records that the shard being composed assigns this variable.
  pub fn mark_initialized(&mut self, binding: Binding) {
    if let Binding::Mesh(i) = binding {
      let (name, slot) = &self.env.mesh_layout.slots[i];
      self.analysis.access(name, slot.ty, true);
    }
    if let Binding::Local(i) = binding {
      self.initialized[i] = true;
    }
  }

  /// Like [`ComposeCtx::compose_flow`], for a flow that might not run (a
  /// branch, or a loop body that can run zero times). Variables it declares
  /// or assigns are not definitely initialized after it.
  pub fn compose_flow_conditional(
    &mut self,
    flow: &[ShardDef],
    input: Type,
  ) -> Result<CompiledFlow<B>> {
    let before = self.initialized.clone();
    let result = self.compose_flow(flow, input);
    for (i, init) in self.initialized.iter_mut().enumerate() {
      *init = before.get(i).copied().unwrap_or(false);
    }
    result
  }

  /// Runs `f` as a region that might not run, as a whole: inside it,
  /// assignments are seen by what follows (Repeat's Until, then its
  /// Action); after it, nothing it assigned is definitely assigned.
  pub fn conditional_region<R>(&mut self, f: impl FnOnce(&mut Self) -> Result<R>) -> Result<R> {
    let before = self.initialized.clone();
    let result = f(self);
    for (i, init) in self.initialized.iter_mut().enumerate() {
      *init = before.get(i).copied().unwrap_or(false);
    }
    result
  }

  fn child_prefix(&self, child: &Child) -> Vec<PathStep> {
    let mut path = Vec::new();
    if let Some((param, item)) = self.current_args.as_ref().and_then(|a| a.param_of(child)) {
      path.push(PathStep::Param(param.into()));
      if let Some(item) = item {
        path.push(PathStep::Item(item));
      }
    }
    if let Child::Wire(name) = child {
      path.push(PathStep::Wire(name.clone()));
    }
    path
  }

  /// Composes a nested flow (e.g. a `When` body) with the given input type,
  /// into the same local frame. The flow is a block: names it declares are
  /// not visible after it (golden-path.md §3.2).
  pub fn compose_flow(&mut self, flow: &[ShardDef], input: Type) -> Result<CompiledFlow<B>> {
    let child = Child::Flow(flow.as_ptr());
    let visible = self.locals.visible();
    self.blocks += 1;
    let result = self.compose_flow_unscoped(flow, input);
    self.blocks -= 1;
    self.locals.restore_visible(visible);
    if let Ok(flow) = &result {
      let prefix = self.child_prefix(&child);
      self.analysis.include(&flow.analysis, &prefix);
    }
    result
  }

  /// Composes a flow whose declarations stay visible after it: a wire body,
  /// or a wire inlined by `Do`, which shares its caller's variables.
  fn compose_flow_unscoped(&mut self, flow: &[ShardDef], input: Type) -> Result<CompiledFlow<B>> {
    if self.depth >= MAX_FLOW_DEPTH {
      return Err(crate::Error::Diagnostic(Box::new(
        crate::diagnostic::Diagnostic::new(
          crate::diagnostic::Phase::Compose,
          "compose-error",
          "too-deep",
          format!(
            "flows nest more than {MAX_FLOW_DEPTH} levels deep (nested flows and wires run through Do or Spawn count)"
          ),
        ),
      )));
    }
    let path_len = self.diagnostic_path.len();
    if let Some((param, item)) = self
      .current_args
      .as_ref()
      .and_then(|args| args.param_of(&Child::Flow(flow.as_ptr())))
    {
      self.diagnostic_path.push(PathStep::Param(param.into()));
      if let Some(item) = item {
        self.diagnostic_path.push(PathStep::Item(item));
      }
    }
    self.depth += 1;
    let child = self.next_flow;
    self.next_flow = 0;
    Arc::make_mut(&mut self.site.path).push(SiteStep::Flow(child));
    let result = self.compose_flow_at_depth(flow, input);
    Arc::make_mut(&mut self.site.path).pop();
    self.next_flow = child + 1;
    self.depth -= 1;
    self.diagnostic_path.truncate(path_len);
    result
  }

  fn compose_flow_at_depth(&mut self, flow: &[ShardDef], input: Type) -> Result<CompiledFlow<B>> {
    let saved = self.input;
    let mut analysis = Analysis::default();
    let mut nodes = Vec::with_capacity(flow.len());
    let mut code = Vec::with_capacity(flow.len());
    let mut ty = input;
    // Once a shard never produces a value (`Stop`), the rest of the flow is
    // unreachable but still checked: the next shard gets a None input (it
    // receives nothing), later ones the types that really flow, and the
    // flow's output stays `Never`.
    let mut diverged = false;
    for (index, def) in flow.iter().enumerate() {
      self.input = if ty == Type::never() {
        Type::none()
      } else {
        ty
      };
      self.failed_child = None;
      self.cache.stats.shard_composes += 1;
      // Decode against the shard's declared parameters (the same
      // declarations its documentation is generated from), then compose.
      Arc::make_mut(&mut self.site.path).push(SiteStep::Node(index));
      let saved_next = self.next_flow;
      self.next_flow = 0;
      self.diagnostic_path.push(PathStep::Shard {
        index,
        name: def.ty.name().into(),
      });
      let node_input = self.input;
      let parent_analysis = std::mem::replace(
        &mut self.analysis,
        Analysis {
          effects: def.ty.desc.effects,
          lifetime: def.ty.desc.lifetime,
          ..Analysis::default()
        },
      );
      let composed = decode(&def.ty.desc, &def.args).and_then(|args| {
        check_input(def.ty, self.input)?;
        let args = Arc::new(args);
        let parent_args = self.current_args.replace(args.clone());
        let result = B::compose_shard(def.ty, &args, self).map_err(|err| {
          // An error from a nested flow or wire: name the parameter
          // holding it.
          match self.failed_child.take().and_then(|c| args.param_of(&c)) {
            Some((param, item)) => {
              let err = match item {
                Some(i) => err.prefix_path(PathStep::Item(i)),
                None => err,
              };
              err.prefix_path(PathStep::Param(param.to_string()))
            }
            None => err,
          }
        });
        self.current_args = parent_args;
        result
      });
      self.diagnostic_path.pop();
      Arc::make_mut(&mut self.site.path).pop();
      self.next_flow = saved_next;
      let mut node_analysis = std::mem::replace(&mut self.analysis, parent_analysis);
      let composed = match composed {
        Ok(c) => c,
        Err(err) => {
          self.input = saved;
          self.failed_child = Some(Child::Flow(flow.as_ptr()));
          let err = with_input_source(err, &flow[..index]);
          return Err(err.prefix_path(PathStep::Shard {
            index,
            name: def.ty.name().to_string(),
          }));
        }
      };
      node_analysis.occurrences.insert(
        0,
        Occurrence {
          path: Vec::new(),
          input: node_input,
          output: composed.output,
          effects: node_analysis.effects,
          lifetime: node_analysis.lifetime,
        },
      );
      analysis.include(
        &node_analysis,
        &[PathStep::Shard {
          index,
          name: def.ty.name().into(),
        }],
      );
      code.push(crate::inline::Instruction::new(
        B::inline(&composed.compiled),
        def.ty.name(),
        composed.output,
      ));
      nodes.push(composed.compiled);
      if composed.output == Type::never() {
        diverged = true;
      }
      ty = composed.output;
    }
    self.input = saved;
    let output = if diverged { Type::never() } else { ty };
    crate::inline::lower_scratch_releases(&mut code);
    Ok(CompiledFlow {
      analysis,
      nodes,
      code,
      output,
    })
  }

  fn wire_def(&mut self, name: &str) -> Option<Arc<WireDef>> {
    let def = self.env.wires.get(name).cloned().map(Arc::new);
    let dep = Dep::Wire {
      name: name.to_string(),
      def: def.clone(),
    };
    self.deps.push(dep.clone());
    self.restart_deps.push(dep);
    def
  }

  /// Composes another wire inline, sharing this wire's local frame (`Do`).
  pub fn compose_inline(&mut self, name: &str, input: Type) -> Result<CompiledFlow<B>> {
    let Some(def) = self.wire_def(name) else {
      return Err(wire_error("unknown-wire", format!("unknown wire: {name}")));
    };
    if self.composing.iter().any(|n| n == name) {
      return Err(wire_error(
        "recursive-wire",
        format!("recursive wire reference: {name}"),
      ));
    }
    self.composing.push(name.to_string());
    let parent_path =
      std::mem::replace(&mut self.diagnostic_path, vec![PathStep::Wire(name.into())]);
    let parent_args = self.current_args.take();
    // A wire's body is its own top level, even when `Do` inlines it.
    let blocks = std::mem::replace(&mut self.blocks, 0);
    let flow = self.compose_flow_unscoped(&def.flow, input).map_err(|err| {
      self.failed_child = Some(Child::Wire(name.to_string()));
      err.prefix_path(PathStep::Wire(name.to_string()))
    });
    self.blocks = blocks;
    self.composing.pop();
    self.diagnostic_path = parent_path;
    self.current_args = parent_args;
    if let Ok(flow) = &flow {
      self.analysis.include(
        &flow.analysis,
        &self.child_prefix(&Child::Wire(name.into())),
      );
    }
    flow
  }

  /// Composes a Do call with a mesh-local replacement boundary. The complete
  /// dependency list still validates caches; dependencies inside this call
  /// do not force an otherwise unchanged caller to restart.
  pub fn compose_reloadable_inline(
    &mut self,
    name: &str,
    input: Type,
  ) -> Result<Arc<InlineCall<B>>> {
    let key = self.site.clone();
    let before = self.locals.clone();
    let initialized_before = self.initialized.clone();
    let parent_deps = std::mem::take(&mut self.restart_deps);
    // Descendant keys include this body's identity: an old in-flight body
    // cannot accidentally select a newly rearranged descendant call site.
    if let Some(def) = self.env.wires.get(name) {
      Arc::make_mut(&mut self.site.path).push(SiteStep::Wire(Arc::new(def.clone())));
    }
    let result = self.compose_inline(name, input);
    self.site = key.clone();
    let deps = std::mem::replace(&mut self.restart_deps, parent_deps);
    let flow = result?;
    let call = Arc::new(InlineCall {
      key: key.clone(),
      name: name.to_string(),
      signature: InlineSignature {
        input,
        before,
        after: self.locals.clone(),
        initialized_before,
        initialized_after: self.initialized.clone(),
        output: flow.output,
      },
      deps,
      local_paths: self.local_paths.clone(),
      flow,
    });
    self.inline_calls.insert(key, call.clone());
    Ok(call)
  }

  /// Compiles another wire on its own (`Spawn`), through the cache. Its
  /// dependencies become dependencies of the wire being composed.
  pub fn compose_wire(&mut self, name: &str, input: Type) -> Result<Arc<CompiledWire<B>>> {
    let Some(def) = self.wire_def(name) else {
      return Err(wire_error("unknown-wire", format!("unknown wire: {name}")));
    };
    let wire = self
      .cache
      .get_or_compose(&def, input, self.env, self.composing, self.depth)
      .inspect_err(|err| {
        // Composing the wire failed inside it (its path starts at the
        // wire), as opposed to the reference itself failing.
        let inside = err
          .diagnostic()
          .is_some_and(|d| d.path.first() == Some(&PathStep::Wire(name.to_string())));
        if inside {
          self.failed_child = Some(Child::Wire(name.to_string()));
        }
      })?;
    self.analysis.include(
      &wire.flow.analysis,
      &self.child_prefix(&Child::Wire(name.into())),
    );
    self.deps.extend(wire.deps.iter().cloned());
    self.restart_deps.extend(wire.deps.iter().cloned());
    self.inline_calls.extend(
      wire
        .inline_calls
        .iter()
        .map(|(k, v)| (k.clone(), v.clone())),
    );
    Ok(wire)
  }
}

/// Enforces a shard's declared input (`InputDesc::Types` or
/// `InputDesc::Typed`) before its compose runs, so no shard repeats the
/// check. `Any` and `Ignored` inputs are left to the shard.
fn check_input(ty: &ShardType, input: Type) -> Result<()> {
  use crate::describe::InputDesc;
  use crate::diagnostic::TypeRef;
  let (expected, refs) = match ty.desc.input {
    InputDesc::Types(types) => {
      if types.iter().any(|t| t.matches(input)) {
        return Ok(());
      }
      // "Int, Float or Float3".
      let names: Vec<&str> = types.iter().map(|t| t.name()).collect();
      let expected = match names.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} or {last}", rest.join(", ")),
        _ => names.join(""),
      };
      (
        expected,
        types.iter().copied().map(TypeRef::named).collect(),
      )
    }
    InputDesc::Typed(full) => {
      let full = full();
      if full.accepts(input) {
        return Ok(());
      }
      (full.to_string(), vec![TypeRef::of(full)])
    }
    InputDesc::Any | InputDesc::Ignored => return Ok(()),
  };
  Err(crate::Error::Diagnostic(Box::new(
    crate::diagnostic::Diagnostic::new(
      crate::diagnostic::Phase::Compose,
      "input-type-mismatch",
      "input-type-mismatch",
      format!("{} needs {expected} input, got {input}", ty.name()),
    )
    .shard(ty.name())
    .types(Some(TypeRef::of(input)), refs),
  )))
}

/// Adds where the input came from to a mismatch on the failing shard's own
/// input (not to errors from inside its nested flows, which already have
/// path steps). `before` are the shards before it in the same flow; those
/// that pass their input through are skipped to reach the producer.
fn with_input_source(err: crate::Error, before: &[ShardDef]) -> crate::Error {
  use crate::describe::OutputDesc;
  let crate::Error::Diagnostic(mut d) = err else {
    return err;
  };
  let about_input = matches!(d.kind, "input-type-mismatch") || d.code == "variable-type-mismatch";
  if !about_input || !d.path.is_empty() || d.input_from.is_some() {
    return crate::Error::Diagnostic(d);
  }
  let mut via = Vec::new();
  let mut origin = None;
  for (index, def) in before.iter().enumerate().rev() {
    if matches!(def.ty.desc.output, OutputDesc::Passthrough) {
      via.push((index, def.ty.name().to_string()));
    } else {
      origin = Some((index, def.ty.name().to_string()));
      break;
    }
  }
  via.reverse();
  let source = crate::diagnostic::InputSource { origin, via };
  d.message = format!("{} ({})", d.message, source.describe());
  d.input_from = Some(source);
  crate::Error::Diagnostic(d)
}

/// A structured error about a referenced wire; the referencing shard adds
/// itself and its parameter.
fn wire_error(code: &'static str, message: String) -> crate::Error {
  crate::Error::Diagnostic(Box::new(crate::diagnostic::Diagnostic::new(
    crate::diagnostic::Phase::Compose,
    "compose-error",
    code,
    message,
  )))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
  /// Lookups that reused a cached compiled wire.
  pub hits: u64,
  /// Lookups that had to compose.
  pub misses: u64,
  /// Wires actually composed (equals `misses` unless compose failed).
  pub wire_composes: u64,
  /// Individual shard compose calls.
  pub shard_composes: u64,
}

struct Entry<B: Backend> {
  def: WireDef,
  input: Type,
  compiled: Arc<CompiledWire<B>>,
}

pub type HashFn = fn(&WireDef, Type) -> u64;

fn default_hash(def: &WireDef, input: Type) -> u64 {
  let mut hasher = DefaultHasher::new();
  def.hash(&mut hasher);
  input.hash(&mut hasher);
  hasher.finish()
}

/// Compose cache with two-step lookup (contract §4).
pub struct ComposeCache<B: Backend> {
  entries: HashMap<u64, Vec<Entry<B>>>,
  hash_fn: HashFn,
  pub stats: CacheStats,
}

impl<B: Backend> Default for ComposeCache<B> {
  fn default() -> ComposeCache<B> {
    ComposeCache::with_hash_fn(default_hash)
  }
}

impl<B: Backend> ComposeCache<B> {
  /// A cache with a custom primary-key hash (tests force collisions with it).
  pub fn with_hash_fn(hash_fn: HashFn) -> ComposeCache<B> {
    ComposeCache {
      entries: HashMap::new(),
      hash_fn,
      stats: CacheStats::default(),
    }
  }

  pub(crate) fn get_or_compose(
    &mut self,
    def: &WireDef,
    input: Type,
    env: &ComposeEnv<'_>,
    composing: &mut Vec<String>,
    depth: usize,
  ) -> Result<Arc<CompiledWire<B>>> {
    let key = (self.hash_fn)(def, input);
    // Step 1: candidates by primary key (the hash only indexes; equality
    // decides). Step 2: revalidate each candidate's recorded dependencies.
    if let Some(candidates) = self.entries.get(&key) {
      for entry in candidates {
        if entry.def == *def && entry.input == input && entry.compiled.deps_valid(env) {
          self.stats.hits += 1;
          return Ok(entry.compiled.clone());
        }
      }
    }
    self.stats.misses += 1;

    if composing.contains(&def.name) {
      return Err(wire_error(
        "recursive-wire",
        format!("recursive wire reference: {}", def.name),
      ));
    }
    composing.push(def.name.clone());
    let result = {
      let mut ctx = ComposeCtx {
        analysis: Analysis::default(),
        input,
        locals: FrameLayout::default(),
        env,
        cache: self,
        composing,
        deps: Vec::new(),
        restart_deps: Vec::new(),
        inline_calls: HashMap::new(),
        site: InlineKey {
          root: Arc::new(def.clone()),
          input,
          path: Arc::default(),
        },
        next_flow: 0,
        diagnostic_path: vec![PathStep::Wire(def.name.clone())],
        local_paths: Vec::new(),
        current_args: None,
        initialized: Vec::new(),
        failed_child: None,
        depth,
        blocks: 0,
      };
      ctx
        .compose_flow_unscoped(&def.flow, input)
        .map(|flow| {
          (
            flow,
            ctx.locals,
            ctx.deps,
            ctx.restart_deps,
            ctx.inline_calls,
          )
        })
        .map_err(|err| err.prefix_path(PathStep::Wire(def.name.clone())))
    };
    composing.pop();
    let (flow, locals, deps, restart_deps, inline_calls) = result?;

    self.stats.wire_composes += 1;
    let compiled = Arc::new(CompiledWire {
      name: def.name.clone(),
      looped: def.looped,
      input,
      flow,
      locals,
      deps,
      definition: Arc::new(def.clone()),
      restart_deps,
      inline_calls,
    });
    self.entries.entry(key).or_default().push(Entry {
      def: def.clone(),
      input,
      compiled: compiled.clone(),
    });
    Ok(compiled)
  }
}
