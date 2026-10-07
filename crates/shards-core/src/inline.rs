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
pub(crate) fn run(
  code: &[Instruction],
  mut index: usize,
  input: Var,
  locals: &mut [Var],
  mesh: &mut [Var],
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
          // Snapshot before mutating the slot: the input may alias the
          // destination (including a sequence pushing itself).
          scratch = (*value).clone();
          value = &scratch;
          match &mut *frames.slot(*b) {
            Var::Seq(items) => std::sync::Arc::make_mut(items).push(scratch.clone()),
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
        // Consume the old output. A shared allocation belongs to a saved
        // snapshot, so leave it alone and start empty; never copy its items.
        let mut output = match std::mem::take(&mut value) {
          Var::Seq(items) => Arc::try_unwrap(items).unwrap_or_default(),
          _ => Vec::new(),
        };
        while let Some(instruction) = code.get(index) {
          let Op::SeqMake(items) = &instruction.op else {
            break;
          };
          output.clear();
          output.extend(items.iter().map(&read));
          // Checking builds materialize each intermediate output; release
          // keeps the owned buffer until the constructor segment ends.
          #[cfg(any(debug_assertions, feature = "output-checks"))]
          leaf::check_output(
            instruction.check.0,
            instruction.check.1,
            &Var::Seq(Arc::new(output.clone())),
          )?;
          index += 1;
        }
        value = Var::Seq(Arc::new(output));
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
        let (_, output) = run(&instructions(ops), 0, input, &mut locals, &mut []).unwrap();
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
        run(&code, 0, Var::None, &mut locals, &mut mesh)
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
    let (next, _) = run(&code, 0, Var::None, &mut locals, &mut []).unwrap();
    assert_eq!(next, 1);
    // The fallback can return a fresh owner even though the preceding segment
    // had already cleared scratch. Its successor must release that new owner.
    let input = Var::Seq(Arc::new(vec![locals[0].clone()]));
    let (_, output) = run(&code, next + 1, input, &mut locals, &mut []).unwrap();
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
    let (_, output) = run(&code, 0, input.clone(), &mut [], &mut []).unwrap();
    assert_eq!(output, input);
    let mut locals = vec![input.clone()];
    let code = instructions([Op::Push(Binding::Local(0))]);
    let (_, output) = run(&code, 0, input.clone(), &mut locals, &mut []).unwrap();
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
      let (index, actual) = run(&code, 0, input, &mut locals, &mut mesh).unwrap();
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
    let (index, output) = run(&code, 0, Var::None, &mut locals, &mut []).unwrap();
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
    let (_, output) = run(&code, 0, Var::None, &mut locals, &mut []).unwrap();
    assert_eq!(output, Var::string("element"));
    assert_eq!(locals[0], output);
    locals[0] = old.clone();
    let code = instructions([
      Op::get(Binding::Local(0)),
      Op::Push(Binding::Local(0)),
      Op::Push(Binding::Local(0)),
    ]);
    let (_, output) = run(&code, 0, Var::None, &mut locals, &mut []).unwrap();
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
      let (_, actual) = run(&instructions([op]), 0, lhs, &mut [rhs], &mut []).unwrap();
      assert_eq!(actual, expected);
    }
    let error = run(
      &instructions([Op::AddIntConst(1)]),
      0,
      Var::Int(i64::MAX),
      &mut [],
      &mut [],
    )
    .unwrap_err();
    assert_eq!(error.to_string(), overflow().to_string());
  }
}
