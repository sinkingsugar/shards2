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
    err.contains("bad.shs:2:7: compose error: Math.Add needs Int, Float, Float2, Float3 or Float4 input, got String"),
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

#[test]
fn watch_reloads_atomic_saves_and_keeps_running_after_rejected_edits() {
  use std::io::{BufRead, Write};
  use std::process::{Child, Stdio};
  use std::sync::mpsc;
  use std::time::Duration;

  struct KillOnDrop(Child);
  impl Drop for KillOnDrop {
    fn drop(&mut self) {
      let _ = self.0.kill();
      let _ = self.0.wait();
    }
  }

  for backend in [&[][..], &["--stackful"][..]] {
    let file = script("watch.shs", "41");
    let mut child = KillOnDrop(
      Command::new(env!("CARGO_BIN_EXE_shards2"))
        .arg("watch")
        .args(backend)
        .arg(&file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap(),
    );
    let (send, lines) = mpsc::channel();
    let stdout = child.0.stdout.take().unwrap();
    let stderr = child.0.stderr.take().unwrap();
    let errors = send.clone();
    std::thread::spawn(move || {
      for line in std::io::BufReader::new(stdout).lines() {
        if send.send(line.unwrap()).is_err() {
          break;
        }
      }
    });
    std::thread::spawn(move || {
      for line in std::io::BufReader::new(stderr).lines() {
        if errors.send(line.unwrap()).is_err() {
          break;
        }
      }
    });
    let wait_for = |expected: &str| {
      let deadline = std::time::Instant::now() + Duration::from_secs(10);
      loop {
        let line = lines
          .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
          .unwrap_or_else(|e| panic!("waiting for {expected}: {e}"));
        if line.contains(expected) {
          break line;
        }
      }
    };
    wait_for("root: 41");
    std::fs::write(&file, "unknown").unwrap();
    wait_for("edit rejected; previous execution retained");
    let replacement = format!("{file}.new");
    std::fs::write(&replacement, "42").unwrap();
    std::fs::rename(&replacement, &file).unwrap();
    wait_for("root: 42");
    let live = |body| {
      format!(
        r#"@wire(inner {{{body}}})
@wire(main {{Once({{0 >= n}}) Inc(n) Do(inner)}} Looped: true)
@mesh(m) @schedule(m main) @run(m FPS: 10)"#
      )
    };
    std::fs::write(&file, live(r#"f"old {n}" Log"#)).unwrap();
    wait_for("old 1");
    std::fs::write(&file, live(r#"f"new {n}" Log"#)).unwrap();
    let line = wait_for("new ");
    assert!(line.strip_prefix("new ").unwrap().parse::<i64>().unwrap() > 1);
    // A changed interface is rejected; explicit restart accepts it and
    // resets script locals without restarting the watching process.
    std::fs::write(&file, live(r#"f"restart {n}" Log 123"#)).unwrap();
    wait_for("edit rejected; previous execution retained");
    child.0.stdin.as_mut().unwrap().write_all(b"r\n").unwrap();
    wait_for("restart 1");
    std::fs::write(&file, r#""pending" Log Pause(1000.0)"#).unwrap();
    wait_for("pending");
    child.0.stdin.as_mut().unwrap().write_all(b"q\n").unwrap();
    wait_for("root: cancelled");
    assert!(child.0.wait().unwrap().success());
  }
}
