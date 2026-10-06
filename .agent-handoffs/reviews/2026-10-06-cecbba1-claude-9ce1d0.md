# Review: golden path M0 (authoring eval harness and baseline) and M1 (opaque tables)
- Record: 2026-10-06-cecbba1-claude-9ce1d0
- Author: Claude Code review session
- Created: 2026-10-06T15:36:05Z
- Base: 2bf82a779ab57105b9113ac45443e30111f4ed90
- Reviewed: cecbba14ecc31890e585497b5623c8739e535eea on `golden-path` (equal to `origin/golden-path`). No tracked file was modified; the only untracked file was the concurrent review [2026-10-06-cecbba1-codex-55d37a](2026-10-06-cecbba1-codex-55d37a.md), which appeared during this review.
- Scope: M0 harness (40cb29bb69c42a10e7e963bf8a5d77deab9e4f59), baseline (7bd1852b73560d1fb4ed61dc0766749b1c7ec7e9) and M1 (cecbba14ecc31890e585497b5623c8739e535eea), read against `docs/golden-path.md` §7.1, §8 and §9. Included: the runner, all 38 tasks, the `current` primer, `baseline.csv`, the three non-clean baseline transcripts, `var.rs` and its call sites, the host test, and the status and embedding docs. Excluded: the M0 docs hygiene in 2bf82a7, the other 35 transcripts beyond a consistency count, and any new model invocation.
- Provenance: fresh review. The Codex record above was read after my own findings were formed; its two findings are corroborated below instead of repeated.
- Checks: all observed locally on Linux x86-64 at cecbba1, all passing: `cargo fmt --all -- --check`; `cargo fmt --manifest-path examples/esp32/Cargo.toml --all -- --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace` (both native schedulers); docs-off (`cargo test -p shards-core -p shards-io --no-default-features --test metadata --test catalog`, `cargo test -p shards-lang --no-default-features`); `cargo test --release -p shards-lang --test lang nesting_up_to_the_limit`; `cargo clippy -p shards-io --all-targets --features rustls-ring -- -D warnings`; wasm lint (`cargo clippy -p shards-core -p shards-lang --target wasm32-wasip1 --lib --tests -- -D warnings`); wasm tests built and run under Node WASI (`prototype` 38, `metadata` 19, `lang` 61 passed); `authoring-eval verify` against the release `shards2` (38 of 38 references). Not run: macOS, the ESP32 firmware build and QEMU boot, any model call. No CI run exists for these commits (see F4).

## Gate summary

- **M1 gate met.** `BTreeMap` appears only in `crates/shards-core/src/var.rs` (a private field and `pub(crate)` helpers) and `inline.rs` (internal); no public signature names it, and no other crate, example or bench does. The embedding guide documents the accessors. `host_contract::hosts_use_opaque_collections` uses only the public API. The `TableMake` unique/shared conversion in `inline.rs` behaves as before.
- **M0 gate met as written.** The harness exists, the references are tested in-process, and the committed rows are consistent with the 38 transcripts (37 semantic passes, 36 first-pass checks). The findings below are about whether the baseline can do the job decision D12 gives it.

## Corroboration of the Codex findings

- `2026-10-06-cecbba1-codex-55d37a#F2` (schedule names split on whitespace): reproduced through the real runner with a canned reply (`--cmd "cat >/dev/null; cat reply.md" --only 26-spawn`, output to a scratch CSV). A correct program logging `10`, `20`, `30` was scored `wrong-output` with `@schedule(m,main)` and also with `@schedule (m main)` (a space before the parenthesis, which the parser accepts); `@schedule(m, main)` and a multi-line form passed. Related: log lines and result lines share stdout, so `1 | Log("root")` prints `root: 1` twice and the runner cannot tell them apart. This supports that record's direction (structured output from the CLI) over a better regex.
- `2026-10-06-cecbba1-codex-55d37a#F1` (timeout does not kill descendants): agreed from the code path at `bench/authoring/src/main.rs:150-161`; not rerun here.

## F1 — P2 Pin the model behind the baseline, and keep the old syntax runnable as a control
- Location: `bench/authoring/baseline.csv` (`Model` column), `bench/authoring/src/main.rs:404-427` (`ask`, `claude-json`), `docs/current-state.md:21`.
- Evidence: every row says `sonnet`, the CLI alias passed on the command line. The runner reads only `result`, `is_error` and `usage` from the envelope, and the transcripts hold no model identifier (`grep -rli "claude-\|model" bench/authoring/transcripts` finds nothing). `current-state.md` records the CLI version, date and commit, not the model the alias resolved to. The reruns the plan compares against this baseline (after M5 and M7) happen later, with `sonnet` resolving to whatever it does then, and M2 deletes the old syntax from the tree, so the control cannot be rerun from `HEAD`.
- Impact: a later difference in pass rate or repair rounds cannot be separated into "the syntax changed" and "the model changed". D12 makes this eval the arbiter of syntax disputes.
- Suggested direction: record the resolved model ID per row (run with a full model ID, or keep the envelope's model field), and state it in `current-state.md`. Tag the last pre-M2 commit and say in the README that the `current` variant is rerun with a `shards2` built from that tag (`--shards2` already allows it), so both variants run against the same model on the same day.

## F2 — P2 The baseline has no headroom and no variance estimate
- Location: `bench/authoring/baseline.csv`, `docs/current-state.md:29`.
- Evidence: 37 of 38 semantic passes and 36 of 38 first-pass checks, one trial per task. `current-state.md` already says the tasks are too easy to discriminate, but nothing is scheduled to change it and the milestone is marked done.
- Impact: a variant can only tie or lose against this baseline, and a one- or two-task swing is indistinguishable from run-to-run noise that was never measured. The M5 rerun would produce a number that cannot support a decision either way.
- Suggested direction: before relying on a comparison, add a `--trials N` option (rows already carry task, model and variant), run at least one weaker model, and add harder tasks; with F1's control in place this does not have to block M2. Otherwise record in `current-state.md` that the baseline is saturated and name the milestone where that is fixed.

## F3 — P2 Record what kind of error each repaired first pass hit
- Location: `bench/authoring/src/main.rs:509-523` (`run_task`), `262-278` (`classify_check`).
- Evidence: `Failure` is set only when the task ends in failure. The two baseline rows with a repair (`10-seq-double`, `30-price-lookup`) have `FirstPassSuccess=false`, `RepairRounds=1` and an empty `Failure`; that they were compose errors (`possibly-uninitialized`, `input-type-mismatch`) and not syntax or missing shards is visible only in the transcripts. §8 asks to separate syntax failures from missing-shard failures.
- Impact: with semantic success saturated (F2), first-pass failures by kind are the main signal for comparing syntax variants, and the CSV does not carry it. Adding the column after more baselines exist leaves old rows without it.
- Suggested direction: a `FirstFailure` column (class of the first failing check, optionally the diagnostic codes). The two existing rows can be filled from their transcripts.

## F4 — P2 M0 and M1 have no CI run, and the status file's "Verified" section is stale
- Location: `.github/workflows/ci.yml:3-6`, `.github/workflows/esp32.yml:5-17`, `docs/current-state.md:40-42`, `AGENTS.md:80`, `docs/golden-path.md:7`.
- Evidence: both workflows run on pushes to `main` and on pull requests only. The work is on `golden-path`; `gh run list --branch golden-path` returns nothing and `gh pr list --head golden-path --state all` returns nothing. M1 changes `shards-core`, which is in the firmware workflow's path filter. `current-state.md` still says "This checkpoint is a documentation change" and records no checks for the harness or for M1. `AGENTS.md` and the golden path both say milestones land on `main`.
- Impact: the milestone rule "pass the full check set" has only local Linux evidence (this record's Checks line). macOS and the ESP32 link and QEMU boot are unverified for a core API change.
- Suggested direction: open a pull request for `golden-path` (both workflows then run), or trigger the firmware workflow by hand, and record the result in `current-state.md`; align `AGENTS.md` with the branch actually used.

## F5 — P3 Two of the three non-clean baseline outcomes trace to the primer, not the syntax
- Location: `bench/authoring/reference/current.md:14`, `:62-70`; `docs/current-state.md:30-31`.
- Evidence: (a) the primer's own looped example (`Pause` at the end, `@run(main Iterations: 3)`) logs `1`, `2` and then `ticker: still running after 3 ticks` when run; the model copied the shape and `27-persistent-total` failed. `current-state.md` says "until then the primer should say it", and the primer does not. (b) Line 14 says reading a missing key is a compile error; that holds for literal keys, while `Take` with a variable key gives `Int | None`, which caused the `30-price-lookup` repair. (c) `current-state.md` attributes the `10-seq-double` repair to a `While` body; the transcript shows a `Repeat` body.
- Impact: if the M2 primer fixes these while changing the syntax, the improvement is credited to the syntax.
- Suggested direction: decide explicitly: fix the `current` primer and rerun the affected baseline tasks, or carry the same text into the next variant so primers differ only in the syntax under test.

## F6 — P3 The `Tokens` column is nearly constant
- Location: `bench/authoring/src/main.rs:410-419`, `bench/authoring/README.md` (Results).
- Evidence: the sum of input, cache and output tokens is 22518 to 23406 for all 36 single-round rows. `authoring-eval reference` prints 50,490 bytes, so the fixed prompt dominates; the README attributes about 20k tokens a call to the CLI's system prompt, which does not obviously fit with a reference of that size (not measured here).
- Impact: the size of the model's answer, the part that varies with syntax, is lost in a constant.
- Suggested direction: record output tokens in their own column.

## F7 — P3 A reply that breaks the output format is scored as a language syntax failure
- Location: `bench/authoring/src/main.rs:432-442` (`extract_program`).
- Evidence: a canned reply with the correct program in a fenced block followed by one sentence containing a literal triple backtick was run through the runner with `--rounds 0`: the text between the last two fence markers was taken as the program and the row ended `syntax`. Any reply with an odd number of fence markers is mis-split the same way.
- Impact: reply-format mistakes are counted as language errors. None occurred in the baseline.
- Suggested direction: pair fences from the start of a line, prefer the last complete block, and use a separate failure class when no block is found.

## F8 — P3 A failure inside the runner's own `check` or `run` step drops the row and the transcript
- Location: `bench/authoring/src/main.rs:509`, `:527`, `:574`.
- Evidence: by code path, not observed. `check` returns `Err` when `shards2 check --json` prints no JSON (a crash, or the 30-second timeout); `run_task` propagates it, the worker thread exits before writing the row or the transcript, and when every worker has exited this way the remaining tasks are never started. The CLI held up against the inputs tried here (200,000 nested brackets, 3,000 nested `If`s, a 300,000-line file, out-of-range integers, recursive `Do` and `Spawn`), so a trigger is not known.
- Impact: a program that crashes or hangs the checker is exactly what the eval should record, and it would be lost.
- Suggested direction: turn these errors into a row with a `harness-error` class and keep the transcript.

## F9 — P3 Hosts cannot derive a changed table without rebuilding it, and the plan text still says `TableRef`
- Location: `crates/shards-core/src/var.rs:130` (`Table::iter`), `:180` (`TableBuilder`), `docs/golden-path.md:186`.
- Evidence: `iter` yields `(&str, &Var)` and `TableBuilder` starts empty, with no `From<&Table>`, `Extend` or `IntoIterator for &Table`. A host shard that returns its input table with one key changed must reinsert every entry, allocating a new `Arc<str>` per key; before M1 it could `Arc::make_mut` the map. Separately, the choice of `&Table` over the plan's `TableRef` is explained only in the commit message; §7.1 of the contract still shows `Option<TableRef>`.
- Impact: small today, but `current-state.md` says the external host has not moved to this API yet, so this is the cheap moment to settle the update path. It is not a gate failure: §7.1 lists only read access, a builder and sorted iteration.
- Suggested direction: `Table::to_builder()` (or `TableBuilder: From<&Table>`) that reuses the key `Arc`s, and a one-line update to §7.1.

## Notes without a finding

- Task prompts and expected outputs were read for all 38 tasks; none is ambiguous enough to report. Several constraints are not checked by output equality (real tick delays in `23-retry-backoff`, "do not hard-code" in `12-seq-max`), as the Codex record also notes.
- `Maybe` without `Silent: true` prints its caught error on stdout, between log lines, so `19-parse-fallback` passes only with `Silent: true`. The primer shows that form, so this is fair, but it is one more reason to separate the log from other output.
- `Var::Seq(Arc<Vec<Var>>)` keeps public storage. §7.1 names only `BTreeMap`, so this is within the plan.

Status: F1 to F9 are pending. `2026-10-06-cecbba1-codex-55d37a#F1` and `#F2` remain pending in their own record. Published in the shared checkout only; no commit or push.

Receiving-context invocation:
`/review-handoff address .agent-handoffs/reviews/2026-10-06-cecbba1-claude-9ce1d0.md`
