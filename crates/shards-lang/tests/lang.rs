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
    fn loose_code_runs_as_the_root_wire() {
      let report = run(
        "// word forms and operators
0 >= n
Repeat({Inc(n)} Times: 3)
n | Add(10) = result
When({result | IsMoreEqual(13)} {
  result | Add(1) > n
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
@wire(ticker { Once({0 >= ticks}) Inc(ticks) } Looped: true)
@mesh(main)
@schedule(main answer)
@schedule(main ticker)
@run(main Iterations: 5)",
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
        "main-wire/1:Do/Wire/helper/0:When/Action/3:Math.Add"
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
      let report = check("1 = x\n2 > x");
      let d = &report.diagnostics[0];
      assert_eq!(d.code, "immutable-variable");
      // At `> x`, the assignment that fails.
      assert_eq!(at(d), (2, 3));
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
      let report = run("1 >> xs\n2 >> xs\nxs", &no_defines());
      assert_eq!(
        completed(&report, "root"),
        Var::Seq(std::sync::Arc::new(vec![Var::Int(1), Var::Int(2)]))
      );
      let report = check("1 >> xs\n\"s\" >> xs");
      assert_eq!(report.diagnostics[0].code, "variable-type-mismatch");
      assert_eq!(at(&report.diagnostics[0]), (2, 5));
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
\"ff\" | ParseInt(Base: 16) | ToHex = hex
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
\"gone\" | Match([\"biting\" {\"hook\"} \"gone\" {\"despawned\"} none {\"other\"}] Passthrough: false) = c
\"x\" | Match([\"biting\" {\"hook\"} none {\"other\"}] Passthrough: false) = d
3 | Match([1 {\"one\"}]) = e
Maybe({[1 2] | Take(5)} {\"fallback\"} Silent: true) = f
Maybe({\"fine\"} {\"fallback\"}) = g
true = far
false = close
All(far {n | IsMore(1)}) = h
All(far close) = i
Any(close {n | Is(5)}) = j
0 >= count
Repeat({Inc(count)} Until: {count | IsMoreEqual(3)})
Repeat({Inc(count)} Until: {count | IsMoreEqual(100)} Times: 2)
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
          "0 >= ticks
If({true} {Pause Inc(ticks)})
All({Pause true} {Inc(ticks) true})
1 | Match([1 {Pause Inc(ticks)}])
Maybe({Pause [1] | Take(3)} {Inc(ticks)})
Repeat({Pause Inc(ticks)} Until: {ticks | IsMoreEqual(7)})
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
      assert_eq!(d.path_string(), "root/2:All/Conditions/#1/1:Math.Add");
      assert_eq!(at(d), (2, 14));
    }

    // --- review findings (Astra and Opus, 2026-10-05) ---

    #[test]
    fn a_failed_once_runs_again_instead_of_exposing_unassigned_variables() {
      // The first attempt fails inside Once before assigning x; Maybe's Else
      // fixes the index. Once must run again (not count as done), so x is
      // assigned before it is read: never `x: none`.
      let (report, lines) = shards_core::log::capture(|| {
        run(
          "1 >= k
Repeat({
  Maybe({
    Once({[10] | Take(k) >= x})
    x | Log(\"x\")
  } {Math.Dec(k)} Silent: true)
} Times: 2)",
          &no_defines(),
        )
      });
      assert!(report.succeeded());
      assert_eq!(lines, ["x: 10"]);
    }

    #[test]
    fn temporaries_are_scoped_per_inlined_occurrence() {
      let report = run("@wire(w {f\"{Add(1)}\"})\n1 | Do(w)\n1.0 | Do(w)", &no_defines());
      assert_eq!(completed(&report, "root"), Var::string("2.0"));
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
When(a {3 >= y})
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
    fn push_restarts_its_sequence_each_iteration() {
      let (_, lines) = shards_core::log::capture(|| {
        run(
          "@wire(w {1 >> s  s | Count | Log(\"cleared\")  1 | Push(kept Clear: false)  kept | Count | Log(\"kept\")} Looped: true)
@mesh(m)
@schedule(m w)
@run(m Iterations: 3)",
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
      let report = run("0 >= n\n[n (Inc(n) n)]", &no_defines());
      assert_eq!(
        completed(&report, "root"),
        Var::Seq(std::sync::Arc::new(vec![Var::Int(0), Var::Int(1)]))
      );
    }

    #[test]
    fn until_may_assign_what_action_reads() {
      let report = run("0 >= n\nRepeat({x | Log} Until: {n | ToString >= x  Inc(n) | IsMore(2)})\nn", &no_defines());
      assert_eq!(completed(&report, "root"), Var::Int(3));
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

    #[test]
    fn push_initialized_in_once_keeps_its_sequence() {
      let (report, lines) = shards_core::log::capture(|| {
        run(
          "@wire(w {Once({0 >> s})  s | Count | Log} Looped: true)
@mesh(m)
@schedule(m w)
@run(m Iterations: 3)",
          &no_defines(),
        )
      });
      assert!(report.succeeded());
      assert_eq!(lines, ["1", "1", "1"]);
      // The declaring Push starts the sequence over each iteration; pushes
      // after it (here in a Repeat) grow it within the iteration.
      let (_, lines) = shards_core::log::capture(|| {
        run(
          "@wire(w {0 >> s  Repeat({1 >> s} Times: 2)  s | Count | Log} Looped: true)
@mesh(m)
@schedule(m w)
@run(m Iterations: 2)",
          &no_defines(),
        )
      });
      assert_eq!(lines, ["3", "3"]);
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
        ("Repeat({{next}} Times: 1)", 2),
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
        "5 | Maybe({Add(1)}) = a\n5 | Maybe({[1] | Take(3)} Silent: true) = b\n[a b]",
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
      src.push_str("@wire(ticker {Pause} Looped: true)\n@mesh(m)\n");
      for i in 0..64 {
        src.push_str(&format!("@schedule(m e{i})\n"));
      }
      src.push_str("@schedule(m ticker)\n@run(m Iterations: 3)");
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
        check("@wire(w { 1 | Add(1) })\n@mesh(main)\n@schedule(main w)\n@run(main FPS: 30)");
      assert!(report.ok(), "{}", report.to_json());
      assert_eq!(
        report.to_json(),
        "{\"ok\":true,\"file\":\"t.shs\",\"diagnostics\":[]}"
      );
    }
  };
}

#[cfg(not(target_family = "wasm"))]
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

#[test]
fn unknown_names_suggest_the_closest() {
  let d = load_errors("1 | Ad(1)");
  assert_eq!(
    (d[0].phase.name(), d[0].kind),
    ("construct", "unknown-shard")
  );
  assert_eq!(d[0].did_you_mean, ["Add"]);
  assert_eq!(at(&d[0]), (1, 5));

  let d = load_errors("0 >= n\ninc(n)");
  assert!(
    d[0]
      .message
      .contains("shard names start with an uppercase letter"),
    "{}",
    d[0].message
  );
  assert_eq!(d[0].did_you_mean[0], "Inc");

  let d = load_errors("Repeat({} Tims: 3)");
  assert_eq!(d[0].code, "unknown-argument");
  assert_eq!(d[0].did_you_mean, ["Times"]);
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
  let deep = format!("{}{}", "Repeat({".repeat(500), "} Times: 1)".repeat(500));
  assert_eq!(load_errors(&deep)[0].code, "too-deep");
  assert_eq!(load_errors("{x: 1 y:}")[0].code, "missing-value");
  let d = load_errors("Repeat(Times: Action: {})");
  assert_eq!((d[0].code, at(&d[0])), ("missing-value", (1, 8)));
  assert_eq!(load_errors("0x1g")[0].code, "number-form");
  assert_eq!(
    load_errors("@run(m FPS: 1e-300)\n@mesh(m)")[0]
      .param
      .as_deref(),
    Some("FPS")
  );
  assert!(
    load_errors("@mesh(m)\n@run(m Iterations: 0)")[0]
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

#[test]
fn constructs_not_supported_yet_are_rejected_explicitly() {
  for (text, needle) in [
    ("{1: 2}", "table keys are strings"),
    ("[{1: x}]", "table keys are strings"),
    ("Type::Value", "enums"),
    ("#(1 | Add(1))", "evaluation while loading"),
    ("Pause(1.0 | Add(1.0))", "Pause.Seconds takes a literal"),
  ] {
    let d = load_errors(text);
    assert!(d[0].message.contains(needle), "{text}: {}", d[0].message);
  }
}
