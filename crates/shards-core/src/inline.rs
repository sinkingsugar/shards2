//! Compose-selected builtins and a borrowed accumulator for uninterrupted runs.
//!
//! The pointer accumulator never escapes `run`. It refers to owned scratch,
//! an immutable instruction constant, a frame slot, or numeric result storage.
//! Frames are exclusively borrowed and cannot resize or suspend during a run.
//! Each operation consumes its input before replacing storage and reanchors the
//! accumulator after a write. A run ends before any arbitrary shard activation,
//! returning owned output so nested flows, host writes and suspension see a snapshot.

use std::any::{Any, TypeId};

use crate::compose::Binding;
use crate::error::{Error, Result};
use crate::shards::Operand;
use crate::shards::data;
use crate::shards::leaf::{self, LeafShard};
use crate::shards::math::{self, BinOp};
use crate::shards::values;
use crate::types::{Shape, Type};
use crate::var::{Float4, Table, Var};
use std::sync::Arc;

/// Opaque internal optimization hook. Ordinary shards use the default `None`.
#[doc(hidden)]
pub struct InlineOp(pub(crate) Op);

// Explicit tags keep dispatch a direct byte load rather than decoding niches.
#[repr(u8)]
pub(crate) enum Op {
  Fallback,
  Const(Var),
  GetLocal(SlotOffset),
  GetMesh(SlotOffset),
  Set(Binding),
  Inc(Binding),
  ConstDrop(Var),
  GetLocalDrop(SlotOffset),
  GetMeshDrop(SlotOffset),
  SetDrop(Binding),
  IncDrop(Binding),
  Take(Operand),
  /// A literal key resolved on a fixed table (golden path §7.3).
  TakeSlot(usize),
  Push(Binding),
  SeqMake(Vec<Operand>),
  /// A struct table: its shape and one operand per slot, in key order.
  TableMake(Shape, Vec<Operand>),
  AddIntConst(i64),
  AddIntBound(Binding),
  AddFloatConst(f64),
  AddFloatBound(Binding),
  AddFloat4Const(Float4),
  AddFloat4Bound(Binding),
  /// Any other arithmetic (vectors, mixed numbers, subtract/multiply/
  /// divide): the shared numeric rule without a node activation.
  Arith(BinOp, Operand),
  /// An ordered comparison of numbers.
  Compare(Cmp, Operand),
  /// The language's equality (`Is`, or `IsNot` when negated).
  Equal(bool, Operand),
  /// Passes the value through (`Keep`: its slot was set at frame creation).
  Pass,
  /// A call to a straight-line body from a stateless, non-recursive site:
  /// runs the callee's code on its kept locals right here (`VmCalls`
  /// supplies them), without an engine step. When the site is not ready
  /// (first call, a reload) the run stops before it and the engine enters
  /// the frame.
  VmCall,
  /// A `Repeat` whose body (and `until` flow) is straight-line code: the
  /// loop runs here, on this frame's locals, without engine steps. Boxed
  /// so every instruction stays 48 bytes.
  VmRepeat(Box<VmLoop>),
  /// A `When` or `While` whose predicate and action are straight-line
  /// code: runs here like `VmRepeat`, and passes its input through.
  VmBranch(Box<VmBranch>),
}

#[cfg(test)]
mod size {
  #[test]
  fn an_instruction_is_48_bytes_in_release_layout() {
    // `Instruction` adds the output check in debug builds; the op itself is
    // one value plus a tag.
    assert_eq!(std::mem::size_of::<super::Op>(), 48);
  }
}

/// What a VM-level `Repeat` runs.
pub(crate) struct VmLoop {
  pub body: Arc<crate::flow::CompiledFlow>,
  pub times: Option<Operand>,
  pub until: Option<Arc<crate::flow::CompiledFlow>>,
}

/// What a VM-level `When` (`looping: false`) or `While` runs.
pub(crate) struct VmBranch {
  pub pred: Arc<crate::flow::CompiledFlow>,
  pub body: Arc<crate::flow::CompiledFlow>,
  pub looping: bool,
}

/// Whether a flow's code runs through the VM from start to end: no node
/// activates through its shard. Composite instructions (`VmCall`,
/// `VmRepeat`, `VmBranch`) count as code; a composite without one is a
/// `Fallback`.
pub(crate) fn straight_line(code: &[Instruction]) -> bool {
  code.iter().all(|i| !matches!(i.op, Op::Fallback))
}

/// Whether a flow's code is one `run` call that needs nothing from the
/// engine: straight-line, without a site, loop or branch (which ask the
/// engine for frames) and without a constructor (which `run` stops at).
pub(crate) fn leaf_code(code: &[Instruction]) -> bool {
  code.iter().all(|i| {
    !matches!(
      i.op,
      Op::Fallback | Op::VmCall | Op::VmRepeat(_) | Op::VmBranch(_)
    ) && !i.is_constructor()
  })
}

/// What the engine lends a run for the call sites and child flows of the
/// frame being run. Copyable, so a nested run gets its own for the frame
/// it runs in.
pub(crate) trait VmCalls: Copy {
  /// The callee of the call site at instruction `site`, when it can run
  /// here: a stateless, non-recursive site with a kept frame, the current
  /// revision, a straight-line body whose own sites are ready too, and
  /// within the call depth. `None` sends the run back to the engine.
  fn enter(&self, site: usize) -> Option<VmCallee<Self>>;
  /// The same test without entering: readiness of the site and, through
  /// it, of everything its body would run.
  fn site_ready(&self, site: usize) -> bool;
  /// The frame of child flow `i` of the composite at `site`.
  fn child(&self, site: usize, i: usize) -> Option<Self>;
}

/// A callee a run enters: its code, its kept locals (a buffer that outlives
/// the run and never resizes, see the engine's `switch_scope` contract)
/// and how to bind the arguments.
pub(crate) struct VmCallee<C> {
  pub code: &'static [Instruction],
  /// Whether the code holds a constructor (then it runs through
  /// `run_segment`, otherwise through `run` directly).
  pub constructors: bool,
  pub locals: *mut [Var],
  pub param_slots: &'static [usize],
  /// The locals that are neither parameters nor the input: cleared at
  /// entry (the others are bound).
  pub scratch: &'static [usize],
  pub input_slot: usize,
  pub args: &'static [Operand],
  pub ignores_input: bool,
  pub calls: C,
}

/// No engine: calls are never ready, child flows have no frame. For code
/// that needs none (`leaf_code`), and tests.
#[derive(Clone, Copy)]
pub(crate) struct NoCalls;

impl VmCalls for NoCalls {
  fn enter(&self, _: usize) -> Option<VmCallee<Self>> {
    None
  }
  fn site_ready(&self, _: usize) -> bool {
    false
  }
  fn child(&self, _: usize, _: usize) -> Option<Self> {
    Some(NoCalls)
  }
}

/// Whether every call site and loop in `code` can run here now: checked
/// before a VM loop starts, so a body never runs halfway and then needs
/// the engine (a reload, the one thing that changes readiness, happens
/// between ticks).
pub(crate) fn vm_ready<C: VmCalls>(code: &[Instruction], calls: &C) -> bool {
  code
    .iter()
    .enumerate()
    .all(|(i, instruction)| match &instruction.op {
      Op::VmCall => calls.site_ready(i),
      Op::VmRepeat(l) => {
        calls
          .child(i, 0)
          .is_some_and(|c| vm_ready(&l.body.code, &c))
          && l
            .until
            .as_ref()
            .is_none_or(|u| calls.child(i, 1).is_some_and(|c| vm_ready(&u.code, &c)))
      }
      Op::VmBranch(b) => {
        calls
          .child(i, 0)
          .is_some_and(|c| vm_ready(&b.pred.code, &c))
          && calls
            .child(i, 1)
            .is_some_and(|c| vm_ready(&b.body.code, &c))
      }
      _ => true,
    })
}

/// Runs `code` from `from` as far as the VM goes: constructors through
/// `construct`, everything else through `run`, until the end of the code
/// or an instruction the VM cannot run (a `Fallback`, or a call site that
/// is not ready), which it stops before.
pub(crate) fn run_segment<C: VmCalls>(
  code: &[Instruction],
  from: usize,
  input: Var,
  locals: &mut [Var],
  mesh: &mut [Var],
  calls: &C,
) -> Result<(usize, Var)> {
  let (mut pc, mut value) = (from, input);
  loop {
    let before = pc;
    (pc, value) = if pc < code.len() && code[pc].is_constructor() {
      construct(code, pc, value, locals, mesh)?
    } else {
      run(code, pc, value, locals, mesh, calls)?
    };
    if pc == code.len() || pc == before {
      return Ok((pc, value));
    }
  }
}

/// Which ordering a comparison instruction tests for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cmp {
  Less,
  LessEqual,
  More,
  MoreEqual,
}

impl Cmp {
  fn holds(self, o: std::cmp::Ordering) -> bool {
    match self {
      Cmp::Less => o.is_lt(),
      Cmp::LessEqual => o.is_le(),
      Cmp::More => o.is_gt(),
      Cmp::MoreEqual => o.is_ge(),
    }
  }
}

impl Op {
  /// The instruction's kind, for tests and diagnostics.
  pub(crate) fn name(&self) -> &'static str {
    match self {
      Op::Fallback => "fallback",
      Op::Const(_) => "const",
      Op::GetLocal(_) => "get-local",
      Op::GetMesh(_) => "get-mesh",
      Op::Set(_) => "set",
      Op::Inc(_) => "inc",
      Op::ConstDrop(_) => "const-drop",
      Op::GetLocalDrop(_) => "get-local-drop",
      Op::GetMeshDrop(_) => "get-mesh-drop",
      Op::SetDrop(_) => "set-drop",
      Op::IncDrop(_) => "inc-drop",
      Op::Take(_) => "take",
      Op::TakeSlot(_) => "take-slot",
      Op::Push(_) => "push",
      Op::SeqMake(_) => "seq-make",
      Op::TableMake(..) => "table-make",
      Op::AddIntConst(_) => "add-int-const",
      Op::AddIntBound(_) => "add-int-bound",
      Op::AddFloatConst(_) => "add-float-const",
      Op::AddFloatBound(_) => "add-float-bound",
      Op::AddFloat4Const(_) => "add-float4-const",
      Op::AddFloat4Bound(_) => "add-float4-bound",
      Op::Arith(..) => "arith",
      Op::Compare(..) => "compare",
      Op::Equal(..) => "equal",
      Op::Pass => "pass",
      Op::VmCall => "vm-call",
      Op::VmRepeat(_) => "vm-repeat",
      Op::VmBranch(_) => "vm-branch",
    }
  }
}

/// Byte offset constructed only from a Var slot index. Its alignment is thus
/// guaranteed independently of the frame chosen by an instance.
#[derive(Clone, Copy)]
pub(crate) struct SlotOffset(usize);

impl SlotOffset {
  fn new(index: usize) -> Self {
    Self(
      index
        .checked_mul(size_of::<Var>())
        .expect("inline slot offset overflow"),
    )
  }
}

impl Op {
  fn get(binding: Binding) -> Self {
    match binding {
      Binding::Local(index) => Self::GetLocal(SlotOffset::new(index)),
      Binding::Mesh(index) => Self::GetMesh(SlotOffset::new(index)),
    }
  }
}

pub(crate) struct Instruction {
  pub op: Op,
  #[cfg(any(debug_assertions, feature = "output-checks"))]
  check: (&'static str, Type),
}

impl Instruction {
  pub fn is_constructor(&self) -> bool {
    matches!(self.op, Op::SeqMake(_) | Op::TableMake(..))
  }

  pub fn new(op: Option<InlineOp>, _name: &'static str, _output: Type) -> Self {
    Self {
      op: op.map_or(Op::Fallback, |v| v.0),
      #[cfg(any(debug_assertions, feature = "output-checks"))]
      check: (_name, _output),
    }
  }
}

/// Specialize cleanup at compose time. A segment enters with owned input;
/// Take/Push/generic vector arithmetic may introduce owned scratch again.
/// Once released, subsequent reanchoring instructions need no cleanup branch.
/// Keep the one-instruction-per-node mapping for lifecycle and diagnostics.
pub(crate) fn lower_scratch_releases(code: &mut [Instruction]) {
  let mut may_own = true;
  for instruction in code {
    match &mut instruction.op {
      Op::Const(_) | Op::GetLocal(_) | Op::GetMesh(_) | Op::Set(_) | Op::Inc(_) if may_own => {
        instruction.op = match std::mem::replace(&mut instruction.op, Op::Fallback) {
          Op::Const(v) => Op::ConstDrop(v),
          Op::GetLocal(offset) => Op::GetLocalDrop(offset),
          Op::GetMesh(offset) => Op::GetMeshDrop(offset),
          Op::Set(b) => Op::SetDrop(b),
          Op::Inc(b) => Op::IncDrop(b),
          _ => unreachable!(),
        };
        may_own = false;
      }
      Op::ConstDrop(_)
      | Op::GetLocalDrop(_)
      | Op::GetMeshDrop(_)
      | Op::SetDrop(_)
      | Op::IncDrop(_) => may_own = false,
      Op::Fallback
      | Op::SeqMake(_)
      | Op::TableMake(..)
      | Op::Take(_)
      | Op::TakeSlot(_)
      | Op::Push(_)
      | Op::VmCall
      | Op::VmRepeat(_)
      | Op::VmBranch(_)
      | Op::AddFloat4Const(_)
      | Op::AddFloat4Bound(_) => may_own = true,
      Op::Const(_)
      | Op::GetLocal(_)
      | Op::GetMesh(_)
      | Op::Set(_)
      | Op::Inc(_)
      | Op::AddIntConst(_)
      | Op::AddIntBound(_)
      | Op::AddFloatConst(_)
      | Op::AddFloatBound(_)
      // Results are numbers or booleans, written to the numeric slot.
      | Op::Arith(..)
      | Op::Compare(..)
      | Op::Equal(..)
      | Op::Pass => {}
    }
  }
}

/// Select by Rust implementation identity, never by a user-controlled name.
/// Only operations that need no activation state participate. Normal
/// lifecycle stays in nodes.
pub(crate) fn leaf<L: LeafShard>(c: &L::Compiled, output: Type) -> Option<InlineOp> {
  let id = TypeId::of::<L>();
  let c = c as &dyn Any;
  let op = if id == TypeId::of::<leaf::Const>() {
    Op::Const(c.downcast_ref::<Var>()?.clone())
  } else if id == TypeId::of::<leaf::Get>() {
    Op::get(*c.downcast_ref::<Binding>()?)
  } else if id == TypeId::of::<leaf::VarDecl>()
    || id == TypeId::of::<leaf::Bind>()
    || id == TypeId::of::<leaf::Update>()
  {
    Op::Set(*c.downcast_ref::<Binding>()?)
  } else if id == TypeId::of::<leaf::Inc>() {
    Op::Inc(*c.downcast_ref::<Binding>()?)
  } else if id == TypeId::of::<leaf::Keep>() {
    Op::Pass
  } else if id == TypeId::of::<data::Take>() {
    match c.downcast_ref::<data::TakeCode>()? {
      data::TakeCode::Key(key) => Op::Take(key.clone()),
      data::TakeCode::Slot(index) => Op::TakeSlot(*index),
    }
  } else if id == TypeId::of::<data::SeqMake>() {
    Op::SeqMake(c.downcast_ref::<Vec<Operand>>()?.clone())
  } else if id == TypeId::of::<data::TableMake>() {
    let code = c.downcast_ref::<data::TableCode>()?;
    Op::TableMake(code.shape, code.values.clone())
  } else if id == TypeId::of::<data::Push>() {
    Op::Push(*c.downcast_ref::<Binding>()?)
  } else if id == TypeId::of::<leaf::Add>() {
    match (output, c.downcast_ref::<Operand>()?) {
      (ty, Operand::Const(Var::Int(v))) if ty == Type::int() => Op::AddIntConst(*v),
      (ty, Operand::Bound(b)) if ty == Type::int() => Op::AddIntBound(*b),
      (ty, Operand::Const(Var::Float(v))) if ty == Type::float() => Op::AddFloatConst(*v),
      (ty, Operand::Bound(b)) if ty == Type::float() => Op::AddFloatBound(*b),
      (ty, Operand::Const(Var::Float4(v))) if ty == Type::float4() => Op::AddFloat4Const(*v),
      (ty, Operand::Bound(b)) if ty == Type::float4() => Op::AddFloat4Bound(*b),
      (_, operand) => Op::Arith(BinOp::Add, operand.clone()),
    }
  } else if id == TypeId::of::<math::Binary<math::SubOp>>() {
    Op::Arith(BinOp::Subtract, c.downcast_ref::<Operand>()?.clone())
  } else if id == TypeId::of::<math::Binary<math::MulOp>>() {
    Op::Arith(BinOp::Multiply, c.downcast_ref::<Operand>()?.clone())
  } else if id == TypeId::of::<math::Binary<math::DivOp>>() {
    Op::Arith(BinOp::Divide, c.downcast_ref::<Operand>()?.clone())
  } else if id == TypeId::of::<leaf::IsLess>() {
    Op::Compare(Cmp::Less, c.downcast_ref::<Operand>()?.clone())
  } else if id == TypeId::of::<leaf::IsMoreEqual>() {
    Op::Compare(Cmp::MoreEqual, c.downcast_ref::<Operand>()?.clone())
  } else if id == TypeId::of::<values::Ordered<values::IsMoreSpec>>() {
    Op::Compare(Cmp::More, c.downcast_ref::<Operand>()?.clone())
  } else if id == TypeId::of::<values::Ordered<values::IsLessEqualSpec>>() {
    Op::Compare(Cmp::LessEqual, c.downcast_ref::<Operand>()?.clone())
  } else if id == TypeId::of::<values::Equality<values::IsSpec>>() {
    Op::Equal(false, c.downcast_ref::<Operand>()?.clone())
  } else if id == TypeId::of::<values::Equality<values::IsNotSpec>>() {
    Op::Equal(true, c.downcast_ref::<Operand>()?.clone())
  } else {
    return None;
  };
  Some(InlineOp(op))
}

#[cold]
fn invalid() -> Error {
  Error::Activation("inline builtin: operand type mismatch".into())
}

#[cold]
fn overflow() -> Error {
  Error::Activation("Math.Add: integer overflow".into())
}

// Raw frame bases are derived once from the exclusive borrows. In particular,
// do not recreate a mutable slice reference while an accumulator points into
// it. Bounds checks precede every offset; the resulting pointer is only used
// within `run` while both original slices remain exclusively borrowed.
struct Frames {
  locals: *mut Var,
  locals_len: usize,
  locals_bytes: usize,
  mesh: *mut Var,
  mesh_len: usize,
  mesh_bytes: usize,
}

impl Frames {
  #[inline(always)]
  fn slot_offset<const MESH: bool>(&self, offset: SlotOffset) -> *mut Var {
    let (base, bytes) = if MESH {
      (self.mesh, self.mesh_bytes)
    } else {
      (self.locals, self.locals_bytes)
    };
    assert!(offset.0 < bytes, "inline binding outside its frame");
    // SAFETY: SlotOffset is an aligned whole-Var offset constructed with
    // checked multiplication. Frame byte lengths are multiples of Var size,
    // so offset < bytes also proves the entire Var fits. Bases come from the
    // original exclusive borrows; no new mutable reference is constructed.
    unsafe { base.cast::<u8>().add(offset.0).cast::<Var>() }
  }

  #[inline(always)]
  fn slot(&self, b: Binding) -> *mut Var {
    let (base, len, index) = match b {
      Binding::Local(i) => (self.locals, self.locals_len, i),
      Binding::Mesh(i) => (self.mesh, self.mesh_len, i),
    };
    assert!(index < len, "inline binding outside its frame");
    // SAFETY: checked against the length of the exclusively borrowed frame.
    unsafe { base.add(index) }
  }
}

/// Runs until a fallback node or the end, returning an owned snapshot. There
/// are no callbacks, suspension points, frame resizes or escaping references.
/// Code must be lowered and entry must be at a segment boundary (flow start or
/// immediately after a fallback/constructor), as enforced by both flow loops.
pub(crate) fn run<C: VmCalls>(
  code: &[Instruction],
  mut index: usize,
  input: Var,
  locals: &mut [Var],
  mesh: &mut [Var],
  calls: &C,
) -> Result<(usize, Var)> {
  let frames = Frames {
    locals: locals.as_mut_ptr(),
    locals_len: locals.len(),
    locals_bytes: std::mem::size_of_val(locals),
    mesh: mesh.as_mut_ptr(),
    mesh_len: mesh.len(),
    mesh_bytes: std::mem::size_of_val(mesh),
  };
  // Own the incoming output so replacing it releases captured values before
  // later frame mutations. Compose-selected Drop variants release it when
  // reanchoring to external storage; Take/Push/generic arithmetic replace it with their output.
  // Typed numeric arithmetic can leave only a resource-free numeric value in
  // scratch: its input must be numeric, and no obsolete owning value survives
  // the other reanchoring operations. This avoids a cleanup branch per Add.
  let mut scratch = input;
  // Numeric results own no heap storage. Overwrite this slot without running
  // Var's general drop dispatch on every arithmetic operation. Only explicit
  // Int/Float/Float4 constructors may be written here; generic results use
  // `scratch`. It is never read until initialized and no pointer escapes run.
  let mut numeric = std::mem::MaybeUninit::<Var>::uninit();
  let mut value: *const Var = &scratch;
  while let Some(instruction) = code.get(index) {
    // SAFETY: `value` starts at scratch and each arm reanchors it to live
    // code, scratch, numeric storage or a checked frame slot. Reads end before
    // any write. No reference survives replacement of its owner; no callback
    // can mutate it.
    unsafe {
      match &instruction.op {
        Op::Fallback | Op::SeqMake(_) | Op::TableMake(..) => break,
        Op::Const(v) => {
          value = v;
        }
        Op::GetLocal(offset) => value = frames.slot_offset::<false>(*offset),
        Op::GetMesh(offset) => value = frames.slot_offset::<true>(*offset),
        Op::Set(b) => {
          let target = frames.slot(*b);
          if !std::ptr::eq(value, target) {
            let copy = (*value).clone();
            *target = copy;
          }
          value = target;
        }
        Op::Inc(b) => {
          let target = frames.slot(*b);
          let Var::Int(n) = &mut *target else {
            return Err(Error::Activation("Inc: variable is not an Int".into()));
          };
          *n = n
            .checked_add(1)
            .ok_or_else(|| Error::Activation("Inc: integer overflow".into()))?;
          value = target;
        }
        Op::ConstDrop(v) => {
          value = v;
          scratch = Var::None;
        }
        Op::GetLocalDrop(offset) => {
          value = frames.slot_offset::<false>(*offset);
          scratch = Var::None;
        }
        Op::GetMeshDrop(offset) => {
          value = frames.slot_offset::<true>(*offset);
          scratch = Var::None;
        }
        Op::SetDrop(b) => {
          let target = frames.slot(*b);
          if !std::ptr::eq(value, target) {
            let copy = (*value).clone();
            *target = copy;
          }
          value = target;
          scratch = Var::None;
        }
        Op::IncDrop(b) => {
          let target = frames.slot(*b);
          let Var::Int(n) = &mut *target else {
            return Err(Error::Activation("Inc: variable is not an Int".into()));
          };
          *n = n
            .checked_add(1)
            .ok_or_else(|| Error::Activation("Inc: integer overflow".into()))?;
          value = target;
          scratch = Var::None;
        }
        Op::Take(key) => {
          let key = match key {
            Operand::Const(v) => v,
            Operand::Bound(b) => &*frames.slot(*b),
          };
          scratch = data::take_value(&*value, key)?;
          value = &scratch;
        }
        Op::TakeSlot(index) => {
          scratch = data::take_slot(&*value, *index)?;
          value = &scratch;
        }
        Op::Push(b) => {
          // The accumulator never points into a sequence's buffer (a Take
          // copies its element out), so only an input that is the
          // destination itself (a sequence pushing itself) needs a
          // snapshot before the slot is mutated.
          let target = frames.slot(*b);
          let pushed = if std::ptr::eq(value, target) {
            scratch = (*value).clone();
            value = &scratch;
            scratch.clone()
          } else {
            (*value).clone()
          };
          match &mut *target {
            Var::Seq(items) => std::sync::Arc::make_mut(items).push(pushed),
            _ => {
              return Err(Error::Activation(
                "Push: the variable is not a sequence".into(),
              ));
            }
          }
        }
        Op::AddIntConst(rhs) => {
          let Var::Int(lhs) = &*value else {
            return Err(invalid());
          };
          let result = lhs.checked_add(*rhs).ok_or_else(overflow)?;
          numeric.write(Var::Int(result));
          value = numeric.as_ptr();
        }
        Op::AddIntBound(b) => {
          let (Var::Int(lhs), Var::Int(rhs)) = (&*value, &*frames.slot(*b)) else {
            return Err(invalid());
          };
          let result = lhs.checked_add(*rhs).ok_or_else(overflow)?;
          numeric.write(Var::Int(result));
          value = numeric.as_ptr();
        }
        Op::AddFloatConst(rhs) => {
          let lhs = match &*value {
            Var::Float(v) => *v,
            Var::Int(v) => *v as f64,
            _ => return Err(invalid()),
          };
          numeric.write(Var::Float(lhs + rhs));
          value = numeric.as_ptr();
        }
        Op::AddFloatBound(b) => {
          let number = |v: &Var| match v {
            Var::Float(v) => Ok(*v),
            Var::Int(v) => Ok(*v as f64),
            _ => Err(invalid()),
          };
          let result = number(&*value)? + number(&*frames.slot(*b))?;
          numeric.write(Var::Float(result));
          value = numeric.as_ptr();
        }
        Op::AddFloat4Const(rhs) => {
          if let Var::Float4(lhs) = &*value {
            let result = std::array::from_fn(|i| lhs[i] + rhs[i]);
            numeric.write(Var::Float4(Float4(result)));
            value = numeric.as_ptr();
          } else {
            scratch = add_generic(&*value, &Var::Float4(*rhs))?;
            value = &scratch;
          }
        }
        Op::AddFloat4Bound(b) => {
          let rhs = &*frames.slot(*b);
          if let (Var::Float4(lhs), Var::Float4(rhs)) = (&*value, rhs) {
            let result = std::array::from_fn(|i| lhs[i] + rhs[i]);
            numeric.write(Var::Float4(Float4(result)));
            value = numeric.as_ptr();
          } else {
            scratch = add_generic(&*value, rhs)?;
            value = &scratch;
          }
        }
        Op::Pass => {}
        Op::VmCall => match vm_call_op(calls, index, value, &frames)? {
          Some(output) => {
            scratch = output;
            value = &scratch;
          }
          None => break,
        },
        Op::VmRepeat(l) => {
          // The loop's input is its output; the body may write the slot
          // the accumulator points at, so take a snapshot first.
          if !std::ptr::eq(value, &scratch) && !std::ptr::eq(value, numeric.as_ptr()) {
            scratch = (*value).clone();
            value = &scratch;
          }
          if !vm_repeat_op(l, calls, index, value, &frames)? {
            break;
          }
        }
        Op::VmBranch(b) => {
          // Pass-through like the loop: snapshot the input first.
          if !std::ptr::eq(value, &scratch) && !std::ptr::eq(value, numeric.as_ptr()) {
            scratch = (*value).clone();
            value = &scratch;
          }
          if !vm_branch_op(b, calls, index, value, &frames)? {
            break;
          }
        }
        Op::Arith(op, rhs) => {
          let rhs = match rhs {
            Operand::Const(v) => v,
            Operand::Bound(b) => &*frames.slot(*b),
          };
          numeric.write(math::arith(*op, arith_name(*op), &*value, rhs)?);
          value = numeric.as_ptr();
        }
        Op::Compare(cmp, rhs) => {
          let rhs = match rhs {
            Operand::Const(v) => v,
            Operand::Bound(b) => &*frames.slot(*b),
          };
          let ordering = crate::shards::compare(&*value, rhs.clone())?;
          numeric.write(Var::Bool(cmp.holds(ordering)));
          value = numeric.as_ptr();
        }
        Op::Equal(negate, rhs) => {
          let rhs = match rhs {
            Operand::Const(v) => v,
            Operand::Bound(b) => &*frames.slot(*b),
          };
          numeric.write(Var::Bool(values::values_equal(&*value, rhs) != *negate));
          value = numeric.as_ptr();
        }
      }
      #[cfg(any(debug_assertions, feature = "output-checks"))]
      leaf::check_output(instruction.check.0, instruction.check.1, &*value)?;
    }
    index += 1;
  }
  // SAFETY: the accumulator is still live by the invariant above. Cloning at
  // the boundary makes the result independent of all borrowed storage.
  let output = if std::ptr::eq(value, &scratch) {
    scratch
  } else {
    unsafe { (*value).clone() }
  };
  Ok((index, output))
}

/// The shard name an arithmetic error reports.
fn arith_name(op: BinOp) -> &'static str {
  match op {
    BinOp::Add => "Math.Add",
    BinOp::Subtract => "Math.Subtract",
    BinOp::Multiply => "Math.Multiply",
    BinOp::Divide => "Math.Divide",
  }
}

/// Out of line so `run`'s own frame stays small: a VM call's state would
/// otherwise be set up in every segment's prologue.
#[inline(never)]
unsafe fn vm_call_op<C: VmCalls>(
  calls: &C,
  index: usize,
  value: *const Var,
  frames: &Frames,
) -> Result<Option<Var>> {
  unsafe {
    let Some(callee) = calls.enter(index) else {
      return Ok(None);
    };
    // SAFETY: the callee's locals are a kept frame's buffer, distinct
    // from this frame's, alive and unmoved for the run (the engine's
    // contract for invocation locals); nothing else addresses it
    // while the callee runs.
    let callee_locals: &mut [Var] = &mut *callee.locals;
    let input = if callee.ignores_input {
      Var::None
    } else {
      (*value).clone()
    };
    for slot in callee.scratch {
      callee_locals[*slot] = Var::None;
    }
    for (slot, op) in callee.param_slots.iter().zip(callee.args) {
      callee_locals[*slot] = match op {
        Operand::Const(v) => v.clone(),
        Operand::Bound(b) => (*frames.slot(*b)).clone(),
      };
    }
    callee_locals[callee.input_slot] = input.clone();
    // SAFETY: the mesh frame through the same pointer this run uses.
    let mesh = std::slice::from_raw_parts_mut(frames.mesh, frames.mesh_len);
    let (_, output) = if callee.constructors {
      run_segment(callee.code, 0, input, callee_locals, mesh, &callee.calls)?
    } else {
      run(callee.code, 0, input, callee_locals, mesh, &callee.calls)?
    };
    Ok(Some(output))
  }
}

/// Out of line, like [`vm_call_op`]. `Ok(false)`: the loop cannot run here
/// (a call site in it is not ready), nothing was executed.
#[inline(never)]
unsafe fn vm_repeat_op<C: VmCalls>(
  l: &VmLoop,
  calls: &C,
  index: usize,
  value: *const Var,
  frames: &Frames,
) -> Result<bool> {
  unsafe {
    let (Some(body_calls), until_calls) = (
      calls.child(index, 0),
      l.until.as_ref().map(|_| calls.child(index, 1)),
    ) else {
      return Ok(false);
    };
    if !vm_ready(&l.body.code, &body_calls)
      || until_calls.as_ref().is_some_and(|u| {
        u.as_ref()
          .is_none_or(|c| !vm_ready(&l.until.as_ref().expect("until").code, c))
      })
    {
      return Ok(false);
    }
    let limit = match &l.times {
      None => None,
      Some(op) => match op {
        Operand::Const(Var::Int(t)) => Some((*t).max(0)),
        Operand::Bound(b) => match &*frames.slot(*b) {
          Var::Int(t) => Some((*t).max(0)),
          _ => return Err(Error::Activation("Repeat: times is not an Int".into())),
        },
        Operand::Const(_) => {
          return Err(Error::Activation("Repeat: times is not an Int".into()));
        }
      },
    };
    // SAFETY: the frames through the same pointers this run uses; the
    // body runs on the same locals, with this run's accumulator kept
    // in owned storage (above) and no instruction of this run live.
    let (locals, mesh) = (
      std::slice::from_raw_parts_mut(frames.locals, frames.locals_len),
      std::slice::from_raw_parts_mut(frames.mesh, frames.mesh_len),
    );
    let body = &l.body.code;
    let constructors = body.iter().any(Instruction::is_constructor);
    let mut count = 0;
    while !crate::shards::control::repeat_exhausted(limit, count) {
      if let (Some(until), Some(Some(c))) = (&l.until, &until_calls) {
        let (_, stop) = run_segment(&until.code, 0, (*value).clone(), locals, mesh, c)?;
        match stop {
          Var::Bool(true) => break,
          Var::Bool(false) => {}
          _ => return Err(Error::Activation("Repeat: until is not a Bool".into())),
        }
      }
      if constructors {
        run_segment(body, 0, (*value).clone(), locals, mesh, &body_calls)?;
      } else {
        run(body, 0, (*value).clone(), locals, mesh, &body_calls)?;
      }
      count += 1;
    }

    Ok(true)
  }
}

/// Out of line, like [`vm_repeat_op`]: `Ok(false)` when a call site in
/// the predicate or the action is not ready, nothing executed.
#[inline(never)]
unsafe fn vm_branch_op<C: VmCalls>(
  b: &VmBranch,
  calls: &C,
  index: usize,
  value: *const Var,
  frames: &Frames,
) -> Result<bool> {
  unsafe {
    let (Some(pred_calls), Some(body_calls)) = (calls.child(index, 0), calls.child(index, 1))
    else {
      return Ok(false);
    };
    if !vm_ready(&b.pred.code, &pred_calls) || !vm_ready(&b.body.code, &body_calls) {
      return Ok(false);
    }
    // SAFETY: as in `vm_repeat_op`.
    let (locals, mesh) = (
      std::slice::from_raw_parts_mut(frames.locals, frames.locals_len),
      std::slice::from_raw_parts_mut(frames.mesh, frames.mesh_len),
    );
    let pred = &b.pred.code;
    let body = &b.body.code;
    let pred_constructors = pred.iter().any(Instruction::is_constructor);
    let body_constructors = body.iter().any(Instruction::is_constructor);
    loop {
      let (_, taken) = if pred_constructors {
        run_segment(pred, 0, (*value).clone(), locals, mesh, &pred_calls)?
      } else {
        run(pred, 0, (*value).clone(), locals, mesh, &pred_calls)?
      };
      match taken {
        Var::Bool(true) => {}
        Var::Bool(false) => break,
        _ => {
          return Err(Error::Activation("predicate did not output a Bool".into()));
        }
      }
      if body_constructors {
        run_segment(body, 0, (*value).clone(), locals, mesh, &body_calls)?;
      } else {
        run(body, 0, (*value).clone(), locals, mesh, &body_calls)?;
      }
      if !b.looping {
        break;
      }
    }
    Ok(true)
  }
}

#[cold]
fn add_generic(a: &Var, b: &Var) -> Result<Var> {
  crate::shards::math::arith(crate::shards::math::BinOp::Add, "Math.Add", a, b)
}

/// Constructors consume their input. Reuse only its uniquely owned allocation,
/// never a value retained in shard state. All operands are read-only frame or
/// compiled values, so clearing unique input storage cannot change an operand.
/// The returned value owns the output; nothing stays behind after consumption.
pub(crate) fn construct(
  code: &[Instruction],
  mut index: usize,
  mut value: Var,
  locals: &[Var],
  mesh: &[Var],
) -> Result<(usize, Var)> {
  use std::sync::Arc;
  let read = |operand: &Operand| match operand {
    Operand::Const(v) => v.clone(),
    Operand::Bound(Binding::Local(i)) => locals[*i].clone(),
    Operand::Bound(Binding::Mesh(i)) => mesh[*i].clone(),
  };
  while let Some(instruction) = code.get(index) {
    match &instruction.op {
      Op::SeqMake(_) => {
        // Consume the old output. A uniquely owned sequence is rebuilt in
        // place, handle and buffer included; a shared one belongs to a
        // saved snapshot, so it is left alone and a new one is built.
        let mut output: Option<Arc<Vec<Var>>> = match std::mem::take(&mut value) {
          Var::Seq(items) => Some(items),
          _ => None,
        };
        while let Some(instruction) = code.get(index) {
          let Op::SeqMake(items) = &instruction.op else {
            break;
          };
          match output.as_mut().and_then(Arc::get_mut) {
            // Same length: overwrite the slots (one drop and one write
            // each, no length bookkeeping).
            Some(buffer) if buffer.len() == items.len() => {
              for (slot, operand) in buffer.iter_mut().zip(items) {
                *slot = read(operand);
              }
            }
            Some(buffer) => {
              buffer.clear();
              buffer.extend(items.iter().map(&read));
            }
            None => output = Some(Arc::new(items.iter().map(&read).collect())),
          }
          // Checking builds materialize each intermediate output; release
          // keeps the owned buffer until the constructor segment ends.
          #[cfg(any(debug_assertions, feature = "output-checks"))]
          leaf::check_output(
            instruction.check.0,
            instruction.check.1,
            &Var::Seq(output.clone().expect("built above")),
          )?;
          index += 1;
        }
        value = Var::Seq(output.expect("a constructor ran"));
      }
      Op::TableMake(..) => {
        // Consume the old output: a struct table of the same shape that
        // nobody else holds is overwritten in place; anything else is left
        // to its owners and a new table is built. There are no callbacks
        // or frame writes between constructors.
        let mut output = match std::mem::take(&mut value) {
          Var::Table(table) => Some(table),
          _ => None,
        };
        while let Some(instruction) = code.get(index) {
          let Op::TableMake(shape, operands) = &instruction.op else {
            break;
          };
          match output.as_mut().and_then(|t| t.unique_slots(*shape)) {
            Some(slots) => {
              for (slot, operand) in slots.iter_mut().zip(operands) {
                *slot = read(operand);
              }
            }
            None => output = Some(Table::with_shape(*shape, operands.iter().map(&read))),
          }
          #[cfg(any(debug_assertions, feature = "output-checks"))]
          leaf::check_output(
            instruction.check.0,
            instruction.check.1,
            &Var::Table(output.clone().expect("built above")),
          )?;
          index += 1;
        }
        value = Var::Table(output.expect("a constructor ran"));
      }
      _ => break,
    }
  }
  Ok((index, value))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn instructions(ops: impl IntoIterator<Item = Op>) -> Vec<Instruction> {
    let mut code: Vec<_> = ops
      .into_iter()
      .map(|op| Instruction::new(Some(InlineOp(op)), "test", Type::any()))
      .collect();
    lower_scratch_releases(&mut code);
    code
  }

  #[test]
  fn constructor_reuses_unique_sequence_storage_but_preserves_shared_input() {
    use std::sync::Arc;
    let mut buffer = Vec::with_capacity(8);
    buffer.push(Var::Int(1));
    let allocation = buffer.as_ptr();
    let code = instructions([
      Op::SeqMake(vec![Operand::Const(Var::Int(2))]),
      Op::SeqMake(vec![
        Operand::Const(Var::Int(3)),
        Operand::Const(Var::Int(4)),
      ]),
    ]);
    let (index, output) = construct(&code, 0, Var::Seq(Arc::new(buffer)), &[], &[]).unwrap();
    assert_eq!(index, 2);
    let Var::Seq(items) = output else {
      panic!("expected sequence")
    };
    assert_eq!(items.as_ptr(), allocation);
    assert_eq!(&**items, &[Var::Int(3), Var::Int(4)]);
    let snapshot = Var::Seq(items);
    let code = instructions([Op::SeqMake(vec![Operand::Const(Var::Int(5))])]);
    let (_, output) = construct(&code, 0, snapshot.clone(), &[], &[]).unwrap();
    assert_eq!(snapshot, Var::Seq(Arc::new(vec![Var::Int(3), Var::Int(4)])));
    assert_eq!(output, Var::Seq(Arc::new(vec![Var::Int(5)])));
  }

  #[test]
  fn obsolete_input_and_take_scratch_are_released_before_push() {
    use std::sync::Arc;
    for through_take in [false, true] {
      for replacement in [Op::Const(Var::Int(1)), Op::get(Binding::Local(1))] {
        let mut locals = vec![Var::Seq(Arc::new(vec![Var::Int(7)])), Var::Int(1)];
        let Var::Seq(items) = &locals[0] else {
          unreachable!()
        };
        let allocation = Arc::as_ptr(items);
        let captured = Var::Seq(Arc::new(vec![locals[0].clone()]));
        let (input, mut ops) = if through_take {
          (
            Var::Seq(Arc::new(vec![captured])),
            vec![Op::Take(Operand::Const(Var::Int(0)))],
          )
        } else {
          (captured, vec![])
        };
        ops.extend([replacement, Op::Push(Binding::Local(0))]);
        let (_, output) =
          run(&instructions(ops), 0, input, &mut locals, &mut [], &NoCalls).unwrap();
        assert_eq!(output, Var::Int(1));
        let Var::Seq(items) = &locals[0] else {
          unreachable!()
        };
        assert_eq!(Arc::as_ptr(items), allocation);
        assert_eq!(&**items, &[Var::Int(7), Var::Int(1)]);
      }
    }
  }

  #[test]
  fn specialized_get_checks_the_selected_frame() {
    // A longer other frame must not make an invalid offset pass its check.
    for binding in [Binding::Local(2), Binding::Mesh(3)] {
      let code = instructions([Op::get(binding)]);
      let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut locals = vec![
          Var::Int(1);
          if matches!(binding, Binding::Local(_)) {
            2
          } else {
            4
          }
        ];
        let mut mesh = vec![Var::Int(2); 3];
        run(&code, 0, Var::None, &mut locals, &mut mesh, &NoCalls)
      }));
      assert!(result.is_err(), "out-of-frame offset must be rejected");
    }
    let result = std::panic::catch_unwind(|| SlotOffset::new(usize::MAX));
    assert!(result.is_err(), "slot multiplication must not wrap");
  }

  #[test]
  fn owned_input_is_released_after_a_generic_boundary() {
    use std::sync::Arc;
    let mut locals = vec![Var::Seq(Arc::new(vec![Var::Int(7)]))];
    let Var::Seq(items) = &locals[0] else {
      unreachable!()
    };
    let allocation = Arc::as_ptr(items);
    let code = instructions([
      Op::Const(Var::None),
      Op::Fallback,
      Op::Const(Var::Int(1)),
      Op::Push(Binding::Local(0)),
    ]);
    let (next, _) = run(&code, 0, Var::None, &mut locals, &mut [], &NoCalls).unwrap();
    assert_eq!(next, 1);
    // The fallback can return a fresh owner even though the preceding segment
    // had already cleared scratch. Its successor must release that new owner.
    let input = Var::Seq(Arc::new(vec![locals[0].clone()]));
    let (_, output) = run(&code, next + 1, input, &mut locals, &mut [], &NoCalls).unwrap();
    assert_eq!(output, Var::Int(1));
    let Var::Seq(items) = &locals[0] else {
      unreachable!()
    };
    assert_eq!(Arc::as_ptr(items), allocation);
    assert_eq!(&**items, &[Var::Int(7), Var::Int(1)]);
  }

  #[test]
  fn owned_input_survives_passthrough_and_self_push() {
    use std::sync::Arc;
    let input = Var::Seq(Arc::new(vec![Var::Int(7)]));
    let code = instructions([Op::Fallback]);
    let (_, output) = run(&code, 0, input.clone(), &mut [], &mut [], &NoCalls).unwrap();
    assert_eq!(output, input);
    let mut locals = vec![input.clone()];
    let code = instructions([Op::Push(Binding::Local(0))]);
    let (_, output) = run(&code, 0, input.clone(), &mut locals, &mut [], &NoCalls).unwrap();
    assert_eq!(output, input);
    assert_eq!(locals[0], Var::Seq(Arc::new(vec![Var::Int(7), input])));
  }

  #[test]
  fn borrowed_slots_and_scratch_match_owned_execution() {
    // Differential execution exercises self-assignment, both frame kinds,
    // scratch reuse, and replacing reference-counted values with numbers.
    let initial = vec![Var::string("held"), Var::Int(3), Var::Int(4)];
    let mut locals = initial.clone();
    let mut mesh = initial.clone();
    let mut expected_locals = initial.clone();
    let mut expected_mesh = initial;
    let mut expected = Var::None;
    let mut seed = 42u64;
    for _ in 0..40 {
      let input = expected.clone();
      let mut code = Vec::new();
      for _ in 0..20 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let index = (seed >> 32) as usize % 3;
        let binding = if seed & 1 == 0 {
          Binding::Local(index)
        } else {
          Binding::Mesh(index)
        };
        let frame = if seed & 1 == 0 {
          &mut expected_locals
        } else {
          &mut expected_mesh
        };
        let op = match (seed >> 8) % 5 {
          0 => {
            expected = Var::Int(7);
            Op::Const(expected.clone())
          }
          1 => {
            expected = frame[index].clone();
            Op::get(binding)
          }
          2 => {
            frame[index] = expected.clone();
            Op::Set(binding)
          }
          3 if matches!(frame[index], Var::Int(_)) => {
            let Var::Int(n) = &mut frame[index] else {
              unreachable!()
            };
            *n += 1;
            expected = frame[index].clone();
            Op::Inc(binding)
          }
          _ if matches!(expected, Var::Int(_)) => {
            expected = add_generic(&expected, &Var::Int(2)).unwrap();
            Op::AddIntConst(2)
          }
          _ => {
            expected = Var::string("replacement");
            Op::Const(expected.clone())
          }
        };
        code.push(op);
      }
      let code = instructions(code);
      let (index, actual) = run(&code, 0, input, &mut locals, &mut mesh, &NoCalls).unwrap();
      assert_eq!(index, code.len());
      assert_eq!(actual, expected);
      assert_eq!(locals, expected_locals);
      assert_eq!(mesh, expected_mesh);
    }
  }

  #[test]
  fn snapshots_survive_frame_and_code_replacement() {
    let mut locals = vec![Var::string("original")];
    let code = instructions([
      Op::get(Binding::Local(0)),
      Op::Set(Binding::Local(0)),
      Op::Fallback,
    ]);
    let (index, output) = run(&code, 0, Var::None, &mut locals, &mut [], &NoCalls).unwrap();
    assert_eq!(index, 2);
    locals[0] = Var::string("changed");
    drop(code);
    assert_eq!(output, Var::string("original"));
  }

  #[test]
  fn take_then_replace_owner_and_push_self_preserve_snapshots() {
    use std::sync::Arc;
    let old = Var::Seq(Arc::new(vec![Var::string("element")]));
    let mut locals = vec![old.clone()];
    let code = instructions([
      Op::get(Binding::Local(0)),
      Op::Take(Operand::Const(Var::Int(0))),
      Op::Set(Binding::Local(0)),
    ]);
    let (_, output) = run(&code, 0, Var::None, &mut locals, &mut [], &NoCalls).unwrap();
    assert_eq!(output, Var::string("element"));
    assert_eq!(locals[0], output);
    locals[0] = old.clone();
    let code = instructions([
      Op::get(Binding::Local(0)),
      Op::Push(Binding::Local(0)),
      Op::Push(Binding::Local(0)),
    ]);
    let (_, output) = run(&code, 0, Var::None, &mut locals, &mut [], &NoCalls).unwrap();
    assert_eq!(output, old);
    assert_eq!(
      locals[0],
      Var::Seq(Arc::new(vec![Var::string("element"), old.clone(), old]))
    );
  }

  #[test]
  fn math_matches_generic_path_and_reports_overflow() {
    for (lhs, rhs, op) in [
      (Var::Int(2), Var::Int(3), Op::AddIntBound(Binding::Local(0))),
      (
        Var::Int(2),
        Var::Float(0.5),
        Op::AddFloatBound(Binding::Local(0)),
      ),
      (
        Var::Float4(Float4([1.0, -0.0, f32::MIN_POSITIVE, f32::MAX])),
        Var::Float4(Float4([2.0, -0.0, -f32::MIN_POSITIVE, f32::MAX])),
        Op::AddFloat4Bound(Binding::Local(0)),
      ),
      (
        Var::Int(2),
        Var::Float4(Float4([1.0; 4])),
        Op::AddFloat4Bound(Binding::Local(0)),
      ),
      (
        Var::Float4(Float4([1.0; 4])),
        Var::Int(2),
        Op::AddFloat4Bound(Binding::Local(0)),
      ),
    ] {
      let expected = add_generic(&lhs, &rhs).unwrap();
      let (_, actual) = run(&instructions([op]), 0, lhs, &mut [rhs], &mut [], &NoCalls).unwrap();
      assert_eq!(actual, expected);
    }
    let error = run(
      &instructions([Op::AddIntConst(1)]),
      0,
      Var::Int(i64::MAX),
      &mut [],
      &mut [],
      &NoCalls,
    )
    .unwrap_err();
    assert_eq!(error.to_string(), overflow().to_string());
  }

  #[test]
  fn a_vm_branch_runs_its_action_on_a_true_predicate_and_loops_for_while() {
    use crate::flow::CompiledFlow;
    use std::sync::Arc;
    let flow = |ops: Vec<Op>| {
      Arc::new(CompiledFlow {
        nodes: Vec::new(),
        code: instructions(ops),
        output: Type::any(),
        analysis: Default::default(),
        leaf: false,
      })
    };
    // Predicate: local 0 < 3; action: Inc(local 0). The branch's output is
    // its input, snapshotted from the slot the action writes.
    let pred = flow(vec![
      Op::get(Binding::Local(0)),
      Op::Compare(Cmp::Less, Operand::Const(Var::Int(3))),
    ]);
    let body = flow(vec![Op::Inc(Binding::Local(0))]);
    for (looping, expected) in [(false, 1), (true, 3)] {
      let mut locals = vec![Var::Int(0)];
      let code = instructions([
        Op::get(Binding::Local(0)),
        Op::VmBranch(Box::new(VmBranch {
          pred: pred.clone(),
          body: body.clone(),
          looping,
        })),
      ]);
      let (pc, out) = run(&code, 0, Var::None, &mut locals, &mut [], &NoCalls).unwrap();
      assert_eq!((pc, out), (2, Var::Int(0)), "the input before the branch");
      assert_eq!(locals[0], Var::Int(expected));
    }
    // A false predicate skips the action.
    let mut locals = vec![Var::Int(5)];
    let code = instructions([
      Op::Const(Var::Int(9)),
      Op::VmBranch(Box::new(VmBranch {
        pred: pred.clone(),
        body: body.clone(),
        looping: false,
      })),
    ]);
    let (pc, out) = run(&code, 0, Var::None, &mut locals, &mut [], &NoCalls).unwrap();
    assert_eq!((pc, out), (2, Var::Int(9)));
    assert_eq!(locals[0], Var::Int(5));
    // A predicate without a Bool output is an error; a call site that is
    // not ready stops the run before the branch.
    let code = instructions([
      Op::Const(Var::Int(9)),
      Op::VmBranch(Box::new(VmBranch {
        pred: flow(vec![Op::Const(Var::Int(1))]),
        body: body.clone(),
        looping: false,
      })),
    ]);
    let err = run(&code, 0, Var::None, &mut locals, &mut [], &NoCalls).unwrap_err();
    assert!(err.to_string().contains("Bool"), "{err}");
    let code = instructions([
      Op::Const(Var::Int(9)),
      Op::VmBranch(Box::new(VmBranch {
        pred: pred.clone(),
        body: flow(vec![Op::VmCall]),
        looping: false,
      })),
    ]);
    let (pc, out) = run(&code, 0, Var::None, &mut locals, &mut [], &NoCalls).unwrap();
    assert_eq!((pc, out), (1, Var::Int(9)));
  }

  #[test]
  fn a_vm_loop_runs_its_body_the_given_times_and_keeps_its_input() {
    use crate::flow::CompiledFlow;
    use std::sync::Arc;
    // Body: Inc(local 0), four times, from a literal and from a bound count;
    // the loop's output is its input, snapshotted from the slot it points at.
    let body = Arc::new(CompiledFlow {
      nodes: Vec::new(),
      code: instructions([Op::Inc(Binding::Local(0))]),
      output: Type::any(),
      analysis: Default::default(),
      leaf: false,
    });
    for times in [
      Operand::Const(Var::Int(4)),
      Operand::Bound(Binding::Local(1)),
    ] {
      let mut locals = vec![Var::Int(0), Var::Int(4)];
      let code = instructions([
        Op::get(Binding::Local(0)),
        Op::VmRepeat(Box::new(VmLoop {
          body: body.clone(),
          times: Some(times),
          until: None,
        })),
      ]);
      let (pc, out) = run(&code, 0, Var::None, &mut locals, &mut [], &NoCalls).unwrap();
      assert_eq!((pc, out), (2, Var::Int(0)), "the input before the loop");
      assert_eq!(locals[0], Var::Int(4));
    }
    // A body with a call site that is not ready stops the run before the loop.
    let calling = Arc::new(CompiledFlow {
      nodes: Vec::new(),
      code: instructions([Op::VmCall]),
      output: Type::any(),
      analysis: Default::default(),
      leaf: false,
    });
    let code = instructions([
      Op::Const(Var::Int(1)),
      Op::VmRepeat(Box::new(VmLoop {
        body: calling,
        times: None,
        until: None,
      })),
    ]);
    let (pc, out) = run(&code, 0, Var::None, &mut [], &mut [], &NoCalls).unwrap();
    assert_eq!((pc, out), (1, Var::Int(1)));
  }
}
