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
  Table(Table),
}

impl Var {
  pub fn string(s: &str) -> Var {
    Var::String(Arc::from(s))
  }

  /// A table from key/value pairs; a repeated key keeps the last value.
  pub fn table<K: Into<Arc<str>>>(entries: impl IntoIterator<Item = (K, Var)>) -> Var {
    Var::Table(entries.into_iter().collect())
  }

  /// The table, if this is one.
  pub fn as_table(&self) -> Option<&Table> {
    match self {
      Var::Table(t) => Some(t),
      _ => None,
    }
  }

  /// The elements, if this is a sequence.
  pub fn as_seq(&self) -> Option<&[Var]> {
    match self {
      Var::Seq(items) => Some(items),
      _ => None,
    }
  }

  /// The text, if this is a string.
  pub fn as_str(&self) -> Option<&str> {
    match self {
      Var::String(s) => Some(s),
      _ => None,
    }
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
        Type::fixed_table(entries.map().iter().map(|(k, v)| (k.clone(), v.type_of())))
      }
    }
  }
}

/// A table: string keys in sorted order, values shared copy-on-write
/// (docs/values-and-types.md §2). Cloning is cheap. The storage is private
/// so it can change (golden-path.md §7.3) without changing this API: build
/// one with [`TableBuilder`] or `collect`, read it with [`get`](Table::get)
/// and [`iter`](Table::iter).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Table(Arc<BTreeMap<Arc<str>, Var>>);

impl Table {
  pub fn new() -> Table {
    Table::default()
  }

  pub fn builder() -> TableBuilder {
    TableBuilder::default()
  }

  /// A builder holding this table's entries, to derive a changed table.
  /// Takes the storage when no other table shares it; otherwise copies
  /// the entries, sharing their keys and values.
  pub fn into_builder(self) -> TableBuilder {
    TableBuilder(Arc::try_unwrap(self.0).unwrap_or_else(|shared| (*shared).clone()))
  }

  pub fn len(&self) -> usize {
    self.0.len()
  }

  pub fn is_empty(&self) -> bool {
    self.0.is_empty()
  }

  pub fn get(&self, key: &str) -> Option<&Var> {
    self.0.get(key)
  }

  pub fn contains_key(&self, key: &str) -> bool {
    self.0.contains_key(key)
  }

  /// Entries in key order.
  pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &Var)> + DoubleEndedIterator {
    self.0.iter().map(|(k, v)| (&**k, v))
  }

  /// Keys in sorted order.
  pub fn keys(&self) -> impl ExactSizeIterator<Item = &str> + DoubleEndedIterator {
    self.0.keys().map(|k| &**k)
  }

  /// Values in key order.
  pub fn values(&self) -> impl ExactSizeIterator<Item = &Var> + DoubleEndedIterator {
    self.0.values()
  }

  /// How many tables share this storage. For tests that check snapshot
  /// sharing and release; not a stable API.
  #[doc(hidden)]
  pub fn storage_owners(&self) -> usize {
    Arc::strong_count(&self.0)
  }

  pub(crate) fn from_map(map: BTreeMap<Arc<str>, Var>) -> Table {
    Table(Arc::new(map))
  }

  pub(crate) fn map(&self) -> &BTreeMap<Arc<str>, Var> {
    &self.0
  }

  /// The storage when no other table shares it.
  pub(crate) fn into_unique(self) -> Option<BTreeMap<Arc<str>, Var>> {
    Arc::try_unwrap(self.0).ok()
  }
}

impl<K: Into<Arc<str>>> FromIterator<(K, Var)> for Table {
  /// A repeated key keeps the last value.
  fn from_iter<I: IntoIterator<Item = (K, Var)>>(entries: I) -> Table {
    Table::from_map(entries.into_iter().map(|(k, v)| (k.into(), v)).collect())
  }
}

impl From<Table> for Var {
  fn from(t: Table) -> Var {
    Var::Table(t)
  }
}

/// Builds a [`Table`]; a repeated key keeps the last value.
#[derive(Debug, Default)]
pub struct TableBuilder(BTreeMap<Arc<str>, Var>);

impl TableBuilder {
  pub fn new() -> TableBuilder {
    TableBuilder::default()
  }

  pub fn insert(&mut self, key: impl Into<Arc<str>>, value: Var) -> &mut TableBuilder {
    self.0.insert(key.into(), value);
    self
  }

  /// Removes a key, returning its value.
  pub fn remove(&mut self, key: &str) -> Option<Var> {
    self.0.remove(key)
  }

  pub fn get(&self, key: &str) -> Option<&Var> {
    self.0.get(key)
  }

  pub fn with(mut self, key: impl Into<Arc<str>>, value: Var) -> TableBuilder {
    self.insert(key, value);
    self
  }

  pub fn build(self) -> Table {
    Table::from_map(self.0)
  }
}

impl Var {
  /// Human text (`Log`, `ToString`, `String.Format`), like 1.x's except
  /// that floats keep every digit:
  /// strings as they are (also inside sequences and tables), whole floats
  /// without `.0` (`3`, `[1 2.5]`), very large or small ones in exponent
  /// form. Not round-trippable: `Display` gives source syntax.
  pub fn text(&self) -> String {
    let mut out = String::new();
    write_text(self, &mut out);
    out
  }
}

/// A float as human text: the shortest form that reads back as the same
/// value (`3.14159265358`, `6416.2715`, `0.1`), whole numbers without `.0`
/// (`3`, `-0`), exponent form only when very large or small (`1e20`).
/// 1.x rounded to six significant digits (`6416.27`); 2.0 keeps the exact
/// value, a listed deviation.
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
  fn text_prints_values_for_people() {
    let seq = |v: Vec<Var>| Var::Seq(Arc::new(v));
    // Whole floats, strings and nesting as 1.x printed them; digits exact.
    assert_eq!(Var::Float(3.0).text(), "3");
    assert_eq!(Var::Float(0.1).text(), "0.1");
    assert_eq!(Var::Float(-0.0).text(), "-0");
    assert_eq!(Var::Float(1e20).text(), "1e20");
    // Exact, not 1.x's six significant digits (a listed deviation).
    assert_eq!(Var::Float(1234.56789012).text(), "1234.56789012");
    assert_eq!(
      Var::Float4([6416.2715, 514.1416, 8.651538, 4.5194016]).text(),
      "@f4(6416.2715 514.1416 8.651538 4.5194016)"
    );
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
