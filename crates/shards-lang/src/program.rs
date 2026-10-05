//! Loading, checking and running a source program on either scheduler.
//!
//! `check` reports every problem it can, located in the source: syntax
//! problems first (lowering a broken tree only cascades), then lowering and
//! compose problems. Compose diagnostics carry occurrence paths; the source
//! map turns them into file, line and column.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use shards_core::diagnostic::{Diagnostic, Phase, json_str};
use shards_core::{Catalog, Error, InstanceId, Outcome, Type, Var, WireDef};

use crate::lower::{Lowered, ROOT_WIRE, lower};
use crate::parser::parse;
use crate::problem::locate;
use crate::source::Source;

/// A scheduler's mesh, as the frontend drives it. Implemented for both.
pub trait Host {
  type Wire;
  fn create() -> Self;
  fn add_wire(&mut self, def: WireDef);
  fn compile(&mut self, name: &str) -> shards_core::Result<Self::Wire>;
  fn spawn(&mut self, wire: &Self::Wire) -> shards_core::Result<InstanceId>;
  fn tick(&mut self) -> usize;
  fn running(&self) -> usize;
  fn take_outcome(&mut self, id: InstanceId) -> Option<Outcome>;
  /// Removes every finished record in one pass: id, wire, outcome.
  fn take_finished(&mut self) -> Vec<(InstanceId, String, Outcome)>;
}

macro_rules! host {
  ($mesh:ty, $backend:ty) => {
    impl Host for $mesh {
      type Wire = std::sync::Arc<shards_core::CompiledWire<$backend>>;
      fn create() -> Self {
        <$mesh>::new()
      }
      fn add_wire(&mut self, def: WireDef) {
        <$mesh>::add_wire(self, def)
      }
      fn compile(&mut self, name: &str) -> shards_core::Result<Self::Wire> {
        <$mesh>::compile(self, name, Type::none())
      }
      fn spawn(&mut self, wire: &Self::Wire) -> shards_core::Result<InstanceId> {
        <$mesh>::spawn(self, wire, Var::None)
      }
      fn tick(&mut self) -> usize {
        <$mesh>::tick(self)
      }
      fn running(&self) -> usize {
        <$mesh>::running(self)
      }
      fn take_outcome(&mut self, id: InstanceId) -> Option<Outcome> {
        <$mesh>::take_outcome(self, id)
      }
      fn take_finished(&mut self) -> Vec<(InstanceId, String, Outcome)> {
        <$mesh>::take_finished(self)
      }
    }
  };
}

host!(shards_core::Mesh, shards_core::Stackless);
#[cfg(not(any(target_family = "wasm", target_os = "espidf")))]
host!(shards_core::StackfulMesh, shards_core::Stackful);

/// A source program, parsed and lowered.
pub struct Program {
  pub source: Source,
  pub lowered: Lowered,
}

/// The result of `check`, in the 1.x `shards check --json` envelope.
pub struct CheckReport {
  pub file: String,
  pub diagnostics: Vec<Diagnostic>,
}

impl CheckReport {
  pub fn ok(&self) -> bool {
    self.diagnostics.is_empty()
  }

  pub fn to_json(&self) -> String {
    let diagnostics: Vec<String> = self.diagnostics.iter().map(Diagnostic::to_json).collect();
    format!(
      "{{\"ok\":{},\"file\":{},\"diagnostics\":[{}]}}",
      self.ok(),
      json_str(&self.file),
      diagnostics.join(",")
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
  fn diagnostic(&self, wire: &str, err: Error) -> Diagnostic {
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
  fn unreachable_roots(&self) -> Vec<String> {
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
  pub fn compose<H: Host>(&self) -> Vec<Diagnostic> {
    let mut mesh = H::create();
    for def in &self.lowered.wires {
      mesh.add_wire(def.clone());
    }
    let mut out: Vec<Diagnostic> = Vec::new();
    for wire in self.entries().into_iter().chain(self.unreachable_roots()) {
      if let Err(err) = mesh.compile(&wire) {
        let d = self.diagnostic(&wire, err);
        if !out.contains(&d) {
          out.push(d);
        }
      }
    }
    out
  }

  /// Runs the program on `H`: the entry wires, ticked at the `@run` rate
  /// (as fast as possible without one) until every instance finishes or
  /// the `Iterations` limit is reached.
  pub fn run<H: Host>(&self) -> Result<RunReport, Vec<Diagnostic>> {
    let mut mesh = H::create();
    for def in &self.lowered.wires {
      mesh.add_wire(def.clone());
    }
    let mut instances = Vec::new();
    let mut errors = Vec::new();
    for wire in self.entries() {
      match mesh.compile(&wire) {
        Ok(compiled) => match mesh.spawn(&compiled) {
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
/// running when the `Iterations` limit stopped the mesh).
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

/// Checks a source: syntax, lowering and compose, on scheduler `H`.
pub fn check<H: Host>(
  source: Source,
  catalog: &Catalog,
  defines: &HashMap<String, String>,
) -> CheckReport {
  let file = source.name.clone();
  match Program::load(source, catalog, defines) {
    Err((_, diagnostics)) => CheckReport { file, diagnostics },
    Ok(program) => CheckReport {
      file,
      diagnostics: program.compose::<H>(),
    },
  }
}
