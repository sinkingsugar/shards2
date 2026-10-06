fn main() {
  embuild::espidf::sysenv::output();
  println!("cargo:rustc-check-cfg=cfg(stackful)");
  println!("cargo:rustc-check-cfg=cfg(feature, values(\"docs\"))");
  #[cfg(feature = "acceptance")]
  acceptance();
}

// ESP-IDF has no libtest harness. Expand the shared suite's one backend macro
// and call its test functions directly, preserving each test's cfg attributes.
// Changes to the actual tests automatically enter the firmware suite.
#[cfg(feature = "acceptance")]
fn acceptance() {
  use quote::{ToTokens, quote};
  use std::{fs, path::PathBuf};
  use syn::{Item, parse_quote};
  let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
  for (name, path, macro_name) in [
    (
      "prototype",
      "../../crates/shards-core/tests/prototype.rs",
      "acceptance_tests",
    ),
    (
      "metadata",
      "../../crates/shards-core/tests/metadata.rs",
      "metadata_tests",
    ),
    (
      "lang",
      "../../crates/shards-lang/tests/lang.rs",
      "lang_tests",
    ),
    (
      "trampoline",
      "../../crates/shards-core/tests/trampoline.rs",
      "",
    ),
  ] {
    println!("cargo:rerun-if-changed={path}");
    // This shared frontend fixture is relative to tests/lang.rs; retain its
    // location when compiling the generated file from OUT_DIR.
    let source = fs::read_to_string(path).unwrap().replace(
      "\"../../../examples/esp32/smoke.shs\"",
      &format!("{:?}", fs::canonicalize("smoke.shs").unwrap()),
    );
    let expanded = if macro_name.is_empty() {
      source
    } else {
      let start = source.find(&format!("macro_rules! {macro_name}")).unwrap();
      let body_start =
        start + source[start..].find("($mesh:ty) => {").unwrap() + "($mesh:ty) => {".len();
      let body_end = body_start + source[body_start..].find("\n  };\n}").unwrap();
      let body = source[body_start..body_end].replace("$mesh", "shards_core::Mesh");
      let suffix = &source[body_end + "\n  };\n}".len()..];
      let mut tail = syn::parse_file(suffix).unwrap();
      tail
        .items
        .retain(|i| !matches!(i, Item::Mod(m) if m.ident == "stackful" || m.ident == "stackless"));
      format!(
        "{} mod cases {{ {} }} {}",
        &source[..start],
        body,
        tail.into_token_stream()
      )
    };
    fn runnable(items: &mut Vec<Item>) {
      let mut calls = Vec::new();
      for item in items.iter_mut() {
        match item {
          Item::Fn(f) if f.attrs.iter().any(|a| a.path().is_ident("test")) => {
            f.attrs.retain(|a| !a.path().is_ident("test"));
            let cfgs: Vec<_> = f
              .attrs
              .iter()
              .filter(|a| a.path().is_ident("cfg"))
              .collect();
            let name = &f.sig.ident;
            calls.push(
              quote! { #(#cfgs)* { println!("acceptance: {}", stringify!(#name)); #name(); std::thread::sleep(std::time::Duration::from_millis(10)); } },
            );
          }
          Item::Mod(m) => {
            if let Some((_, children)) = &mut m.content {
              runnable(children);
              let name = &m.ident;
              let cfgs: Vec<_> = m
                .attrs
                .iter()
                .filter(|a| a.path().is_ident("cfg"))
                .collect();
              calls.push(quote! { #(#cfgs)* #name::run_suite(); });
            }
          }
          _ => {}
        }
      }
      items.push(parse_quote! { pub fn run_suite() { #(#calls)* } });
    }
    let mut file = syn::parse_file(&expanded).unwrap();
    runnable(&mut file.items);
    // Inner doc comments belong to the original file, not an include! expansion.
    file.attrs.clear();
    fs::write(
      out.join(format!("{name}.rs")),
      file.into_token_stream().to_string(),
    )
    .unwrap();
  }
}
