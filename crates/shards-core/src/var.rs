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

impl Var {
  /// Human text, as 1.x prints values (`Log`, `ToString`, `String.Format`):
  /// strings as they are (also inside sequences and tables), whole floats
  /// without `.0` (`3`, `[1 2.5]`), very large or small ones in exponent
  /// form. Not round-trippable: `Display` gives source syntax.
  pub fn text(&self) -> String {
    let mut out = String::new();
    write_text(self, &mut out);
    out
  }
}

fn float_text(f: f64, out: &mut String) {
  use std::fmt::Write;
  let _ = if f.is_finite() && f.fract() == 0.0 && f.abs() < 1e16 {
    write!(out, "{f:.0}")
  } else if f != 0.0 && (f.abs() >= 1e16 || f.abs() < 1e-5) {
    write!(out, "{f:e}")
  } else {
    write!(out, "{f}")
  };
}

fn write_text(v: &Var, out: &mut String) {
  match v {
    Var::None => out.push_str("none"),
    Var::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
    Var::Int(i) => out.push_str(&i.to_string()),
    Var::Float(f) => float_text(*f, out),
    Var::Float2(c) => vector(out, "@f2(", c.iter().copied()),
    Var::Float3(c) => vector(out, "@f3(", c.iter().map(|x| shortest(*x))),
    Var::Float4(c) => vector(out, "@f4(", c.iter().map(|x| shortest(*x))),
    Var::String(s) => out.push_str(s),
    Var::Seq(items) => {
      out.push('[');
      for (i, item) in items.iter().enumerate() {
        if i > 0 {
          out.push(' ');
        }
        write_text(item, out);
      }
      out.push(']');
    }
    Var::Table(entries) => {
      out.push('{');
      for (i, (k, item)) in entries.iter().enumerate() {
        if i > 0 {
          out.push(' ');
        }
        out.push_str(k);
        out.push_str(": ");
        write_text(item, out);
      }
      out.push('}');
    }
  }
}

fn vector(out: &mut String, open: &str, components: impl Iterator<Item = f64>) {
  out.push_str(open);
  for (i, x) in components.enumerate() {
    if i > 0 {
      out.push(' ');
    }
    float_text(x, out);
  }
  out.push(')');
}

/// An f32 as the f64 its shortest decimal form denotes, so 0.1f32 prints
/// `0.1`, not `0.10000000149011612`.
fn shortest(f: f32) -> f64 {
  f.to_string().parse().unwrap_or(f64::from(f))
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn text_prints_values_as_1x_does() {
    let seq = |v: Vec<Var>| Var::Seq(Arc::new(v));
    // Checked against the 1.x Release binary's Log output.
    assert_eq!(Var::Float(3.0).text(), "3");
    assert_eq!(Var::Float(0.1).text(), "0.1");
    assert_eq!(Var::Float(-0.0).text(), "-0");
    assert_eq!(Var::Float(1e20).text(), "1e20");
    assert_eq!(
      seq(vec![Var::Float(1.0), Var::Float(2.5)]).text(),
      "[1 2.5]"
    );
    assert_eq!(
      seq(vec![Var::string("a"), Var::string("b c")]).text(),
      "[a b c]"
    );
    assert_eq!(
      Var::table([("k", Var::string("v")), ("n", Var::Float(2.0))]).text(),
      "{k: v n: 2}"
    );
    assert_eq!(Var::Float3([1.0, 2.5, 0.1]).text(), "@f3(1 2.5 0.1)");
    // Display stays source syntax, round-trippable.
    assert_eq!(Var::Float(3.0).to_string(), "3.0");
    assert_eq!(Var::string("a").to_string(), "\"a\"");
  }
}
