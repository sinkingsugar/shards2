#[cfg(feature = "acceptance")]
// Generated token streams lose source newlines and macro lint exemptions.
// These style lints run on the original suites in the workspace instead.
#[allow(
  clippy::possible_missing_else,
  clippy::type_complexity,
  clippy::missing_const_for_thread_local
)]
mod acceptance {
  mod prototype {
    include!(concat!(env!("OUT_DIR"), "/prototype.rs"));
  }
  mod metadata {
    include!(concat!(env!("OUT_DIR"), "/metadata.rs"));
  }
  mod host_contract {
    include!(concat!(env!("OUT_DIR"), "/host_contract.rs"));
  }
  mod registry {
    include!(concat!(env!("OUT_DIR"), "/registry.rs"));
  }
  mod lang {
    include!(concat!(env!("OUT_DIR"), "/lang.rs"));
  }
  mod trampoline {
    include!(concat!(env!("OUT_DIR"), "/trampoline.rs"));
  }
  pub fn run() {
    // Free each suite's native stack before the next suite starts. The
    // instance-heavy core cases need heap; recursive frontend composition
    // needs a larger stack. All test depths and instance counts are retained.
    for (name, stack, run) in [
      ("prototype", 48 * 1024, prototype::run_suite as fn()),
      ("metadata", 48 * 1024, metadata::run_suite as fn()),
      ("host_contract", 48 * 1024, host_contract::run_suite as fn()),
      ("registry", 48 * 1024, registry::run_suite as fn()),
      ("lang", 128 * 1024, lang::run_suite as fn()),
      ("trampoline", 96 * 1024, trampoline::run_suite as fn()),
    ] {
      std::thread::Builder::new()
        .name(name.into())
        .stack_size(stack)
        .spawn(move || {
          run();
          // SAFETY: query of this running task; no pointers escape.
          let remaining = unsafe { esp_idf_sys::uxTaskGetStackHighWaterMark(std::ptr::null_mut()) };
          println!("acceptance suite {name}: stack min free {remaining} of {stack} B");
        })
        .expect("acceptance task")
        .join()
        .expect("acceptance suite");
      std::thread::sleep(std::time::Duration::from_millis(10));
    }
    println!("Shards ESP32 acceptance suites passed");
  }
}

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
  let ticks = report.ticks;
  drop(report);
  drop(program);
  drop(catalog);
  #[cfg(feature = "acceptance")]
  acceptance::run();
  // Low-water marks since boot: parse, compose and run all happened above.
  // Stack depth follows the code path, so emulator runs measure it too.
  // SAFETY: plain FreeRTOS/ESP-IDF queries; a null handle is this task.
  let (stack_free, heap_free) = unsafe {
    (
      esp_idf_sys::uxTaskGetStackHighWaterMark(std::ptr::null_mut()),
      esp_idf_sys::esp_get_minimum_free_heap_size(),
    )
  };
  println!(
    "Shards ESP32 smoke test passed: 42 ({} ticks); main stack min free {} of {} B, heap min free {} B",
    ticks,
    stack_free,
    esp_idf_sys::CONFIG_ESP_MAIN_TASK_STACK_SIZE,
    heap_free
  );
}
