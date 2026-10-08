# Verification: implementer's checks for the flat control flow commit (aae5c01), for review
- Record: 2026-10-07-aae5c01-claude-0aff1d
- Author: Claude Code implementation session (the session that took the golden-path work over on 2026-10-07)
- Created: 2026-10-07T15:57:06Z
- Review: none yet. This record extends [2026-10-07-7bac245-claude-58967b](2026-10-07-7bac245-claude-58967b.md) with the commit that followed it; it is the implementing session's own evidence, not an independent review, and closes no finding.
- Resolution: not applicable (no findings to respond to).
- Verified code: aae5c01bc2adfd624274332e8cedebb1f0978f24 (`aae5c01`, Lower Repeat, While, When and If to flat code with jumps), followed by e8afac2969f2cec8247fb530c47c3d3ed30557ae (`e8afac2`, benchmark results and docs only); both pushed on `golden-path`, draft PR #3.

## What changed and where to look

Stage one of flat control flow, agreed with the author before it started: `Repeat`, `While`, `When` and `If` are lowered at compose into the parent flow's code; `Match`, `Once`, `Sub`, `Maybe`, `Conditions` and calls keep their frames (the next stages, listed in `docs/current-state.md` under open items). Suggested focus, in order of risk:

1. `crates/shards-core/src/compose.rs`, `ComposeCtx::flatten` and the `Flat` builder: the lowering of each composite (saved input in a hidden slot, loop counter, `JumpIfNot`/`JumpIf`/`Jump`/`LoopTest`, jump targets patched when known, child code relocated and child node indices offset on `append`); `FrameLayout::declare_hidden` and `ComposeCtx::declare_hidden` (hidden slots are ordinary locals: cleared per iteration or at a stateless entry, never visible to names, included in `scratch_slots`).
2. `crates/shards-core/src/flow.rs`, `CompiledFlow::pc_nodes` and `node_at`: every instruction maps to the node it stands for, `NO_NODE` for control instructions; the invariant that a run never stops at a control instruction (they never fail and never suspend) is what makes `node_at` total for the engine.
3. `crates/shards-core/src/stackless/engine.rs`: every former `states[pc]`/`nodes[pc]` is `states[node]`/`nodes[node]` through `Frame::node()`; completion is `pc == code.len()`; `EngineCalls::callee` maps the site's instruction index to its node; the dispatch arms and `child_flow`/`Control::len` for the four composites are unreachable/zero.
4. `crates/shards-core/src/inline.rs`: the jump arms (`continue` skips the increment and the debug output check; the value pointer is left as is), `LoopTest` (counts down an Int in a hidden slot; non-Int is an error), `lower_scratch_releases` restarting its analysis at jump targets, `vm_ready` reduced to call sites, `VmRepeat`/`VmBranch`/`VmLoop`/`vm_repeat_op`/`vm_branch_op` and `VmCalls::child` removed.
5. Semantics preserved by construction, worth a reviewer's check: a child that must suspend keeps its node and resumes where the run stopped; a failing node inside a flattened composite fails the frame, as the composite's own failure did; `Return`, `Stop` and `Restart` propagate as before; `Maybe` still catches through its own child frame; a call site inside a loop that is not ready stops the run at that instruction, the engine enters it there and the loop continues after it (so no readiness check before a loop, and no half-run body).

Deliberate choices worth challenging: hidden slots are allocated per composite (one for the saved input, one for a counter), not shared between sibling composites; the saved input is written with `Set` (a clone for heap values) once per composite entry; the `If` without `else` and without passthrough yields the input when the predicate is false, as the dispatch did; `Predicated` and `RepeatCompiled` keep their `Arc<CompiledFlow>` children (compose reads them; the engine never does); `Control::len` returns 0 for the four composites instead of removing the variants, so a host or test that still constructs such a node gets no frames and a clear `unreachable!` if it is ever dispatched.

## Checks run by the implementing session

- Locally on macOS (aarch64), pinned toolchain 1.98.1, at `aae5c01`: `cargo fmt --check` (workspace and ESP32 example), `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, the `rustls-ring` clippy build, the docs-off tests for core, io and lang, the release nesting case, the wasm lint, the eight WASI suites under Node, and Miri (`nightly-2026-10-03`) on `inline::tests` (including the new jump test) and `arena`.
- CI for `aae5c01`: native passed ([run 37646786516](https://github.com/sinkingsugar/shards2/actions/runs/37646786516)); ESP32 passed on all three chips through all seven suites ([run 37646786600](https://github.com/sinkingsugar/shards2/actions/runs/37646786600)); the classic ESP32 ends the frontend suite with a 17.0 KiB heap low-water mark (QEMU, not a board).
- Tests changed with the semantics: the trampoline suite's dispatch counts count framed levels only (`If` and `Repeat` levels dispatch nothing), the branch test asserts flat code (`jump-if-not` present, two `fallback`s: `Log` and `Maybe`), and the VM unit test covers jumps, the counter, a site that is not ready mid-loop and scratch releases at a target.
- Benchmarks at `aae5c01` (this Mac, interleaved with the `7bac245` binaries): entity 87 to 89 ns per instance tick against 101, heap 1.30 KiB per instance against 1.68; nested resume 37 to 39 ns against 83 to 86; VM fixed cost per iteration 4.3 ns against 10.3 and 1.x's 5.2; `do-int` 1,460 / 5,973 ns at widths 64 / 256 against 1,581 / 6,319 (supplementary run; the full run's `do-int` cells hit an intermittent slow mode of this laptop, explained in `docs/runtime-performance-overview.md`). Recorded under `bench/*/results/2026-10-07-aae5c01` and `bench/vm-execution/results/2026-10-07-aae5c01-do-int`.

## Not done

- Stages two and three of flat control flow: `Match` (jump table), `Once` (hidden flag), `Sub` (inline), `Maybe` (handler table), call inlining (locals base offset).
- The authoring eval reruns for M5 and M7, and independent reviews of every post-gate commit, as in the previous record.

Receiving-context invocation:
`$review-handoff publish` after reviewing `7bac245210054bdbc3a592f866b7120ae2456de3..e8afac2969f2cec8247fb530c47c3d3ed30557ae` on `golden-path` against `docs/golden-path.md`, starting from `.agent-handoffs/verifications/2026-10-07-aae5c01-claude-0aff1d.md`.
