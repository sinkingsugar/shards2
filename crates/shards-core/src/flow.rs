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
  /// The code runs through one `inline::run` call with no engine help: no
  /// node activates through its shard, no call site, no constructor
  /// (`inline::leaf_code`). A composite runs such a child flow inside its
  /// own step instead of entering a frame for it.
  pub(crate) leaf: bool,
  /// For every instruction, the node it stands for (an index into
  /// `nodes`): what the engine activates, or dispatches, when the VM stops
  /// at it. A composite lowered to flat code (`Repeat`, `While`, `When`,
  /// `If`) contributes its children's nodes and its own control
  /// instructions, which stand for no node (`NO_NODE`).
  pub(crate) pc_nodes: Vec<u32>,
  /// What lowering added beyond the flow's own nodes, when it added
  /// anything (most flows: `None`, one word).
  pub(crate) lowered: Option<Box<Lowered>>,
}

/// The parts of a flow's code that compose lowering added (`ComposeCtx::flatten`).
#[derive(Default)]
pub(crate) struct Lowered {
  /// Every call inlined into this code, as the range of `nodes` its body
  /// brought (`start`, `len`; possibly empty), in order. The nodes outside
  /// them are the flow's own: a recompiled flow whose inlined bodies
  /// changed has the same own nodes, which is how a live instance moves
  /// onto it with its state (`Engine::rebase`).
  pub inlined: Vec<(u32, u32)>,
  /// The hidden slots this code writes that may hold a heap value: an
  /// inlined call's input, arguments and locals, a flattened composite's
  /// saved input. Each is cleared where its value ends (`Op::Clear`), and
  /// all of them when the code ends otherwise (a failure, a `Stop`).
  pub released_slots: Vec<u32>,
}

/// The `pc_nodes` entry of a control instruction: never activated.
pub(crate) const NO_NODE: u32 = u32::MAX;

impl CompiledFlow {
  /// `Lowered::inlined`.
  pub(crate) fn inlined(&self) -> &[(u32, u32)] {
    self.lowered.as_ref().map_or(&[], |l| &l.inlined)
  }

  /// `Lowered::released_slots`.
  pub(crate) fn released_slots(&self) -> &[u32] {
    self.lowered.as_ref().map_or(&[], |l| &l.released_slots)
  }

  /// The node instruction `pc` stands for; `nodes.len()` at the end of
  /// the code. A control instruction stands for none and is never asked.
  #[inline]
  pub(crate) fn node_at(&self, pc: usize) -> usize {
    match self.pc_nodes.get(pc) {
      Some(&node) => {
        debug_assert!(node != NO_NODE, "a control instruction stopped a run");
        node as usize
      }
      None => self.nodes.len(),
    }
  }
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
