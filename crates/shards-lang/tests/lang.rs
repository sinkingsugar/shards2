//! Frontend acceptance: source programs parse, lower, compose and run with
//! matching results on both schedulers, and every problem is located in
//! the source.

use std::collections::HashMap;

use shards_core::diagnostic::Diagnostic;
use shards_core::{Catalog, Outcome, Var};
use shards_lang::{Program, Source};

fn catalog() -> Catalog {
  Catalog::new(&[shards_core::shards::CATALOG]).unwrap()
}

fn no_defines() -> HashMap<String, String> {
  HashMap::new()
}

/// Load errors (syntax or lowering) of a source.
fn load_errors(text: &str) -> Vec<Diagnostic> {
  match Program::load(Source::new("t.shs", text), &catalog(), &no_defines()) {
    Ok(_) => panic!("expected load errors"),
    Err((_, d)) => d,
  }
}

fn at(d: &Diagnostic) -> (u32, u32) {
  (d.line.unwrap(), d.column.unwrap())
}

macro_rules! lang_tests {
  ($mesh:ty) => {
    use super::*;

    type Mesh = $mesh;

    fn run(text: &str, defines: &HashMap<String, String>) -> shards_lang::RunReport {
      let program = match Program::load(Source::new("t.shs", text), &catalog(), defines) {
        Ok(p) => p,
        Err((_, d)) => panic!("load failed: {d:?}"),
      };
      program
        .run::<Mesh>()
        .unwrap_or_else(|d| panic!("run failed: {d:?}"))
    }

    fn check(text: &str) -> shards_lang::CheckReport {
      shards_lang::check::<Mesh>(Source::new("t.shs", text), &catalog(), &no_defines())
    }

    fn completed(report: &shards_lang::RunReport, wire: &str) -> Var {
      match report.outcomes.iter().find(|(w, _)| w == wire) {
        Some((_, Some(Outcome::Completed(v)))) => v.clone(),
        other => panic!("{wire}: {other:?}"),
      }
    }

    #[test]
    fn constructor_segments_preserve_snapshots_and_change_table_shapes() {
      let report = run(
        "1 | Var(n)\n{z: n a: 2}\n{a: 3 z: n} | Var(saved)\n4 | Update(n)\n{b: n a: 5}\n{a: n b: 6} | Var(result)\n[saved result]",
        &no_defines(),
      );
      assert_eq!(completed(&report, "root"), Var::Seq(std::sync::Arc::new(vec![
        Var::table([("a", Var::Int(3)), ("z", Var::Int(1))]),
        Var::table([("a", Var::Int(4)), ("b", Var::Int(6))]),
      ])));
      let report = run("1 | Var(n)\n{z: n a: 2}\n{c: n}", &no_defines());
      assert_eq!(completed(&report, "root"), Var::table([("c", Var::Int(1))]));
    }

    #[test]
    fn collection_outputs_keep_saved_snapshots() {
      for (expression, expected) in [
        ("[n n]", "[[1 1] [2 2] [3 3]]"),
        ("{a: n b: n}", "[{a: 1 b: 1} {a: 2 b: 2} {a: 3 b: 3}]"),
      ] {
        let source = format!("0 | Var(n)\n[] | Var(saved)\nRepeat({{ Inc(n) {expression} | Push(saved) }} times: 3)\nsaved | Is({expected})");
        assert_eq!(completed(&run(&source, &no_defines()), "root"), Var::Bool(true));
      }
    }

    fn reload(session: &mut shards_lang::Session<Mesh>, text: &str) -> Vec<shards_lang::Finished> {
      session.reload(Source::new("reload.shs", text), &catalog(), &no_defines())
        .unwrap_or_else(|(_, d)| panic!("reload failed: {d:?}"))
    }

    fn preserve(session: &mut shards_lang::Session<Mesh>, text: &str) -> Vec<shards_lang::Finished> {
      session.reload_preserving(Source::new("preserve.shs", text), &catalog(), &no_defines())
        .unwrap_or_else(|(_, d)| panic!("preserving reload failed: {d:?}"))
    }

    #[test]
    fn preserving_reload_pins_suspended_do_and_keeps_caller_counter() {
      let source = |value| format!(r#"@wire(inner {{ Pause() {value} }})
@wire(outer {{ Do(inner) }})
@wire(main {{ Keep(n 0) Inc(n) Log Do(outer) Log }} looped: true)
@mesh(m) @schedule(m main) @run(m)"#);
      let mut session = shards_lang::Session::<Mesh>::new();
      preserve(&mut session, &source(10));
      let (_, lines) = shards_core::log::capture(|| {
        session.tick(); // n=1, paused inside inner.
        assert!(preserve(&mut session, &source(20)).is_empty());
        session.tick(); // old call returns 10.
        session.tick(); // n=2, enters new inner.
        session.tick(); // new call returns 20.
        assert!(preserve(&mut session, &source(30)).is_empty());
        session.tick();
        session.tick();
      });
      assert_eq!(lines, ["1", "10", "2", "20", "3", "30"]);
    }

    #[test]
    fn preserving_reload_updates_deep_calls_inside_a_never_returning_parent() {
      let source = |value| format!(r#"@wire(inner {{ {value} Log Pause() }})
@wire(outer {{ Keep(n 0) Repeat({{ Inc(n) Log Do(inner) }} forever: true) }})
Do(outer)"#);
      let mut session = shards_lang::Session::<Mesh>::new();
      preserve(&mut session, &source(10));
      let (_, lines) = shards_core::log::capture(|| {
        session.tick();
        assert!(preserve(&mut session, &source(20)).is_empty());
        session.tick();
        assert!(preserve(&mut session, &source(30)).is_empty());
        session.tick();
      });
      assert_eq!(lines, ["1", "10", "2", "20", "3", "30"]);
    }

    #[test]
    fn preserving_reload_keeps_unrelated_wires_and_mesh_values() {
      let source = |value| format!(r#"@wire(main {{ Inc(shared) Log {value} Log }} looped: true)
@wire(ticker {{ Keep(n 100) Inc(n) Log }} looped: true)
@mesh(m) @schedule(m main) @schedule(m ticker) @run(m)"#);
      let mut mesh = Mesh::new();
      mesh.declare_var("shared", Var::Int(0), true);
      let mut session = shards_lang::Session::with_mesh(mesh);
      preserve(&mut session, &source(10));
      let (_, lines) = shards_core::log::capture(|| {
        session.tick();
        let ended = preserve(&mut session, &source(20));
        assert_eq!(ended.len(), 1);
        assert_eq!(ended[0].wire, "main");
        session.tick();
      });
      // Retained instances keep their scheduler order; replacements append.
      assert_eq!(lines, ["1", "10", "101", "102", "2", "20"]);
    }

    #[test]
    fn preserving_reload_rejects_incompatible_do_interfaces_atomically() {
      let source = |body| format!(r#"@wire(inner {{ {body} }})
@wire(main {{ Keep(n 0) Inc(n) Log Do(inner) ToString Log }} looped: true)
@mesh(m) @schedule(m main) @run(m)"#);
      let mut session = shards_lang::Session::<Mesh>::new();
      preserve(&mut session, &source("10"));
      let (_, lines) = shards_core::log::capture(|| {
        session.tick();
        for body in [r#""new type""#, "1 = new-local 20"] {
          let (_, d) = session.reload_preserving(Source::new("bad.shs", source(body)), &catalog(), &no_defines()).unwrap_err();
          assert_eq!(d[0].code, "reload-incompatible");
          assert_eq!(d[0].line, Some(1));
          session.tick();
        }
      });
      assert_eq!(lines, ["1", "10", "2", "10", "3", "10"]);
    }

    #[test]
    fn preserving_reload_explains_and_locates_binding_changes() {
      let source = |body| format!("@wire(step {{\n{body}\n}})\n@wire(outer {{ Do(step) }})\n@wire(main {{ Do(outer) }} looped: true)\n@mesh(m) @schedule(m main) @run(m)");
      for (old, new, reason, line) in [
        ("10", "Once({\n  1 | Var(extra)\n}) 10", "new local `extra`", 3),
        ("1 | Var(extra) 10", "10", "local `extra` was removed", 1),
        ("1 | Var(extra) 10", "1.5 | Var(extra) 10", "local `extra` changed type from Int to Float", 2),
        ("1 = extra 10", "1 | Var(extra) 10", "local `extra` changed mutability", 2),
        // A name declared in a branch does not escape it.
        ("1 | Var(extra) 10", "When(true {1 | Var(extra)}) 10", "local `extra` was removed", 2),
        ("10", "\"hello\"", "output type changed from Int to String", 1),
      ] {
        let mut session = shards_lang::Session::<Mesh>::new();
        preserve(&mut session, &source(old));
        session.tick();
        let (source, diagnostics) = session.reload_preserving(
          Source::new("edit.shs", source(new)), &catalog(), &no_defines()
        ).unwrap_err();
        let d = &diagnostics[0];
        assert_eq!(d.code, "reload-incompatible");
        assert!(d.message.contains(reason), "{}", d.message);
        assert!(d.message.contains("press r in watch"));
        // Outer interfaces can also change; binding errors still point into
        // the actual declaring wire, including declarations inside Once.
        if !reason.starts_with("output") && !reason.contains("removed") {
          assert_eq!(d.line, Some(line), "{}", shards_lang::render(d, &source));
          assert!(source.text.lines().nth(line as usize - 1).unwrap().contains("extra"));
        }
      }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn file_watcher_callbacks_preserve_reject_restart_and_stop() {
      use shards_lang::{FileWatcher, WatchControl, WatchEvent};
      use std::cell::Cell;
      use std::time::{Duration, Instant};
      let path = std::env::temp_dir().join(format!("shards-watch-{}-{}.shs", std::process::id(), module_path!().replace("::", "-")));
      struct Remove(std::path::PathBuf);
      impl Drop for Remove { fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); } }
      let _remove = Remove(path.clone());
      let source = |value| format!("@wire(step {{{value} Log}})\n@wire(main {{Keep(n 0) Inc(n) Log Do(step)}} looped: true)\n@mesh(m) @schedule(m main) @run(m fps: 0.1)");
      std::fs::write(&path, source(10)).unwrap();
      let command = Cell::new(WatchControl::Continue);
      let mut revisions = 0;
      let mut rejected = 0;
      let mut ticks = 0;
      let mut stopped = false;
      let start = Instant::now();
      let mut session = shards_lang::Session::<Mesh>::new();
      let (_, lines) = shards_core::log::capture(|| FileWatcher::new(&path).run(
        &mut session, &catalog(), &no_defines(),
        || {
          assert!(start.elapsed() < Duration::from_secs(5), "watcher did not exit");
          command.replace(WatchControl::Continue)
        },
        |event| match event {
          WatchEvent::Reloaded { restarted, .. } => {
            revisions += 1;
            match revisions {
              1 => { assert!(!restarted); std::fs::write(&path, "Missing.Shard").unwrap(); }
              2 => { assert!(!restarted); command.set(WatchControl::Restart); }
              3 => { assert!(restarted); command.set(WatchControl::Stop); }
              _ => panic!("unexpected reload"),
            }
          }
          WatchEvent::Rejected { source: rejected_source, diagnostics } => {
            rejected += 1;
            assert_eq!(rejected_source.text, "Missing.Shard");
            assert!(!diagnostics.is_empty());
            // Atomic saves exercise content comparison independent of mtime.
            let replacement = path.with_extension("new");
            std::fs::write(&replacement, source(20)).unwrap();
            std::fs::rename(replacement, &path).unwrap();
          }
          WatchEvent::Tick(_) => ticks += 1,
          WatchEvent::Stopped(finished) => { stopped = true; assert!(!finished.is_empty()); }
          WatchEvent::ReadError(error) => panic!("{error}"),
        }
      ));
      assert_eq!(revisions, 3);
      assert_eq!(rejected, 1);
      assert!(ticks >= 3);
      assert!(stopped);
      assert_eq!(lines, ["1", "10", "2", "20", "1", "20"]);
    }

    #[test]
    fn preserving_reload_keeps_unchanged_do_once_state_and_other_sessions() {
      let source = |value| format!(r#"@wire(stable {{Keep(n 0) Inc(n) Log}})
@wire(changed {{{value} Log}})
@wire(main {{Do(stable) Do(changed)}} looped: true)
@mesh(m) @schedule(m main) @run(m)"#);
      let mut first = shards_lang::Session::<Mesh>::new();
      let mut second = shards_lang::Session::<Mesh>::new();
      preserve(&mut first, &source(10));
      preserve(&mut second, &source(10));
      let (_, lines) = shards_core::log::capture(|| {
        first.tick();
        second.tick();
        preserve(&mut first, &source(20));
        first.tick();
        second.tick();
      });
      assert_eq!(lines, ["1", "10", "1", "10", "2", "20", "2", "10"]);
    }

    #[test]
    fn preserving_reload_retries_failed_nested_instantiation_after_maybe() {
      let source = |body| format!(r#"@wire(inner {{{body}}})
@wire(main {{Keep(n 0) Inc(n) Log Maybe({{Do(inner)}} silent: true)}} looped: true)
@mesh(m) @schedule(m main) @run(m)"#);
      let mut session = shards_lang::Session::<Mesh>::new();
      preserve(&mut session, &source("10"));
      let (_, lines) = shards_core::log::capture(|| {
        session.tick();
        preserve(&mut session, &source(r#"Probe("bad" "fail-instantiate") 10"#));
        assert!(session.tick().is_empty());
        assert!(session.tick().is_empty());
        preserve(&mut session, &source("20"));
        assert!(session.tick().is_empty());
      });
      assert_eq!(lines, ["1", "2", "3", "4"]);
      assert_eq!(session.running(), 1);
    }

    #[test]
    fn preserving_reload_keeps_old_call_sites_when_a_pinned_parent_body_changes() {
      let source = |outer: &str, value| format!(r#"@wire(inner {{{value}}})
@wire(outer {{{outer}}})
@wire(main {{Do(outer) Log}} looped: true)
@mesh(m) @schedule(m main) @run(m)"#);
      let mut session = shards_lang::Session::<Mesh>::new();
      preserve(&mut session, &source("Pause() Do(inner)", 10));
      let (_, lines) = shards_core::log::capture(|| {
        session.tick();
        preserve(&mut session, &source("Pause() 0 Do(inner)", 20));
        session.tick(); // Old parent body pins its original descendant sites.
        session.tick();
        session.tick();
      });
      assert_eq!(lines, ["10", "20"]);
    }

    #[test]
    fn preserving_reload_does_not_renumber_other_wires_temporaries() {
      let source = |body| format!(r#"@wire(changed {{{body}}})
@wire(ticker {{Keep(n 0) Inc(n) Add(0 | Add(1)) Log}} looped: true)
@mesh(m) @schedule(m changed) @schedule(m ticker) @run(m)"#);
      let mut session = shards_lang::Session::<Mesh>::new();
      preserve(&mut session, &source("1"));
      let (_, lines) = shards_core::log::capture(|| {
        session.tick();
        preserve(&mut session, &source("1 Add(0 | Add(2))"));
        session.tick();
      });
      assert_eq!(lines, ["2", "3"]);
    }

    #[cfg(not(target_family = "wasm"))]
    #[test]
    fn preserving_reload_attempts_cleanup_once_when_boundary_cleanup_panics() {
      use shards_core::shards::{take_probe_events, ProbeEventKind};
      let source = |body| format!(r#"@wire(inner {{{body}}})
@wire(main {{Probe("parent") Do(inner)}} looped: true)
@mesh(m) @schedule(m main) @run(m)"#);
      take_probe_events();
      let mut session = shards_lang::Session::<Mesh>::new();
      preserve(&mut session, &source(r#"Probe("old" "panic-cleanup") 10"#));
      session.tick();
      take_probe_events();
      preserve(&mut session, &source("20"));
      let finished = session.tick();
      assert!(matches!(finished[0].outcome, Outcome::Failed(_)));
      let events = take_probe_events();
      for tag in ["old", "parent"] {
        assert_eq!(events.iter().filter(|e| e.tag == tag && e.kind == ProbeEventKind::Cleanup).count(), 1);
      }
      assert!(session.stop().is_empty());
    }

    #[test]
    fn preserving_reload_prepares_retained_spawned_input_specializations() {
      let source = |input, amount| format!(r#"@wire(inner {{Add({amount})}})
@wire(child {{Do(inner) Log Pause()}} looped: true)
@wire(main {{{input} Spawn(child)}})
@mesh(m) @schedule(m main) @run(m)"#);
      let mut session = shards_lang::Session::<Mesh>::new();
      preserve(&mut session, &source("1", 1));
      let (_, lines) = shards_core::log::capture(|| {
        session.tick(); // Spawn Int child.
        session.tick(); // Child prints 2 and pauses.
        preserve(&mut session, &source("1.5", 2));
        session.tick(); // Old child completes iteration; new main spawns Float child.
        session.tick(); // Both specializations must select the new inner.
      });
      assert_eq!(lines, ["2", "3", "3.5"]);
    }

    #[test]
    fn preserving_reload_recovers_failed_entries_on_the_next_accepted_save() {
      let source = |divisor| format!(r#"@wire(inner {{1 Div({divisor})}})
@wire(main {{Do(inner)}})
@mesh(m) @schedule(m main) @run(m)"#);
      let mut session = shards_lang::Session::<Mesh>::new();
      preserve(&mut session, &source(0));
      assert!(matches!(session.tick()[0].outcome, Outcome::Failed(_)));
      assert!(session.tick().is_empty());
      preserve(&mut session, &source(1));
      assert!(matches!(session.tick()[0].outcome, Outcome::Completed(Var::Int(1))));
    }

    #[test]
    fn preserving_reload_removes_scheduled_callers_before_checking_their_interface() {
      let source = |value, scheduled| format!(r#"@wire(inner {{{value}}})
@wire(main {{Do(inner) Log Pause()}} looped: true)
@wire(other {{Pause()}} looped: true)
@mesh(m) @schedule(m {scheduled}) @run(m)"#);
      let mut session = shards_lang::Session::<Mesh>::new();
      preserve(&mut session, &source("10", "main"));
      session.tick();
      let finished = preserve(&mut session, &source(r#""new type""#, "other"));
      assert_eq!(finished.len(), 1);
      assert_eq!(finished[0].wire, "main");
      assert!(matches!(finished[0].outcome, Outcome::Cancelled));
      assert!(session.tick().is_empty());
    }

    #[test]
    fn preserving_reload_instantiates_restarted_roots_unchanged_children_once() {
      use shards_core::shards::{take_probe_events, ProbeEventKind};
      let source = |value| format!(r#"@wire(inner {{Probe("child")}})
@wire(main {{{value} Do(inner)}} looped: true)
@mesh(m) @schedule(m main) @run(m)"#);
      take_probe_events();
      let mut session = shards_lang::Session::<Mesh>::new();
      preserve(&mut session, &source(1));
      session.tick();
      preserve(&mut session, &source(2));
      take_probe_events();
      session.tick();
      let events = take_probe_events();
      assert_eq!(events.iter().filter(|e| e.kind == ProbeEventKind::Instantiate).count(), 1);
      assert_eq!(events.iter().filter(|e| e.kind == ProbeEventKind::Cleanup).count(), 0);
    }

    #[test]
    fn preserving_reload_does_not_repeat_unchanged_completed_effects() {
      let mut session = shards_lang::Session::<Mesh>::new();
      let source = r#"@wire(setup {"setup" Log})
@wire(ticker {Pause()} looped: true)
@mesh(m) @schedule(m setup) @schedule(m ticker) @run(m)"#;
      preserve(&mut session, source);
      let (_, lines) = shards_core::log::capture(|| {
        session.tick();
        preserve(&mut session, &format!("// only a comment\n{source}"));
        session.tick();
      });
      assert_eq!(lines, ["setup"]);
    }

    #[test]
    fn reload_rejects_bad_edits_without_resetting_the_running_program() {
      let mut session = shards_lang::Session::<Mesh>::new();
      reload(&mut session, r#"@wire(tick { Keep(n 0) Inc(n) Log } looped: true)
@mesh(m) @schedule(m tick) @run(m fps: 30)"#);
      let (_, lines) = shards_core::log::capture(|| {
        session.tick();
        for bad in ["When({", "No.SuchShard", "1 | Take(0)",
          "@wire(unused { missing }) 42"] {
          let (_, d) = session.reload(Source::new("bad.shs", bad), &catalog(), &no_defines()).unwrap_err();
          assert!(!d.is_empty());
          assert_eq!(d[0].file.as_deref(), Some("bad.shs"));
          assert!(d[0].line.is_some());
          session.tick();
        }
      });
      assert_eq!(lines, ["1", "2", "3", "4", "5"]);
      assert_eq!(session.ticks(), 5);
      assert_eq!(session.frame_interval(), Some(std::time::Duration::from_secs_f64(1.0 / 30.0)));
      assert_eq!(reload(&mut session, "42").len(), 1);
      assert_eq!(session.ticks(), 0);
      let finished = session.tick();
      assert!(matches!(finished[0].outcome, Outcome::Completed(Var::Int(42))));
      assert!(session.tick().is_empty());
      assert_eq!(session.ticks(), 1);
    }

    #[test]
    fn reload_cancels_nested_flows_and_spawned_children_before_new_activation() {
      use shards_core::shards::{take_probe_events, ProbeEventKind};
      take_probe_events();
      let mut session = shards_lang::Session::<Mesh>::new();
      reload(&mut session, r#"@wire(child { Probe("child") Pause(1000.0) })
@wire(inner { Probe("inner") Pause(1000.0) })
@wire(main { Spawn(child) Do(inner) })
@mesh(m) @schedule(m main) @run(m)"#);
      session.tick();
      session.tick();
      take_probe_events();
      let stopped = reload(&mut session, r#"Probe("new") 42"#);
      assert_eq!(stopped.len(), 2);
      assert!(stopped.iter().all(|f| matches!(f.outcome, Outcome::Cancelled)));
      let events = take_probe_events();
      assert_eq!(events.len(), 2);
      assert!(events.iter().all(|e| e.kind == ProbeEventKind::Cleanup));
      assert!(events.iter().any(|e| e.tag == "child"));
      assert!(events.iter().any(|e| e.tag == "inner"));
      session.tick();
      let events = take_probe_events();
      assert!(events.iter().all(|e| e.tag == "new"));
      assert_eq!(events.iter().filter(|e| e.kind == ProbeEventKind::Cleanup).count(), 1);
      assert!(session.stop().is_empty());
      assert!(session.stop().is_empty());
    }

    #[test]
    fn reload_recompiles_changed_callees_and_resets_once_and_locals() {
      let mut session = shards_lang::Session::<Mesh>::new();
      for value in [10, 20, 30] {
        reload(&mut session, &format!(r#"@wire(value {{ {value} }})
@wire(main {{ Keep(n 0) Once({{Do(value) | Update(n)}}) Inc(n) Log }} looped: true)
@mesh(m) @schedule(m main) @run(m iterations: 2)"#));
        let (finished, lines) = shards_core::log::capture(|| {
          assert!(session.tick().is_empty());
          session.tick()
        });
        assert_eq!(lines, [(value + 1).to_string(), (value + 2).to_string()]);
        assert!(matches!(finished[0].outcome, Outcome::Cancelled));
        assert_eq!(session.running(), 0);
        assert!(session.tick().is_empty());
      }
    }

    #[test]
    fn session_reports_spawned_failures_once() {
      let mut session = shards_lang::Session::<Mesh>::new();
      reload(&mut session, r#"@wire(child { 1 | Div(0) }) Spawn(child)"#);
      let entries = session.tick();
      assert_eq!(entries.len(), 1);
      let children = session.tick();
      assert_eq!(children.len(), 1);
      assert_eq!(children[0].wire, "child");
      assert!(matches!(children[0].outcome, Outcome::Failed(_)));
      assert!(session.tick().is_empty());
    }

    #[cfg(not(target_family = "wasm"))]
    #[test]
    fn reload_reports_cleanup_failure_and_still_cleans_other_instances() {
      use shards_core::shards::{take_probe_events, ProbeEventKind};
      take_probe_events();
      let mut session = shards_lang::Session::<Mesh>::new();
      reload(&mut session, r#"@wire(a { Probe("a" "panic-cleanup") Pause(1000.0) })
@wire(b { Probe("b") Pause(1000.0) })
@mesh(m) @schedule(m a) @schedule(m b) @run(m)"#);
      session.tick();
      take_probe_events();
      let finished = reload(&mut session, "42");
      assert!(matches!(finished[0].outcome, Outcome::Failed(_)));
      assert!(matches!(finished[1].outcome, Outcome::Cancelled));
      assert_eq!(take_probe_events().iter().filter(|e| e.kind == ProbeEventKind::Cleanup).count(), 2);
      assert!(matches!(session.tick()[0].outcome, Outcome::Completed(Var::Int(42))));
    }

    #[test]
    fn esp32_firmware_script_completes_after_suspending() {
      let report = run(
        include_str!("../../../examples/esp32/smoke.shs"),
        &no_defines(),
      );
      assert_eq!(report.outcomes.len(), 1);
      assert_eq!(completed(&report, "root"), Var::Int(42));
      assert!(report.spawned_failures.is_empty());
      assert!(report.ticks >= 2);
    }

    #[test]
    fn loose_code_runs_as_the_root_wire() {
      let report = run(
        "// word forms and operators
0 | Var(n)
Repeat({Inc(n)} times: 3)
n | Add(10) = result
When({result | IsMoreEqual(13)} {
  result | Add(1) | Update(n)
})
n",
        &no_defines(),
      );
      assert_eq!(completed(&report, "root"), Var::Int(14));
    }

    #[test]
    fn declared_wires_run_on_the_scheduled_mesh() {
      let report = run(
        "@wire(add-one { Add(1) })
@wire(answer { 41 | Do(add-one) })
@wire(ticker { Keep(ticks 0) Inc(ticks) } looped: true)
@mesh(main)
@schedule(main answer)
@schedule(main ticker)
@run(main iterations: 5)",
        &no_defines(),
      );
      assert_eq!(completed(&report, "answer"), Var::Int(42));
      // The looped wire is still running when the iteration limit stops it.
      assert_eq!(report.ticks, 5);
      assert!(matches!(
        report.outcomes.iter().find(|(w, _)| w == "ticker"),
        Some((_, None))
      ));
    }

    #[test]
    fn literals_tables_and_script_arguments() {
      let mut defines = HashMap::new();
      defines.insert("who".to_string(), "world".to_string());
      let report = run(
        "{name: @who count: 2 ratio: 2.5e-1 tags: [\"a\" \"b\"]} = t
// `{}` is an empty flow where a flow is expected
When({true} {})
t",
        &defines,
      );
      assert_eq!(
        completed(&report, "root"),
        Var::table([
          ("name", Var::string("world")),
          ("count", Var::Int(2)),
          ("ratio", Var::Float(0.25)),
          (
            "tags",
            Var::Seq(std::sync::Arc::new(vec![
              Var::string("a"),
              Var::string("b")
            ]))
          ),
        ])
      );
    }

    #[test]
    fn compose_errors_point_at_the_failing_source() {
      let report = check(
        "@wire(helper {
  When({true} {
    1 | Add(2)
    \"a\" | Add(2)
  })
})
@wire(main-wire { 0 | Do(helper) })
@mesh(main)
@schedule(main main-wire)
@run(main)",
      );
      assert!(!report.ok());
      let d = &report.diagnostics[0];
      assert_eq!(d.code, "input-type-mismatch");
      // The second Add, inside the called wire's When body.
      assert_eq!(at(d), (4, 11));
      assert_eq!(
        d.path_string(),
        "main-wire/1:Do/wire/helper/0:When/action/3:Math.Add"
      );
      let json = report.to_json();
      assert!(
        json.starts_with("{\"ok\":false,\"file\":\"t.shs\",\"diagnostics\":[{"),
        "{json}"
      );
      assert!(json.contains("\"line\":4,\"column\":11"), "{json}");
    }

    #[test]
    fn variable_errors_point_at_the_assignment() {
      let report = check("1 = x\n2 | Update(x)");
      let d = &report.diagnostics[0];
      assert_eq!(d.code, "immutable-binding");
      // At the Update that fails; the binding is the related location.
      assert_eq!(at(d), (2, 5));
      let related = d.related.as_ref().unwrap();
      assert_eq!((related.line, related.column), (Some(1), Some(3)));
    }

    #[test]
    fn paths_read_tables_and_sequences() {
      let report = run(
        "{a: [10 20] b: {c: \"deep\"}} = t\nt.a.1 = second\nt.b.c = deep\n[second deep]",
        &no_defines(),
      );
      assert_eq!(
        completed(&report, "root"),
        Var::Seq(std::sync::Arc::new(vec![Var::Int(20), Var::string("deep")]))
      );
      // A typo on a fixed table is a compose error at the key, with suggestions.
      let report = check("{name: 1 count: 2} = t\nt.cuont");
      let d = &report.diagnostics[0];
      assert_eq!(d.code, "unknown-key");
      assert_eq!(d.did_you_mean, ["count"]);
      assert_eq!(at(d), (2, 3));
    }

    #[test]
    fn f_strings_and_computed_values() {
      let report = run(
        "5 = n
f\"n is {n}, next {n | Add(1)}, {{literal}}\" = text
[1 n n | Add(1)] = items
{a: n b: n | Add(10)} = table
// A computed parameter is computed before the shard; the input flows on.
1 | Add(n | Add(5)) = sum
[text items table sum]",
        &no_defines(),
      );
      let seq = |v: Vec<Var>| Var::Seq(std::sync::Arc::new(v));
      assert_eq!(
        completed(&report, "root"),
        seq(vec![
          Var::string("n is 5, next 6, {literal}"),
          seq(vec![Var::Int(1), Var::Int(5), Var::Int(6)]),
          Var::table([("a", Var::Int(5)), ("b", Var::Int(15))]),
          Var::Int(11),
        ])
      );
    }

    #[test]
    fn push_appends_to_a_sequence() {
      let report = run("[] | Var(xs)\n1 | Push(xs)\n2 | Push(xs)\nxs", &no_defines());
      assert_eq!(
        completed(&report, "root"),
        Var::Seq(std::sync::Arc::new(vec![Var::Int(1), Var::Int(2)]))
      );
      let report = check("[0] | Var(xs)\n\"s\" | Push(xs)");
      assert_eq!(report.diagnostics[0].code, "variable-type-mismatch");
      assert_eq!(at(&report.diagnostics[0]), (2, 7));
      // Push neither declares nor assigns an immutable binding.
      let report = check("1 | Push(xs)");
      assert_eq!(report.diagnostics[0].code, "unknown-variable");
      assert!(report.diagnostics[0].message.contains("`value | Var(xs)`"));
      let report = check("[] = xs\n1 | Push(xs)");
      assert_eq!(report.diagnostics[0].code, "immutable-binding");
    }

    #[test]
    fn computed_values_in_a_wire_inlined_twice() {
      // Do composes the wire into the caller's frame at both call sites.
      let report = run(
        "@wire(bump { Add(1 | Add(1)) })\n0 | Do(bump) | Do(bump)",
        &no_defines(),
      );
      assert_eq!(completed(&report, "root"), Var::Int(4));
    }

    #[test]
    fn numbers_mix_in_arithmetic_and_comparisons() {
      let report = run(
        "5 | Math.Divide(2.0) = a
5.0 | Div(2) = b
5 | Div(2) = c
1.5 | IsMore(1) = d
1 | Is(1.0) = e
[1 2 3] | ToFloat3 | Mul(2) | Math.Subtract(1.0) | Math.Length = f
[a b c d e f]",
        &no_defines(),
      );
      let seq = |v: Vec<Var>| Var::Seq(std::sync::Arc::new(v));
      let len = ((1.0f64).powi(2) + 3.0f64.powi(2) + 5.0f64.powi(2)).sqrt();
      assert_eq!(
        completed(&report, "root"),
        seq(vec![
          Var::Float(2.5),
          Var::Float(2.5),
          Var::Int(2),
          Var::Bool(true),
          Var::Bool(true),
          Var::Float(len),
        ])
      );
      // Vectors of different sizes do not mix.
      let report = check("[1 2 3] | ToFloat3 = v\n[1 2] | ToFloat2 | Math.Add(v)");
      assert_eq!(report.diagnostics[0].code, "input-type-mismatch");
      // Int division by zero is an activation error.
      let report = run("1 | Div(0)", &no_defines());
      assert!(matches!(&report.outcomes[0].1, Some(Outcome::Failed(_))));
    }

    #[test]
    fn value_shards_through_source() {
      let (report, lines) = shards_core::log::capture(|| {
        run(
          "[\"a\" \"b\"] | Count | Log(\"count\")
\"ff\" | ParseInt(base: 16) | ToHex = hex
\"gone\" | IsAny([\"gone\" \"unknown\"]) | Not = live
none | IsNone = nothing
42 | ToString | Log
Time.Now = t0
Time.Now | Math.Subtract(t0) | IsMoreEqual(0.0) = later
[hex live nothing later]",
          &no_defines(),
        )
      });
      assert_eq!(lines, ["count: 2", "42"]);
      assert_eq!(
        completed(&report, "root"),
        Var::Seq(std::sync::Arc::new(vec![
          Var::string("0xff"),
          Var::Bool(false),
          Var::Bool(true),
          Var::Bool(true),
        ]))
      );
      // Stop ends the instance; nothing after it runs.
      let (report, lines) =
        shards_core::log::capture(|| run("1 | Log\nStop\n2 | Log", &no_defines()));
      assert_eq!(lines, ["1"]);
      assert!(matches!(&report.outcomes[0].1, Some(Outcome::Stopped)));
    }

    #[test]
    fn control_flow_shards() {
      let report = run(
        "5 = n
n | If({IsMore(3)} {\"big\"} {\"small\"}) = a
n | If({IsMore(10)} {\"big\"}) = b
\"gone\" | Match([\"biting\" {\"hook\"} \"gone\" {\"despawned\"} none {\"other\"}] passthrough: false) = c
\"x\" | Match([\"biting\" {\"hook\"} none {\"other\"}] passthrough: false) = d
3 | Match([1 {\"one\"}]) = e
Maybe({[1 2] | Take(5)} {\"fallback\"} silent: true) = f
Maybe({\"fine\"} {\"fallback\"}) = g
true = far
false = close
All(far {n | IsMore(1)}) = h
All(far close) = i
Any(close {n | Is(5)}) = j
0 | Var(count)
Repeat({Inc(count)} until: {count | IsMoreEqual(3)})
Repeat({Inc(count)} until: {count | IsMoreEqual(100)} times: 2)
[a b c d e f g h i j count]",
        &no_defines(),
      );
      let s = Var::string;
      assert_eq!(
        completed(&report, "root"),
        Var::Seq(std::sync::Arc::new(vec![
          s("big"),
          Var::Int(5),
          s("despawned"),
          s("other"),
          Var::Int(3),
          s("fallback"),
          s("fine"),
          Var::Bool(true),
          Var::Bool(false),
          Var::Bool(true),
          Var::Int(5),
        ]))
      );
    }

    #[test]
    fn control_flow_resumes_after_suspending_inside() {
      // Each control shard suspends (Pause) inside a nested flow and must
      // resume there, not restart, on both schedulers.
      let (report, lines) = shards_core::log::capture(|| {
        run(
          "0 | Var(ticks)
If({true} {Pause Inc(ticks)})
All({Pause true} {Inc(ticks) true})
1 | Match([1 {Pause Inc(ticks)}])
Maybe({Pause [1] | Take(3)} {Inc(ticks)})
Repeat({Pause Inc(ticks)} until: {ticks | IsMoreEqual(7)})
ticks",
          &no_defines(),
        )
      });
      assert_eq!(completed(&report, "root"), Var::Int(7));
      // Maybe logs the error it caught (not Silent).
      assert_eq!(lines.len(), 1, "{lines:?}");
      assert!(lines[0].starts_with("Maybe: activation error: Take: index 3"), "{lines:?}");
    }

    #[test]
    fn control_flow_compose_errors() {
      let report = check("Repeat({})");
      assert_eq!(report.diagnostics[0].code, "missing-argument");
      let report = check("1 | Match([\"a\" {}])");
      assert_eq!(report.diagnostics[0].code, "unmatchable-case");
      let report = check("All({1})");
      assert_eq!(report.diagnostics[0].code, "predicate-not-bool");
      // A failing shard inside the second condition flow: located there.
      let report = check("true = far\nAll(far {1 | Add(\"x\")})");
      let d = &report.diagnostics[0];
      assert_eq!(d.path_string(), "root/2:All/conditions/#1/1:Math.Add");
      assert_eq!(at(d), (2, 14));
    }

    // --- review findings (Astra and Opus, 2026-10-05) ---

    #[test]
    fn a_failed_once_runs_again() {
      // The first attempt fails inside Once before assigning x; Maybe's Else
      // fixes the index. Once must run again (not count as done), so x is
      // assigned before it is read: never the initial `x: 0`.
      let (report, lines) = shards_core::log::capture(|| {
        run(
          "1 | Var(k)
0 | Var(x)
Repeat({
  Maybe({
    Once({[10] | Take(k) | Update(x)})
    x | Log(\"x\")
  } {Math.Dec(k)} silent: true)
} times: 2)",
          &no_defines(),
        )
      });
      assert!(report.succeeded());
      assert_eq!(lines, ["x: 10"]);
    }

    #[test]
    fn temporaries_are_scoped_per_inlined_occurrence() {
      let report = run("@wire(w {f\"{Add(1)}\"})\n1 | Do(w)\n1.0 | Do(w)", &no_defines());
      assert_eq!(completed(&report, "root"), Var::string("2"));
    }

    #[test]
    fn the_suggested_all_rewrite_compiles_and_flow_parameters_take_shorthands() {
      // The exact rewrite the And diagnostic suggests.
      let d = load_errors("true = a\nfalse = b\nIf({a | And | b} {1} {2})");
      assert!(d[0].message.contains("`If(All(a b) ...)`"));
      let report = run(
        "true = a
false = b
If(All(a b) {1} {2}) = x
When(a {3 | Var(y)})
If(Any(b {a}) {4} {5}) = z
[x z]",
        &no_defines(),
      );
      assert_eq!(
        completed(&report, "root"),
        Var::Seq(std::sync::Arc::new(vec![Var::Int(2), Var::Int(4)]))
      );
    }

    #[test]
    fn spawned_failures_fail_the_run() {
      let report = run("@wire(child {1 | Div(0)})\nSpawn(child)", &no_defines());
      assert!(!report.succeeded());
      assert_eq!(report.spawned_failures.len(), 1);
      assert_eq!(report.spawned_failures[0].0, "child");
    }

    #[test]
    fn mixed_number_equality_and_order_are_exact_and_consistent() {
      let report = run(
        "9007199254740993 | Is(9007199254740992.0) = a
9007199254740993 | IsMore(9007199254740992.0) = b
[1 2] | Is([1.0 2.0]) = c
1 | IsAny([1.0 \"a\"]) = d
9.21e18 | ToInt = e
[a b c d e]",
        &no_defines(),
      );
      assert_eq!(
        completed(&report, "root"),
        Var::Seq(std::sync::Arc::new(vec![
          Var::Bool(false),
          Var::Bool(true),
          Var::Bool(true),
          Var::Bool(true),
          Var::Int(9_210_000_000_000_000_000),
        ]))
      );
      let report = run("9.3e18 | ToInt", &no_defines());
      assert!(!report.succeeded());
    }

    #[test]
    fn per_iteration_and_kept_sequences() {
      // `Var` declares afresh on every iteration; `Keep` persists.
      let (_, lines) = shards_core::log::capture(|| {
        run(
          "@wire(w {Keep(kept [])  [] | Var(s)  1 | Push(s)  s | Count | Log(\"cleared\")  1 | Push(kept)  kept | Count | Log(\"kept\")} looped: true)
@mesh(m)
@schedule(m w)
@run(m iterations: 3)",
          &no_defines(),
        )
      });
      assert_eq!(
        lines,
        ["cleared: 1", "kept: 1", "cleared: 1", "kept: 2", "cleared: 1", "kept: 3"]
      );
    }

    #[test]
    fn check_composes_every_wire_that_can_run() {
      let report = check("@wire(unused { 1 | Add(\"x\") })\n1");
      assert_eq!(report.diagnostics.len(), 1, "{}", report.to_json());
      assert_eq!(at(&report.diagnostics[0]), (1, 20));
    }

    #[test]
    fn computed_elements_keep_source_order() {
      let report = run("0 | Var(n)\n[n (Inc(n) n)]", &no_defines());
      assert_eq!(
        completed(&report, "root"),
        Var::Seq(std::sync::Arc::new(vec![Var::Int(0), Var::Int(1)]))
      );
    }

    #[test]
    fn until_runs_before_action_and_may_assign_what_it_reads() {
      let report = run("0 | Var(n)\n\"\" | Var(x)\nRepeat({x | Log} until: {n | ToString | Update(x)  Inc(n) | IsMore(2)})\nn", &no_defines());
      assert_eq!(completed(&report, "root"), Var::Int(3));
      // Until and action are separate blocks: a name declared in one is
      // not visible in the other.
      let report = check("0 | Var(n)\nRepeat({x | Log} until: {n | ToString | Var(x)  Inc(n) | IsMore(2)})");
      assert_eq!(report.diagnostics[0].code, "unknown-variable");
    }

    #[test]
    fn uppercase_path_keys_stay_one_segment() {
      let report = run("{a: {Key: {sub: 1}}} = t\nt.a.Key.sub", &no_defines());
      assert_eq!(completed(&report, "root"), Var::Int(1));
    }

    #[test]
    fn unknown_dashed_names_explain_the_dash() {
      let report = check("5 = x\nx-1");
      assert!(report.diagnostics[0].message.contains("`-` is part of names"), "{}", report.diagnostics[0].message);
    }

    // --- second review round (2026-10-05) ---

    /// Golden path test E: binding errors name the variable and help.
    #[test]
    fn bindings_report_unknown_duplicate_and_immutable_names() {
      let report = check("0 | Var(counter)\n1 | Update(coutner)");
      let d = &report.diagnostics[0];
      assert_eq!((d.code, at(d)), ("unknown-variable", (2, 5)));
      assert_eq!(d.did_you_mean, ["counter"]);

      let report = check("0 | Var(counter)\n1 | Var(counter)");
      let d = &report.diagnostics[0];
      assert_eq!((d.code, at(d)), ("duplicate-binding", (2, 5)));
      assert!(d.message.contains("`Update(counter)`"), "{}", d.message);
      let related = d.related.as_ref().unwrap();
      assert_eq!((related.line, related.column), (Some(1), Some(5)));
      assert!(report.to_json().contains(r#""related":{"message":"counter is declared here","line":1,"column":5}"#), "{}", report.to_json());

      let report = check("0 = counter\n1 | Update(counter)");
      assert_eq!(report.diagnostics[0].code, "immutable-binding");
      // `=` cannot rebind a visible name either, nor shadow a mesh variable.
      let report = check("0 = x\n1 = x");
      assert_eq!(report.diagnostics[0].code, "duplicate-binding");
      // Disjoint sibling blocks may reuse a name.
      let report = check("If(true {1 | Var(t) t} {2 | Var(t) t}) | Log");
      assert!(report.ok(), "{}", report.to_json());
      // A block may update an enclosing mutable variable.
      let report = run("0 | Var(n)\nWhen(true {5 | Update(n)})\nn", &no_defines());
      assert_eq!(completed(&report, "root"), Var::Int(5));
    }

    #[test]
    fn input_is_a_reserved_name() {
      for text in ["1 | Var(input)", "1 = input", "@wire(w {Keep(input 0)} looped: true) @mesh(m) @schedule(m w) @run(m)"] {
        let report = check(text);
        assert_eq!(report.diagnostics[0].code, "reserved-name", "{text}");
        assert!(report.diagnostics[0].message.contains("entry value"));
      }
    }

    #[test]
    fn keep_holds_state_across_iterations() {
      let (report, lines) = shards_core::log::capture(|| {
        run(
          "@wire(w {Keep(n 10)  n | Math.Add(1) | Update(n)  n | Log} looped: true)
@mesh(m) @schedule(m w) @run(m iterations: 3)",
          &no_defines(),
        )
      });
      assert!(report.succeeded());
      assert_eq!(lines, ["11", "12", "13"]);
      // Keep belongs to a wire's top level, outside branches and loops.
      let report = check("@wire(w {When(true {Keep(n 0)})} looped: true) @mesh(m) @schedule(m w) @run(m)");
      let d = &report.diagnostics[0];
      assert_eq!((d.code, at(d)), ("keep-not-top-level", (1, 21)));
      // Its initial value is a literal, and its type follows it.
      let report = check("@wire(w {Keep(n 0) 1.5 | Update(n)} looped: true) @mesh(m) @schedule(m w) @run(m)");
      assert_eq!(report.diagnostics[0].code, "variable-type-mismatch");
    }

    /// `n` wires, each running the next through Do, called from the root.
    fn do_chain(n: usize) -> String {
      let mut src = String::new();
      for i in 0..n {
        src.push_str(&format!("@wire(w{i} {{Do(w{})}})\n", i + 1));
      }
      src.push_str(&format!("@wire(w{n} {{1}})\nDo(w0)"));
      src
    }

    #[test]
    fn deep_do_chains_are_a_diagnostic_not_a_crash() {
      // Within the limit it runs, on the stackful scheduler's coroutine
      // stack too.
      assert_eq!(completed(&run(&do_chain(40), &no_defines()), "root"), Var::Int(1));
      for n in [80, 400, 3000] {
        let report = check(&do_chain(n));
        assert_eq!(report.diagnostics[0].code, "too-deep", "{n}");
      }
      // Spawn chains compose recursively too.
      let mut src = String::new();
      for i in 0..200 {
        src.push_str(&format!("@wire(s{i} {{Spawn(s{})}})\n", i + 1));
      }
      src.push_str("@wire(s200 {1})\nSpawn(s0)");
      assert_eq!(check(&src).diagnostics[0].code, "too-deep");
    }

    /// A chain of `n` wires, each running the next through Do inside the
    /// given control-flow wrapper (`{next}` is replaced by the Do).
    fn wrapped_chain(n: usize, wrapper: &str) -> String {
      let mut src = String::new();
      for i in 0..n {
        let body = wrapper.replace("{next}", &format!("Do(w{})", i + 1));
        src.push_str(&format!("@wire(w{i} {{{body}}})\n"));
      }
      src.push_str(&format!("@wire(w{n} {{1}})\nDo(w0)"));
      src
    }

    #[test]
    fn nesting_up_to_the_limit_runs_through_every_control_shard() {
      // Each wrapper adds two or three flow levels per wire; the chain goes
      // as deep as compose allows, and must run on both schedulers (the
      // stackful one on its coroutine stack, in debug builds too).
      for (wrapper, levels) in [
        ("If(All({true} {{next} true}) {1} {2})", 3),
        ("When({true} {{next}})", 2),
        ("Repeat({{next}} times: 1)", 2),
        ("1 | Match([1 {{next}}])", 2),
        ("Maybe({{next}} {2})", 2),
        ("Any({false} {{next} false})", 2),
      ] {
        let n = (shards_core::compose::MAX_FLOW_DEPTH - 2) / levels;
        let src = wrapped_chain(n, wrapper);
        assert!(check(&src).ok(), "{wrapper}: {}", check(&src).to_json());
        assert!(run(&src, &no_defines()).succeeded(), "{wrapper}");
        // The chain is at the limit: one more wire is too deep. If compose's
        // level counting drifts, this fails instead of the test silently
        // running shallower than the limit (and the release CI step with it).
        let deeper = check(&wrapped_chain(n + 1, wrapper));
        assert_eq!(
          deeper.diagnostics.first().map(|d| d.code),
          Some("too-deep"),
          "{wrapper}: n = {n} should be the deepest chain that composes"
        );
      }
    }

    #[test]
    fn maybe_without_else_passes_its_input_through() {
      // As in 1.x: Action runs for its effects, on success or failure.
      let report = run(
        "5 | Maybe({Add(1)}) = a\n5 | Maybe({[1] | Take(3)} silent: true) = b\n[a b]",
        &no_defines(),
      );
      assert_eq!(
        completed(&report, "root"),
        Var::Seq(std::sync::Arc::new(vec![Var::Int(5), Var::Int(5)]))
      );
    }

    #[test]
    fn entry_outcomes_survive_retirement_of_finished_records() {
      // Many one-shot entries finish on the first tick; a looped entry keeps
      // the mesh running. Finished records are drained from the mesh every
      // tick; the report still has every entry's outcome.
      let mut src = String::new();
      for i in 0..64 {
        src.push_str(&format!("@wire(e{i} {{{i}}})\n"));
      }
      src.push_str("@wire(ticker {Pause} looped: true)\n@mesh(m)\n");
      for i in 0..64 {
        src.push_str(&format!("@schedule(m e{i})\n"));
      }
      src.push_str("@schedule(m ticker)\n@run(m iterations: 3)");
      let report = run(&src, &no_defines());
      assert_eq!(report.ticks, 3);
      for i in 0..64 {
        assert_eq!(completed(&report, &format!("e{i}")), Var::Int(i));
      }
      assert!(matches!(report.outcomes.iter().find(|(w, _)| w == "ticker"), Some((_, None))));
    }

    #[test]
    fn code_after_stop_keeps_its_types() {
      assert!(check("1 | Stop\n1 | Add(1)").ok());
      // Nested Whens count one bracket and one brace per level.
      let mut src = String::from("1");
      for _ in 0..30 {
        src = format!("When({{true}} {{{src}}})");
      }
      assert!(check(&src).ok());
    }

    #[test]
    fn unreachable_wire_cycles_are_checked() {
      let report = check("@wire(a {Do(b)})\n@wire(b {Do(a)})\n1");
      assert_eq!(report.diagnostics.len(), 1, "{}", report.to_json());
      assert_eq!(report.diagnostics[0].code, "recursive-wire");
    }

    #[test]
    fn a_valid_program_checks_clean() {
      let report =
        check("@wire(w { 1 | Add(1) })\n@mesh(main)\n@schedule(main w)\n@run(main fps: 30)");
      assert!(report.ok(), "{}", report.to_json());
      assert_eq!(
        report.to_json(),
        "{\"ok\":true,\"file\":\"t.shs\",\"diagnostics\":[]}"
      );
    }
  };
}

#[cfg(stackful)]
mod stackful {
  lang_tests!(shards_core::StackfulMesh);
}

mod stackless {
  lang_tests!(shards_core::Mesh);
}

// --- load errors (scheduler-independent) ---

#[test]
fn syntax_errors_are_located_and_name_language_constructs() {
  let d = load_errors("\"Hello\" | Log(");
  assert_eq!(
    (d[0].phase.name(), d[0].kind, d[0].code),
    ("parse", "syntax", "unclosed")
  );
  assert_eq!(at(&d[0]), (1, 14));
  assert!(
    d[0]
      .message
      .contains("missing `)` for the parameters of `Log` opened at 1:14"),
    "{}",
    d[0].message
  );

  let d = load_errors("@wire(w {\n  1 | Add(1)\n)");
  assert!(
    d[0]
      .message
      .contains("missing `}` for the flow opened at 1:9 (found `)` at 3:1)"),
    "{}",
    d[0].message
  );

  // Several problems in one pass.
  let d = load_errors("1 | Add(1); 2\nnull | Add(1.)");
  let codes: Vec<&str> = d.iter().map(|d| d.code).collect();
  assert_eq!(codes, ["semicolon", "null", "number-form"]);
  assert!(d[2].message.contains("write 1.0"), "{}", d[2].message);
}

/// Labels are lowercase (golden-path.md D3): uppercase names a shard.
#[test]
fn uppercase_labels_are_rejected_with_the_lowercase_fix() {
  let d = load_errors("Repeat({} Times: 3)");
  assert_eq!((d[0].code, at(&d[0])), ("parameter-name", (1, 11)));
  assert!(
    d[0]
      .message
      .contains("parameter labels are lowercase: `times:`"),
    "{}",
    d[0].message
  );
  let d = load_errors("@wire(w {} Looped: true)");
  assert_eq!(d[0].code, "parameter-name");
}

#[test]
fn unknown_names_suggest_the_closest() {
  let d = load_errors("1 | Ad(1)");
  assert_eq!(
    (d[0].phase.name(), d[0].kind),
    ("construct", "unknown-shard")
  );
  assert_eq!(d[0].did_you_mean, ["Add"]);
  assert_eq!(at(&d[0]), (1, 5));

  let d = load_errors("0 | Var(n)\ninc(n)");
  assert!(
    d[0]
      .message
      .contains("shard names start with an uppercase letter"),
    "{}",
    d[0].message
  );
  assert_eq!(d[0].did_you_mean[0], "Inc");

  let d = load_errors("Repeat({} tims: 3)");
  assert_eq!(d[0].code, "unknown-argument");
  assert_eq!(d[0].did_you_mean, ["times"]);
  assert_eq!(at(&d[0]), (1, 11));

  let d = load_errors("@missing");
  assert_eq!(d[0].code, "unknown-define");
  assert!(d[0].message.contains("pass it as `missing:value`"));
}

#[test]
fn the_top_level_declares_when_there_is_a_run() {
  let d = load_errors("@wire(w { 1 })\n@mesh(main)\n@schedule(main w)\n@run(main)\n1 | Add(1)");
  assert_eq!(d[0].code, "code-outside-wire");
  assert_eq!(at(&d[0]), (5, 1));

  let d = load_errors("@mesh(main)\n@schedule(main nope)\n@run(main) | Add(1)");
  let codes: Vec<&str> = d.iter().map(|d| d.code).collect();
  assert!(
    codes.contains(&"piped-declaration") && codes.contains(&"unknown-wire"),
    "{codes:?}"
  );
}

#[test]
fn and_or_point_to_all_and_any() {
  let d = load_errors("true = a\nfalse = b\nIf({a | And | b} {})");
  assert_eq!(d[0].code, "and-or");
  assert_eq!(d[0].did_you_mean, ["All"]);
  assert!(
    d[0].message.contains("`If(All(a b) ...)`"),
    "{}",
    d[0].message
  );
  let d = load_errors("Or");
  assert_eq!(d[0].did_you_mean, ["Any"]);
  let d = load_errors("1 | Match([\"a\"])");
  assert_eq!(d[0].code, "cases");
}

#[test]
fn malformed_input_is_reported_not_crashed_on() {
  let deep = format!("{}1{}", "[".repeat(5000), "]".repeat(5000));
  let d = load_errors(&deep);
  assert_eq!(d[0].code, "too-deep");
  let deep = format!("{}{}", "Repeat({".repeat(500), "} times: 1)".repeat(500));
  assert_eq!(load_errors(&deep)[0].code, "too-deep");
  assert_eq!(load_errors("{x: 1 y:}")[0].code, "missing-value");
  let d = load_errors("Repeat(times: action: {})");
  assert_eq!((d[0].code, at(&d[0])), ("missing-value", (1, 8)));
  assert_eq!(load_errors("0x1g")[0].code, "number-form");
  assert_eq!(
    load_errors("@run(m fps: 1e-300)\n@mesh(m)")[0]
      .param
      .as_deref(),
    Some("fps")
  );
  assert!(
    load_errors("@mesh(m)\n@run(m iterations: 0)")[0]
      .message
      .contains("positive whole number")
  );
  assert_eq!(
    load_errors("@wire(w {})\n@mesh(m)\n@schedule(m w)\n1")[0].code,
    "schedule-without-run"
  );
}

#[test]
fn every_1x_string_escape_is_accepted() {
  let program = Program::load(
    Source::new("t.shs", "\"\\'\\b\\f\\v\""),
    &catalog(),
    &no_defines(),
  );
  assert!(program.is_ok());
}

/// The old assignment forms fail with a plain error naming the word form.
#[test]
fn removed_assignment_forms_say_what_to_write() {
  for (text, code, needle, column) in [
    ("1 >= n", "removed-operator", "`value | Var(name)`", 3),
    (
      "0 | Var(n)\n1 > n",
      "removed-operator",
      "`value | Update(name)`",
      3,
    ),
    (
      "[] | Var(xs)\n1 >> xs",
      "removed-operator",
      "`value | Push(name)`",
      3,
    ),
    ("1 | Set(n)", "removed-shard", "`value | Var(name)`", 5),
    ("1 | Ref(n)", "removed-shard", "`value = name`", 5),
  ] {
    let d = load_errors(text);
    assert_eq!(d[0].code, code, "{text}");
    assert!(d[0].message.contains(needle), "{text}: {}", d[0].message);
    assert_eq!(d[0].column, Some(column), "{text}");
  }
}

#[test]
fn constructs_not_supported_yet_are_rejected_explicitly() {
  for (text, needle) in [
    ("{1: 2}", "table keys are strings"),
    ("[{1: x}]", "table keys are strings"),
    ("Type::Value", "enums"),
    ("#(1 | Add(1))", "evaluation while loading"),
    ("Pause(1.0 | Add(1.0))", "Pause.seconds takes a literal"),
  ] {
    let d = load_errors(text);
    assert!(d[0].message.contains(needle), "{text}: {}", d[0].message);
  }
}
