//! Values (contract §9, docs/values-and-types.md §2, golden path §7).
//!
//! `Var` is 32 bytes aligned to 16, as 1.x's `SHVar` (what `Float4` needs;
//! alignment 32 measured the same in time and cost a third more in every
//! frame, instruction and step that holds a value, see `bench/values`). The
//! explicit `u8` tag keeps the layout defined (RFC 2195): the tag at offset
//! 0, `Float4` at offset 16, and the table payload after the tag.
//! `tests/values.rs` pins these.

use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use crate::types::{Shape, Type};

#[derive(Clone, Debug, Default)]
#[repr(u8, align(16))]
pub enum Var {
  #[default]
  None,
  Bool(bool),
  Int(i64),
  Float(f64),
  Float2(Float2),
  Float3(Float3),
  Float4(Float4),
  String(Arc<str>),
  Seq(Arc<Vec<Var>>),
  /// String keys in sorted order (docs/values-and-types.md §2).
  Table(Table),
}

/// Two 32-bit floats. Derefs to the array; `Var::Float2(v)` reads `v[0]`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(transparent)]
pub struct Float2(pub [f32; 2]);

/// Three 32-bit floats.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(transparent)]
pub struct Float3(pub [f32; 3]);

/// Four 32-bit floats, aligned for packed arithmetic.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C, align(16))]
pub struct Float4(pub [f32; 4]);

macro_rules! vector_payload {
  ($name:ident, $n:literal, $variant:ident) => {
    impl Deref for $name {
      type Target = [f32; $n];
      fn deref(&self) -> &[f32; $n] {
        &self.0
      }
    }
    impl DerefMut for $name {
      fn deref_mut(&mut self) -> &mut [f32; $n] {
        &mut self.0
      }
    }
    impl From<[f32; $n]> for $name {
      fn from(c: [f32; $n]) -> $name {
        $name(c)
      }
    }
    impl From<$name> for [f32; $n] {
      fn from(v: $name) -> [f32; $n] {
        v.0
      }
    }
    impl From<[f32; $n]> for Var {
      fn from(c: [f32; $n]) -> Var {
        Var::$variant($name(c))
      }
    }
    impl From<$name> for Var {
      fn from(v: $name) -> Var {
        Var::$variant(v)
      }
    }
  };
}

vector_payload!(Float2, 2, Float2);
vector_payload!(Float3, 3, Float3);
vector_payload!(Float4, 4, Float4);

impl Var {
  pub fn string(s: &str) -> Var {
    Var::String(Arc::from(s))
  }

  pub fn float2(x: f32, y: f32) -> Var {
    Var::Float2(Float2([x, y]))
  }

  pub fn float3(x: f32, y: f32, z: f32) -> Var {
    Var::Float3(Float3([x, y, z]))
  }

  pub fn float4(x: f32, y: f32, z: f32, w: f32) -> Var {
    Var::Float4(Float4([x, y, z, w]))
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
      Var::Table(table) => match &table.0 {
        TableRepr::Struct { shape, slots } => {
          Type::fixed_table_of(*shape, slots.iter().map(Var::type_of))
        }
        TableRepr::Map(entries) => {
          Type::fixed_table(entries.iter().map(|(k, v)| (k.clone(), v.type_of())))
        }
      },
    }
  }

  /// The same value with every table (at any depth) in struct
  /// representation, its key shape interned. Compose applies this to
  /// literals, so a fixed-typed value is a struct table at runtime
  /// (golden path §7.3); hosts may apply it to values they build once and
  /// pass many times. Interns a shape per distinct key set, so do not apply
  /// it to values with unbounded key sets.
  pub fn into_struct_tables(self) -> Var {
    if !self.holds_map_table() {
      return self;
    }
    match self {
      Var::Seq(items) => Var::Seq(Arc::new(
        items.iter().cloned().map(Var::into_struct_tables).collect(),
      )),
      Var::Table(table) => Var::Table(table.into_struct()),
      other => other,
    }
  }

  fn holds_map_table(&self) -> bool {
    match self {
      Var::Seq(items) => items.iter().any(Var::holds_map_table),
      Var::Table(table) => match &table.0 {
        TableRepr::Map(_) => true,
        TableRepr::Struct { slots, .. } => slots.iter().any(Var::holds_map_table),
      },
      _ => false,
    }
  }
}

/// A table: string keys in sorted order, values shared copy-on-write
/// (docs/values-and-types.md §2). Cloning is cheap. Two representations
/// with one behavior (golden path §7.3): a **struct** table holds an
/// interned key [`Shape`] and one slot per key, which is what compose
/// produces for every fixed table type, so a literal-key read is an indexed
/// load; a **map** table holds sorted entries, for open tables and values
/// hosts build with [`TableBuilder`] or `collect`. Equality, hashing,
/// printing, iteration and `type_of` do not depend on the representation.
#[derive(Clone)]
pub struct Table(TableRepr);

#[derive(Clone)]
enum TableRepr {
  Struct { shape: Shape, slots: Arc<[Var]> },
  Map(Arc<Vec<(Arc<str>, Var)>>),
}

impl Default for Table {
  fn default() -> Table {
    Table(TableRepr::Map(Arc::new(Vec::new())))
  }
}

impl Table {
  pub fn new() -> Table {
    Table::default()
  }

  pub fn builder() -> TableBuilder {
    TableBuilder::default()
  }

  /// A struct table of `shape`, with one value per key in the shape's
  /// (sorted) key order. Panics if the counts differ: a caller bug, like a
  /// struct literal with a missing field.
  pub fn with_shape(shape: Shape, values: impl IntoIterator<Item = Var>) -> Table {
    let slots: Arc<[Var]> = values.into_iter().collect();
    assert_eq!(
      slots.len(),
      shape.len(),
      "table of shape {shape} needs {} values",
      shape.len()
    );
    Table(TableRepr::Struct { shape, slots })
  }

  /// The key shape of a struct table; `None` for a map table.
  pub fn shape(&self) -> Option<Shape> {
    match &self.0 {
      TableRepr::Struct { shape, .. } => Some(*shape),
      TableRepr::Map(_) => None,
    }
  }

  /// This table as a struct table of its own keys (interning the shape);
  /// nested tables are converted too. Same contents, same storage when it
  /// already is one.
  pub fn into_struct(self) -> Table {
    match self.0 {
      TableRepr::Struct { shape, slots } => {
        if !slots.iter().any(Var::holds_map_table) {
          return Table(TableRepr::Struct { shape, slots });
        }
        let slots = slots.iter().cloned().map(Var::into_struct_tables).collect();
        Table(TableRepr::Struct { shape, slots })
      }
      TableRepr::Map(entries) => {
        let shape = Shape::new(entries.iter().map(|(k, _)| k.clone()));
        let values = entries.iter().map(|(_, v)| v.clone().into_struct_tables());
        Table::with_shape(shape, values)
      }
    }
  }

  /// A builder holding this table's entries, to derive a changed table.
  /// Takes the storage when no other table shares it; otherwise copies
  /// the entries, sharing their keys and values.
  pub fn into_builder(self) -> TableBuilder {
    TableBuilder(match self.0 {
      TableRepr::Map(entries) => {
        Arc::try_unwrap(entries).unwrap_or_else(|shared| (*shared).clone())
      }
      TableRepr::Struct { shape, slots } => shape
        .keys()
        .iter()
        .cloned()
        .zip(slots.iter().cloned())
        .collect(),
    })
  }

  pub fn len(&self) -> usize {
    match &self.0 {
      TableRepr::Struct { slots, .. } => slots.len(),
      TableRepr::Map(entries) => entries.len(),
    }
  }

  pub fn is_empty(&self) -> bool {
    self.len() == 0
  }

  #[inline]
  pub fn get(&self, key: &str) -> Option<&Var> {
    match &self.0 {
      TableRepr::Struct { shape, slots } => shape.index_of(key).map(|i| &slots[i]),
      TableRepr::Map(entries) => entries
        .binary_search_by(|(k, _)| (**k).cmp(key))
        .ok()
        .map(|i| &entries[i].1),
    }
  }

  /// The value at `index` in key order: the slot of a struct table, the
  /// entry of a map table. Compose resolves a literal key on a fixed table
  /// to its index (golden path §7.3); a map value admitted by a fixed type
  /// has exactly the type's keys, so its sorted entries are the slots.
  #[inline]
  pub fn slot(&self, index: usize) -> Option<&Var> {
    match &self.0 {
      TableRepr::Struct { slots, .. } => slots.get(index),
      TableRepr::Map(entries) => entries.get(index).map(|(_, v)| v),
    }
  }

  pub fn contains_key(&self, key: &str) -> bool {
    self.get(key).is_some()
  }

  /// Entries in key order.
  pub fn iter(&self) -> Iter<'_> {
    Iter(match &self.0 {
      TableRepr::Struct { shape, slots } => IterRepr::Struct(shape.keys().iter().zip(slots.iter())),
      TableRepr::Map(entries) => IterRepr::Map(entries.iter()),
    })
  }

  /// Keys in sorted order.
  pub fn keys(&self) -> impl ExactSizeIterator<Item = &str> + DoubleEndedIterator {
    self.iter().map(|(k, _)| k)
  }

  /// Values in key order.
  pub fn values(&self) -> impl ExactSizeIterator<Item = &Var> + DoubleEndedIterator {
    self.iter().map(|(_, v)| v)
  }

  /// How many tables share this storage. For tests that check snapshot
  /// sharing and release; not a stable API.
  #[doc(hidden)]
  pub fn storage_owners(&self) -> usize {
    match &self.0 {
      TableRepr::Struct { slots, .. } => Arc::strong_count(slots),
      TableRepr::Map(entries) => Arc::strong_count(entries),
    }
  }

  /// The slots of a struct table of `shape` when nothing else shares them,
  /// for a constructor to overwrite in place (docs/values-and-types.md §7).
  pub(crate) fn unique_slots(&mut self, shape: Shape) -> Option<&mut [Var]> {
    match &mut self.0 {
      TableRepr::Struct { shape: s, slots } if *s == shape => Arc::get_mut(slots),
      _ => None,
    }
  }

  fn from_sorted(entries: Vec<(Arc<str>, Var)>) -> Table {
    Table(TableRepr::Map(Arc::new(entries)))
  }
}

/// A table's entries in key order.
pub struct Iter<'a>(IterRepr<'a>);

enum IterRepr<'a> {
  Struct(std::iter::Zip<std::slice::Iter<'a, Arc<str>>, std::slice::Iter<'a, Var>>),
  Map(std::slice::Iter<'a, (Arc<str>, Var)>),
}

impl<'a> Iterator for Iter<'a> {
  type Item = (&'a str, &'a Var);

  fn next(&mut self) -> Option<(&'a str, &'a Var)> {
    match &mut self.0 {
      IterRepr::Struct(zip) => zip.next().map(|(k, v)| (&**k, v)),
      IterRepr::Map(entries) => entries.next().map(|(k, v)| (&**k, v)),
    }
  }

  fn size_hint(&self) -> (usize, Option<usize>) {
    match &self.0 {
      IterRepr::Struct(zip) => zip.size_hint(),
      IterRepr::Map(entries) => entries.size_hint(),
    }
  }
}

impl DoubleEndedIterator for Iter<'_> {
  fn next_back(&mut self) -> Option<Self::Item> {
    match &mut self.0 {
      IterRepr::Struct(zip) => zip.next_back().map(|(k, v)| (&**k, v)),
      IterRepr::Map(entries) => entries.next_back().map(|(k, v)| (&**k, v)),
    }
  }
}

impl ExactSizeIterator for Iter<'_> {}

impl<'a> IntoIterator for &'a Table {
  type Item = (&'a str, &'a Var);
  type IntoIter = Iter<'a>;
  fn into_iter(self) -> Iter<'a> {
    self.iter()
  }
}

impl<K: Into<Arc<str>>> FromIterator<(K, Var)> for Table {
  /// A repeated key keeps the last value.
  fn from_iter<I: IntoIterator<Item = (K, Var)>>(entries: I) -> Table {
    let mut builder = TableBuilder::new();
    for (k, v) in entries {
      builder.insert(k, v);
    }
    builder.build()
  }
}

impl From<Table> for Var {
  fn from(t: Table) -> Var {
    Var::Table(t)
  }
}

/// Representation-independent: equal contents are equal (with `Var`'s
/// bitwise float identity), whichever representation holds them.
impl PartialEq for Table {
  fn eq(&self, other: &Table) -> bool {
    if let (TableRepr::Struct { shape: a, slots: x }, TableRepr::Struct { shape: b, slots: y }) =
      (&self.0, &other.0)
    {
      return a == b && (Arc::ptr_eq(x, y) || x == y);
    }
    self.len() == other.len() && self.iter().eq(other.iter())
  }
}

impl Eq for Table {}

impl Hash for Table {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.len().hash(state);
    for (k, v) in self.iter() {
      k.hash(state);
      v.hash(state);
    }
  }
}

impl fmt::Debug for Table {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_map().entries(self.iter()).finish()
  }
}

/// Builds a [`Table`] (a map table); a repeated key keeps the last value.
/// Entries are kept sorted, so inserting keys in order appends.
#[derive(Debug, Default)]
pub struct TableBuilder(Vec<(Arc<str>, Var)>);

impl TableBuilder {
  pub fn new() -> TableBuilder {
    TableBuilder::default()
  }

  fn position(&self, key: &str) -> std::result::Result<usize, usize> {
    self.0.binary_search_by(|(k, _)| (**k).cmp(key))
  }

  pub fn insert(&mut self, key: impl Into<Arc<str>>, value: Var) -> &mut TableBuilder {
    let key = key.into();
    match self.position(&key) {
      Ok(i) => self.0[i].1 = value,
      Err(i) => self.0.insert(i, (key, value)),
    }
    self
  }

  /// Removes a key, returning its value.
  pub fn remove(&mut self, key: &str) -> Option<Var> {
    self.position(key).ok().map(|i| self.0.remove(i).1)
  }

  pub fn get(&self, key: &str) -> Option<&Var> {
    self.position(key).ok().map(|i| &self.0[i].1)
  }

  pub fn with(mut self, key: impl Into<Arc<str>>, value: Var) -> TableBuilder {
    self.insert(key, value);
    self
  }

  pub fn build(self) -> Table {
    Table::from_sorted(self.0)
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
    Var::Float2(c) => vector(out, "@f2(", c.iter().map(|x| shortest(*x))),
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
      (Var::Float2(a), Var::Float2(b)) => same_bits(&a.0, &b.0),
      (Var::Float3(a), Var::Float3(b)) => same_bits(&a.0, &b.0),
      (Var::Float4(a), Var::Float4(b)) => same_bits(&a.0, &b.0),
      (Var::String(a), Var::String(b)) => a == b,
      (Var::Seq(a), Var::Seq(b)) => a == b,
      (Var::Table(a), Var::Table(b)) => a == b,
      _ => false,
    }
  }
}

fn same_bits(a: &[f32], b: &[f32]) -> bool {
  a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
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
      Var::float4(6416.2715, 514.1416, 8.651538, 4.5194016).text(),
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
    assert_eq!(Var::float3(1.0, 2.5, 0.1).text(), "@f3(1 2.5 0.1)");
    // f32 components print their shortest form, not their f64 expansion.
    assert_eq!(Var::float2(0.1, 0.2).text(), "@f2(0.1 0.2)");
    assert_eq!(Var::float2(0.1, 0.2).to_string(), "@f2(0.1 0.2)");
    // Display stays source syntax, round-trippable.
    assert_eq!(Var::Float(3.0).to_string(), "3.0");
    assert_eq!(Var::string("a").to_string(), "\"a\"");
  }
}
