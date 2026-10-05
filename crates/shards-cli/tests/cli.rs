//! The `shards2` command end to end.

use std::process::Command;

fn script(name: &str, text: &str) -> String {
  let dir = std::env::temp_dir().join(format!("shards2-cli-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let path = dir.join(name);
  std::fs::write(&path, text).unwrap();
  path.to_string_lossy().into_owned()
}

fn shards2(args: &[&str]) -> (i32, String, String) {
  let out = Command::new(env!("CARGO_BIN_EXE_shards2"))
    .args(args)
    .output()
    .unwrap();
  (
    out.status.code().unwrap_or(-1),
    String::from_utf8_lossy(&out.stdout).into_owned(),
    String::from_utf8_lossy(&out.stderr).into_owned(),
  )
}

#[test]
fn check_json_reports_located_diagnostics() {
  let file = script("bad.shs", "0 >= n\n\"a\" | Add(2)\n");
  let (code, out, _) = shards2(&["check", "--json", &file]);
  assert_eq!(code, 1);
  assert!(out.starts_with("{\"ok\":false,"), "{out}");
  assert!(out.contains("\"code\":\"input-type-mismatch\""), "{out}");
  assert!(out.contains("\"line\":2,\"column\":7"), "{out}");

  let (code, _, err) = shards2(&["check", "--stackful", &file]);
  assert_eq!(code, 1);
  assert!(
    err.contains("bad.shs:2:7: compose error: cannot add Int to String"),
    "{err}"
  );
  assert!(err.contains("  2 | \"a\" | Add(2)\n    |       ^"), "{err}");
}

#[test]
fn check_ok_and_run_with_script_arguments() {
  let file = script("ok.shs", "@n | Ref(text)\n40 | Add(2)\n");
  let (code, out, _) = shards2(&["check", "--json", &file, "n:hello"]);
  assert_eq!(
    (code, out.trim()),
    (
      0,
      format!("{{\"ok\":true,\"file\":\"{file}\",\"diagnostics\":[]}}").as_str()
    )
  );
  // Script arguments may come before the file.
  let (code, _, err) = shards2(&["check", "n:hello", &file]);
  assert_eq!(code, 0, "{err}");
  for backend in [&[][..], &["--stackful"][..]] {
    let mut args = vec!["run"];
    args.extend_from_slice(backend);
    args.extend_from_slice(&[&file, "n:hello"]);
    let (code, out, err) = shards2(&args);
    assert_eq!((code, out.as_str()), (0, "root: 42\n"), "{err}");
  }
}

#[test]
fn syntax_errors_are_rendered_with_the_source_line() {
  let file = script("syntax.shs", "\"Hello\" | Log(\n");
  let (code, _, err) = shards2(&["check", &file]);
  assert_eq!(code, 1);
  assert!(err.contains("syntax.shs:1:14: parse error: missing `)` for the parameters of `Log` opened at 1:14 [unclosed]"), "{err}");
}

#[test]
fn catalog_commands() {
  let (code, out, _) = shards2(&["describe", "Add"]);
  assert_eq!(code, 0);
  assert!(
    out.contains("\"name\":\"Math.Add\",\"aliases\":[\"Add\"]"),
    "{out}"
  );
  let (code, _, err) = shards2(&["describe", "Ad"]);
  assert_eq!(code, 2);
  assert!(err.contains("did you mean: Add"), "{err}");
  let (code, out, _) = shards2(&["search", "http"]);
  assert_eq!(code, 0);
  assert!(out.contains("Http.Get: "), "{out}");
  let (code, _, err) = shards2(&[]);
  assert_eq!(code, 2);
  assert!(err.contains("usage:"));
}
