//! Values (contract §9, docs/values-and-types.md §2). Prototype only: a
//! minimal enum, measured against 1.x's 32-byte `SHVar` before any layout is
//! committed to.

use std::collections::BTreeMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::types::Type;

#[derive(Clone, Debug, Default)]
pub enum Var {
  #[default]
  None,
  Bool(bool),
  Int(i64),
  Float(f64),
  /// Two 64-bit floats, as in 1.x.
  Float2([f64; 2]),
  Float3([f32; 3]),
  Float4([f32; 4]),
  String(Arc<str>),
  Seq(Arc<Vec<Var>>),
  /// String keys in sorted order (docs/values-and-types.md §2).
  Table(Arc<BTreeMap<Arc<str>, Var>>),
}

impl Var {
  pub fn string(s: &str) -> Var {
    Var::String(Arc::from(s))
  }

  /// A table from key/value pairs; a repeated key keeps the last value.
  pub fn table<K: Into<Arc<str>>>(entries: impl IntoIterator<Item = (K, Var)>) -> Var {
    Var::Table(Arc::new(
      entries.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    ))
  }

  /// The value's type. A table is the fixed table of its keys; a sequence
  /// of mixed element types is a sequence of their union.
  pub fn type_of(&self) -> Type {
    match self {
      Var::None => Type::none(),
      Var::Bool(_) => Type::bool(),
      Var::Int(_) => Type::int(),
      Var::Float(_) => Type::float(),
      Var::Float2(_) => Type::float2(),
      Var::Float3(_) => Type::float3(),
      Var::Float4(_) => Type::float4(),
      Var::String(_) => Type::string(),
      Var::Seq(items) => {
        let mut members: Vec<Type> = Vec::new();
        for item in items.iter() {
          let ty = item.type_of();
          if !members.contains(&ty) {
            members.push(ty);
          }
        }
        if members.is_empty() {
          Type::seq(Type::any())
        } else {
          Type::seq(Type::union(members))
        }
      }
      Var::Table(entries) => {
        Type::fixed_table(entries.iter().map(|(k, v)| (k.clone(), v.type_of())))
      }
    }
  }
}

/// Source syntax: `none`, `true`, `42`, `1.5`, `"text"`, `[1 2]`,
/// `{key: value}`, `@f3(1.0 2.0 3.0)`.
impl fmt::Display for Var {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Var::None => write!(f, "none"),
      Var::Bool(b) => write!(f, "{b}"),
      Var::Int(i) => write!(f, "{i}"),
      Var::Float(x) => write!(f, "{x:?}"),
      Var::Float2(v) => write!(f, "@f2({:?} {:?})", v[0], v[1]),
      Var::Float3(v) => write!(f, "@f3({:?} {:?} {:?})", v[0], v[1], v[2]),
      Var::Float4(v) => write!(f, "@f4({:?} {:?} {:?} {:?})", v[0], v[1], v[2], v[3]),
      Var::String(s) => write!(f, "{s:?}"),
      Var::Seq(items) => {
        write!(f, "[")?;
        for (i, item) in items.iter().enumerate() {
          if i > 0 {
            write!(f, " ")?;
          }
          write!(f, "{item}")?;
        }
        write!(f, "]")
      }
      Var::Table(entries) => {
        write!(f, "{{")?;
        for (i, (k, v)) in entries.iter().enumerate() {
          if i > 0 {
            write!(f, " ")?;
          }
          write!(f, "{}: {v}", crate::types::key_text(k))?;
        }
        write!(f, "}}")
      }
    }
  }
}

/// Identity equality: floats compare by bit pattern, so `Var` can be part of
/// cache keys (parameter values). Not the language's `Is` semantics.
impl PartialEq for Var {
  fn eq(&self, other: &Var) -> bool {
    match (self, other) {
      (Var::None, Var::None) => true,
      (Var::Bool(a), Var::Bool(b)) => a == b,
      (Var::Int(a), Var::Int(b)) => a == b,
      (Var::Float(a), Var::Float(b)) => a.to_bits() == b.to_bits(),
      (Var::Float2(a), Var::Float2(b)) => a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits()),
      (Var::Float3(a), Var::Float3(b)) => a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits()),
      (Var::Float4(a), Var::Float4(b)) => a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits()),
      (Var::String(a), Var::String(b)) => a == b,
      (Var::Seq(a), Var::Seq(b)) => a == b,
      (Var::Table(a), Var::Table(b)) => a == b,
      _ => false,
    }
  }
}

impl Eq for Var {}

impl Hash for Var {
  fn hash<H: Hasher>(&self, state: &mut H) {
    std::mem::discriminant(self).hash(state);
    match self {
      Var::None => {}
      Var::Bool(v) => v.hash(state),
      Var::Int(v) => v.hash(state),
      Var::Float(v) => v.to_bits().hash(state),
      Var::Float2(v) => v.iter().for_each(|x| x.to_bits().hash(state)),
      Var::Float3(v) => v.iter().for_each(|x| x.to_bits().hash(state)),
      Var::Float4(v) => v.iter().for_each(|x| x.to_bits().hash(state)),
      Var::String(v) => v.hash(state),
      Var::Seq(v) => v.hash(state),
      Var::Table(v) => v.hash(state),
    }
  }
}
