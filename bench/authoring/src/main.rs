//! Authoring eval runner (docs/golden-path.md §8).
//!
//! ```text
//! authoring-eval verify [--shards2 PATH]
//! authoring-eval reference [--variant NAME] [--shards2 PATH]
//! authoring-eval run --model NAME --cmd "<shell command>" --out FILE
//!     [--format text|claude-json] [--variant NAME] [--rounds N] [--jobs N]
//!     [--only TEXT] [--transcripts DIR] [--shards2 PATH]
//! ```
//!
//! `verify` checks every task's reference solution against its expected
//! output, without a model. `reference` prints what the model receives:
//! the variant's language primer (`reference/<variant>.md`) followed by
//! `shards2 catalog` and `shards2 describe` for every shard.
//!
//! `run` sends each task with the reference to `sh -c <cmd>` on stdin,
//! from an empty temporary directory, and reads the reply from stdout
//! (`text`), or from Claude Code's `--output-format json` envelope
//! (`claude-json`, which also reports tokens). The last fenced code block
//! of the reply is the program. `shards2 check --json` diagnostics go back
//! to the model for at most `--rounds` repairs (default 5); a program that
//! checks is then run and its log compared with the task's expected
//! lines. One CSV row per task is appended to `--out` as it finishes.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const USAGE: &str = "usage:
  authoring-eval verify [--shards2 PATH]
  authoring-eval reference [--variant NAME] [--shards2 PATH]
  authoring-eval run --model NAME --cmd \"<shell command>\" --out FILE
      [--format text|claude-json] [--variant NAME] [--rounds N] [--jobs N]
      [--only TEXT] [--transcripts DIR] [--shards2 PATH]";

const CSV_HEADER: &str =
  "Task,Model,Variant,SemanticSuccess,FirstPassSuccess,RepairRounds,Tokens,LatencyMs,Failure";

const MODEL_TIMEOUT: Duration = Duration::from_secs(600);
const SHARDS_TIMEOUT: Duration = Duration::from_secs(30);

fn manifest_dir() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

struct Task {
  id: String,
  prompt: String,
  expected: Vec<String>,
  reference: Option<String>,
}

/// A task file has `=== prompt`, `=== expected` and optionally
/// `=== reference` sections.
fn parse_task(id: &str, text: &str) -> Result<Task, String> {
  let mut sections: Vec<(String, String)> = Vec::new();
  for line in text.lines() {
    if let Some(name) = line.strip_prefix("=== ") {
      sections.push((name.trim().to_string(), String::new()));
    } else if let Some((_, body)) = sections.last_mut() {
      body.push_str(line);
      body.push('\n');
    } else if !line.trim().is_empty() {
      return Err(format!("{id}: text before the first section"));
    }
  }
  let get = |name: &str| {
    sections
      .iter()
      .find(|(n, _)| n == name)
      .map(|(_, b)| b.trim().to_string())
  };
  for (name, _) in &sections {
    if !["prompt", "expected", "reference"].contains(&name.as_str()) {
      return Err(format!("{id}: unknown section `{name}`"));
    }
  }
  Ok(Task {
    id: id.to_string(),
    prompt: get("prompt").ok_or(format!("{id}: missing prompt"))?,
    expected: get("expected")
      .ok_or(format!("{id}: missing expected"))?
      .lines()
      .map(str::to_string)
      .collect(),
    reference: get("reference"),
  })
}

fn load_tasks(only: Option<&str>) -> Result<Vec<Task>, String> {
  let dir = manifest_dir().join("tasks");
  let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
    .map_err(|e| format!("{}: {e}", dir.display()))?
    .filter_map(|e| e.ok().map(|e| e.path()))
    .filter(|p| p.extension().is_some_and(|e| e == "task"))
    .collect();
  paths.sort();
  let mut tasks = Vec::new();
  for path in paths {
    let id = path.file_stem().unwrap().to_string_lossy().to_string();
    if only.is_some_and(|o| !id.contains(o)) {
      continue;
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    tasks.push(parse_task(&id, &text)?);
  }
  Ok(tasks)
}

struct Output {
  status: Option<i32>,
  stdout: String,
  stderr: String,
  timed_out: bool,
}

/// Runs a command to completion or until `timeout`, feeding `stdin`.
fn run_with_timeout(mut cmd: Command, stdin: &str, timeout: Duration) -> Result<Output, String> {
  let mut child = cmd
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(|e| format!("spawn: {e}"))?;
  let mut input = child.stdin.take().unwrap();
  let stdin = stdin.to_string();
  let writer = std::thread::spawn(move || {
    let _ = input.write_all(stdin.as_bytes());
  });
  let mut out = child.stdout.take().unwrap();
  let mut err = child.stderr.take().unwrap();
  let reader = std::thread::spawn(move || {
    let mut s = String::new();
    let _ = out.read_to_string(&mut s);
    s
  });
  let err_reader = std::thread::spawn(move || {
    let mut s = String::new();
    let _ = err.read_to_string(&mut s);
    s
  });
  let start = Instant::now();
  let (status, timed_out) = loop {
    if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
      break (status.code(), false);
    }
    if start.elapsed() > timeout {
      let _ = child.kill();
      let _ = child.wait();
      break (None, true);
    }
    std::thread::sleep(Duration::from_millis(20));
  };
  let _ = writer.join();
  Ok(Output {
    status,
    stdout: reader.join().unwrap_or_default(),
    stderr: err_reader.join().unwrap_or_default(),
    timed_out,
  })
}

fn shards2_path(explicit: Option<&str>) -> Result<PathBuf, String> {
  if let Some(p) = explicit
    .map(str::to_string)
    .or(std::env::var("SHARDS2").ok())
  {
    return Ok(PathBuf::from(p));
  }
  let target = manifest_dir().join("../../target");
  for profile in ["release", "debug"] {
    let p = target.join(profile).join("shards2");
    if p.exists() {
      return Ok(p);
    }
  }
  Err("shards2 not found: build it (`cargo build -p shards-cli`) or pass --shards2".into())
}

fn shards2(bin: &Path, args: &[&str], cwd: &Path) -> Result<Output, String> {
  let mut cmd = Command::new(bin);
  cmd.args(args).current_dir(cwd);
  run_with_timeout(cmd, "", SHARDS_TIMEOUT)
}

/// The primer for `variant` followed by the generated catalog.
fn reference(bin: &Path, variant: &str) -> Result<String, String> {
  let primer_path = manifest_dir()
    .join("reference")
    .join(format!("{variant}.md"));
  let primer =
    std::fs::read_to_string(&primer_path).map_err(|e| format!("{}: {e}", primer_path.display()))?;
  let cwd = std::env::temp_dir();
  let index = shards2(bin, &["catalog"], &cwd)?;
  if index.status != Some(0) {
    return Err(format!("shards2 catalog failed: {}", index.stderr));
  }
  let json: serde_json::Value =
    serde_json::from_str(&index.stdout).map_err(|e| format!("catalog json: {e}"))?;
  let mut out = primer.trim_end().to_string();
  out.push_str("\n\n## Shard catalog\n\nOne `shards2 describe` record per shard:\n\n");
  for shard in json["shards"].as_array().ok_or("catalog: no shards")? {
    let name = shard["name"]
      .as_str()
      .ok_or("catalog: shard without name")?;
    let d = shards2(bin, &["describe", name], &cwd)?;
    if d.status != Some(0) {
      return Err(format!("shards2 describe {name} failed: {}", d.stderr));
    }
    out.push_str(d.stdout.trim());
    out.push('\n');
  }
  Ok(out)
}

/// Wires whose outcome `shards2 run` prints after the log: `root` and the
/// targets of `@schedule(mesh wire)`.
fn outcome_names(program: &str) -> Vec<String> {
  let mut names = vec!["root".to_string()];
  let mut rest = program;
  while let Some(i) = rest.find("@schedule(") {
    rest = &rest[i + "@schedule(".len()..];
    let args = rest.split(')').next().unwrap_or("");
    if let Some(wire) = args.split_whitespace().nth(1) {
      names.push(wire.to_string());
    }
  }
  names
}

/// The log matches when it equals the expected lines followed by at most
/// one outcome line per wire in `names`.
fn log_matches(stdout: &str, expected: &[String], names: &[String]) -> bool {
  let lines: Vec<&str> = stdout.lines().collect();
  if lines.len() < expected.len() || lines[..expected.len()] != *expected {
    return false;
  }
  let tail = &lines[expected.len()..];
  if tail.len() > names.len() {
    return false;
  }
  let mut unused: Vec<&String> = names.iter().collect();
  tail.iter().all(|line| {
    match unused.iter().position(|n| {
      line
        .strip_prefix(n.as_str())
        .is_some_and(|r| r.starts_with(": "))
    }) {
      Some(i) => {
        unused.remove(i);
        true
      }
      None => false,
    }
  })
}

/// Why a program failed the task, from a check report and a run.
fn classify_check(report: &serde_json::Value) -> &'static str {
  let diagnostics = report["diagnostics"]
    .as_array()
    .cloned()
    .unwrap_or_default();
  let errors = diagnostics.iter().filter(|d| d["severity"] == "error");
  let mut kind = "compose";
  for d in errors {
    if d["phase"] == "parse" {
      return "syntax";
    }
    if d["code"] == "unknown-shard" {
      kind = "missing-shard";
    }
  }
  kind
}

struct Verdict {
  passed: bool,
  failure: &'static str,
  detail: String,
}

/// Runs a checked program and compares its log with the expected lines.
fn test(bin: &Path, dir: &Path, program: &str, expected: &[String]) -> Result<Verdict, String> {
  let run = shards2(bin, &["run", "solution.shs"], dir)?;
  let detail = format!(
    "exit {:?}\nstdout:\n{}stderr:\n{}",
    run.status, run.stdout, run.stderr
  );
  let (passed, failure) = if run.timed_out {
    (false, "timeout")
  } else if run.status != Some(0) {
    (false, "runtime")
  } else if log_matches(&run.stdout, expected, &outcome_names(program)) {
    (true, "")
  } else {
    (false, "wrong-output")
  };
  Ok(Verdict {
    passed,
    failure,
    detail,
  })
}

struct Scratch(PathBuf);

impl Scratch {
  fn new(tag: &str) -> Result<Scratch, String> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("authoring-eval-{}-{n}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(Scratch(dir))
  }
}

impl Drop for Scratch {
  fn drop(&mut self) {
    let _ = std::fs::remove_dir_all(&self.0);
  }
}

/// Checks a program in `dir`; returns the report and whether it is ok.
fn check(bin: &Path, dir: &Path, program: &str) -> Result<(serde_json::Value, String), String> {
  std::fs::write(dir.join("solution.shs"), program).map_err(|e| e.to_string())?;
  let out = shards2(bin, &["check", "--json", "solution.shs"], dir)?;
  let json: serde_json::Value = serde_json::from_str(out.stdout.trim())
    .map_err(|e| format!("check --json: {e}: {}{}", out.stdout, out.stderr))?;
  Ok((json, out.stdout.trim().to_string()))
}

fn verify(bin: &Path) -> Result<bool, String> {
  let tasks = load_tasks(None)?;
  let mut ok = true;
  for task in &tasks {
    let Some(program) = &task.reference else {
      println!("{}: no reference", task.id);
      continue;
    };
    let scratch = Scratch::new(&task.id)?;
    let (report, raw) = check(bin, &scratch.0, program)?;
    if report["ok"] != true {
      ok = false;
      println!("{}: check failed: {raw}", task.id);
      continue;
    }
    let v = test(bin, &scratch.0, program, &task.expected)?;
    if v.passed {
      println!("{}: ok", task.id);
    } else {
      ok = false;
      println!("{}: {}\n{}", task.id, v.failure, v.detail);
    }
  }
  println!("{} tasks", tasks.len());
  Ok(ok)
}

#[derive(Clone, Copy, PartialEq)]
enum Format {
  Text,
  ClaudeJson,
}

struct RunConfig {
  model: String,
  cmd: String,
  format: Format,
  variant: String,
  rounds: usize,
  reference: String,
  bin: PathBuf,
  transcripts: Option<PathBuf>,
}

struct Reply {
  text: String,
  tokens: Option<u64>,
}

fn ask(config: &RunConfig, prompt: &str) -> Result<Reply, String> {
  let scratch = Scratch::new("model")?;
  let mut cmd = Command::new("sh");
  cmd.arg("-c").arg(&config.cmd).current_dir(&scratch.0);
  let out = run_with_timeout(cmd, prompt, MODEL_TIMEOUT)?;
  if out.timed_out {
    return Err("model command timed out".into());
  }
  if out.status != Some(0) {
    return Err(format!(
      "model command exit {:?}: {}{}",
      out.status, out.stderr, out.stdout
    ));
  }
  match config.format {
    Format::Text => Ok(Reply {
      text: out.stdout,
      tokens: None,
    }),
    Format::ClaudeJson => {
      let json: serde_json::Value =
        serde_json::from_str(out.stdout.trim()).map_err(|e| format!("claude json: {e}"))?;
      if json["is_error"] == true {
        return Err(format!("model error: {}", json["result"]));
      }
      let usage = &json["usage"];
      let tokens = [
        "input_tokens",
        "cache_creation_input_tokens",
        "cache_read_input_tokens",
        "output_tokens",
      ]
      .iter()
      .map(|k| usage[k].as_u64().unwrap_or(0))
      .sum();
      Ok(Reply {
        text: json["result"]
          .as_str()
          .ok_or("claude json: no result")?
          .to_string(),
        tokens: Some(tokens),
      })
    }
  }
}

/// The last fenced code block of a reply, or the whole reply.
fn extract_program(reply: &str) -> String {
  let fences: Vec<usize> = reply.match_indices("```").map(|(i, _)| i).collect();
  if fences.len() >= 2 {
    let start = fences[fences.len() - 2] + 3;
    let block = &reply[start..fences[fences.len() - 1]];
    // Drop the info string (```shards).
    let body = block.split_once('\n').map_or("", |(_, b)| b);
    return body.trim_end().to_string() + "\n";
  }
  reply.trim().to_string() + "\n"
}

const INSTRUCTIONS: &str = "The program is checked with `shards2 check` and then run with \
`shards2 run`; the lines it logs are compared exactly with the expected output, so log nothing \
else. Reply with the complete program in a single fenced code block and nothing else.";

fn first_prompt(config: &RunConfig, task: &Task) -> String {
  format!(
    "You are writing a program in the Shards language. Use only what the reference below \
     describes.\n\n<reference>\n{}\n</reference>\n\nTask:\n{}\n\n{INSTRUCTIONS}\n",
    config.reference, task.prompt
  )
}

fn repair_prompt(config: &RunConfig, task: &Task, program: &str, report: &str) -> String {
  format!(
    "{}\nYour previous program:\n```shards\n{}```\n\n`shards2 check --json` reported:\n{}\n\n\
     Fix the program. {INSTRUCTIONS}\n",
    first_prompt(config, task),
    program,
    report
  )
}

struct Row {
  semantic: bool,
  first_pass: bool,
  repairs: usize,
  tokens: Option<u64>,
  latency: Duration,
  failure: String,
}

fn run_task(config: &RunConfig, task: &Task, log: &mut String) -> Result<Row, String> {
  let scratch = Scratch::new(&task.id)?;
  let mut row = Row {
    semantic: false,
    first_pass: false,
    repairs: 0,
    tokens: Some(0),
    latency: Duration::ZERO,
    failure: String::new(),
  };
  let mut prompt = first_prompt(config, task);
  log.push_str(&format!("# {}\n\n## Task\n\n{}\n", task.id, task.prompt));
  loop {
    let start = Instant::now();
    let reply = ask(config, &prompt);
    row.latency += start.elapsed();
    let reply = match reply {
      Ok(r) => r,
      Err(e) => {
        log.push_str(&format!("\n## Model error\n\n{e}\n"));
        row.failure = "model-error".into();
        return Ok(row);
      }
    };
    row.tokens = match (row.tokens, reply.tokens) {
      (Some(a), Some(b)) => Some(a + b),
      _ => None,
    };
    let program = extract_program(&reply.text);
    log.push_str(&format!(
      "\n## Reply {}\n\n{}\n",
      row.repairs,
      reply.text.trim()
    ));
    let (report, raw) = check(&config.bin, &scratch.0, &program)?;
    let ok = report["ok"] == true;
    if row.repairs == 0 {
      row.first_pass = ok;
    }
    if ok {
      break;
    }
    log.push_str(&format!("\n## Check\n\n```json\n{raw}\n```\n"));
    if row.repairs == config.rounds {
      row.failure = classify_check(&report).into();
      return Ok(row);
    }
    row.repairs += 1;
    prompt = repair_prompt(config, task, &program, &raw);
  }
  let program =
    std::fs::read_to_string(scratch.0.join("solution.shs")).map_err(|e| e.to_string())?;
  let v = test(&config.bin, &scratch.0, &program, &task.expected)?;
  log.push_str(&format!(
    "\n## Run ({})\n\n```\n{}```\n",
    if v.passed { "pass" } else { v.failure },
    v.detail
  ));
  row.semantic = v.passed;
  row.failure = v.failure.into();
  Ok(row)
}

fn csv_field(s: &str) -> String {
  if s.contains([',', '"', '\n']) {
    format!("\"{}\"", s.replace('"', "\"\""))
  } else {
    s.to_string()
  }
}

fn run(config: RunConfig, tasks: Vec<Task>, out: &Path, jobs: usize) -> Result<(), String> {
  let fresh = !out.exists() || std::fs::metadata(out).map(|m| m.len() == 0).unwrap_or(true);
  let mut file = std::fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open(out)
    .map_err(|e| format!("{}: {e}", out.display()))?;
  if fresh {
    writeln!(file, "{CSV_HEADER}").map_err(|e| e.to_string())?;
  }
  if let Some(dir) = &config.transcripts {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
  }
  let total = tasks.len();
  let queue = Arc::new(Mutex::new(tasks.into_iter().rev().collect::<Vec<_>>()));
  let file = Arc::new(Mutex::new(file));
  let passed = Arc::new(AtomicUsize::new(0));
  let config = Arc::new(config);
  let workers: Vec<_> = (0..jobs.max(1))
    .map(|_| {
      let (queue, file, passed, config) =
        (queue.clone(), file.clone(), passed.clone(), config.clone());
      std::thread::spawn(move || -> Result<(), String> {
        loop {
          let Some(task) = queue.lock().unwrap().pop() else {
            return Ok(());
          };
          let mut log = String::new();
          let row = run_task(&config, &task, &mut log)?;
          if let Some(dir) = &config.transcripts {
            std::fs::write(dir.join(format!("{}.md", task.id)), &log).map_err(|e| e.to_string())?;
          }
          if row.semantic {
            passed.fetch_add(1, Ordering::Relaxed);
          }
          let line = [
            csv_field(&task.id),
            csv_field(&config.model),
            csv_field(&config.variant),
            row.semantic.to_string(),
            row.first_pass.to_string(),
            row.repairs.to_string(),
            row.tokens.map_or(String::new(), |t| t.to_string()),
            row.latency.as_millis().to_string(),
            row.failure.clone(),
          ]
          .join(",");
          eprintln!("{line}");
          let mut f = file.lock().unwrap();
          writeln!(f, "{line}").map_err(|e| e.to_string())?;
          f.flush().map_err(|e| e.to_string())?;
        }
      })
    })
    .collect();
  for w in workers {
    w.join().map_err(|_| "worker panicked".to_string())??;
  }
  eprintln!("{} of {total} tasks passed", passed.load(Ordering::Relaxed));
  Ok(())
}

struct Args {
  command: String,
  flags: std::collections::HashMap<String, String>,
}

fn parse_args() -> Result<Args, String> {
  let mut args = std::env::args().skip(1);
  let command = args.next().ok_or("missing command")?;
  let mut flags = std::collections::HashMap::new();
  while let Some(flag) = args.next() {
    let name = flag
      .strip_prefix("--")
      .ok_or(format!("unexpected argument {flag}"))?;
    let value = args.next().ok_or(format!("{flag} needs a value"))?;
    flags.insert(name.to_string(), value);
  }
  Ok(Args { command, flags })
}

fn main_inner() -> Result<bool, String> {
  let args = parse_args()?;
  let flag = |n: &str| args.flags.get(n).map(String::as_str);
  let known: &[&str] = match args.command.as_str() {
    "verify" => &["shards2"],
    "reference" => &["shards2", "variant"],
    "run" => &[
      "shards2",
      "variant",
      "model",
      "cmd",
      "out",
      "format",
      "rounds",
      "jobs",
      "only",
      "transcripts",
    ],
    other => return Err(format!("unknown command {other}")),
  };
  if let Some(unknown) = args.flags.keys().find(|k| !known.contains(&k.as_str())) {
    return Err(format!("unknown option --{unknown}"));
  }
  let bin = shards2_path(flag("shards2"))?;
  let variant = flag("variant").unwrap_or("current").to_string();
  match args.command.as_str() {
    "verify" => verify(&bin),
    "reference" => {
      println!("{}", reference(&bin, &variant)?);
      Ok(true)
    }
    _ => {
      let number = |n: &str, default: usize| {
        flag(n).map_or(Ok(default), |v| {
          v.parse().map_err(|_| format!("--{n} takes a number"))
        })
      };
      let format = match flag("format").unwrap_or("text") {
        "text" => Format::Text,
        "claude-json" => Format::ClaudeJson,
        other => return Err(format!("unknown format {other}")),
      };
      let config = RunConfig {
        model: flag("model").ok_or("--model is required")?.to_string(),
        cmd: flag("cmd").ok_or("--cmd is required")?.to_string(),
        format,
        reference: reference(&bin, &variant)?,
        variant,
        rounds: number("rounds", 5)?,
        bin,
        transcripts: flag("transcripts").map(PathBuf::from),
      };
      let tasks = load_tasks(flag("only"))?;
      if tasks.is_empty() {
        return Err("no tasks selected".into());
      }
      let out = PathBuf::from(flag("out").ok_or("--out is required")?);
      run(config, tasks, &out, number("jobs", 4)?)?;
      Ok(true)
    }
  }
}

fn main() -> ExitCode {
  match main_inner() {
    Ok(true) => ExitCode::SUCCESS,
    Ok(false) => ExitCode::FAILURE,
    Err(e) => {
      eprintln!("authoring-eval: {e}\n{USAGE}");
      ExitCode::from(2)
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn log_matching_strips_only_outcome_lines() {
    let expected = vec!["1".to_string(), "2".to_string()];
    let names = outcome_names("@mesh(m) @schedule(m main) @run(m)");
    assert!(log_matches("1\n2\n", &expected, &names));
    assert!(log_matches(
      "1\n2\nroot: 2\nmain: stopped\n",
      &expected,
      &names
    ));
    assert!(!log_matches("1\n2\n3\n", &expected, &names));
    assert!(!log_matches("1\n2\nroot: 2\nroot: 2\n", &expected, &names));
    assert!(!log_matches("1\n2\nother: 2\n", &expected, &names));
    assert!(!log_matches("1\n", &expected, &names));
  }

  #[test]
  fn program_is_the_last_fenced_block() {
    assert_eq!(extract_program("x\n```shards\n1 | Log\n```\n"), "1 | Log\n");
    assert_eq!(extract_program("```\na\n```\n```shards\nb\n```"), "b\n");
    assert_eq!(extract_program("1 | Log"), "1 | Log\n");
  }

  /// Every reference solution runs and logs exactly the expected lines,
  /// so the tasks stay solvable as the language changes.
  #[test]
  fn reference_solutions_pass() {
    let catalog = shards_core::Catalog::new(&[shards_core::shards::CATALOG]).unwrap();
    for task in load_tasks(None).unwrap() {
      let source = shards_lang::Source::new(&task.id, task.reference.clone().unwrap());
      let program = shards_lang::Program::load(source, &catalog, &Default::default())
        .unwrap_or_else(|(_, d)| panic!("{}: {d:?}", task.id));
      let (report, lines) =
        shards_core::log::capture(|| program.run::<shards_core::Mesh>().unwrap());
      assert!(report.succeeded(), "{}", task.id);
      assert_eq!(lines, task.expected, "{}", task.id);
    }
  }

  #[test]
  fn every_task_parses() {
    let tasks = load_tasks(None).unwrap();
    assert!(tasks.len() >= 30);
    assert!(
      tasks
        .iter()
        .all(|t| !t.expected.is_empty() && t.reference.is_some())
    );
  }
}
