//! Immutable call-site descriptions and mesh-local inline revision selection.
//!
//! A Do call pins its selected flow until it returns. Selection is scoped to
//! a mesh; shared compiled artifacts are never mutated by a reload.

use std::cell::Cell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::compose::{Backend, CompiledWire, ComposeEnv, Dep, FrameLayout, WireDef};
use crate::diagnostic::{Diagnostic, PathStep, Phase};
use crate::flow::CompiledFlow;
use crate::{Error, Result, Type};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SiteStep {
  Flow(usize),
  Node(usize),
  Wire(Arc<WireDef>),
}

/// Persistent call-site path. Extending a nested call shares its ancestors;
/// retaining every call boundary must not copy all preceding steps.
#[derive(Clone, Default)]
pub(crate) struct SitePath(Option<Arc<SitePathNode>>);

struct SitePathNode {
  step: SiteStep,
  parent: SitePath,
  len: usize,
}

impl SitePath {
  fn len(&self) -> usize {
    self.0.as_ref().map_or(0, |node| node.len)
  }

  pub fn push(&mut self, step: SiteStep) {
    let len = self.len() + 1;
    let parent = std::mem::take(self);
    self.0 = Some(Arc::new(SitePathNode { step, parent, len }));
  }

  pub fn pop(&mut self) {
    if let Some(node) = self.0.take() {
      *self = node.parent.clone();
    }
  }

  fn reversed(&self) -> impl Iterator<Item = &SiteStep> {
    std::iter::successors(self.0.as_deref(), |node| node.parent.0.as_deref()).map(|node| &node.step)
  }
}

impl PartialEq for SitePath {
  fn eq(&self, other: &Self) -> bool {
    // Order is significant; iterating both paths from leaf to root preserves
    // structural equality without materializing either path.
    self.len() == other.len() && self.reversed().eq(other.reversed())
  }
}
impl Eq for SitePath {}

impl Hash for SitePath {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.len().hash(state);
    for step in self.reversed() {
      step.hash(state);
    }
  }
}

impl std::fmt::Debug for SitePath {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_tuple("SitePath(leaf to root)")
      .field(&self.reversed().collect::<Vec<_>>())
      .finish()
  }
}

impl Drop for SitePath {
  fn drop(&mut self) {
    let mut tail = self.0.take();
    while let Some(node) = tail {
      match Arc::try_unwrap(node) {
        Ok(mut node) => tail = node.parent.0.take(),
        Err(_) => break, // another owner keeps the rest of this prefix alive
      }
    }
  }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct InlineKey {
  pub root: Arc<WireDef>,
  pub input: Type,
  pub path: SitePath,
}

#[derive(Clone, PartialEq)]
pub(crate) struct InlineSignature {
  pub input: Type,
  pub before: FrameLayout,
  pub after: FrameLayout,
  pub initialized_before: Vec<bool>,
  pub initialized_after: Vec<bool>,
  pub output: Type,
}

/// The immutable default for one Do call site, also used as a revision.
pub struct InlineCall<B: Backend> {
  pub(crate) key: InlineKey,
  pub(crate) name: String,
  pub(crate) signature: InlineSignature,
  pub(crate) deps: Vec<Dep>,
  pub(crate) local_paths: Vec<Vec<PathStep>>,
  pub(crate) flow: CompiledFlow<B>,
}

pub(crate) type InlineRegistry<B> = HashMap<InlineKey, Arc<InlineCall<B>>>;

/// A mesh's installed Do revisions. Keys are structural (they hash whole wire
/// definitions), so a Do consults the registry once per accepted reload, when
/// `revision` changes, rather than on every call.
pub(crate) struct Revisions<B: Backend> {
  pub calls: InlineRegistry<B>,
  /// Accepted preserving reloads; 0 means the registry was never installed.
  pub revision: u64,
  /// Registry lookups made by Do calls, for tests and diagnostics.
  pub lookups: Cell<u64>,
}

impl<B: Backend> Default for Revisions<B> {
  fn default() -> Self {
    Self {
      calls: HashMap::new(),
      revision: 0,
      lookups: Cell::new(0),
    }
  }
}

impl<B: Backend> Revisions<B> {
  pub fn select(&self, key: &InlineKey) -> Option<Arc<InlineCall<B>>> {
    self.lookups.set(self.lookups.get() + 1);
    self.calls.get(key).cloned()
  }

  pub fn install(&mut self, calls: InlineRegistry<B>) {
    self.calls = calls;
    self.revision += 1;
  }
}

pub(crate) fn reusable<B: Backend>(wire: &CompiledWire<B>, env: &ComposeEnv<'_>) -> bool {
  env.wires.get(&wire.name) == Some(wire.definition.as_ref())
    && wire.restart_deps.iter().all(|d| d.still_valid(env))
}

pub(crate) fn validate<B: Backend>(
  old: &CompiledWire<B>,
  env: &ComposeEnv<'_>,
  active: &InlineRegistry<B>,
  next: &InlineRegistry<B>,
) -> Result<()> {
  if !reusable(old, env) {
    return Ok(());
  }
  for (key, call) in old.inline_calls.iter().chain(
    active
      .iter()
      .filter(|(k, _)| k.root == old.definition && k.input == old.input),
  ) {
    if let Some(new) = next.get(key)
      && call.signature != new.signature
    {
      let (reason, slot) = incompatibility(&call.signature, &new.signature);
      let mut diagnostic = Diagnostic::new(
        Phase::Compose,
        "reload-incompatible",
        "reload-incompatible",
        format!(
          "cannot preserve caller of {}: {reason}; use a full restart (press r in watch)",
          new.name
        ),
      );
      diagnostic.path = slot
        .and_then(|i| new.local_paths.get(i).cloned())
        .unwrap_or_else(|| vec![PathStep::Wire(new.name.clone())]);
      return Err(Error::Diagnostic(Box::new(diagnostic)));
    }
  }
  Ok(())
}

pub(crate) fn reuse_unchanged<B: Backend>(old: &InlineRegistry<B>, next: &mut InlineRegistry<B>) {
  for (key, new) in next {
    if let Some(previous) = old.get(key)
      && previous.signature == new.signature
      && previous.deps == new.deps
    {
      *new = previous.clone();
    }
  }
}

/// Report a concrete difference, preferring declarations over their knock-on
/// effects on slot indices or the flow's output.
fn incompatibility(old: &InlineSignature, new: &InlineSignature) -> (String, Option<usize>) {
  for (name, slot) in new.after.slots() {
    if old.after.lookup(name).is_none() {
      return (
        format!(
          "new local `{name}` ({}) changes the retained local frame",
          slot.ty
        ),
        Some(slot.index),
      );
    }
  }
  for (name, _) in old.after.slots() {
    if new.after.lookup(name).is_none() {
      return (
        format!("local `{name}` was removed from the retained local frame"),
        None,
      );
    }
  }
  for (label, before, after) in [
    ("entry", &old.before, &new.before),
    ("exit", &old.after, &new.after),
  ] {
    for ((name, a), (new_name, b)) in before.slots().iter().zip(after.slots()) {
      let reason = if name != new_name {
        Some(format!(
          "local slot {} changed from `{name}` to `{new_name}` at call {label}",
          a.index
        ))
      } else if a.ty != b.ty {
        Some(format!(
          "local `{name}` changed type from {} to {}",
          a.ty, b.ty
        ))
      } else if a.mutable != b.mutable {
        Some(format!("local `{name}` changed mutability"))
      } else {
        None
      };
      if let Some(reason) = reason {
        return (reason, Some(b.index));
      }
    }
    if before.len() != after.len() {
      return (
        format!(
          "local frame at call {label} changed from {} to {} slots",
          before.len(),
          after.len()
        ),
        None,
      );
    }
  }
  if old.input != new.input {
    return (
      format!("input type changed from {} to {}", old.input, new.input),
      None,
    );
  }
  if old.output != new.output {
    return (
      format!("output type changed from {} to {}", old.output, new.output),
      None,
    );
  }
  for (a, b) in [
    (&old.initialized_before, &new.initialized_before),
    (&old.initialized_after, &new.initialized_after),
  ] {
    if let Some(index) = a.iter().zip(b).position(|(a, b)| a != b) {
      let name = &new.after.slots()[index].0;
      return (
        format!("local `{name}` changed definite initialization"),
        Some(index),
      );
    }
  }
  ("local binding contract changed".into(), None)
}

#[cfg(test)]
mod path_tests {
  use super::*;
  use std::collections::hash_map::DefaultHasher;

  fn hash(path: &SitePath) -> u64 {
    let mut h = DefaultHasher::new();
    path.hash(&mut h);
    h.finish()
  }

  #[test]
  fn call_paths_share_prefixes_but_compare_by_content() {
    let mut first = SitePath::default();
    first.push(SiteStep::Flow(0));
    first.push(SiteStep::Node(2));
    let saved = first.clone();
    first.push(SiteStep::Flow(1));
    assert!(Arc::ptr_eq(
      first.0.as_ref().unwrap().parent.0.as_ref().unwrap(),
      saved.0.as_ref().unwrap()
    ));
    assert_ne!(first, saved);
    first.pop();
    assert_eq!(first, saved);
    let mut independent = SitePath::default();
    independent.push(SiteStep::Flow(0));
    independent.push(SiteStep::Node(2));
    assert_eq!(independent, saved);
    assert_eq!(hash(&independent), hash(&saved));
    independent.pop();
    independent.push(SiteStep::Flow(2));
    assert_ne!(independent, saved);
  }

  #[test]
  fn long_paths_drop_iteratively_with_shared_ancestors() {
    let mut path = SitePath::default();
    for i in 0..10_000 {
      path.push(SiteStep::Node(i));
    }
    let saved = path.clone();
    for i in 10_000..20_000 {
      path.push(SiteStep::Node(i));
    }
    drop(path);
    assert_eq!(saved.len(), 10_000);
    assert_eq!(Arc::strong_count(saved.0.as_ref().unwrap()), 1);
    drop(saved);
  }
}
