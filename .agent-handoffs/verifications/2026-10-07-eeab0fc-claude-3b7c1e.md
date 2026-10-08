# Verification: M4 device gate closed at eeab0fc
- Record: 2026-10-07-eeab0fc-claude-3b7c1e
- Author: Claude Code implementation session (the session that took the golden-path work over on 2026-10-07)
- Created: 2026-10-07T06:49:33Z
- Review: [2026-10-06-ab4bc6b-codex-ace9ae](../reviews/2026-10-06-ab4bc6b-codex-ace9ae.md); [2026-10-06-75ec9da-claude-fd6500](../reviews/2026-10-06-75ec9da-claude-fd6500.md)
- Resolution: [2026-10-06-c88c59d-codex-17a2c5](../resolutions/2026-10-06-c88c59d-codex-17a2c5.md)
- Verified code: eeab0fc4d6ed313c4ad4ff5f750eeffddb7c3940 (`Reserve known entry batches within the device memory budget`), the commit that followed the three-file patch retained with [2026-10-06-c88c59d-codex-184775](../reviews/2026-10-06-c88c59d-codex-184775.md). `git apply --check -R` of that patch (docs, bench and handoff paths excluded) succeeds against eeab0fc, so the reviewed source is what landed.

## 2026-10-06-ab4bc6b-codex-ace9ae#F1
- Result: verified.
- Evidence: observed through `gh run view` on the ESP32 workflow run for eeab0fc, [actions/runs/37526212515](https://github.com/sinkingsugar/shards2/actions/runs/37526212515): the three firmware jobs (`esp32`, `esp32c3`, `esp32s3`) all conclude `success`. Each QEMU log prints `Shards ESP32 acceptance suites passed` after the six suites (prototype, metadata, host_contract, registry, lang, trampoline), including `entry_outcomes_survive_retirement_of_finished_records`, the case the previous ESP32 run died in, and then the smoke test line. Minimum free heap after the suites: 37,612 bytes (C3), 5,936 bytes (ESP32). Minimum free main stack: 25,872 and 24,048 of 131,072 bytes. The native workflow at the same commit, [actions/runs/37526212486](https://github.com/sinkingsugar/shards2/actions/runs/37526212486), concludes `success` (Linux, macOS, WASI).
- Limits: QEMU only, no physical board. The ESP32 heap margin at the end of the suites is under 6 KiB; the gate does not require more, but a later fixture that grows the suite must watch it.

## 2026-10-06-75ec9da-claude-fd6500#F1
- Result: verified (device sufficiency reached at eeab0fc; the finding's measurement work was done in the follow-up commits aaf50c4, c88c59d and eeab0fc).
- Evidence: as above; the suite that failed on ESP32 for frontend and compose peaks now passes on all three chips with no acceptance limit or instance count reduced (the 65-entry retirement case and the full-depth tooling coverage are both in the suite that ran).
- Limits: the counting-allocator measurements in the finding were not repeated here; this verifies the outcome the finding asked for (device pass), not the per-fixture peak numbers.

## Gate status (golden-path.md §6.4)

1. Structural test: `crates/shards-core/tests/trampoline.rs`, in the suites that passed above on native, WASI and all three chips.
2. Soft thresholds: the five-trial run at c88c59d (pending re-poll 0.285× stackful, mixed 0.995× M3 stackless), independently checked in [2026-10-06-c88c59d-codex-843667](2026-10-06-c88c59d-codex-843667.md). eeab0fc changes no code on those benchmark paths.
3. Every shared-suite test on native debug and release, docs-off, WASI and ESP-IDF QEMU: the two CI runs above. Targeted Miri on the frame arena: not rerun at eeab0fc by this session (the network on 2026-10-07 did not allow installing a nightly); the arena source is unchanged since the run reported in `docs/current-state.md`, and it stays unchanged by the deletion commit. Rerun it when the toolchain is available and record the result.

The deletion of stackful (`corosensei`, `cfg(stackful)`, `--stackful`, the parity test macros, the `Backend` type parameter) is the commit after eeab0fc on `golden-path`; it is implementation work, not part of this verification.

Receiving-context invocation:
`$review-handoff status .agent-handoffs/verifications/2026-10-07-eeab0fc-claude-3b7c1e.md`
