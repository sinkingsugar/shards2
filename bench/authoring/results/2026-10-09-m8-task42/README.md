# Task 42 (compose-time value, M8): Haiku and Sonnet

Task 42 asks for a function `Label` and a `Log` prefix computed from it at compose time (`Log`'s `prefix` takes only a literal). All **6/6** trials logged the expected lines on the **first check, with no repairs**:

| Model | Correct output | Clean first check | Repairs | Output tokens (per trial) |
|---|---:|---:|---:|---|
| `claude-haiku-4-5-20251001` | 3/3 | 3/3 | 0 | 4,544, 3,675, 1,351 |
| `claude-sonnet-5-5` | 3/3 | 3/3 | 0 | 337, 490, 443 |

Every final reply computes the prefix with `#( )`:
- two Haiku replies declare it once, `@const(prefix #( Label(name: "build" number: 7) ))`, and log with `Log(@prefix)`;
- the other four write `Log(#( Label(name: "build" number: 7) ))` at each `Log`.

None writes the literal `"build-7"`. All six log the three numbers with three separate statements rather than a loop. The prompt allows that, and the expected log cannot tell the two apart. The reference solution uses a `Repeat`.

Three trials per model on one task are evidence that the primer's `#( )` and `@const` sections are usable, not a measure of difficulty: the task is small.

## Provenance

- **Calls:** real model calls on 2026-10-09 through Claude Code 2.1.295, as part of finishing M8 at the author's request.
- **Isolation:** tools, external MCP configuration and session persistence were disabled. Only the public reference (variant `functions`) and the task prompt were supplied; the hidden reference solution was excluded.
- **Snapshot:** runtime and runner at `3ceac16` on `metaprogramming`, built in release (Rust 1.98.1, macOS aarch64).
- **Hashes (SHA-256, first 16 hex digits):** task file `fe1efdb48aafb0e1`, `reference/functions.md` `f8eca676bbb3953e`.
- **References:** all 42 passed `authoring-eval verify` through that binary first. Without `--shards2`, the runner picked the checkout's stale `target/release/shards2` (built elsewhere: this machine sets `CARGO_TARGET_DIR`), with which the task 42 reference failed with `computed-literal`. The runner now honors `CARGO_TARGET_DIR`.

```sh
authoring-eval run --variant functions --only 42 \
  --model MODEL --format claude-json \
  --cmd 'claude -p --model MODEL --output-format json --tools "" --setting-sources "" --strict-mcp-config --no-session-persistence' \
  --jobs 3 --trials 3 --shards2 $CARGO_TARGET_DIR/release/shards2 \
  --out SHORT.csv --transcripts transcripts/SHORT
```

`MODEL` was `claude-haiku-4-5-20251001` (`SHORT` `haiku`) or `claude-sonnet-5-5` (`SHORT` `sonnet`); both ran concurrently. Results are in `haiku.csv` and `sonnet.csv`, with complete transcripts under `transcripts/`.
