use std::collections::HashMap;

use shards_core::{Catalog, Mesh, Outcome, Var};
use shards_lang::{Program, Source};

fn main() {
  esp_idf_sys::link_patches();

  let catalog = Catalog::new(&[shards_core::shards::CATALOG]).expect("core catalog");
  let program = Program::load(
    Source::new("smoke.shs", include_str!("../smoke.shs")),
    &catalog,
    &HashMap::new(),
  )
  .unwrap_or_else(|(_, diagnostics)| panic!("load failed: {diagnostics:?}"));
  let report = program.run::<Mesh>().expect("compose and run");
  assert_eq!(report.outcomes.len(), 1);
  assert_eq!(report.outcomes[0].1, Some(Outcome::Completed(Var::Int(42))));
  assert!(report.spawned_failures.is_empty());
  assert!(report.ticks >= 2, "Pause must suspend and resume");
  println!(
    "Shards ESP32 smoke test passed: 42 ({} ticks)",
    report.ticks
  );
}
