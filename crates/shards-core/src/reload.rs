//! Immutable call-site descriptions and mesh-local inline revision selection.
//!
//! A Do call pins its selected flow until it returns. Selection is scoped to
//! a mesh; shared compiled artifacts are never mutated by a reload.

use std::collections::HashMap;
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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct InlineKey {
  pub root: Arc<WireDef>,
  pub input: Type,
  pub path: Vec<SiteStep>,
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
