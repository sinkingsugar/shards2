# Verification: inline segment input lifetime
- Record: 2026-10-05-4cbfb0e-claude-ac5c2c
- Author: Claude Code session (review-handoff "verify" invocation; same session that wrote the review)
- Created: 2026-10-05T18:47:07Z
- Review: [inline segment input retention](../reviews/2026-10-05-a003cb7-claude-3dc841.md)
- Resolution: [inline segment input lifetime resolution](../resolutions/2026-10-05-52d5fac-codex-982116.md)
- Verified code: 4cbfb0e8782790b14f8c35938143012aa31d815d (clean checkout). `git diff 52d5fac 4cbfb0e -- crates` is empty, so the runtime and tests equal the resolution's fix commit 52d5fac605e011b6e65c6045bbf1a7da933465b9.

## 2026-10-05-a003cb7-claude-3dc841#F1
- Result: verified.
- Evidence:
  - Inspection: both flow loops (`flow.rs`, `stackless/mod.rs`) now move `value` into `inline::run`, which owns it as `scratch`. Const, Get, Set and Inc reanchor `value` first, then clear `scratch`. Take, Push and the generic Float4 fallback replace `scratch` by assignment, and the assignment drops the old input before Push mutates its slot. The typed numeric arms skip clearing, which is sound: they read the accumulator as Int/Float/Float4, so `scratch` holds at most a resource-free number. At the boundary, `scratch` is moved out when the accumulator points at it and cloned otherwise. I found no path that keeps an owning segment input alive across a frame write, and no pointer is used after its owner is replaced.
  - Negative control, run by me: I put 52d5fac's `tests/prototype.rs` on top of an a003cb7 worktree. `discarded_inline_input_does_not_copy_captured_sequence` failed on both schedulers with "obsolete input forced a copy" (different allocation addresses). At 4cbfb0e it passes.
  - Tests at 4cbfb0e, all passing: `cargo test -p shards-core --test prototype discarded` (6); `--lib inline` (7); `cargo test --release -p shards-core --lib --test prototype --features output-checks` (17 + 87); `cargo test -p shards-lang --test lang` (121); `cargo +nightly-2026-10-03 miri test -p shards-core --lib inline::tests` (7).
  - Timings, 40,000 iterations, single wall-clock runs of `shards2 run`, stackless / stackful:

    | Body before `Push(acc Clear: false)` | 98a75c6 | a003cb7 | 4cbfb0e |
    |---|---:|---:|---:|
    | `[acc]` then `1 \|` (the review repro) | 9 / 9 ms | 2503 / 2495 ms | 5 / 5 ms |
    | `{a: acc}` then `1 \|` | 9 / 13 | 2494 / 2507 | 6 / 6 |
    | `[acc] \| Take(0)` then `1 \|` (Take scratch) | 10 / 12 | 2511 / 2546 | 6 / 6 |
    | `[acc]` then `n \|` (Get replaces the input) | 7 / 13 | 2520 / 2531 | 5 / 5 |
    | `[acc]`, `n >= m`, then `1 \|` (Set first) | 8 / 16 | 2510 / 2501 | 5 / 5 |
    | `[acc] \| Count` then `1 \|` (control) | 8 / 11 | 7 / 11 | 12 / 10 |

    All runs printed `40000`.
- Limits:
  - The timings are single-process wall clock with startup included, not a benchmark. I did not rerun the 25-workload suite. The resolution reports that the hot-chain cost of this fix is Get(Int) at 2.05× 1.x, so 19 of 25 cells now meet the 2× target instead of 20. I did not reproduce that number.
  - I did not rerun the full check set, the WASI runs or clippy, and I did not check CI.
  - The pre-existing passthrough case (`acc | ExpectSeq` then Push, already quadratic at 98a75c6) is outside this finding and still open as a note.
