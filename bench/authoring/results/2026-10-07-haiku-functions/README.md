# Haiku authoring comparison after M5/M7

On the 39 matched tasks (117 trials per variant), the current snapshot produced correct output in **115/117** trials versus **111/117** for the pre-M2 control, with **106/117** versus **99/117** clean first checks. This is a small descriptive comparison of complete snapshots, not proof of a syntax improvement. The harder shared tasks exposed substantially more first-check failures than the original set.

| Group | Control correct output | Functions correct output | Control clean first check | Functions clean first check |
|---|---:|---:|---:|---:|
| All 39 matched tasks | 111/117 | 115/117 | 99/117 | 106/117 |
| Original 37 matched tasks | 106/111 | 110/111 | 99/111 | 105/111 |
| Harder shared tasks 40–41 | 5/6 | 5/6 | 0/6 | 1/6 |
| Task 25, changed prompt (unpaired) | 3/3 | 3/3 | 2/3 | 3/3 |
| Task 39, recursion (new capability) | not supported | 0/3 | not supported | 0/3 |

Matched-task correct-output totals by trial were **37, 36, 38** for the control and **39, 39, 37** for functions, each out of 39. Compiler repair rounds totaled **28 versus 14**. Matched output tokens totaled **615,183 versus 561,897**; total reported tokens were **3,704,592 versus 5,272,705**, reflecting the larger current reference (103,597 bytes versus 50,721). Median model latency per matched trial, including repairs, was 32.628 versus 32.868 seconds. These timings include service variability.

The full, differently sized task sets scored 114/120 and 118/123. All 243 trials and their transcripts are present, with the requested model ID in every row. The runs made 291 model calls including compiler repairs, plus the separate connectivity pilot. There were no service errors, timeouts, harness errors, or failed runtime executions; all final failures were wrong-output results. [summary.json](summary.json) contains the complete breakdown, including first diagnostic codes and unsuccessful trial IDs.

Manual inspection of the completed new-task replies found:

- **Task 25:** all three current replies define one parameterized `Clamp` function and call it three times; they do not duplicate the body.
- **Task 39:** every final reply really recurses and keeps a per-invocation partial sum. All compute 16, 5, 6 and -1, but all add log labels and therefore fail the frozen exact-output oracle. Trial 2 also supplies the node index as pipeline input rather than the requested parameter. Each trial initially declares `[[Int]]` for child lists containing empty sequences, then broadens/narrows types in response to diagnostics; trial 3 first repairs reversed assignment syntax. These are recorded failures, not upgraded to semantic passes because the numeric sums are right.
- **Task 40:** all six final replies process the supplied sequences and compute counters rather than hardcoding lines. The control needs three repairs in every trial; functions needs one, three and one. Control trial 3 uses `sum > 0 || invalid > 0` as its final-flush test, which passes these fixtures but would miss an unfinished batch whose valid sum is zero or negative. A local probe replacing the first sequence's trailing `"4"` with `"0"` confirmed that it omits the required final batch line (five lines instead of six). This is a coverage limit: add such a trailing batch in a future task revision rather than silently changing this run's inputs or scores.
- **Task 41:** control trial 2 computes the right results but omits `Silent: true` on `Maybe`, adding an error log line. Functions trial 3 puts a constant-true predicate after its item-membership check, accepts the unknown item at price zero, and reports three rejections instead of four. The other four replies validate records and compute the required totals. All six use the supplied records rather than literal output lines.

The original suite remains near saturation. The new shared tasks offer repair and first-check signal, but six trials per variant do not establish generalization or statistical significance. The tree task demonstrates specific type and output-protocol friction; its zero strict passes must remain visible when assessing M7 authoring.

Real model calls on 2026-10-07, using `claude-haiku-4-5-20251001` through Claude Code 2.1.293. Three trials per task, two concurrent workers per variant; the two variants ran concurrently. The user approved the reference/task transfer and model quota use. Tools, external MCP configuration and session persistence were disabled. Only the public reference and task prompt were supplied initially; compiler repairs also supplied the preceding candidate and diagnostics. Hidden reference solutions were excluded.

The current snapshot is `d24e6e5bd28ee02079f49bb7f42199ef7253ce7e`, variant `functions` (41 tasks). The control is `authoring-control-v1`, `de932c6c9baec04878d6974166f91735ffe099c6`, variant `current` (40 tasks). Its source archive was extended with tasks 40–41 using the current prompts and expected outputs and the pre-M2 references in `bench/authoring/control`, as documented in the [eval README](../../README.md). Both runners and CLIs were built in release mode with Rust 1.98.1 on Linux x86-64. All references passed their respective CLIs before model calls began; current references also passed in-process.

Each runner was launched from its own source tree:

```sh
target/release/authoring-eval run --variant VARIANT \
  --model claude-haiku-4-5-20251001 --format claude-json \
  --cmd 'claude -p --model claude-haiku-4-5-20251001 --output-format json --tools "" --setting-sources "" --strict-mcp-config --no-session-persistence' \
  --jobs 2 --trials 3 --out VARIANT.csv --transcripts transcripts/VARIANT
```

`VARIANT` was `functions` or `current`; absolute output paths kept both runs in this directory. The runner's default limit was five compiler repairs. A separate one-task connectivity pilot is retained under `pilot/` and excluded from comparison totals.

[provenance.json](provenance.json) records SHA-256 hashes of both binaries, all task files and each rendered reference. The exact references sent are [functions-reference.txt](functions-reference.txt) and [current-reference.txt](current-reference.txt). CSVs retain every scored trial, and `transcripts/` retains replies, compiler feedback and runtime output. Recompute the summary and check trial/transcript consistency with:

```sh
python3 bench/authoring/results/2026-10-07-haiku-functions/summarize.py
```

The matched aggregate excludes task 25 (its prompt changed from a reusable routine to a function with parameters) and task 39 (recursion is a new capability). This leaves 39 identical prompts and expected outputs per variant, including the harder shared tasks 40–41. Those two tasks are also reported separately from the 37 original matched tasks. The reference and catalog changed along with syntax and runtime behavior, so this measures the complete snapshots, not a causal syntax effect. Three trials are descriptive evidence, not a reliable significance estimate.

`FirstPassSuccess` means the initial reply passed the compiler; it does not imply correct output. Semantic success is exact output equality after any compiler repairs. The automated judge does not enforce implementation constraints such as recursion or avoiding hardcoded answers; manual checks of the new tasks are reported with the results.
