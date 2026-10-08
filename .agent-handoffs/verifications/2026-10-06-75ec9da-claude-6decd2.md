# Verification: M4 implementation checkpoint
- Record: 2026-10-06-75ec9da-claude-6decd2
- Author: Claude Code review session
- Created: 2026-10-06T19:58:37Z
- Review: [2026-10-06-ab4bc6b-codex-ace9ae](../reviews/2026-10-06-ab4bc6b-codex-ace9ae.md)
- Resolution: [2026-10-06-ab4bc6b-codex-98c68f](../resolutions/2026-10-06-ab4bc6b-codex-98c68f.md)
- Verified code: 75ec9dacc8ac7d41d07c821512a21d8cf2d26dc2 on `golden-path`, clean working tree. Its implementation code is identical to ab4bc6b744701b503e0524a2e1783b02dc14455c (`git diff --stat ab4bc6b 75ec9da` lists only handoff records, `bench/trampoline` evidence and two docs).
- Not rerun: the full `AGENTS.md` check set, WASI, Miri, macOS, and any device build (device results below are read from GitHub Actions logs). Compose and reload internals were inspected only through the M4 diff.

**Summary: the device gate is still failing, so M4 is not closed and stackful stays. No other blocker found.** New findings from this pass are in [2026-10-06-75ec9da-claude-fd6500](../reviews/2026-10-06-75ec9da-claude-fd6500.md).

## 2026-10-06-ab4bc6b-codex-ace9ae#F1
- Result: still-failing (the resolution's `deferred` is accurate)
- Evidence: GitHub Actions, ESP32 workflow: run 37514844876 at ab4bc6b and run 37521678341 at 75ec9da both fail on `esp32` and `esp32c3` and pass on `esp32s3`. From the 75ec9da logs: `esp32` aborts in `deep_do_chains_are_a_diagnostic_not_a_crash` (test 117, `memory allocation of 1144 bytes failed`); `esp32c3` aborts in `code_after_stop_keeps_its_types` (test 121, allocations of 80 and 8 bytes); `esp32s3` runs all 139 tests.
- Evidence, the budget: on S3 the `lang` suite takes the main stack low-water mark from 118,084 to 23,928 bytes free, so parsing and composing use about 107 KB of the 128 KiB stack, and the heap low-water mark from 108,504 to 22,076 bytes. The `trampoline` suite moves neither mark. Boot logs list about 309 KiB of data RAM on ESP32, 325 KiB on C3 and 392 KiB on S3; after the 128 KiB stack that leaves about 181, 197 and 264 KiB of heap, against a peak of about 242 KiB that S3 actually used. The two failing chips are short by tens of kilobytes, not by a fragmentation accident.
- Assessment of the 128 KiB reservation: it cannot be lowered while compose recurses (107 KB used), and it is a third or more of the RAM on ESP32 and C3. S3 passing with 24 KB of stack and 22 KB of heap to spare is not evidence for the other two.
- Lead: [2026-10-06-75ec9da-claude-fd6500#F1](../reviews/2026-10-06-75ec9da-claude-fd6500.md) measures where the heap goes (parse and lower first, compose second, the engine a small share).
- Limits: QEMU only; no device build was run by this session.

## Claim 1: the engine (`stackless/arena.rs`, `stackless/engine.rs`)
- Result: verified, no findings
- Evidence, inspection: read both files, `stackless/mod.rs` and the runtime diff. Frames hold owned handles and `Arc`s to their compiled owner, never references into the arena; the stackless module contains no `unsafe`. Activation, initialization and cleanup are loops over explicit work lists. A suspended leaf is re-entered through `current` without touching a control node. Every non-suspend exit resets the continuation and the frame's program counter once per level. A failed build cleans exactly the initialized states of its subtree; `Do` swaps its body only at phase 0 and rebuilds an empty child on the next entry.
- Evidence, tests: `cargo test -p shards-core --test prototype --test metadata --test trampoline --test host_contract --test registry` and `cargo test -p shards-lang --test lang` all passed (87, 32, 6, 10, 1 and 132); `prototype` and `trampoline` also passed in release.
- Evidence, differential: three new scripts that suspend inside `Once`, `Maybe`, `All`, `Any`, `While` predicates, `Match` cases, `Repeat` `until:`, `SubFlow`, `Do` and spawned wires, raise errors through `Maybe` after a suspension, and `Stop` from a nested loop. `shards2 run --json` output is byte-identical between the trampoline and `--stackful`, and identical to a `shards2` built from c0aaa39.
- Limits: `Return` and `Restart` are not reachable from scripts; they are covered only by the Rust suites. Miri not rerun (and the arena has no unsafe code for it to check).

## Claim 2: shared metadata (`signature.rs`, `compose.rs`, `reload.rs`)
- Result: verified
- Evidence: `shards2 check --json` output, which carries every occurrence path and type, is byte-identical between c0aaa39 and this snapshot for four scripts (up to 30 KB of report), including one where the same wires are reached through several `Do` sites at different depths. The occurrence node is always inserted at index 0, so the tree keeps pre-order. Dependency and key comparisons stay by content.
- Limits: reload validity after the `Arc` changes was checked only through the passing `preserving_reload` tests.

## Claim 3: device test generation and fixture changes
- Result: verified for coverage accounting; the budget question is answered under F1
- Evidence: natively the six suites have 150 non-stackful tests; the S3 run executed 139. The 11 missing are the 9 `cfg(panic = "unwind")` tests, the file watcher test and the allocation-count test, all excluded by an explicit `cfg`. Device-only reductions (`do_chain(20)`, one over-limit chain, an 80-wire spawn chain, parser inputs just over the limit) still cross each limit they test.
- Limits: one reduction applies to native too; see [2026-10-06-75ec9da-claude-fd6500#F2](../reviews/2026-10-06-75ec9da-claude-fd6500.md).

## Claim 4: performance evidence
- Result: verified
- Evidence: recomputed medians from `bench/trampoline/2026-10-06-shared-gate.csv` (195 rows, 5 per cell): pending depth 32 is 16.64 ns against 54.49 ns stackful (0.305x), mixed 118.0 against 112.8 for M3 (+4.61%). Reproduced here with three alternating runs: pending depth 32 at 16.3 to 16.9 ns against 54.7 to 55.1 ns stackful; mixed 119.5 to 120.5 against 113.4 to 119.2 for a c0aaa39 build. Per-instance state measured 1,306 bytes against 978, as the runtime overview states; instance creation measured 2.15 against 1.20 microseconds.
- Limits: same machine class as the original run, other work running, no pinning.

Status: `2026-10-06-ab4bc6b-codex-ace9ae#F1` remains open (still failing). The four implementation claims are verified for this snapshot. Published in the shared checkout only; not committed.
