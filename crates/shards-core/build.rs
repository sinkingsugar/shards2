// `cfg(stackful)`: the target has the stackful (coroutine) mesh. Wasm and
// ESP-IDF builds have only the stackless one. Keep in step with the
// `corosensei` target condition in `crates/shards-core/Cargo.toml`.
fn main() {
  println!("cargo::rustc-check-cfg=cfg(stackful)");
  let family = std::env::var("CARGO_CFG_TARGET_FAMILY").unwrap_or_default();
  let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
  if !family.split(',').any(|f| f == "wasm") && os != "espidf" {
    println!("cargo::rustc-cfg=stackful");
  }
}
