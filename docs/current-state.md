# Current state

Checkpoint: 2026-10-06. Check `git log` and the working tree for later changes. The plan for current work is [golden-path.md](golden-path.md); it overrides older statements elsewhere. Earlier checkpoints, review rounds and the 1.x deviation list are in [history.md](history.md).

## Implemented

- **Core** (`shards-core`): compose once, instantiate many (shared immutable `Compiled`, small per-instance `State`), a content-keyed compose cache with recorded dependencies, frame slots with definite-initialization checks, exactly-once cleanup through shared lifecycle helpers.
- **Two schedulers:** stackless (default) and stackful, one per mesh. Leaf and async shards have one implementation; control-flow shards have two. Stackless runs on wasm (WASI) and ESP-IDF.
- **VM:** compose-selected builtin instructions with a borrowed accumulator; arithmetic near 1.x parity, collections 3 to 4x slower ([runtime overview](runtime-performance-overview.md)).
- **I/O** (`shards-io`): `Http.Get` on a shared Tokio runtime, with cancellation verified.
- **Values and types:** Float2/3/4, string-keyed sorted tables (`Arc<BTreeMap>`, still public), fixed/open table types, unions, `Never`, one acceptance rule ([values-and-types.md](values-and-types.md)).
- **Metadata:** every catalog shard has a `ShardDesc`; structured diagnostics with occurrence paths; JSON catalog, describe and search.
- **Frontend** (`shards-lang`, `shards-cli`): hand-written parser with spans and recovery, lowering to wire definitions, `check --json`, `run`, `watch`, state-preserving hot reload ([embedding.md §5](embedding.md#5-warm-sessions-and-hot-reload)).
- **ESP32:** firmware example linked and booted in QEMU for three chips in CI; no physical-board run claimed.

## Golden path progress

| Milestone | Status |
|---|---|
| M0 docs hygiene | done (this file, `history.md`, README) |
| M0 authoring eval | done: 38 tasks with references checked by `cargo test`, the `current` primer, a runner for any model CLI ([README](../bench/authoring/README.md)). Baseline: Sonnet via Claude Code 2.1.291, 2026-10-06, at 40cb29b (`baseline.csv`, transcripts in `bench/authoring/transcripts/baseline-sonnet`). Other models not run yet |
| M1 to M7 | not started |

The current syntax (`>=`, `>`, `>>`, `Do`, uppercase labels) is what the code accepts until M2 and M5 land.

## Baseline findings

- The tasks are easy for Sonnet on today's syntax (one wrong output, two one-round repairs), so later variant runs need harder tasks or weaker models to discriminate.
- Wrong output (`27-persistent-total`): a looped wire already yields once per iteration, so a `Pause` at its end makes each iteration take two ticks. The primer's looped example shows `Pause`, and the model copied it. Decide in M4 whether a looped wire should yield both times; until then the primer should say it.
- Repairs: a variable declared inside a `While` body read after it (`may be uninitialized`), and an open-table `Take` giving `Int | None` that needed `ExpectInt`.

## Open items outside the golden path

- The type registry never frees; scoped registries are needed for long `watch` sessions (golden path §7.4 adds a soak test).
- The compose cache is unbounded.
- Recursive types, objects, bytes; broader module ports; browser wasm; graphics and physics.
- The external host is porting its shards to 2.0 ([embedding.md](embedding.md)); M1 changes the table API it uses.

## Verified

This checkpoint is a documentation change. The last code checkpoint and its review are recorded in [history.md](history.md). Re-run the check set in `AGENTS.md` before relying on any status above.
