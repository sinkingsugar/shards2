#!/usr/bin/env python3
"""Alternate the same workloads across preserved M3 and candidate M4 binaries.
Usage: run.py BASELINE_EXAMPLE_DIR CANDIDATE_EXAMPLE_DIR OUTPUT.csv [trials]
Build the M3 directory with the candidate bench_depth.rs, so only engine code
changes. Each command validates its completed-work count before reporting.
"""
import csv
import pathlib
import subprocess
import sys

baseline, candidate, output = map(pathlib.Path, sys.argv[1:4])
trials = int(sys.argv[4]) if len(sys.argv) > 4 else 5
engines = [
    ("old-stackless", baseline, ["--stackless"]),
    ("stackful", baseline, []),
    ("trampoline", candidate, ["--stackless"]),
]
with output.open("x", newline="") as file, output.with_suffix(".log").open("x") as raw:
    writer = csv.DictWriter(file, fieldnames=["trial", "engine", "workload", "depth", "ns_per_instance_tick"])
    writer.writeheader()
    for trial in range(trials):
        # Rotate order and alternate direction; no blocks of one engine's trials.
        order = engines[trial % 3:] + engines[:trial % 3]
        if trial % 2:
            order = order[::-1]
        for engine, directory, flags in order:
            for binary, args in [("bench_depth", []), ("bench_depth", ["--progress"]), ("bench_instances", ["1000"])]:
                result = subprocess.run([str(directory / binary), *args, *flags],
                                        check=True, text=True, capture_output=True)
                raw.write(f"trial={trial + 1} engine={engine} command={binary} args={args + flags}\n")
                raw.write(result.stdout)
                raw.flush()
                for line in result.stdout.splitlines():
                    fields = dict(item.split("=", 1) for item in line.split() if "=" in item)
                    if binary == "bench_instances":
                        assert int(fields["iterations"]) == 500000, fields
                        assert int(fields["instances"]) == 1000, fields
                    writer.writerow(dict(trial=trial + 1, engine=engine,
                                         workload=fields.get("phase", "mixed"),
                                         depth=fields.get("depth", ""),
                                         ns_per_instance_tick=fields["ns_per_instance_tick"]))
                file.flush()
                print(f"trial={trial + 1} engine={engine} workload={binary}", flush=True)
