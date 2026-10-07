# Current state

Checkpoint: 2026-10-07. Check `git log` and the working tree for later changes. The plan for current work is [golden-path.md](golden-path.md); it overrides older statements elsewhere. Earlier checkpoints, review rounds and the 1.x deviation list are in [history.md](history.md).

## Implemented

- **Core** (`shards-core`): compose once, instantiate many (shared immutable `Compiled`, small per-instance `State`), a content-keyed compose cache with recorded dependencies, frame slots with definite-initialization checks, exactly-once cleanup through shared lifecycle helpers.
- **One scheduler:** `Mesh` runs instances on an iterative, directly resumable engine (`stackless/`: generation-tagged frame arena, central control continuations, iterative initialization and cleanup, a per-mesh call-depth limit). The same engine runs on native, wasm (WASI) and ESP-IDF. The stackful prototype scheduler was deleted after the M4 gate.
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
| M4 trampoline | gate closed at `eeab0fc` ([verification](../.agent-handoffs/verifications/2026-10-07-eeab0fc-claude-3b7c1e.md)); stackful deleted at `744d6cb`. Astra review of the deletion commit pending |
| M5 functions | implemented, verified offline only (see below), Astra review pending: `@fn` with input/output/params/stateful/pure/uses/mutates, calls written like shards, private frames per invocation, stateful call sites, `Return`, mesh access declared never inferred, `pure` checked, reload on functions (§11: selection at entry, admission, `Keep` migration, reset policy and report), `Do` deleted. Tests A to K pass. Remaining for the gate: the authoring eval rerun (needs a model CLI and the network) |
| M6, M7 | not started |

The code accepts the M2 syntax plus M5's `@fn`. `Do` is gone: a named body is a function with its own frame, and `Return` ends the nearest function (or the root iteration).

**M5 notes.** A function declared `input: None` ignores its input (so it can start a statement anywhere) and binds `input` to `none`. Mesh bindings inside bodies are resolved against the mesh the body is composed for and recorded as dependencies (the body cache is per mesh), rather than linked as requirement indices at instantiation as §4 sketches; revisit if bodies are ever shared across meshes. Recursion is rejected at compose (`recursive-function`) until M7. Under preserving reload, a caller whose own definition did not change keeps its compiled body even when a callee changed, since call sites select callees by name at entry; admission compares inferred effects and mesh access, so an edit that gives a function a new effect or a new mesh access is rejected until the callers are restarted. The entity benchmark's think step is a function now (`bench::functions`), which changes the mixed workload measured in M4; the trampoline gate numbers refer to the `Do` version.

**Stackful deletion (M4, second commit).** Removed: `runtime/` (the coroutine mesh), `shards/stackful.rs`, `corosensei`, both `build.rs` files and `cfg(stackful)`, `--stackful`, the per-scheduler test macros, and the `Backend` trait with its type parameter. What remains is one contract: `Shard` in `shard.rs` (the former stackless one; `Step` is `Flow` plus `Suspend`, `LeafShard` keeps `Flow`), `ShardType::new(desc).implemented_by::<S>()`, non-generic `ComposeCtx<'_>`, `CompiledWire`, `CompiledFlow`, `ComposeCache`; the frontend's `check`, `Program::run` and `Session` take no mesh type, and the `Host`/`SessionHost`/`ReloadHost` traits are gone (`Session::with_mesh` is the host's configuration point). The catalog JSON lost its `backends` field; a described shard without an implementation composes to `not-implemented`. `InstanceMemory` lost `stack_reserved`. The ESP32 acceptance build script no longer expands a macro; it compiles the test files directly.

M2 notes for later milestones:

- Ordinary locals of a *wire* are not yet fresh per root iteration (§3.4); `Keep` is a leaf whose state applies the initial value on first activation. Function invocations get fresh locals (M5); wires still keep theirs across iterations.
- With block scoping, `possibly-uninitialized` is no longer reachable from the shipped shards (every declaration assigns, and nothing declared in a branch escapes); the check stays for host shards that declare without assigning.
- `LeafCtx::iteration` lost its only user (Push's clearing) and is kept for per-iteration locals.

## Baseline findings

- **Near saturation.** Sonnet failed 1 of 114 runs (an off-by-one in its loop), Haiku 4 (three logged one poll too many or too few; one FizzBuzz branch). Haiku leaves some headroom; harder tasks are still needed before the M5 rerun, and the control tag keeps the old syntax runnable to baseline them.
- **First-pass errors** (both models, all trials) are all compose errors except one missing shard: `variable-exists` 6 times (redeclaring with `>=` where `>` was meant; relevant to M2's assignment forms), `input-type-mismatch` 3, `missing-argument` 2, `possibly-uninitialized` 2, and one each of `immutable-variable`, `too-many-arguments`, `unknown-variable`, `wrong-argument-type`. No syntax (parse) errors.
- Haiku writes about 13 times Sonnet's output tokens (mostly thinking).
- From the superseded run: a looped wire already yields once per iteration, so a trailing `Pause` makes each iteration take two ticks; the first primer's example showed it and the model copied it. The primer now explains it. M4 kept that rule: a root iteration yields even if a `Pause` already yielded within it.

## Open items outside the golden path

- The type registry never frees; scoped registries are needed for long `watch` sessions (golden path §7.4 adds a soak test).
- The compose cache is unbounded.
- Recursive types, objects, bytes; broader module ports; browser wasm; graphics and physics.
- The external host is porting its shards to 2.0 ([embedding.md](embedding.md)); it must move to the opaque `Table` API (M1) and, with this checkpoint, drop the mesh type parameters from `check`, `run` and `Session`.

## Verified

**M4 gate at `eeab0fc4d6ed313c4ad4ff5f750eeffddb7c3940`** ([verification record](../.agent-handoffs/verifications/2026-10-07-eeab0fc-claude-3b7c1e.md)): the native workflow ([run 37526212486](https://github.com/sinkingsugar/shards2/actions/runs/37526212486)) and the ESP32 workflow on all three chips ([run 37526212515](https://github.com/sinkingsugar/shards2/actions/runs/37526212515)) pass; the [five-trial performance run at c88c59d](../bench/trampoline/README.md#persistent-call-site-prefix-follow-up) passes both soft gates (pending re-polls 0.285× stackful, mixed flows 0.995× M3 stackless), independently confirmed in [2026-10-06-c88c59d-codex-843667](../.agent-handoffs/verifications/2026-10-06-c88c59d-codex-843667.md). Arena Miri last ran before the metadata/frontend follow-ups; the arena source is unchanged since, and the deletion commit does not touch it. Rerun it when a nightly toolchain is available and record the result here.

**Stackful deletion (`744d6cb`) and M5 (the commits after it, 2026-10-07):** verified so far on this checkout with Rust 1.93 (the pinned 1.98.1 toolchain and the `shards-io` dependencies could not be downloaded on the day's connection): `shards-core` and `shards-lang` unit, acceptance, functions, metadata, trampoline, host-contract, registry and frontend suites pass in debug, docs-off and the release nesting case, and clippy is clean for both crates. Not yet run: `shards-io`, `shards-cli` and `authoring-eval` builds and tests (including the CLI and embedding tests), the `rustls-ring` clippy build, the wasm lint and WASI tests, the ESP32 QEMU build (its `Cargo.lock` still needs `cargo update -w` to drop `corosensei`), formatting with the pinned rustfmt, and the authoring eval rerun. These must pass (locally or in CI on the draft pull request) before the commits are considered verified, and Astra's reviews of the deletion and of M5 are still due.

M2 findings are independently closed in [.agent-handoffs/verifications/2026-10-06-cd7a56e-codex-af86ad.md](../.agent-handoffs/verifications/2026-10-06-cd7a56e-codex-af86ad.md). CI comes from the draft pull request for `golden-path`; ESP32 verification is QEMU only, not a physical-board run. Inspect the checkout and rerun relevant checks before relying on this checkpoint.
