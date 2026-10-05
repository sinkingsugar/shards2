#!/usr/bin/env python3
"""Refresh scheduler/async measurements; build release examples before running."""
import datetime
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

root = Path(__file__).resolve().parents[2]
shards1 = Path(sys.argv[1]).resolve()
out = Path(sys.argv[2])
cpu = sys.argv[3] if len(sys.argv) > 3 else "2"
records = []

def run(args):
    command = ["taskset", "-c", cpu, *map(str, args)]
    result = subprocess.run(command, cwd=root, text=True, capture_output=True, check=True)
    record = {"command": command, "stdout": result.stdout, "stderr": result.stderr}
    records.append(record)
    print(result.stdout, flush=True)
    return result.stdout

def fields(line):
    return dict(re.findall(r"([\w]+)=([^\s]+)", line))

metadata = {
    "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
    "cpu": cpu,
    "rounds": 3,
    "binaries_sha256": {},
    "note": "Release examples rebuilt at this HEAD. Only benchmark harness changes in working tree. Sequential runs, CPU pinned; frequency not locked. HTTP measured separately without affinity pinning.",
}
for binary in [shards1, *(root / "target/release/examples" / name for name in ["bench_instances", "bench_depth", "bench_async"])]:
    metadata["binaries_sha256"][str(binary)] = hashlib.sha256(binary.read_bytes()).hexdigest()

for round_id in range(3):
    output = run(["bash", "bench/shards-1x/run.sh", shards1, "1"])
    for line in output.splitlines():
        if "BENCH" not in line:
            continue
        f = fields(line)
        expected = int(f["instances"]) * int(f["ticks"])
        assert int(f.get("resumes", f.get("iterations"))) == expected // (1 if "resumes" in f else 2), line
    assert sum("BENCH" in line for line in output.splitlines()) == 7
    for scheduler in (["stackless", "stackful"] if round_id % 2 == 0 else ["stackful", "stackless"]):
        flags = ["--stackless"] if scheduler == "stackless" else []
        for n in [100, 1000, 10000]:
            output = run([root / "target/release/examples/bench_instances", str(n), *flags])
            assert int(fields(output)["iterations"]) == n * 500
        output = run([root / "target/release/examples/bench_depth", *flags])
        assert len(output.splitlines()) == 4
        for line in output.splitlines():
            f = fields(line)
            assert int(f["resumes"]) == int(f["expected"]) == 1000000
    output = run([root / "target/release/examples/bench_async"])
    assert len(output.splitlines()) == 4
    counts = [int(fields(line)["completed_requests"]) for line in output.splitlines()]
    assert counts[0] > 0 and len(set(counts)) == 1, counts

out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps({"metadata": metadata, "records": records}, indent=2) + "\n")
