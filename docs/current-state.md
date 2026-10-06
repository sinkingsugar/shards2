# Current state

Checkpoint: 2026-10-06. Check `git log` and the working tree for later changes. The plan for current work is [golden-path.md](golden-path.md); it overrides older statements elsewhere. Earlier checkpoints, review rounds and the 1.x deviation list are in [history.md](history.md).

## Implemented

- **Core** (`shards-core`): compose once, instantiate many (shared immutable `Compiled`, small per-instance `State`), a content-keyed compose cache with recorded dependencies, frame slots with definite-initialization checks, exactly-once cleanup through shared lifecycle helpers.
- **Two schedulers:** stackless (default) and stackful, one per mesh. Leaf and async shards have one implementation; control-flow shards have two. Stackless runs on wasm (WASI) and ESP-IDF.
- **VM:** compose-selected builtin instructions with a borrowed accumulator; arithmetic near 1.x parity, collections 3 to 4x slower ([runtime overview](runtime-performance-overview.md)).
- **I/O** (`shards-io`): `Http.Get` on a shared Tokio runtime, with cancellation verified.
- **Values and types:** Float2/3/4, string-keyed sorted tables (opaque `Table`, a `BTreeMap` inside), fixed/open table types, unions, `Never`, one acceptance rule ([values-and-types.md](values-and-types.md)).
- **Metadata:** every catalog shard has a `ShardDesc`; structured diagnostics with occurrence paths; JSON catalog, describe and search.
- **Frontend** (`shards-lang`, `shards-cli`): hand-written parser with spans and recovery, lowering to wire definitions, `check --json`, `run`, `watch`, state-preserving hot reload ([embedding.md §5](embedding.md#5-warm-sessions-and-hot-reload)).
- **ESP32:** firmware example linked and booted in QEMU for three chips in CI; no physical-board run claimed.

## Golden path progress

| Milestone | Status |
|---|---|
| M0 docs hygiene | done (this file, `history.md`, README) |
| M0 authoring eval | done: 38 tasks with references checked by `cargo test`, the `current` primer, a runner for any model CLI that judges `shards2 run --json` logs ([README](../bench/authoring/README.md)). Baseline in `bench/authoring/results/2026-10-06-baseline` (3 trials, `shards2` from tag `authoring-control-v1`, Claude Code 2.1.291): `claude-sonnet-5-5` 113 of 114 correct, 112 clean first passes; `claude-haiku-4-5-20251001` 110 of 114, 101 clean first passes. The first single-trial run is kept, superseded, in `results/2026-10-06-sonnet-primer-v1` |
| M1 opaque collections | done: `Var::Table(Table)` with private storage, `as_table`/`as_seq`/`as_str`, `TableBuilder`, sorted iteration (`host_contract::hosts_use_opaque_collections`) |
| M2 surface syntax | done, review fixes verified at `cd7a56e`: lowercase labels (D3, enforced by the parser and `Catalog::new`); `= x`, `Var`, `Update`, `Push` (no declare or clear), `Keep` at a wire's top level (D7); `>=`, `>`, `>>`, `Set`, `Ref` removed with plain errors; blocks scope their declarations, no shadowing (`duplicate-binding` with a related location), `reserved-name` for `input`; exhaustive `Match` with `default:` (D8). Tests E and F pass (F's function case waits for M5) |
| M3 signatures and effects | done, local full checks passed and [Astra review](../.agent-handoffs/reviews/2026-10-06-cd7a56e-codex-76b308.md) found no blockers: common owned/borrowed signature, classified core effects/lifetimes, transitive mesh/effect inference, catalog/describe metadata and source-located occurrence types in `check --json` |
| M4 trampoline | implemented; gate checks in progress. Direct leaf resume, iterative initialization/cleanup, generation-tagged frames, central control continuations, call-depth limit. Stackful remains until native/WASI/device checks, benchmarks and Astra review pass |
| M5 to M7 | not started |

The code accepts the M2 syntax. `Do` stays until M5 replaces it with functions; a wire inlined by `Do` still shares its caller's variables, and its top level counts as a wire top level for `Keep`.

M2 notes for later milestones:

- Ordinary locals are not yet fresh per root iteration (§3.4); `Keep` is a leaf whose state applies the initial value on first activation. M4 preserves the existing scope/storage semantics; the private parameter/Keep/local frame layout lands with M5.
- With block scoping, `possibly-uninitialized` is no longer reachable from the shipped shards (every declaration assigns, and nothing declared in a branch escapes); the check stays for host shards that declare without assigning.
- `LeafCtx::iteration` lost its only user (Push's clearing) and is kept for per-iteration locals.

## Baseline findings

- **Near saturation.** Sonnet failed 1 of 114 runs (an off-by-one in its loop), Haiku 4 (three logged one poll too many or too few; one FizzBuzz branch). Haiku leaves some headroom; harder tasks are still needed before the M5 rerun, and the control tag keeps the old syntax runnable to baseline them.
- **First-pass errors** (both models, all trials) are all compose errors except one missing shard: `variable-exists` 6 times (redeclaring with `>=` where `>` was meant; relevant to M2's assignment forms), `input-type-mismatch` 3, `missing-argument` 2, `possibly-uninitialized` 2, and one each of `immutable-variable`, `too-many-arguments`, `unknown-variable`, `wrong-argument-type`. No syntax (parse) errors.
- Haiku writes about 13 times Sonnet's output tokens (mostly thinking).
- From the superseded run: a looped wire already yields once per iteration, so a trailing `Pause` makes each iteration take two ticks; the first primer's example showed it and the model copied it. The primer now explains it. Decide in M4 whether a looped wire should yield both times.

## Open items outside the golden path

- The type registry never frees; scoped registries are needed for long `watch` sessions (golden path §7.4 adds a soak test).
- The compose cache is unbounded.
- Recursive types, objects, bytes; broader module ports; browser wasm; graphics and physics.
- The external host is porting its shards to 2.0 ([embedding.md](embedding.md)); it must move to the opaque `Table` API (M1).

## Verified

M4 commit `e2c5dad` passed Linux/macOS and WASI CI. Its expanded ESP32 acceptance gate failed: both Xtensa targets exhausted heap in the deep-call case, and the RISC-V target hit generated-source lint errors. The follow-up stores occurrence metadata as shared relative-path trees instead of copying descendants into every ancestor, preserves source-report paths with a regression test, and fixes generated-runner lint handling and watchdog yielding. Full local checks pass again; the device gate remains pending until the follow-up CI run succeeds. Stackful is retained.

Locally on Linux x86-64 for M4 before stackful deletion: the full check set in `AGENTS.md` (fmt, clippy native, rustls and wasm, workspace tests on both schedulers in debug and release, docs-off, release nesting, WASI suites including trampoline, Python benchmark-runner tests, targeted arena Miri). M2 review findings are independently closed in [.agent-handoffs/verifications/2026-10-06-cd7a56e-codex-af86ad.md](../.agent-handoffs/verifications/2026-10-06-cd7a56e-codex-af86ad.md). CI results come from the draft pull request for `golden-path`; macOS and the ESP32 build are verified only there. Re-run the check set before relying on any status above.

The metadata follow-up `0692517` passed Linux/macOS/WASI CI and both repeated benchmark limits. Device lint passed, but deep-call QEMU tests exposed the smoke example’s insufficient task stack (Xtensa compose backtrace) and the 200-wire Spawn rejection fixture’s lexer allocation (C3 ELF backtrace). A 96 KiB stack and device-only 80-wire over-limit Spawn fixture are pending device verification; compose limit 48 and trampoline depth cases are unchanged.

The 96 KiB always-reserved stack (`8818cff`) starved the ESP32 100-instance case; C3 passed the deep-call case but exhausted heap in nested analysis. The next follow-up uses temporary per-suite stacks, releases the smoke program before acceptance, and makes `Program::compose` avoid unused tooling reports. macOS also exposed a 15 ms timing assertion in the blocking-I/O test; it now uses explicit worker release after tick returns. Device and CI results for these follow-ups remain pending.
