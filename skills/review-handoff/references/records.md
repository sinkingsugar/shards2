# Record format

Store UTF-8 Markdown under `.agent-handoffs/reviews/`, `resolutions/`, or `verifications/`. Create directories as needed. Name each file `<UTC-date>-<short-sha>-<author>-<unique-suffix>.md`; use the filename stem as its globally unique record ID. A short random suffix avoids concurrent name collisions. Write a complete record with exclusive creation (`open(path, 'x')` in Python, for example); retry a name collision with a new suffix. Draft privately before publication.

Use the following fields as plain Markdown bullets. No parser or database is required. Use full commit hashes; if uncommitted changes matter, say so and identify the affected files and retain a patch or equivalent reproducible snapshot. Do not imply HEAD represents dirty code. Use repository-relative paths and relative links between records.

## Review

```markdown
# Review: <scope>
- Record: <filename stem>
- Author: <agent/session label; do not guess model identity>
- Created: <UTC timestamp>
- base: <full hash, or not applicable with reason>
- Reviewed: <full hash and working-tree qualification>
- Scope: <included/excluded areas>
- Provenance: <fresh review, or imported report and its source>
- Checks: <commands, outcomes, platform/backend; distinguish reported from observed>

## F1 — <P0/P1/P2/P3> <actionable title>
- Location: <path:line at reviewed snapshot, or symbol>
- Evidence: <repro or concrete code path; expected versus actual>
- Impact: <why this matters>
- Suggested direction: <optional; not a mandated implementation>
```

The stable finding ID is `<review-record-id>#F1` (and F2, etc.). Keep IDs unchanged in responses. An empty review explicitly says no findings within its scope; it does not certify the whole repository.

## Resolution

```markdown
# Resolution: <scope>
- Record: <filename stem>
- Author: <label>
- Created: <UTC timestamp>
- Review: <relative link>
- Code: <full fix hash or reproducible dirty snapshot>
- Provenance: <own work or imported implementation claim>

## <review-record-id>#F1
- Disposition: fixed | disputed | deferred | duplicate
- Change/reason: <what changed or why not; duplicate links original finding ID>
- Evidence: <test names, commands and actual outcomes; unrun checks stated>
- Supersedes: <earlier response ID if applicable, otherwise omit>
```

## Verification

```markdown
# Verification: <scope>
- Record: <filename stem>
- Author: <label>
- Created: <UTC timestamp>
- Review: <relative link>
- Resolution: <relative link>
- Verified code: <full hash or reproducible dirty snapshot>

## <review-record-id>#F1
- Result: verified | still-failing | inconclusive
- Evidence: <independent inspection and targeted check, actual outcome>
- Limits: <untested cases/backends or environmental blockers>
- Supersedes: <earlier verification ID if applicable, otherwise omit>
```

Imported historical records may have an unknown original timestamp; explicitly state that rather than manufacturing precision. Publication time and time of observation are different facts. Correcting a published report requires a new record linking the superseded record/finding and explaining the correction.
