//! A compiled flow: a sequence of compiled nodes. Wires, and shards with
//! sub-flows (`Once`, `When`, `Do`...), hold these; the engine
//! (`stackless/engine.rs`) owns and runs their per-instance state.

use std::sync::Arc;

use crate::shard::CompiledNode;
use crate::types::Type;

/// A compiled flow: shared, immutable, and free of per-instance data.
pub struct CompiledFlow {
  pub(crate) nodes: Vec<Arc<dyn CompiledNode>>,
  pub(crate) code: Vec<crate::inline::Instruction>,
  pub output: Type,
  pub analysis: crate::signature::Analysis,
}

impl CompiledFlow {
  /// The kind of instruction each top-level node compiled to (`"fallback"`
  /// for a node that runs through its shard). For tests that pin which
  /// builtin a shard lowers to; not a stable API.
  #[doc(hidden)]
  pub fn instruction_kinds(&self) -> Vec<&'static str> {
    self.code.iter().map(|i| i.op.name()).collect()
  }
}
