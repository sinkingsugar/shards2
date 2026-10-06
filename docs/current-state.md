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
| M2 to M7 | not started |

The current syntax (`>=`, `>`, `>>`, `Do`, uppercase labels) is what the code accepts until M2 and M5 land.

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

Locally on Linux x86-64 at the commit resolving the M0/M1 reviews: the full check set in `AGENTS.md` (fmt, clippy native, rustls and wasm, workspace tests on both schedulers, docs-off, release nesting, WASI suites). CI results come from the draft pull request for `golden-path`; macOS and the ESP32 build are verified only there. Re-run the check set before relying on any status above.
