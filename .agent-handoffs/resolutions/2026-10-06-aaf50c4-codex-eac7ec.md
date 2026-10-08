# Resolution follow-up: shared reload call-site prefixes
- Record: 2026-10-06-aaf50c4-codex-eac7ec
- Author: Codex implementation session
- Created: 2026-10-06T20:13:47.916972+00:00
- Review: [2026-10-06-75ec9da-claude-fd6500](../reviews/2026-10-06-75ec9da-claude-fd6500.md)
- Code: aaf50c4c1d4a9e845c87a9e4430f28af5d8fa813 plus exact compose/reload patch in [independent review](../reviews/2026-10-06-aaf50c4-codex-2633f9.md).
- Provenance: own implementation and full local checks; independent targeted review linked above.

## 2026-10-06-75ec9da-claude-fd6500#F1
- Disposition: deferred
- Supersedes: 2026-10-06-75ec9da-codex-464532#2026-10-06-75ec9da-claude-fd6500#F1 (further reduction, not a claim of device clearance).
- Change/reason: the ESP32 failing1144-byte allocation at ab4bc6b is an ancestor-path copy, not token storage: symbolize its [CI ELF](https://github.com/sinkingsugar/shards2/actions/runs/37514844876) backtrace addresses0x40129471 and0x401726c7 to Vec<SiteStep>::clone / Arc::make_mut in ComposeCtx::compose_reloadable_inline at compose.rs:652. Persistent immutable path nodes now share ancestors across retained keys, use structural order-sensitive equality/hashing and drop unique suffixes iteratively. This addresses the next compose cost identified in the review.
- Evidence: identical frontend_memory probe after this change: do80 compose peak138244 bytes (246004 before); wrapped_when140779 (178203 before). Thirty-level source compose197398 (195678 before), a1720-byte increase explicitly retained. These are native requested allocations, not device budgets. New regressions cover content equality/hash, shared prefix independence, pop/restoration and20,000-step iterative drop. Full local AGENTS check set plus expanded docs-off/WASI passed; independent path/prototype/reload checks passed. Performance repetition and QEMU pending at publication.
- Remaining: all three device runs must pass. Earlier F2 response remains fixed/awaiting verification; stackful is retained.
