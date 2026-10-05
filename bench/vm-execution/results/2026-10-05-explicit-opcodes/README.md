# Explicit opcode and Get-addressing experiment

See [the benchmark report](../../../../docs/vm-execution-benchmarks.md#explicit-opcode-tags-and-specialized-get-addressing)
and [assembly discussion](../../../../docs/vm-assembly-comparison.md#implemented-follow-up-explicit-tags-and-specialized-get-offsets).

The normal CSV/metadata files are the full suite on the final source snapshot.
`layout-matrix.json` is the separate four-way experiment: all scripts, timings,
iterations, backend labels and binary hashes are embedded. `matrix-driver.py`
is the exact driver used, calling the repository's shared output validator.
It runs from the repository root; its `/tmp` paths identify measured binaries,
not dependencies on files retained from the session.

To reproduce matrix candidates, create disposable source directories from
`git archive 4b524723e659f3e45075c1c363bc94bb4cd793ca`. Build the unmodified
baseline with `cargo build --release -p shards-cli`, then build three separate
copies with the corresponding `.patch` applied using `git apply`. Copy each
result to the driver's named binary path, or update its `binaries` dictionary
with their new paths. Supply a release 1.x binary as well. Run the driver on
an available CPU (recorded: CPU 2), with no other local benchmark/build active.
It writes `/tmp/shards-layout-matrix.json` after validating every result/count.
Do not assume timings or binary hashes will match on another toolchain/machine.

The final source adds a bounds regression test and a comment after the matrix;
the full suite, source fingerprints and final disassembly identify it separately.
`layout-metadata.json` records the measured code/stack sizes and instruction
stride. Full candidate patches make the experiment reproducible without a
previous session's temporary files. The public Var representation is unchanged.
