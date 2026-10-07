//! Loading, checking and running a source program on a mesh.
//!
//! `check` reports every problem it can, located in the source: syntax
//! problems first (lowering a broken tree only cascades), then lowering and
//! compose problems. Compose diagnostics carry occurrence paths; the source
//! map turns them into file, line and column.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use shards_core::diagnostic::{Diagnostic, Phase, json_str};
use shards_core::diagnostic::{PathStep, path_json};
use shards_core::signature::{Analysis, Occurrence};
use shards_core::{Catalog, Error, InstanceId, Mesh, Outcome, Type, Var};

use crate::lower::{Lowered, ROOT_WIRE, lower};
use crate::parser::parse;
use crate::problem::locate;
use crate::source::Source;

/// A source program, parsed and lowered.
pub struct Program {
  pub source: Source,
  pub lowered: Lowered,
}

/// The result of `check`, in the 1.x `shards check --json` envelope.
pub struct CheckReport {
  pub file: String,
  pub diagnostics: Vec<Diagnostic>,
  /// Successful root composes, including nested occurrences. Failed roots
  /// report diagnostics rather than presenting a partial analysis as complete.
  pub wires: Vec<WireAnalysis>,
}

pub struct WireAnalysis {
  pub name: String,
  pub analysis: Analysis,
  pub occurrences: Vec<LocatedOccurrence>,
}

pub struct LocatedOccurrence {
  pub occurrence: Occurrence,
  pub span: Option<crate::Span>,
  pub line: Option<u32>,
  pub column: Option<u32>,
}

impl WireAnalysis {
  fn to_json(&self) -> String {
    let occurrences = self
      .occurrences
      .iter()
      .map(|o| {
        let mut fields = vec![
          format!("\"path\":{}", path_json(&o.occurrence.path)),
          format!("\"input\":{}", json_str(&o.occurrence.input.to_string())),
          format!("\"output\":{}", json_str(&o.occurrence.output.to_string())),
          format!("\"effects\":{}", o.occurrence.effects.to_json()),
          format!("\"lifetime\":{}", json_str(o.occurrence.lifetime.name())),
        ];
        if let Some(span) = o.span {
          fields.push(format!(
            "\"span\":{{\"start\":{},\"end\":{}}}",
            span.start, span.end
          ));
        }
        if let (Some(line), Some(column)) = (o.line, o.column) {
          fields.push(format!("\"line\":{line},\"column\":{column}"));
        }
        format!("{{{}}}", fields.join(","))
      })
      .collect::<Vec<_>>()
      .join(",");
    format!(
      "{{\"wire\":{},\"effects\":{},\"lifetime\":{},\"uses\":{},\"mutates\":{},\"occurrences\":[{occurrences}]}}",
      json_str(&self.name),
      self.analysis.effects.to_json(),
      json_str(self.analysis.lifetime.name()),
      Analysis::mesh_json(&self.analysis.uses),
      Analysis::mesh_json(&self.analysis.mutates)
    )
  }
}

impl CheckReport {
  pub fn ok(&self) -> bool {
    self.diagnostics.is_empty()
  }

  pub fn to_json(&self) -> String {
    let diagnostics: Vec<String> = self.diagnostics.iter().map(Diagnostic::to_json).collect();
    format!(
      "{{\"ok\":{},\"file\":{},\"diagnostics\":[{}],\"wires\":[{}]}}",
      self.ok(),
      json_str(&self.file),
      diagnostics.join(","),
      self
        .wires
        .iter()
        .map(WireAnalysis::to_json)
        .collect::<Vec<_>>()
        .join(",")
    )
  }
}

impl Program {
  /// Parses and lowers. Syntax problems stop before lowering.
  pub fn load(
    source: Source,
    catalog: &Catalog,
    defines: &HashMap<String, String>,
  ) -> Result<Program, (Source, Vec<Diagnostic>)> {
    let (tree, problems) = parse(&source);
    if !problems.is_empty() {
      let diagnostics = problems.iter().map(|p| p.to_diagnostic(&source)).collect();
      return Err((source, diagnostics));
    }
    let (lowered, problems) = lower(&tree, catalog, defines);
    if !problems.is_empty() {
      let diagnostics = problems.iter().map(|p| p.to_diagnostic(&source)).collect();
      return Err((source, diagnostics));
    }
    Ok(Program { source, lowered })
  }

  /// The wires that start when the program runs: those scheduled on the
  /// `@run` mesh, or the root wire.
  pub fn entries(&self) -> Vec<String> {
    match &self.lowered.run {
      Some(run) => self
        .lowered
        .meshes
        .iter()
        .filter(|m| m.name == run.mesh)
        .flat_map(|m| m.scheduled.iter().map(|(w, _)| w.clone()))
        .collect(),
      None if self.lowered.root => vec![ROOT_WIRE.to_string()],
      None => self
        .lowered
        .meshes
        .iter()
        .flat_map(|m| m.scheduled.iter().map(|(w, _)| w.clone()))
        .collect(),
    }
  }

  /// A compose error as a located diagnostic.
  pub(crate) fn diagnostic(&self, wire: &str, err: Error) -> Diagnostic {
    let mut d = match err {
      Error::Diagnostic(d) => *d,
      other => Diagnostic::new(
        Phase::Compose,
        "compose-error",
        "compose-error",
        other.to_string(),
      ),
    };
    let span = self.lowered.map.locate(&d).or_else(|| {
      let mut probe = Diagnostic::new(Phase::Compose, "", "", "");
      probe.path = vec![shards_core::diagnostic::PathStep::Wire(wire.to_string())];
      self.lowered.map.locate(&probe)
    });
    if let Some(span) = span {
      locate(&mut d, &self.source, span);
    } else {
      d.file = Some(self.source.name.clone());
    }
    if let Some(related) = &mut d.related {
      let mut probe = Diagnostic::new(Phase::Compose, "", "", "");
      probe.path = related.path.clone();
      if let Some(span) = self.lowered.map.locate(&probe) {
        let (line, column) = self.source.line_col(span.start);
        related.line = Some(line);
        related.column = Some(column);
      }
    }
    // `x-1` is one name in Shards; say so when it is the unknown variable.
    if d.code == "unknown-variable"
      && let Some(name) = d
        .message
        .strip_prefix("unknown variable ")
        .map(str::to_string)
      && let Some((_, tail)) = name.rsplit_once('-')
      && !tail.is_empty()
      && tail.chars().all(|c| c.is_ascii_digit())
    {
      d.message.push_str(&format!(
        "; `-` is part of names, so `{name}` is one name: subtract with `Math.Subtract` (`Sub`)"
      ));
    }
    d
  }

  /// The wires each wire references (`Do`, `Spawn`), by name.
  fn references(&self) -> HashMap<&str, Vec<String>> {
    fn refs(flow: &[shards_core::ShardDef], out: &mut Vec<String>) {
      for def in flow {
        for arg in &def.args {
          match &arg.value {
            shards_core::ParamValue::Wire(name) => out.push(name.clone()),
            shards_core::ParamValue::Flow(f) => refs(f, out),
            shards_core::ParamValue::Cases(cases) => cases.iter().for_each(|(_, f)| refs(f, out)),
            _ => {}
          }
        }
      }
    }
    self
      .lowered
      .wires
      .iter()
      .map(|w| {
        let mut out = Vec::new();
        refs(&w.flow, &mut out);
        (w.name.as_str(), out)
      })
      .collect()
  }

  /// Wires not reachable from the entries: they would only run if
  /// scheduled, with no input, so `check` composes them that way. One root
  /// per group: a wire reachable from an earlier unreachable root is
  /// composed through it (an unreachable cycle reports its recursion once).
  pub(crate) fn unreachable_roots(&self) -> Vec<String> {
    let refs = self.references();
    let mut seen: Vec<String> = Vec::new();
    let visit = |start: &str, seen: &mut Vec<String>| {
      let mut stack = vec![start.to_string()];
      while let Some(w) = stack.pop() {
        if seen.contains(&w) {
          continue;
        }
        if let Some(next) = refs.get(w.as_str()) {
          stack.extend(next.iter().cloned());
        }
        seen.push(w);
      }
    };
    for entry in self.entries() {
      visit(&entry, &mut seen);
    }
    let mut roots = Vec::new();
    for w in &self.lowered.wires {
      if !seen.contains(&w.name) {
        roots.push(w.name.clone());
        visit(&w.name, &mut seen);
      }
    }
    roots
  }

  /// Composes every wire: the entries (wires they reach through `Do` and
  /// `Spawn` compose with them), then one root per unreachable group.
  /// Returns diagnostics without materializing tooling occurrence reports;
  /// use `analyze` when those source-located reports are needed.
  pub fn compose(&self) -> Vec<Diagnostic> {
    self.compose_report(false).diagnostics
  }

  pub fn analyze(&self) -> CheckReport {
    self.compose_report(true)
  }

  fn compose_report(&self, include_analysis: bool) -> CheckReport {
    let mut mesh = Mesh::new();
    for def in &self.lowered.wires {
      mesh.add_wire(def.clone());
    }
    let mut out: Vec<Diagnostic> = Vec::new();
    let mut wires = Vec::new();
    for wire in self.entries().into_iter().chain(self.unreachable_roots()) {
      match mesh.compile(&wire, Type::none()) {
        Err(err) => {
          let d = self.diagnostic(&wire, err);
          if !out.contains(&d) {
            out.push(d);
          }
        }
        Ok(compiled) if include_analysis => {
          let analysis = compiled.flow.analysis.clone();
          let occurrences = analysis
            .occurrences
            .iter()
            .map(|mut occurrence| {
              occurrence.path.insert(0, PathStep::Wire(wire.clone()));
              let mut d = Diagnostic::new(Phase::Compose, "", "", "");
              d.path = occurrence.path.clone();
              let span = self.lowered.map.locate(&d);
              let position = span.map(|s| self.source.line_col(s.start));
              LocatedOccurrence {
                occurrence,
                span,
                line: position.map(|p| p.0),
                column: position.map(|p| p.1),
              }
            })
            .collect();
          wires.push(WireAnalysis {
            name: wire,
            analysis,
            occurrences,
          });
        }
        Ok(_) => {}
      }
    }
    CheckReport {
      file: self.source.name.clone(),
      diagnostics: out,
      wires,
    }
  }

  /// Runs the program on a fresh mesh: the entry wires, ticked at the
  /// `@run` rate (as fast as possible without one) until every instance
  /// finishes or the `iterations` limit is reached.
  pub fn run(&self) -> Result<RunReport, Vec<Diagnostic>> {
    let mut mesh = Mesh::new();
    for def in &self.lowered.wires {
      mesh.add_wire(def.clone());
    }
    let entries = self.entries();
    mesh.reserve_instances(entries.len());
    let mut instances = Vec::with_capacity(entries.len());
    let mut errors = Vec::new();
    for wire in entries {
      match mesh.compile(&wire, Type::none()) {
        Ok(compiled) => match mesh.spawn(&compiled, Var::None) {
          Ok(id) => instances.push((wire, id)),
          Err(err) => errors.push(self.diagnostic(&wire, err)),
        },
        Err(err) => errors.push(self.diagnostic(&wire, err)),
      }
    }
    if !errors.is_empty() {
      return Err(errors);
    }
    let run = self.lowered.run.as_ref();
    // Lowering only accepts FPS values with a representable frame interval.
    let frame = run
      .and_then(|r| r.fps)
      .and_then(|fps| Duration::try_from_secs_f64(1.0 / fps).ok());
    let iterations = run.and_then(|r| r.iterations);
    let entry_ids: std::collections::HashSet<InstanceId> =
      instances.iter().map(|(_, id)| *id).collect();
    // Outcomes of finished entry instances, kept here so the mesh can drop
    // every finished record each tick.
    let mut entry_outcomes: HashMap<InstanceId, Outcome> = HashMap::new();
    let mut spawned = Vec::new();
    let mut ticks = 0i64;
    let mut next = Instant::now();
    loop {
      mesh.tick();
      ticks += 1;
      // Retire every finished instance each tick (long-running programs
      // spawn many): one pass over the records, hashed lookups. Entries'
      // outcomes are kept for the report, and spawned failures.
      for (id, wire, outcome) in mesh.take_finished() {
        if entry_ids.contains(&id) {
          entry_outcomes.insert(id, outcome);
        } else if matches!(outcome, Outcome::Failed(_)) {
          spawned.push((wire, outcome));
        }
      }
      if mesh.running() == 0 || iterations.is_some_and(|n| ticks >= n) {
        break;
      }
      if let Some(frame) = frame {
        next = next.checked_add(frame).unwrap_or_else(Instant::now);
        let now = Instant::now();
        if next > now {
          std::thread::sleep(next - now);
        } else {
          next = now;
        }
      }
    }
    let outcomes = instances
      .into_iter()
      .map(|(wire, id)| (wire, entry_outcomes.remove(&id)))
      .collect();
    Ok(RunReport {
      ticks,
      outcomes,
      spawned_failures: spawned,
    })
  }
}

/// What a run did: ticks, and each entry wire's outcome (`None`: still
/// running when the `iterations` limit stopped the mesh).
#[derive(Debug)]
pub struct RunReport {
  pub ticks: i64,
  pub outcomes: Vec<(String, Option<Outcome>)>,
  /// Spawned instances that failed: their wire and outcome.
  pub spawned_failures: Vec<(String, Outcome)>,
}

impl RunReport {
  /// Whether no entry wire and no spawned instance failed.
  pub fn succeeded(&self) -> bool {
    self.spawned_failures.is_empty()
      && !self
        .outcomes
        .iter()
        .any(|(_, o)| matches!(o, Some(Outcome::Failed(_))))
  }
}

/// Checks a source: syntax, lowering and compose.
pub fn check(source: Source, catalog: &Catalog, defines: &HashMap<String, String>) -> CheckReport {
  let file = source.name.clone();
  match Program::load(source, catalog, defines) {
    Err((_, diagnostics)) => CheckReport {
      file,
      diagnostics,
      wires: Vec::new(),
    },
    Ok(program) => program.analyze(),
  }
}
