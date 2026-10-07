"""Validate this run's artifacts and print its descriptive summary as JSON."""

import csv
import json
import re
import statistics
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def summarize(rows):
    return {
        "runs": len(rows),
        "semantic_success": sum(r["SemanticSuccess"] == "true" for r in rows),
        "first_pass_check": sum(r["FirstPassSuccess"] == "true" for r in rows),
        "semantic_by_trial": {
            str(t): sum(r["SemanticSuccess"] == "true" for r in rows if int(r["Trial"]) == t)
            for t in range(1, 4)
        },
        "first_pass_by_trial": {
            str(t): sum(r["FirstPassSuccess"] == "true" for r in rows if int(r["Trial"]) == t)
            for t in range(1, 4)
        },
        "repair_rounds": sum(int(r["RepairRounds"]) for r in rows),
        "model_calls": len(rows) + sum(int(r["RepairRounds"]) for r in rows),
        "tokens": sum(int(r["Tokens"]) for r in rows),
        "first_failures": dict(Counter(r["FirstFailure"] for r in rows if r["FirstFailure"])),
        "first_diagnostic_codes": dict(Counter(code for r in rows for code in r["_first_codes"])),
        "final_failures": dict(Counter(r["Failure"] for r in rows if r["Failure"])),
        "output_tokens": sum(int(r["OutputTokens"]) for r in rows),
        "median_output_tokens": statistics.median(int(r["OutputTokens"]) for r in rows),
        "median_latency_ms": statistics.median(int(r["LatencyMs"]) for r in rows),
    }


results = {}
for variant in ("current", "functions"):
    with (ROOT / f"{variant}.csv").open() as source:
        rows = list(csv.DictReader(source))
    task_numbers = set(range(1, 42)) - ({39} if variant == "current" else set())
    expected = {(n, t) for n in task_numbers for t in range(1, 4)}
    actual = {(int(r["Task"].split("-", 1)[0]), int(r["Trial"])) for r in rows}
    assert actual == expected and len(rows) == len(expected), (variant, "missing or duplicate rows")
    for row in rows:
        assert row["Variant"] == variant
        assert row["ModelId"] == "claude-haiku-4-5-20251001", row
        transcript = (ROOT / "transcripts" / variant / f'{row["Task"]}.{row["Trial"]}.md').read_text()
        assert ("## Run (pass)" in transcript) == (row["SemanticSuccess"] == "true"), row
        assert transcript.count("\n## Reply ") == int(row["RepairRounds"]) + 1, row
        check = re.search(r"\n## Check\n\n```json\n(.*?)\n```", transcript, re.S)
        row["_first_codes"] = (
            [d["code"] for d in json.loads(check.group(1)).get("diagnostics", []) if "code" in d]
            if check else []
        )
    groups = {
        "all": rows,
        "matched_39": [r for r in rows if int(r["Task"][:2]) not in (25, 39)],
        "original_matched_37": [r for r in rows if int(r["Task"][:2]) < 39 and int(r["Task"][:2]) != 25],
        "harder_shared_2": [r for r in rows if int(r["Task"][:2]) in (40, 41)],
        "changed_prompt_25": [r for r in rows if int(r["Task"][:2]) == 25],
    }
    if variant == "functions":
        groups["recursion_39"] = [r for r in rows if int(r["Task"][:2]) == 39]
    results[variant] = {name: summarize(group) for name, group in groups.items()}
    results[variant]["unsuccessful_runs"] = [
        {k: r[k] for k in ("Task", "Trial", "FirstFailure", "RepairRounds", "Failure")}
        for r in rows if r["SemanticSuccess"] != "true"
    ]

print(json.dumps(results, indent=2))
