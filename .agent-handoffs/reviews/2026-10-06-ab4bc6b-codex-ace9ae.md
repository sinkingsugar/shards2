# Review: M4 device gate, implementation handoff
- Record: 2026-10-06-ab4bc6b-codex-ace9ae
- Author: Codex implementation session
- Created: 2026-10-06T19:48:29.904368+00:00
- Base: c0aaa3961b1d2de4e9b7f9512b49c668c064d727
- Reviewed: ab4bc6b744701b503e0524a2e1783b02dc14455c, clean implementation tree; publication adds documentation and evidence only.
- Scope: publish the known device acceptance blocker and current implementation evidence for independent verification. This is not a fresh source review.
- Provenance: imported implementation-session observations and CI results, rechecked at publication. Earlier [M4 source review](2026-10-06-e2c5dad-codex-02ece2.md) and [metadata sharing review](2026-10-06-0d33bdd-codex-4cc8c8.md) found no blockers within their named scopes; neither cleared device acceptance.
- Checks: observed GitHub CI at this exact SHA: [Linux/macOS/WASI](https://github.com/sinkingsugar/shards2/actions/runs/37514844946) passed; [device run](https://github.com/sinkingsugar/shards2/actions/runs/37514844876) failed on ESP32 and C3, passed on S3. Local suites were run earlier by this implementation session, not rerun for publication.

## F1 — P1 Complete device acceptance before closing M4 or deleting stackful
- Location: `examples/esp32`, `crates/shards-lang/tests/lang.rs`, golden-path §6.4.
- Evidence: [ESP32 job](https://github.com/sinkingsugar/shards2/actions/runs/37514844876/job/112445004375) ends at `deep_do_chains_are_a_diagnostic_not_a_crash`, with `memory allocation of 1144 bytes failed`. [C3 job](https://github.com/sinkingsugar/shards2/actions/runs/37514844876/job/112445004771) gets past that case and `nesting_up_to_the_limit_runs_through_every_control_shard`, then ends at `code_after_stop_keeps_its_types`, with allocation failures of 40 and 8 bytes. These are the last named cases, not a symbol-level attribution of the failing allocation. Both firmware builds succeed; runtime QEMU acceptance fails.
- Evidence: [S3 job](https://github.com/sinkingsugar/shards2/actions/runs/37514844876/job/112445004784) passes all six suites; cumulative minimum free stack is 23,928 of 131,072 bytes and minimum free heap is 22,092 bytes.
- Impact: golden-path §6.4 requires all three device runs. The gate remains open, despite passing native/WASI checks and performance limits. No physical-board result is claimed.
- Suggested direction: measure frontend/compose retained and peak allocations and recursive stack use. Source-map path duplication is a candidate, not an implemented or verified fix. Preserve valid-depth coverage and diagnostic contracts. Do not reduce acceptance requirements to claim a pass.
- Limits: the expanded acceptance suite was not run against the pre-M4 baseline. These results do not establish how much of the failure is a new runtime regression versus previously untested frontend/compose cost. The 128 KiB stack reservation also competes with heap capacity.
