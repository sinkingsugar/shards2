//! `shards2`: check, run and describe Shards programs.
//!
//! ```text
//! shards2 check [--json] [--stackful] <file> [key:value ...]
//! shards2 run [--stackful] <file> [key:value ...]
//! shards2 watch [--stackful] <file> [key:value ...]
//! shards2 describe <shard>
//! shards2 search <text>
//! shards2 catalog
//! ```
//!
//! `check --json` prints the 1.x `shards check --json` envelope
//! (`{ok, file, diagnostics}`); `key:value` arguments are the script's
//! `@key` values. Exit codes: 0 success, 1 problems or a failed run, 2
//! usage.

use std::collections::HashMap;
use std::process::ExitCode;

use shards_core::{Catalog, Outcome};
use shards_lang::{CheckReport, Host, Program, Source, render};

mod watch;

const USAGE: &str = "usage:
  shards2 check [--json] [--stackful] <file> [key:value ...]
  shards2 run [--stackful] <file> [key:value ...]
  shards2 watch [--stackful] <file> [key:value ...]
  shards2 describe <shard>
  shards2 search <text>
  shards2 catalog";

fn catalog() -> Catalog {
  Catalog::new(&[shards_core::shards::CATALOG, shards_io::CATALOG]).expect("catalog")
}

struct Options {
  json: bool,
  stackful: bool,
  file: String,
  defines: HashMap<String, String>,
}

fn options(args: &[String]) -> Result<Options, String> {
  let mut json = false;
  let mut stackful = false;
  let mut file = None;
  let mut defines = HashMap::new();
  for arg in args {
    match arg.as_str() {
      "--json" => json = true,
      "--stackful" => stackful = true,
      a if a.starts_with("--") => return Err(format!("unknown option {a}")),
      a => match script_argument(a) {
        // In any order: `check a:b file.shs` and `check file.shs a:b`.
        Some((k, v)) => {
          defines.insert(k.to_string(), v.to_string());
        }
        None if file.is_none() => file = Some(a.to_string()),
        None => return Err(format!("script arguments are key:value, got {a}")),
      },
    }
  }
  Ok(Options {
    json,
    stackful,
    file: file.ok_or("missing file")?,
    defines,
  })
}

/// `key:value` with a name-like key. A Windows drive path (`C:\\x`,
/// `C:/x`) is a file, not an argument.
fn script_argument(arg: &str) -> Option<(&str, &str)> {
  let (k, v) = arg.split_once(':')?;
  let name_like = k.starts_with(|c: char| c.is_ascii_alphabetic())
    && k
      .chars()
      .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
  let drive = k.len() == 1 && (v.starts_with('\\') || v.starts_with('/'));
  (name_like && !drive).then_some((k, v))
}

fn read(file: &str) -> Result<Source, String> {
  std::fs::read_to_string(file)
    .map(|text| Source::new(file, text))
    .map_err(|e| format!("{file}: {e}"))
}

fn print_report(report: &CheckReport, source: &Source, json: bool) {
  if json {
    println!("{}", report.to_json());
  } else {
    for d in &report.diagnostics {
      eprint!("{}", render(d, source));
    }
  }
}

fn check(o: &Options) -> Result<ExitCode, String> {
  let source = read(&o.file)?;
  let copy = Source::new(source.name.clone(), source.text.clone());
  let report = if o.stackful {
    shards_lang::check::<shards_core::StackfulMesh>(source, &catalog(), &o.defines)
  } else {
    shards_lang::check::<shards_core::Mesh>(source, &catalog(), &o.defines)
  };
  print_report(&report, &copy, o.json);
  if !o.json && report.ok() {
    eprintln!("{}: ok", o.file);
  }
  Ok(if report.ok() {
    ExitCode::SUCCESS
  } else {
    ExitCode::FAILURE
  })
}

fn run(o: &Options) -> Result<ExitCode, String> {
  let source = read(&o.file)?;
  let program = match Program::load(source, &catalog(), &o.defines) {
    Ok(p) => p,
    Err((source, diagnostics)) => {
      for d in &diagnostics {
        eprint!("{}", render(d, &source));
      }
      return Ok(ExitCode::FAILURE);
    }
  };
  if o.stackful {
    run_on::<shards_core::StackfulMesh>(&program)
  } else {
    run_on::<shards_core::Mesh>(&program)
  }
}

fn run_on<H: Host>(program: &Program) -> Result<ExitCode, String> {
  let report = match program.run::<H>() {
    Ok(r) => r,
    Err(diagnostics) => {
      for d in &diagnostics {
        eprint!("{}", render(d, &program.source));
      }
      return Ok(ExitCode::FAILURE);
    }
  };
  for (wire, outcome) in &report.outcomes {
    match outcome {
      Some(Outcome::Completed(v)) => println!("{wire}: {v}"),
      Some(Outcome::Stopped) => println!("{wire}: stopped"),
      Some(Outcome::Cancelled) => println!("{wire}: cancelled"),
      Some(Outcome::Failed(e)) => eprintln!("{wire}: failed: {e}"),
      None => println!("{wire}: still running after {} ticks", report.ticks),
    }
  }
  for (wire, outcome) in &report.spawned_failures {
    if let Outcome::Failed(e) = outcome {
      eprintln!("{wire} (spawned): failed: {e}");
    }
  }
  Ok(if report.succeeded() {
    ExitCode::SUCCESS
  } else {
    ExitCode::FAILURE
  })
}

fn main() -> ExitCode {
  let args: Vec<String> = std::env::args().skip(1).collect();
  let result = match args.first().map(String::as_str) {
    Some("check") => options(&args[1..]).and_then(|o| check(&o)),
    Some("run") => options(&args[1..]).and_then(|o| run(&o)),
    Some("watch") => options(&args[1..]).and_then(|o| {
      if o.json {
        return Err("watch does not support --json".into());
      }
      if o.stackful {
        watch::watch::<shards_core::StackfulMesh>(&o)
      } else {
        watch::watch::<shards_core::Mesh>(&o)
      }
    }),
    Some("describe") if args.len() == 2 => match catalog().describe_json(&args[1]) {
      Some(json) => {
        println!("{json}");
        Ok(ExitCode::SUCCESS)
      }
      None => {
        let c = catalog();
        let names: Vec<String> = c.names().iter().map(|n| n.to_string()).collect();
        let near = shards_lang::problem::closest(&args[1], names, 3);
        let hint = if near.is_empty() {
          String::new()
        } else {
          format!(" (did you mean: {})", near.join(", "))
        };
        Err(format!("no shard named {}{hint}", args[1]))
      }
    },
    Some("search") if args.len() == 2 => {
      for s in catalog().search(&args[1]) {
        println!("{}: {}", s.name(), s.desc.summary);
      }
      Ok(ExitCode::SUCCESS)
    }
    Some("catalog") if args.len() == 1 => {
      println!("{}", catalog().index_json());
      Ok(ExitCode::SUCCESS)
    }
    _ => {
      eprintln!("{USAGE}");
      return ExitCode::from(2);
    }
  };
  result.unwrap_or_else(|e| {
    eprintln!("shards2: {e}");
    ExitCode::from(2)
  })
}
