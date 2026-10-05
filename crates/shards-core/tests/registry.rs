//! The type registry never frees descriptions, so nothing may intern a type
//! per runtime value. Its own test binary: other tests intern concurrently.

use shards_core::{Type, Var};

#[test]
fn checking_values_against_types_interns_nothing() {
  // Intern what the checks use first.
  let (any_table, int) = (Type::any_table(), Type::int());
  let before = Type::registered();
  for i in 0..100 {
    let v = Var::table([(format!("key{i}"), Var::Int(i))]);
    assert!(any_table.admits(&v));
    assert!(!int.admits(&v));
  }
  assert_eq!(Type::registered(), before);
}
