# Agent review inbox

Use the shared [review-handoff skill](../skills/review-handoff/SKILL.md) to pass findings between sessions without copying chat messages.

In Codex: `$review-handoff publish`, `$review-handoff address`, or `$review-handoff verify`. In Claude Code: `/review-handoff` followed by the same request. `status` summarizes pending work. If discovery has not refreshed, point the agent directly at the skill file.

Reviews live in `reviews/`, implementation responses in `resolutions/`, and independent checks in `verifications/` (created on first use). Each record is immutable and references stable finding IDs. Reported fixes await verification. See the skill's [record format](../skills/review-handoff/references/records.md).

Invoke the skill in the receiving session to notify it. Files do not wake agents. Sessions in the same checkout see files immediately; other worktrees or clones need the records transferred through Git. Keep durable records tracked. There is no shared mutable inbox index to race over.

The public repository starts from a squashed snapshot (2026-10-05). Commit hashes cited in records written before it refer to the earlier history, kept in a private archive; they do not exist in the public repository.

The initial review and resolution were imported from the setup conversation and commit history. Their claims are historical evidence, not a new verification of the current checkout.
