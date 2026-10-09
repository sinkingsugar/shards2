# Tasks 43 and 44 (flow parameters, M9): Haiku and Sonnet

Task 43 asks for a function `Retry` taking a block (`Flow`) that runs it until it succeeds, at most `times` times, failing with the last run's error, used twice (a block that succeeds on its third run, and one that always fails, with the failure caught). Task 44 asks for a function `Timed` that runs a block once and outputs the seconds it took.

| Task | Model | Correct output | Clean first check | Repairs | Output tokens (per trial) |
|---|---|---:|---:|---|---|
| 43 | `claude-haiku-4-5-20251001` | 1/3 | 0/3 | 2, 1, 3 | 29,084, 15,865, 41,846 |
| 43 | `claude-sonnet-5-5` | 3/3 | 1/3 | 0, 1, 1 | 1,264, 2,502, 2,346 |
| 44 | `claude-haiku-4-5-20251001` | 3/3 | 3/3 | 0 | 6,898, 6,310, 7,029 |
| 44 | `claude-sonnet-5-5` | 3/3 | 3/3 | 0 | 602, 579, 591 |

Every reply declares the block parameter with `Flow(...)` and runs it with `Run`, and every final `Retry` stops at the first success and fails with the last run's error.

**First-pass failures** (task 43; task 44 had none): no error class M9 introduced (`flow-escapes`, `control-in-flow`, `not-a-flow`) appears. The classes are:
- `output-type-mismatch`, 5 of 6 trials and 12 of the 13 compose errors: `Retry` declared `output: None` with a body ending on a value (`... but its body outputs Int`). This is the function rule from M5 (a body's output must fit the declared output), not about blocks: a function written for its effects ends on whatever its last shard outputs, and the models expect `output: None` to discard it.
- One Haiku trial wrote `IsLess(times - 1)` (`syntax`: infix arithmetic is not part of the language); one also hit `too-many-arguments` and `missing-argument`.

**Haiku's two wrong outputs** on task 43 have correct retry logic and output: their `Maybe` around the failing call is not `silent`, so it logs the caught error, a line the expected log does not have. The prompt says only that the call "must not stop the program".

One finding changed the language before this run: in a first run of the same tasks (same models and trials, primer without it, not kept), a Haiku reply declared `Flow(input: None output: None)` for a block whose output it did not need, and its block's `Int` output was rejected. `output: None` now discards the block's output and `Run` outputs none, mirroring `input: None` (docs/metaprogramming.md §3.5); the primer says so. That first run also showed the `output-type-mismatch` above (both models) and two Haiku replies running one attempt too many; task 44 passed 6/6 clean.

Three trials per model on two tasks are evidence, not a measure of difficulty.

## Provenance

- **Calls:** real model calls on 2026-10-09 through Claude Code 2.1.295, as part of M9's gate.
- **Isolation:** tools, external MCP configuration and session persistence were disabled. Only the public reference (variant `functions`) and the task prompt were supplied; the hidden reference solution was excluded.
- **Snapshot:** runtime and runner from the `metaprogramming` working tree on top of `6977df0` (the M9 commit that records these results), built in release (Rust 1.98.1, macOS aarch64).
- **Hashes (SHA-256, first 16 hex digits):** `tasks/43-retry-block.task` `260a7e3852a8ad91`, `tasks/44-timed-block.task` `c6a583ae76c50cb8`, `reference/functions.md` `6fbc3edaca5c3765`.
- **References:** all 44 passed `authoring-eval verify` through that binary first.

```sh
authoring-eval run --variant functions --only TASK \
  --model MODEL --format claude-json \
  --cmd 'claude -p --model MODEL --output-format json --tools "" --setting-sources "" --strict-mcp-config --no-session-persistence' \
  --jobs 3 --trials 3 --shards2 $CARGO_TARGET_DIR/release/shards2 \
  --out SHORT-TASK.csv --transcripts transcripts/SHORT
```

`TASK` was 43 or 44, `MODEL` `claude-haiku-4-5-20251001` (`SHORT` `haiku`) or `claude-sonnet-5-5` (`SHORT` `sonnet`); the four ran concurrently. Results are in the four CSV files, with complete transcripts under `transcripts/`.
