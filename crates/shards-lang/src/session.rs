//! Host-driven execution with transactional source replacement.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use shards_core::diagnostic::Diagnostic;
use shards_core::{Catalog, InstanceId, Outcome};

use crate::{Host, Program, Source};

/// A host whose instances can all be cancelled before a replacement starts.
///
/// Like the built-in meshes, implementations must defer shard instantiation
/// and activation until `tick`, cancel every child in `cancel_all`, retain
/// cleanup failures as outcomes, and cancel remaining work when dropped.
pub trait SessionHost: Host {
  fn cancel_all(&mut self);
}

impl SessionHost for shards_core::Mesh {
  fn cancel_all(&mut self) {
    self.cancel_all();
  }
}

#[cfg(not(any(target_family = "wasm", target_os = "espidf")))]
impl SessionHost for shards_core::StackfulMesh {
  fn cancel_all(&mut self) {
    self.cancel_all();
  }
}

/// An entry or spawned instance that finished, failed or was cancelled.
/// Returned once; a session does not accumulate a history of outcomes.
#[derive(Debug)]
pub struct Finished {
  pub wire: String,
  pub outcome: Outcome,
}

struct Entry<W> {
  name: String,
  wire: W,
  id: InstanceId,
}

struct Execution<H: Host> {
  mesh: H,
  entries: Vec<Entry<H::Wire>>,
  failed: HashSet<InstanceId>,
  ticks: u64,
  iterations: Option<u64>,
  frame_interval: Option<Duration>,
}

/// A reloadable program on a host-owned event loop (stackless by default).
///
/// `reload` validates all wires and explicitly restarts the whole mesh.
/// `reload_preserving` (with a [`ReloadHost`]) retains compatible callers,
/// locals and mesh values, selecting nested Do bodies at call boundaries.
/// Rejected edits leave execution untouched. Host services live outside this
/// object and survive either mode. Each revision owns its compose cache.
///
/// Calls are synchronous and require exclusive access: reload between ticks.
/// Compilation can delay the host loop. Cancellation drops pending futures;
/// external work still requires cooperative cancellation by the host shard.
pub struct Session<H: SessionHost = shards_core::Mesh> {
  active: Option<Execution<H>>,
}

impl<H: SessionHost> Default for Session<H> {
  fn default() -> Self {
    Self::new()
  }
}

impl<H: SessionHost> Session<H> {
  pub fn new() -> Self {
    Self { active: None }
  }

  /// Starts with a host-configured, idle mesh. Use `reload_preserving` to
  /// keep its declared mesh variables across source revisions.
  pub fn with_mesh(mesh: H) -> Self {
    assert_eq!(mesh.running(), 0, "Session requires an idle mesh");
    Self {
      active: Some(Execution {
        mesh,
        entries: Vec::new(),
        failed: HashSet::new(),
        ticks: 0,
        iterations: None,
        frame_interval: None,
      }),
    }
  }

  /// Parses, lowers, composes every wire and schedules the entries on a fresh
  /// mesh. No candidate shard runs until a subsequent `tick`.
  ///
  /// On success returns the old execution's cancellation outcomes, including
  /// cleanup failures. Those failures do not roll back the replacement.
  /// Load/compose/scheduling errors return the rejected source and diagnostics
  /// and preserve the old execution. Runtime failures after commit are tick
  /// outcomes, not reload errors.
  pub fn reload(
    &mut self,
    source: Source,
    catalog: &Catalog,
    defines: &HashMap<String, String>,
  ) -> Result<Vec<Finished>, (Source, Vec<Diagnostic>)> {
    let program = Program::load(source, catalog, defines)?;
    let mut mesh = H::create();
    for def in &program.lowered.wires {
      mesh.add_wire(def.clone());
    }
    let entries = match compose_entries(&program, &mut mesh) {
      Ok(entries) => entries,
      Err(diagnostics) => return Err((program.source, diagnostics)),
    };
    let mut scheduled = Vec::new();
    for (name, wire) in entries {
      match mesh.spawn(&wire) {
        Ok(id) => scheduled.push(Entry { name, wire, id }),
        Err(err) => {
          let diagnostic = program.diagnostic(&name, err);
          return Err((program.source, vec![diagnostic]));
        }
      }
    }
    let run = program.lowered.run.as_ref();
    let next = Execution {
      mesh,
      entries: scheduled,
      failed: HashSet::new(),
      ticks: 0,
      iterations: run.and_then(|r| r.iterations).map(|n| n as u64),
      frame_interval: run
        .and_then(|r| r.fps)
        .and_then(|fps| Duration::try_from_secs_f64(1.0 / fps).ok()),
    };
    let finished = self.stop();
    self.active = Some(next);
    Ok(finished)
  }

  /// Runs one scheduler tick without sleeping, and retires all finished
  /// records. At `@run`'s iteration limit, cancels remaining instances and
  /// reports their outcomes in this same call. Idle calls are no-ops.
  pub fn tick(&mut self) -> Vec<Finished> {
    let Some(active) = &mut self.active else {
      return Vec::new();
    };
    if active.mesh.running() == 0 {
      return Vec::new();
    }
    active.mesh.tick();
    active.ticks = active.ticks.saturating_add(1);
    if active.iterations.is_some_and(|n| active.ticks >= n) {
      active.mesh.cancel_all();
    }
    let entries: HashSet<_> = active.entries.iter().map(|e| e.id).collect();
    active
      .mesh
      .take_finished()
      .into_iter()
      .map(|(id, wire, outcome)| {
        if entries.contains(&id) && matches!(outcome, Outcome::Failed(_)) {
          active.failed.insert(id);
        }
        Finished { wire, outcome }
      })
      .collect()
  }

  /// Cancels all entries and children, reports their outcomes and releases
  /// the revision's mesh and compose cache. Safe to call repeatedly.
  /// Dropping the session also cancels work, but discards the outcomes.
  pub fn stop(&mut self) -> Vec<Finished> {
    let Some(mut active) = self.active.take() else {
      return Vec::new();
    };
    active.mesh.cancel_all();
    drain(&mut active.mesh)
  }

  pub fn running(&self) -> usize {
    self.active.as_ref().map_or(0, |a| a.mesh.running())
  }

  /// Ticks in the current revision; resets on a successful reload or stop.
  pub fn ticks(&self) -> u64 {
    self.active.as_ref().map_or(0, |a| a.ticks)
  }

  /// Suggested pacing from `@run(FPS:)`. The host chooses when to call tick;
  /// `None` means no rate was requested. Reload can change this value.
  pub fn frame_interval(&self) -> Option<Duration> {
    self.active.as_ref().and_then(|a| a.frame_interval)
  }
}

fn drain<H: Host>(mesh: &mut H) -> Vec<Finished> {
  mesh
    .take_finished()
    .into_iter()
    .map(|(_, wire, outcome)| Finished { wire, outcome })
    .collect()
}

/// A mesh supporting checked, call-boundary replacement. Candidates must be
/// isolated from running instances; commit may only install a successfully
/// validated candidate. Scheduling a wire compiled by that candidate must
/// not fail after commit (except unrecoverable allocation failure).
pub trait ReloadHost: SessionHost {
  fn revision(&self) -> Self;
  fn can_retain(&self, wire: &Self::Wire) -> bool;
  fn validate_reload(
    &self,
    next: &mut Self,
    removed: &HashSet<InstanceId>,
  ) -> shards_core::Result<()>;
  fn install_revision(&mut self, next: Self, removed: &HashSet<InstanceId>);
}

macro_rules! reload_host {
  ($mesh:ty) => {
    impl ReloadHost for $mesh {
      fn revision(&self) -> Self {
        <$mesh>::revision(self)
      }
      fn can_retain(&self, wire: &Self::Wire) -> bool {
        <$mesh>::can_retain(self, wire)
      }
      fn validate_reload(
        &self,
        next: &mut Self,
        removed: &HashSet<InstanceId>,
      ) -> shards_core::Result<()> {
        <$mesh>::validate_reload(self, next, removed)
      }
      fn install_revision(&mut self, next: Self, removed: &HashSet<InstanceId>) {
        <$mesh>::install_revision(self, next, removed)
      }
    }
  };
}
reload_host!(shards_core::Mesh);
#[cfg(not(any(target_family = "wasm", target_os = "espidf")))]
reload_host!(shards_core::StackfulMesh);

impl<H: ReloadHost> Session<H> {
  /// Retains unchanged callers and their locals, Once state and suspended
  /// execution. Changed Do bodies take effect on their next call; a call
  /// already in flight pins its body until it returns. Compatible mesh
  /// variables remain in the same frame. Changes to a root's own definition
  /// or static dependencies restart that root; removed roots are cancelled.
  ///
  /// An incompatible Do interface or binding layout rejects the whole edit.
  /// Use `reload` for an explicit full restart. Unchanged completed entries
  /// stay finished; failed entries retry after an accepted reload.
  pub fn reload_preserving(
    &mut self,
    source: Source,
    catalog: &Catalog,
    defines: &HashMap<String, String>,
  ) -> Result<Vec<Finished>, (Source, Vec<Diagnostic>)> {
    if self.active.is_none() {
      return self.reload(source, catalog, defines);
    }
    let program = Program::load(source, catalog, defines)?;
    let active = self.active.as_mut().expect("active mesh");
    let mut candidate = active.mesh.revision();
    for def in &program.lowered.wires {
      candidate.add_wire(def.clone());
    }
    let entries = match compose_entries(&program, &mut candidate) {
      Ok(entries) => entries,
      Err(diagnostics) => return Err((program.source, diagnostics)),
    };
    // Match scheduled occurrences, not only names (one wire may be scheduled
    // more than once). Finished entry IDs remain here after record retirement.
    let mut retained = HashSet::new();
    let mut plan = Vec::new();
    for (name, wire) in entries {
      let old = active.entries.iter().find(|entry| {
        entry.name == name
          && !retained.contains(&entry.id)
          && !active.failed.contains(&entry.id)
          && candidate.can_retain(&entry.wire)
      });
      let id = old.map(|entry| entry.id);
      if let Some(id) = id {
        retained.insert(id);
      }
      plan.push((name, wire, id));
    }
    let removed = active
      .entries
      .iter()
      .map(|e| e.id)
      .filter(|id| !retained.contains(id))
      .collect();
    if let Err(err) = active.mesh.validate_reload(&mut candidate, &removed) {
      let d = program.diagnostic("", err);
      return Err((program.source, vec![d]));
    }
    active.mesh.install_revision(candidate, &removed);
    let finished = drain(&mut active.mesh);
    active.entries = plan
      .into_iter()
      .map(|(name, wire, id)| {
        let id = id.unwrap_or_else(|| active.mesh.spawn(&wire).expect("validated candidate wire"));
        Entry { name, wire, id }
      })
      .collect();
    let run = program.lowered.run.as_ref();
    active.failed.clear();
    active.ticks = 0;
    active.iterations = run.and_then(|r| r.iterations).map(|n| n as u64);
    active.frame_interval = run
      .and_then(|r| r.fps)
      .and_then(|fps| Duration::try_from_secs_f64(1.0 / fps).ok());
    Ok(finished)
  }
}

fn compose_entries<H: Host>(
  program: &Program,
  mesh: &mut H,
) -> Result<Vec<(String, H::Wire)>, Vec<Diagnostic>> {
  let mut entries = Vec::new();
  let mut diagnostics = Vec::new();
  for name in program.entries() {
    match mesh.compile(&name) {
      Ok(wire) => entries.push((name, wire)),
      Err(err) => diagnostics.push(program.diagnostic(&name, err)),
    }
  }
  for name in program.unreachable_roots() {
    if let Err(err) = mesh.compile(&name) {
      let d = program.diagnostic(&name, err);
      if !diagnostics.contains(&d) {
        diagnostics.push(d);
      }
    }
  }
  if diagnostics.is_empty() {
    Ok(entries)
  } else {
    Err(diagnostics)
  }
}
