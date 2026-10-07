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
use crate::diagnostic::{Diagnostic, PathStep, Phase, TypeRef};
use crate::error::{Error, Result};
use crate::flow::CompiledFlow;
use crate::function::{CallCompiled, CallTarget, CompiledFunction, FunctionDef, KeepSlot};
use crate::reload::{FunctionKey, FunctionRegistry};
use crate::shard::{CompiledNode, Composed, ParamValue, ShardDef, ShardType};
use crate::shards::Operand;
use crate::signature::{Analysis, Effects, Lifetime, Occurrence};
use crate::types::Type;

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
  /// A wire definition referenced by name (`Spawn`), or its absence.
  Wire {
    name: String,
    def: Option<Arc<WireDef>>,
  },
  /// A function definition called by name, or its absence.
  Function {
    name: String,
    def: Option<Arc<FunctionDef>>,
  },
}

/// What compose may read besides parameters and input type.
pub struct ComposeEnv<'a> {
  pub mesh_layout: &'a FrameLayout,
  pub wires: &'a HashMap<String, WireDef>,
  pub functions: &'a HashMap<String, FunctionDef>,
}

impl Dep {
  pub(crate) fn still_valid(&self, env: &ComposeEnv<'_>) -> bool {
    match self {
      Dep::MeshVar { name, found } => env.mesh_layout.lookup(name) == *found,
      Dep::Wire { name, def } => env.wires.get(name) == def.as_deref(),
      Dep::Function { name, def } => env.functions.get(name) == def.as_deref(),
    }
  }
}

/// What the flow being composed belongs to: a wire (which sees the whole
/// mesh) or a function body (which sees only what its signature declares,
/// golden path §3.2).
#[derive(Clone)]
pub(crate) enum Owner {
  Wire,
  Function(Arc<FunctionDef>),
}

/// A compiled wire: shared, immutable, and free of per-instance data.
pub struct CompiledWire {
  pub name: String,
  pub looped: bool,
  pub input: Type,
  pub flow: CompiledFlow,
  /// Layout of each instance's local frame.
  pub locals: FrameLayout,
  /// Everything this compose read, for revalidation (e.g. by another mesh).
  pub deps: Vec<Dep>,
  pub(crate) definition: Arc<WireDef>,
  pub(crate) restart_deps: Vec<Dep>,
  /// The function bodies this wire's call sites (and their callees) were
  /// composed against, for reload admission.
  pub(crate) functions: FunctionRegistry,
}

impl CompiledWire {
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
pub struct ComposeCtx<'a> {
  analysis: Analysis,
  input: Type,
  locals: FrameLayout,
  env: &'a ComposeEnv<'a>,
  cache: &'a mut ComposeCache,
  composing: &'a mut Vec<String>,
  deps: Vec<Dep>,
  restart_deps: Vec<Dep>,
  functions: FunctionRegistry,
  // Structural declaration origins for reload diagnostics, never source spans.
  diagnostic_path: Vec<PathStep>,
  local_paths: Vec<Vec<PathStep>>,
  current_args: Option<Arc<Args>>,
  /// Definite initialization of each local slot at the current compose point.
  initialized: Vec<bool>,
  /// The child flow or wire the last error came out of.
  failed_child: Option<Child>,
  /// How many flows enclose the one being composed, counting wires
  /// composed for `Spawn` and function bodies ([`MAX_FLOW_DEPTH`]).
  depth: usize,
  /// How many blocks (nested flows of shards) enclose the shard being
  /// composed within its wire body; 0 at a wire's top level.
  blocks: usize,
  owner: Owner,
  /// `Keep` slots declared so far (a stateful function's persistent state).
  keeps: Vec<KeepSlot>,
}

/// How deeply flows may nest, counting function bodies and wires composed
/// for `Spawn`. Compose recurses once per level: past this, it reports
/// `too-deep` instead of letting the stack overflow. Real scripts stay far
/// below. Activation does not recurse (golden path §6.3); its separate
/// limit is `Mesh::set_max_call_depth`.
pub const MAX_FLOW_DEPTH: usize = 48;

impl ComposeCtx<'_> {
  /// The input type of the shard being composed.
  pub fn input(&self) -> Type {
    self.input
  }

  /// Resolves a variable: the local frame first, then the mesh frame. Mesh
  /// lookups are recorded as dependencies, including absences. Inside a
  /// function body only mesh variables declared in `uses` or `mutates` are
  /// visible; there is no fallback from an unknown local to the mesh.
  pub fn var(&mut self, name: &str) -> Option<VarInfo> {
    if let Some(slot) = self.locals.lookup(name) {
      return Some(VarInfo {
        binding: Binding::Local(slot.index),
        ty: slot.ty,
        mutable: slot.mutable,
        initialized: self.initialized[slot.index],
      });
    }
    if let Owner::Function(def) = &self.owner
      && !def.declares(name)
    {
      return None;
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
      let declared = match &self.owner {
        Owner::Wire => true,
        Owner::Function(def) => def.declares(&name),
      };
      if declared && !names.contains(&name) {
        names.push(name);
      }
    }
    names.sort();
    names
  }

  /// The declared output type when composing a function body: what
  /// `Return` must produce. `None` in a wire.
  pub fn return_type(&self) -> Option<Type> {
    match &self.owner {
      Owner::Wire => None,
      Owner::Function(def) => Some(def.output),
    }
  }

  /// Whether `Keep` and `Once` may declare persistent state here: in a
  /// wire, or in a function declared `stateful: true`.
  pub fn allows_persistent_state(&self) -> bool {
    match &self.owner {
      Owner::Wire => true,
      Owner::Function(def) => def.stateful,
    }
  }

  /// The function being composed, if any (for messages).
  pub fn function_name(&self) -> Option<&str> {
    match &self.owner {
      Owner::Wire => None,
      Owner::Function(def) => Some(&def.name),
    }
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
        if let Owner::Function(def) = &self.owner
          && self.env.mesh_layout.lookup(name).is_some()
        {
          return variable_error(
            "undeclared-mesh-access",
            format!(
              "{name} is a mesh variable that {} does not declare; add `uses: [{name}]` to read it",
              def.name
            ),
          );
        }
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
          if let Owner::Function(def) = &self.owner
            && !def.may_read(name)
          {
            return variable_error(
              "undeclared-mesh-access",
              format!(
                "{name} is declared in `mutates` only; reading it needs `uses: [{name}]` as well"
              ),
            );
          }
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

  /// Records that the shard being composed assigns this variable. Inside a
  /// function body a mesh variable may be assigned only when declared in
  /// `mutates` (`undeclared-mesh-access` otherwise; the caller names the
  /// shard with [`crate::Error::in_shard`]).
  pub fn mark_initialized(&mut self, binding: Binding) -> Result<()> {
    if let Binding::Mesh(i) = binding {
      let (name, slot) = &self.env.mesh_layout.slots[i];
      if let Owner::Function(def) = &self.owner
        && !def.may_write(name)
      {
        return Err(Error::Diagnostic(Box::new(Diagnostic::new(
          Phase::Compose,
          "compose-error",
          "undeclared-mesh-access",
          format!(
            "{name} is a mesh variable that {} may not assign; add `mutates: [{name}]` to write it",
            def.name
          ),
        ))));
      }
      self.analysis.access(name, slot.ty, true);
    }
    if let Binding::Local(i) = binding {
      self.initialized[i] = true;
    }
    Ok(())
  }

  /// Declares a `Keep` slot: a mutable local that a stateful owner retains
  /// between invocations (and across reloads, by name and type).
  pub fn declare_keep(&mut self, name: &str, ty: Type) -> VarInfo {
    let info = self.declare_local(name, ty, true);
    if let (Binding::Local(slot), Some(PathStep::Shard { index, .. })) =
      (info.binding, self.diagnostic_path.last())
    {
      self.keeps.push(KeepSlot {
        name: name.to_string(),
        slot,
        ty,
        node: *index,
      });
    }
    info
  }

  /// Like [`ComposeCtx::compose_flow`], for a flow that might not run (a
  /// branch, or a loop body that can run zero times). Variables it declares
  /// or assigns are not definitely initialized after it.
  pub fn compose_flow_conditional(
    &mut self,
    flow: &[ShardDef],
    input: Type,
  ) -> Result<CompiledFlow> {
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
  pub fn compose_flow(&mut self, flow: &[ShardDef], input: Type) -> Result<CompiledFlow> {
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

  /// Composes a flow whose declarations stay visible after it: a wire or
  /// function body.
  fn compose_flow_unscoped(&mut self, flow: &[ShardDef], input: Type) -> Result<CompiledFlow> {
    if self.depth >= MAX_FLOW_DEPTH {
      return Err(crate::Error::Diagnostic(Box::new(
        crate::diagnostic::Diagnostic::new(
          crate::diagnostic::Phase::Compose,
          "compose-error",
          "too-deep",
          format!(
            "flows nest more than {MAX_FLOW_DEPTH} levels deep (nested flows, function bodies and wires run through Spawn count)"
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
    let result = self.compose_flow_at_depth(flow, input);
    self.depth -= 1;
    self.diagnostic_path.truncate(path_len);
    result
  }

  fn compose_flow_at_depth(&mut self, flow: &[ShardDef], input: Type) -> Result<CompiledFlow> {
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
      self.diagnostic_path.push(PathStep::Shard {
        index,
        name: def.name().into(),
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
      let composed = match &def.function {
        Some(function) => self.compose_call(def, function),
        None => decode(&def.ty.desc, &def.args).and_then(|args| {
          check_input(def.ty, self.input)?;
          let args = Arc::new(args);
          let parent_args = self.current_args.replace(args.clone());
          let result = def.ty.compose_node(&args, self).map_err(|err| {
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
        }),
      };
      self.diagnostic_path.pop();
      let mut node_analysis = std::mem::replace(&mut self.analysis, parent_analysis);
      let composed = match composed {
        Ok(c) => c,
        Err(err) => {
          self.input = saved;
          self.failed_child = Some(Child::Flow(flow.as_ptr()));
          let err = with_input_source(err, &flow[..index]);
          return Err(err.prefix_path(PathStep::Shard {
            index,
            name: def.name().to_string(),
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
          name: def.name().into(),
        }],
      );
      code.push(crate::inline::Instruction::new(
        composed.compiled.inline(),
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

  /// Compiles another wire on its own (`Spawn`), through the cache. Its
  /// dependencies become dependencies of the wire being composed.
  pub fn compose_wire(&mut self, name: &str, input: Type) -> Result<Arc<CompiledWire>> {
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
    self
      .functions
      .extend(wire.functions.iter().map(|(k, v)| (k.clone(), v.clone())));
    Ok(wire)
  }

  /// Resolves a function definition by name, recording the dependency
  /// (including its absence) for cache validation only: a callee's edit
  /// never restarts its callers (golden path §11), reload admission decides.
  fn function_def(&mut self, name: &str) -> Option<Arc<FunctionDef>> {
    let def = self.env.functions.get(name).cloned().map(Arc::new);
    self.deps.push(Dep::Function {
      name: name.to_string(),
      def: def.clone(),
    });
    def
  }

  /// Composes a call site (golden path §3.3): validates the labels against
  /// the parameter list, resolves literal and variable arguments to
  /// operands read once at entry, composes the body once per input type
  /// (shared by every call site), and checks the caller's declared mesh
  /// access covers the callee's.
  fn compose_call(
    &mut self,
    def: &ShardDef,
    name: &str,
  ) -> Result<Composed<Arc<dyn CompiledNode>>> {
    let Some(fdef) = self.function_def(name) else {
      let known: Vec<String> = self.env.functions.keys().cloned().collect();
      let mut d =
        compose_diagnostic("unknown-function", format!("unknown function {name}")).shard(name);
      d.did_you_mean = crate::diagnostic::closest(name, known, 3);
      return Err(Error::Diagnostic(Box::new(d)));
    };
    if fdef.stateful
      && let Owner::Function(caller) = &self.owner
      && !caller.stateful
    {
      return Err(fn_error(
        name,
        "stateful-call-in-stateless",
        format!(
          "{name} is stateful, so each call site owns an instance of it; {} is stateless and cannot: declare the caller `stateful: true`, or make {name} stateless",
          caller.name
        ),
      ));
    }
    if !fdef.ignores_input() && !fdef.input.accepts(self.input) {
      return Err(Error::Diagnostic(Box::new(
        Diagnostic::new(
          Phase::Compose,
          "input-type-mismatch",
          "input-type-mismatch",
          format!("{name} needs {} input, got {}", fdef.input, self.input),
        )
        .shard(name)
        .types(Some(TypeRef::of(self.input)), vec![TypeRef::of(fdef.input)]),
      )));
    }
    // Labels are validated against the parameter list first; then each
    // argument is a literal or a variable read once at entry.
    let mut given: Vec<Option<Operand>> = vec![None; fdef.params.len()];
    let mut seen_named = false;
    for (position, arg) in def.args.iter().enumerate() {
      let index = match &arg.name {
        Some(label) => {
          seen_named = true;
          match fdef.params.iter().position(|p| p.name == *label) {
            Some(index) => index,
            None => {
              let known: Vec<String> = fdef.params.iter().map(|p| p.name.clone()).collect();
              let mut d = compose_diagnostic(
                "unknown-argument",
                format!(
                  "{name} has no parameter {label} (parameters: {})",
                  known.join(", ")
                ),
              )
              .shard(name)
              .param(label, None);
              d.did_you_mean = crate::diagnostic::closest(label, known, 3);
              return Err(Error::Diagnostic(Box::new(d)));
            }
          }
        }
        None if seen_named => {
          return Err(fn_error(
            name,
            "positional-after-named",
            format!("{name}: positional argument {position} after a named one"),
          ));
        }
        None if position >= fdef.params.len() => {
          return Err(fn_error(
            name,
            "too-many-arguments",
            format!(
              "{name} takes at most {} arguments, got {}",
              fdef.params.len(),
              def.args.len()
            ),
          ));
        }
        None => position,
      };
      let param = &fdef.params[index];
      if given[index].is_some() {
        return Err(
          fn_error(
            name,
            "duplicate-argument",
            format!("{name}: {} given more than once", param.name),
          )
          .with_param(&param.name, index),
        );
      }
      let (operand, ty) = match &arg.value {
        ParamValue::Value(v) => (Operand::Const(v.clone().into_struct_tables()), v.type_of()),
        ParamValue::Var(var) => {
          let info = self
            .read_var(var, name)
            .map_err(|e| e.with_param(&param.name, index))?;
          (Operand::Bound(info.binding), info.ty)
        }
        other => {
          let form = match other {
            ParamValue::Wire(_) => "wire",
            ParamValue::Flow(_) => "flow",
            ParamValue::Cases(_) => "cases",
            _ => unreachable!(),
          };
          return Err(
            fn_error(
              name,
              "wrong-argument-form",
              format!(
                "{name}: {} takes a literal or a variable, got a {form}",
                param.name
              ),
            )
            .with_param(&param.name, index),
          );
        }
      };
      if !param.ty.accepts(ty) {
        let code = if matches!(arg.value, ParamValue::Value(_)) {
          "wrong-argument-type"
        } else {
          "wrong-variable-type"
        };
        return Err(Error::Diagnostic(Box::new(
          compose_diagnostic(
            code,
            format!("{name}: {} must be {}, got {ty}", param.name, param.ty),
          )
          .shard(name)
          .param(&param.name, Some(index))
          .types(Some(TypeRef::of(ty)), vec![TypeRef::of(param.ty)]),
        )));
      }
      given[index] = Some(operand);
    }
    let mut args = Vec::with_capacity(fdef.params.len());
    for (index, (param, operand)) in fdef.params.iter().zip(given).enumerate() {
      args.push(match (operand, &param.default) {
        (Some(operand), _) => operand,
        (None, Some(default)) => Operand::Const(default.clone().into_struct_tables()),
        (None, None) => {
          return Err(
            fn_error(
              name,
              "missing-argument",
              format!("{name}: missing required parameter {}", param.name),
            )
            .with_param(&param.name, index),
          );
        }
      });
    }
    let input = if fdef.ignores_input() {
      Type::none()
    } else {
      self.input
    };
    // A call to a function being composed (direct or mutual recursion,
    // golden path M7) composes against the declared signature: the call
    // site resolves the body at entry, and its effects are the recursive
    // group's, known after the group's first pass.
    if self.composing.iter().any(|n| *n == fdef.name) {
      if fdef.stateful {
        return Err(fn_error(
          name,
          "recursive-stateful",
          format!(
            "{name} is stateful and calls itself (directly or through other functions): a stateful function owns one instance per call site and cannot re-enter it"
          ),
        ));
      }
      let group = self.cache.recursive_reference(name);
      if let Owner::Function(caller) = &self.owner {
        for access in &group.uses {
          if !caller.may_read(&access.name) {
            return Err(fn_error(
              name,
              "undeclared-mesh-access",
              format!(
                "{name} reads mesh variable {}; {} must declare it in `uses: [{}]` to call it",
                access.name, caller.name, access.name
              ),
            ));
          }
        }
        for access in &group.mutates {
          if !caller.may_write(&access.name) {
            return Err(fn_error(
              name,
              "undeclared-mesh-access",
              format!(
                "{name} assigns mesh variable {}; {} must declare it in `mutates: [{}]` to call it",
                access.name, caller.name, access.name
              ),
            ));
          }
        }
      }
      self
        .analysis
        .include(&group, &[PathStep::Function(name.to_string())]);
      self.analysis.lifetime = Lifetime::Stateless;
      return Ok(Composed {
        compiled: crate::shard::erase::<crate::stackless::shards::Call>(CallCompiled {
          target: CallTarget::Lazy {
            key: FunctionKey {
              name: name.to_string(),
              input,
            },
            def: fdef,
          },
          args,
        }),
        output: fdef_output(&self.env.functions[name]),
      });
    }
    let body = self
      .cache
      .get_or_compose_function(&fdef, input, self.env, self.composing, self.depth)
      .map_err(|err| self.explain_caller_local(err, name))?;
    // Mesh access is declared, never granted by inference (§3.1): a caller
    // function must declare what its callees reach.
    if let Owner::Function(caller) = &self.owner {
      for access in &body.flow.analysis.uses {
        if !caller.may_read(&access.name) {
          return Err(fn_error(
            name,
            "undeclared-mesh-access",
            format!(
              "{name} reads mesh variable {}; {} must declare it in `uses: [{}]` to call it",
              access.name, caller.name, access.name
            ),
          ));
        }
      }
      for access in &body.flow.analysis.mutates {
        if !caller.may_write(&access.name) {
          return Err(fn_error(
            name,
            "undeclared-mesh-access",
            format!(
              "{name} assigns mesh variable {}; {} must declare it in `mutates: [{}]` to call it",
              access.name, caller.name, access.name
            ),
          ));
        }
      }
    }
    self.deps.extend(body.deps.iter().cloned());
    self
      .functions
      .extend(body.functions.iter().map(|(k, v)| (k.clone(), v.clone())));
    self.functions.insert(FunctionKey::of(&body), body.clone());
    self
      .analysis
      .include(&body.flow.analysis, &[PathStep::Function(name.to_string())]);
    // The call site's lifetime is the function's: a stateless function
    // keeps nothing between invocations, whatever its body holds inside one.
    self.analysis.lifetime = if fdef.stateful {
      Lifetime::Stateful
    } else {
      Lifetime::Stateless
    };
    Ok(Composed {
      compiled: crate::shard::erase::<crate::stackless::shards::Call>(CallCompiled {
        target: CallTarget::Direct(body),
        args,
      }),
      output: fdef.output,
    })
  }

  /// An `unknown-variable` inside a callee that names one of the caller's
  /// locals: say how to pass it (golden path §3.2, test B).
  fn explain_caller_local(&self, err: Error, function: &str) -> Error {
    let Error::Diagnostic(mut d) = err else {
      return err;
    };
    if d.code == "unknown-variable"
      && let Some(rest) = d.message.strip_prefix("unknown variable ")
    {
      let var: String = rest
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != ';')
        .collect();
      if let Some(slot) = self.locals.lookup(&var) {
        d.message.push_str(&format!(
          "; `{var}` is a local of the caller, and a function sees only its input and parameters: pass it as a parameter (`params: {{{var}: {}}}` in the declaration, `{function}({var}: {var})` at the call)",
          slot.ty
        ));
      }
    }
    Error::Diagnostic(d)
  }
}

fn fdef_output(def: &FunctionDef) -> Type {
  def.output
}

fn compose_diagnostic(code: &'static str, message: String) -> Diagnostic {
  Diagnostic::new(Phase::Compose, "compose-error", code, message)
}

/// A structured compose error about function `name`.
fn fn_error(name: &str, code: &'static str, message: String) -> Error {
  Error::Diagnostic(Box::new(compose_diagnostic(code, message).shard(name)))
}

/// The first occurrence in a body with the picked effect: the shard a
/// `not-pure` diagnostic names.
fn offending_shard(analysis: &Analysis, pick: fn(Effects) -> bool) -> String {
  analysis
    .occurrences
    .iter()
    .find(|o| pick(o.effects))
    .and_then(|o| match o.path.last() {
      Some(PathStep::Shard { name, .. }) => Some(name.clone()),
      _ => None,
    })
    .unwrap_or_else(|| "a shard".to_string())
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
    if def.function.is_none() && matches!(def.ty.desc.output, OutputDesc::Passthrough) {
      via.push((index, def.name().to_string()));
    } else {
      origin = Some((index, def.name().to_string()));
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
  /// Function bodies actually composed (one per definition and input type
  /// while its dependencies hold).
  pub function_composes: u64,
}

struct Entry {
  def: WireDef,
  input: Type,
  compiled: Arc<CompiledWire>,
}

struct FunctionEntry {
  def: Arc<FunctionDef>,
  input: Type,
  compiled: Arc<CompiledFunction>,
}

pub type HashFn = fn(&WireDef, Type) -> u64;

fn default_hash(def: &WireDef, input: Type) -> u64 {
  let mut hasher = DefaultHasher::new();
  def.hash(&mut hasher);
  input.hash(&mut hasher);
  hasher.finish()
}

/// A recursive group being composed (golden path M7): its root is the
/// function whose body was referenced while composing; the group's effects
/// and mesh access are the root's first-pass analysis, applied to every
/// recursive call site in the second pass.
struct RecursiveGroup {
  root: String,
  analysis: Analysis,
}

/// Compose cache with two-step lookup (contract §4).
pub struct ComposeCache {
  entries: HashMap<u64, Vec<Entry>>,
  functions: HashMap<u64, Vec<FunctionEntry>>,
  hash_fn: HashFn,
  pub stats: CacheStats,
  /// Functions referenced while being composed, in the current pass.
  recursive_refs: Vec<String>,
  group: Option<RecursiveGroup>,
}

impl Default for ComposeCache {
  fn default() -> ComposeCache {
    ComposeCache::with_hash_fn(default_hash)
  }
}

impl ComposeCache {
  /// A cache with a custom primary-key hash (tests force collisions with it).
  pub fn with_hash_fn(hash_fn: HashFn) -> ComposeCache {
    ComposeCache {
      entries: HashMap::new(),
      functions: HashMap::new(),
      hash_fn,
      stats: CacheStats::default(),
      recursive_refs: Vec::new(),
      group: None,
    }
  }

  pub(crate) fn get_or_compose(
    &mut self,
    def: &WireDef,
    input: Type,
    env: &ComposeEnv<'_>,
    composing: &mut Vec<String>,
    depth: usize,
  ) -> Result<Arc<CompiledWire>> {
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
        functions: HashMap::new(),
        diagnostic_path: vec![PathStep::Wire(def.name.clone())],
        local_paths: Vec::new(),
        current_args: None,
        initialized: Vec::new(),
        failed_child: None,
        depth,
        blocks: 0,
        owner: Owner::Wire,
        keeps: Vec::new(),
      };
      ctx
        .compose_flow_unscoped(&def.flow, input)
        .map(|flow| (flow, ctx.locals, ctx.deps, ctx.restart_deps, ctx.functions))
        .map_err(|err| err.prefix_path(PathStep::Wire(def.name.clone())))
    };
    composing.pop();
    let (flow, locals, deps, restart_deps, functions) = result?;

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
      functions,
    });
    self.entries.entry(key).or_default().push(Entry {
      def: def.clone(),
      input,
      compiled: compiled.clone(),
    });
    Ok(compiled)
  }

  /// A function body composed against its declared signature, once per
  /// input type (golden path §4): the frame holds the parameters, then
  /// `input`, then the body's locals. Runtime argument values never enter
  /// the key; the definition, the input type and the recorded dependencies
  /// do.
  pub(crate) fn get_or_compose_function(
    &mut self,
    def: &Arc<FunctionDef>,
    input: Type,
    env: &ComposeEnv<'_>,
    composing: &mut Vec<String>,
    depth: usize,
  ) -> Result<Arc<CompiledFunction>> {
    let key = {
      let mut hasher = DefaultHasher::new();
      def.hash(&mut hasher);
      input.hash(&mut hasher);
      hasher.finish()
    };
    if let Some(candidates) = self.functions.get(&key) {
      for entry in candidates {
        if entry.def == *def
          && entry.input == input
          && entry.compiled.deps.iter().all(|d| d.still_valid(env))
        {
          self.stats.hits += 1;
          return Ok(entry.compiled.clone());
        }
      }
    }
    self.stats.misses += 1;
    // A recursive reference while composing is handled by the call site
    // (`compose_call`); reaching here with the name in `composing` cannot
    // happen.
    debug_assert!(!composing.iter().any(|n| n == &def.name));
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
        functions: HashMap::new(),
        diagnostic_path: vec![PathStep::Function(def.name.clone())],
        local_paths: Vec::new(),
        current_args: None,
        initialized: Vec::new(),
        failed_child: None,
        depth,
        blocks: 0,
        owner: Owner::Function(def.clone()),
        keeps: Vec::new(),
      };
      let slot_of = |info: VarInfo| match info.binding {
        Binding::Local(i) => i,
        Binding::Mesh(_) => unreachable!("declared locally"),
      };
      let param_slots: Vec<usize> = def
        .params
        .iter()
        .map(|p| slot_of(ctx.declare_local(&p.name, p.ty, false)))
        .collect();
      let input_slot = slot_of(ctx.declare_local("input", input, false));
      ctx
        .compose_flow_unscoped(&def.body, input)
        .and_then(|flow| {
          if def.output.accepts(flow.output) {
            Ok(flow)
          } else {
            Err(Error::Diagnostic(Box::new(
              compose_diagnostic(
                "output-type-mismatch",
                format!(
                  "{} declares output {} but its body outputs {}",
                  def.name, def.output, flow.output
                ),
              )
              .shard(&def.name)
              .types(
                Some(TypeRef::of(flow.output)),
                vec![TypeRef::of(def.output)],
              ),
            )))
          }
        })
        .map(|flow| {
          (
            flow,
            ctx.locals,
            ctx.deps,
            ctx.keeps,
            ctx.functions,
            input_slot,
            param_slots,
          )
        })
        .map_err(|err| err.prefix_path(PathStep::Function(def.name.clone())))
    };
    composing.pop();
    let (flow, locals, deps, keeps, functions, input_slot, param_slots) = result?;
    if def.pure {
      let analysis = &flow.analysis;
      let effect = [
        (
          analysis.effects.suspends,
          "suspends",
          (|e: Effects| e.suspends) as fn(Effects) -> bool,
        ),
        (analysis.effects.io, "io", |e| e.io),
        (analysis.effects.time, "time", |e| e.time),
        (analysis.effects.random, "random", |e| e.random),
        (analysis.effects.unknown, "unknown", |e| e.unknown),
      ]
      .into_iter()
      .find(|(set, _, _)| *set);
      let reason = if def.stateful {
        Some("it is declared stateful".to_string())
      } else if let Some((_, label, pick)) = effect {
        Some(format!(
          "{} has the effect `{label}`",
          offending_shard(analysis, pick)
        ))
      } else if let Some(access) = analysis.uses.first() {
        Some(format!("it reads mesh variable {}", access.name))
      } else {
        analysis
          .mutates
          .first()
          .map(|access| format!("it assigns mesh variable {}", access.name))
      };
      if let Some(reason) = reason {
        return Err(
          fn_error(
            &def.name,
            "not-pure",
            format!("{} is declared pure, but {reason}", def.name),
          )
          .prefix_path(PathStep::Function(def.name.clone())),
        );
      }
    }
    self.stats.function_composes += 1;
    // The root of a recursive group: its first pass found the group's
    // effects; compose it again with every recursive call site carrying
    // them (a fixpoint after one more pass, since the union is finite).
    let referenced = self.recursive_refs.iter().any(|n| *n == def.name);
    if referenced && self.group.as_ref().is_none_or(|g| g.root != def.name) {
      let outer = self.group.replace(RecursiveGroup {
        root: def.name.clone(),
        analysis: flow.analysis.clone(),
      });
      self.recursive_refs.retain(|n| *n != def.name);
      let result = self.get_or_compose_function(def, input, env, composing, depth);
      self.group = outer;
      return result;
    }
    let compiled = Arc::new(CompiledFunction {
      def: def.clone(),
      input,
      flow,
      locals,
      input_slot,
      param_slots,
      keeps,
      deps,
      functions,
    });
    // Bodies composed during a group's first pass carry provisional
    // effects at their recursive call sites: keep them out of the cache.
    if self.recursive_refs.is_empty() || self.group.is_some() {
      self.functions.entry(key).or_default().push(FunctionEntry {
        def: def.clone(),
        input,
        compiled: compiled.clone(),
      });
    }
    if self.group.as_ref().is_some_and(|g| g.root == def.name) {
      self.group = None;
      self.recursive_refs.retain(|n| *n != def.name);
    }
    Ok(compiled)
  }

  /// A call to a function being composed: records the reference and gives
  /// the analysis its call site carries (the group's, once known).
  pub(crate) fn recursive_reference(&mut self, name: &str) -> Analysis {
    if !self.recursive_refs.iter().any(|n| n == name) {
      self.recursive_refs.push(name.to_string());
    }
    match &self.group {
      Some(group) => Analysis {
        effects: group.analysis.effects,
        lifetime: Lifetime::Stateless,
        uses: group.analysis.uses.clone(),
        mutates: group.analysis.mutates.clone(),
        occurrences: Default::default(),
      },
      None => Analysis::default(),
    }
  }
}
