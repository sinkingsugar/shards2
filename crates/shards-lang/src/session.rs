//! Host-driven execution with transactional source replacement.

use std::collections::HashMap;
use std::time::Duration;

use shards_core::diagnostic::Diagnostic;
use shards_core::{Catalog, Outcome};

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

struct Execution<H> {
  mesh: H,
  ticks: u64,
  iterations: Option<u64>,
  frame_interval: Option<Duration>,
}

/// A reloadable program on a host-owned event loop (stackless by default).
///
/// `reload` validates all wires before replacing the active execution. A
/// rejected edit leaves it untouched. A successful edit cancels the entire
/// old mesh before the new program's first tick. Locals, `Once`, suspended
/// continuations and mesh variables restart; host services live outside this
/// object and survive. Each revision owns its compose cache.
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
      for (name, wire) in entries {
        if let Err(err) = mesh.spawn(&wire) {
          diagnostics.push(program.diagnostic(&name, err));
        }
      }
    }
    if !diagnostics.is_empty() {
      return Err((program.source, diagnostics));
    }
    let run = program.lowered.run.as_ref();
    let next = Execution {
      mesh,
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
    drain(&mut active.mesh)
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
