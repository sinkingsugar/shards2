# Verification: file watching review fixes
- Record: 2026-10-10-eedabf4-codex-c0e78d
- Author: Codex review session (original reviewer)
- Created: 2026-10-10T09:19:44Z
- Review: [2026-10-10-e982fe9-codex-3bec98](../reviews/2026-10-10-e982fe9-codex-3bec98.md)
- Resolution: [2026-10-10-af2ce86-claude-3f6b30](../resolutions/2026-10-10-af2ce86-claude-3f6b30.md)
- Verified code: eedabf453dfefdd67c4cbecbdb5b35712f92c5fe, pulled by fast-forward from origin/metaprogramming. The implementation is af2ce86ebe3cd32042a09300f8476bcf30659c48; the next commit adds its resolution record. Working tree clean before probes; no implementation changes made.

## 2026-10-10-e982fe9-codex-3bec98#F1
- Result: verified
- Evidence: independently inspected `fingerprint`, `Recording::resolve`, `Session::observe_dependencies` and the watcher's structured observation. Dependency fingerprints hash original bytes with their length, avoiding lossy UTF-8 conversion. The original `binary_edit_is_a_new_revision` probe passes: replacing `[0xff]` with `[0xfe]` produces exactly one reload. The committed regression also passes in the frontend suite.
- Limits: fingerprint collisions are theoretically possible; this checks the reported lossy-decoding defect, not mathematical collision freedom.

## 2026-10-10-e982fe9-codex-3bec98#F2
- Result: verified
- Evidence: `FsFiles::resolve` records candidates before reading them; `Recording::resolve` records missing candidates even on failure, and polling observes them through the same Files reader. The original `creating_a_missing_include_retries_the_revision` probe passes. The committed search-precedence regression passes. An additional independent `deleting_and_restoring_a_read_dependency_recovers` probe loads binary data, deletes it, observes exactly one rejection with no repeated rejection, restores it, then observes exactly one reload and no extra reload.
- Limits: checked native filesystem behavior; permission-error changes and every possible custom Files implementation were not exhaustively tested.

## 2026-10-10-e982fe9-codex-3bec98#F3
- Result: verified
- Evidence: after a load attempt, the watcher now records the submitted main text plus dependency fingerprints captured from the returned file bytes, rather than the old dependency set. The original `unchanged_dependencies_do_not_reload_twice` probe passes: one initial reload and logs `["1"]`, even though the entry fails after logging. An additional independent custom-reader probe, `edit_after_read_is_not_swallowed_by_consumed_observation`, reads `@const(v 1)` and writes `@const(v 2)` immediately before returning the old bytes. Polling correctly yields one subsequent reload, logs `["1", "2"]`, and then remains stable. Thus adopting the consumed observation does not swallow this concurrent edit.
- Limits: the deterministic custom-reader test covers an edit after the load's read; it does not exhaust all filesystem interleavings.

## Observed checks and scope

On Linux x86-64 with the pinned toolchain:

- `cargo test -p shards-lang -p shards-cli`: passed, including all four committed `watched_files` regressions and CLI/embedding tests.
- `cargo test -p shards-lang --test review_file_watch -- --nocapture`: all five independent probes passed. The first attempt to compile the supplemental probe used an incorrect public type path; corrected to `shards_lang::files::File` before the successful run.
- `git diff --check`: passed.

The original three probes plus the two additional checks are preserved in [this reproducible patch](2026-10-10-eedabf4-codex-c0e78d.patch). Apply it to the verified checkout and run the second command above. The temporary integration-test file was removed after retaining the patch. Existing implementation regression tests remain in `crates/shards-lang/tests/lang.rs`.

All three findings are closed for this code snapshot. No new actionable finding was identified in the fix diff. This is targeted verification, not a new review of the full original range. No ESP32/WASI/macOS run, Miri rerun, performance measurement or independent CI validation is claimed; these fixes change no unsafe code.

Receiving-context invocation after fetching this record:
`$review-handoff status .agent-handoffs/verifications/2026-10-10-eedabf4-codex-c0e78d.md`
