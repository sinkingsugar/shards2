//! Immutable call-site descriptions and mesh-local inline revision selection.
//!
//! A Do call pins its selected flow until it returns. Selection is scoped to
//! a mesh; shared compiled artifacts are never mutated by a reload.

use std::collections::HashMap;
use std::sync::Arc;

use crate::compose::{Backend, CompiledWire, ComposeEnv, Dep, FrameLayout, WireDef};
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
      return Err(Error::Diagnostic(Box::new(
        crate::diagnostic::Diagnostic::new(
          crate::diagnostic::Phase::Compose,
          "reload-incompatible",
          "reload-incompatible",
          format!("cannot preserve caller of {}: its input, output or local bindings changed; use a full restart", new.name),
        ),
      )).prefix_path(crate::diagnostic::PathStep::Wire(new.name.clone())));
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
