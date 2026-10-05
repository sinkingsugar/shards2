//! Compose-selected builtins and a borrowed accumulator for uninterrupted runs.
//!
//! The pointer accumulator never escapes `run`. It refers to the call's input,
//! an immutable instruction constant, a frame slot, or a local scratch value.
//! Frames are exclusively borrowed and cannot resize or suspend during a run.
//! Each operation consumes its input before replacing storage and reanchors the
//! accumulator after a write. A run ends before any arbitrary shard activation,
//! cloning its output so nested flows, host writes and suspension see a snapshot.

use std::any::{Any, TypeId};

use crate::compose::Binding;
use crate::error::{Error, Result};
use crate::shards::Operand;
use crate::shards::data;
use crate::shards::leaf::{self, LeafShard};
use crate::types::Type;
use crate::var::Var;

/// Opaque internal optimization hook. Ordinary shards use the default `None`.
#[doc(hidden)]
pub struct InlineOp(pub(crate) Op);

pub(crate) enum Op {
  Fallback,
  Const(Var),
  Get(Binding),
  Set(Binding),
  Inc(Binding),
  Take(Operand),
  Push(Binding),
  SeqMake(Vec<Operand>),
  TableMake(Vec<(std::sync::Arc<str>, Operand)>, Type),
  AddIntConst(i64),
  AddIntBound(Binding),
  AddFloatConst(f64),
  AddFloatBound(Binding),
  AddFloat4Const([f32; 4]),
  AddFloat4Bound(Binding),
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

/// Select by Rust implementation identity, never by a user-controlled name.
/// Only operations that need no activation state participate (a non-clearing
/// Push never touches its iteration state). Normal lifecycle stays in nodes.
pub(crate) fn leaf<L: LeafShard>(c: &L::Compiled, output: Type) -> Option<InlineOp> {
  let id = TypeId::of::<L>();
  let c = c as &dyn Any;
  let op = if id == TypeId::of::<leaf::Const>() {
    Op::Const(c.downcast_ref::<Var>()?.clone())
  } else if id == TypeId::of::<leaf::Get>() {
    Op::Get(*c.downcast_ref::<Binding>()?)
  } else if id == TypeId::of::<leaf::Set>()
    || id == TypeId::of::<leaf::Ref>()
    || id == TypeId::of::<leaf::Update>()
  {
    Op::Set(*c.downcast_ref::<Binding>()?)
  } else if id == TypeId::of::<leaf::Inc>() {
    Op::Inc(*c.downcast_ref::<Binding>()?)
  } else if id == TypeId::of::<data::Take>() {
    Op::Take(c.downcast_ref::<Operand>()?.clone())
  } else if id == TypeId::of::<data::SeqMake>() {
    Op::SeqMake(c.downcast_ref::<Vec<Operand>>()?.clone())
  } else if id == TypeId::of::<data::TableMake>() {
    let mut entries = c
      .downcast_ref::<Vec<(std::sync::Arc<str>, Operand)>>()?
      .clone();
    entries.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    Op::TableMake(entries, output)
  } else if id == TypeId::of::<data::Push>() {
    let c = c.downcast_ref::<data::PushCompiled>()?;
    if c.clear {
      return None;
    }
    Op::Push(c.binding)
  } else if id == TypeId::of::<leaf::Add>() {
    match (output, c.downcast_ref::<Operand>()?) {
      (ty, Operand::Const(Var::Int(v))) if ty == Type::int() => Op::AddIntConst(*v),
      (ty, Operand::Bound(b)) if ty == Type::int() => Op::AddIntBound(*b),
      (ty, Operand::Const(Var::Float(v))) if ty == Type::float() => Op::AddFloatConst(*v),
      (ty, Operand::Bound(b)) if ty == Type::float() => Op::AddFloatBound(*b),
      (ty, Operand::Const(Var::Float4(v))) if ty == Type::float4() => Op::AddFloat4Const(*v),
      (ty, Operand::Bound(b)) if ty == Type::float4() => Op::AddFloat4Bound(*b),
      _ => return None,
    }
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
  mesh: *mut Var,
  mesh_len: usize,
}

impl Frames {
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
pub(crate) fn run(
  code: &[Instruction],
  mut index: usize,
  input: &Var,
  locals: &mut [Var],
  mesh: &mut [Var],
) -> Result<(usize, Var)> {
  let frames = Frames {
    locals: locals.as_mut_ptr(),
    locals_len: locals.len(),
    mesh: mesh.as_mut_ptr(),
    mesh_len: mesh.len(),
  };
  // Initial None is overwritten before a pointer is taken; subsequent values
  // are read through `value` and must still be dropped when replaced.
  #[allow(unused_assignments)]
  let mut scratch = Var::None;
  // Numeric results own no heap storage. Overwrite this slot without running
  // Var's general drop dispatch on every arithmetic operation. Only explicit
  // Int/Float/Float4 constructors may be written here; generic results use
  // `scratch`. It is never read until initialized and no pointer escapes run.
  let mut numeric = std::mem::MaybeUninit::<Var>::uninit();
  let mut value: *const Var = input;
  while let Some(instruction) = code.get(index) {
    // SAFETY: `value` starts at input and each arm reanchors it to live input,
    // code, scratch or a checked frame slot. Reads end before any write. No
    // reference survives replacement of its owner; no callback can mutate it.
    unsafe {
      match &instruction.op {
        Op::Fallback | Op::SeqMake(_) | Op::TableMake(..) => break,
        Op::Const(v) => value = v,
        Op::Get(b) => value = frames.slot(*b),
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
        Op::Take(key) => {
          let key = match key {
            Operand::Const(v) => v,
            Operand::Bound(b) => &*frames.slot(*b),
          };
          scratch = data::take_value(&*value, key)?;
          value = &scratch;
        }
        Op::Push(b) => {
          // Snapshot before mutating the slot: the input may alias the
          // destination (including a sequence pushing itself).
          scratch = (*value).clone();
          value = &scratch;
          match &mut *frames.slot(*b) {
            Var::Seq(items) => std::sync::Arc::make_mut(items).push(scratch.clone()),
            slot @ Var::None => *slot = Var::Seq(std::sync::Arc::new(vec![scratch.clone()])),
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
            numeric.write(Var::Float4(result));
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
            numeric.write(Var::Float4(result));
            value = numeric.as_ptr();
          } else {
            scratch = add_generic(&*value, rhs)?;
            value = &scratch;
          }
        }
      }
      #[cfg(any(debug_assertions, feature = "output-checks"))]
      leaf::check_output(instruction.check.0, instruction.check.1, &*value)?;
    }
    index += 1;
  }
  // SAFETY: the accumulator is still live by the invariant above. Cloning at
  // the boundary makes the result independent of all borrowed storage.
  Ok((index, unsafe { (*value).clone() }))
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
        let mut output = match std::mem::take(&mut value) {
          Var::Table(entries) => Arc::try_unwrap(entries).unwrap_or_default(),
          _ => std::collections::BTreeMap::new(),
        };
        let mut table_shape = None;
        while let Some(instruction) = code.get(index) {
          let Op::TableMake(entries, shape) = &instruction.op else {
            break;
          };
          if table_shape == Some(*shape) || output.keys().eq(entries.iter().map(|(key, _)| key)) {
            // Compiled entries and BTreeMap use sorted keys. A matching
            // fixed output type proves equal key sets within this segment;
            // there are no callbacks or frame writes between constructors.
            for (slot, (_, operand)) in output.values_mut().zip(entries) {
              *slot = read(operand);
            }
          } else {
            output.clear();
            output.extend(
              entries
                .iter()
                .map(|(key, operand)| (key.clone(), read(operand))),
            );
          }
          table_shape = Some(*shape);
          #[cfg(any(debug_assertions, feature = "output-checks"))]
          leaf::check_output(
            instruction.check.0,
            instruction.check.1,
            &Var::Table(Arc::new(output.clone())),
          )?;
          index += 1;
        }
        value = Var::Table(Arc::new(output));
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
    ops
      .into_iter()
      .map(|op| Instruction::new(Some(InlineOp(op)), "test", Type::any()))
      .collect()
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
            Op::Get(binding)
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
      let (index, actual) = run(&code, 0, &input, &mut locals, &mut mesh).unwrap();
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
      Op::Get(Binding::Local(0)),
      Op::Set(Binding::Local(0)),
      Op::Fallback,
    ]);
    let (index, output) = run(&code, 0, &Var::None, &mut locals, &mut []).unwrap();
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
      Op::Get(Binding::Local(0)),
      Op::Take(Operand::Const(Var::Int(0))),
      Op::Set(Binding::Local(0)),
    ]);
    let (_, output) = run(&code, 0, &Var::None, &mut locals, &mut []).unwrap();
    assert_eq!(output, Var::string("element"));
    assert_eq!(locals[0], output);
    locals[0] = old.clone();
    let code = instructions([
      Op::Get(Binding::Local(0)),
      Op::Push(Binding::Local(0)),
      Op::Push(Binding::Local(0)),
    ]);
    let (_, output) = run(&code, 0, &Var::None, &mut locals, &mut []).unwrap();
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
        Var::Float4([1.0, -0.0, f32::MIN_POSITIVE, f32::MAX]),
        Var::Float4([2.0, -0.0, -f32::MIN_POSITIVE, f32::MAX]),
        Op::AddFloat4Bound(Binding::Local(0)),
      ),
      (
        Var::Int(2),
        Var::Float4([1.0; 4]),
        Op::AddFloat4Bound(Binding::Local(0)),
      ),
      (
        Var::Float4([1.0; 4]),
        Var::Int(2),
        Op::AddFloat4Bound(Binding::Local(0)),
      ),
    ] {
      let expected = add_generic(&lhs, &rhs).unwrap();
      let (_, actual) = run(&instructions([op]), 0, &lhs, &mut [rhs], &mut []).unwrap();
      assert_eq!(actual, expected);
    }
    let error = run(
      &instructions([Op::AddIntConst(1)]),
      0,
      &Var::Int(i64::MAX),
      &mut [],
      &mut [],
    )
    .unwrap_err();
    assert_eq!(error.to_string(), overflow().to_string());
  }
}
