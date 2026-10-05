# VM execution: 1.x against 2.0

Measured locally on 2026-10-05. This answers a different question from the
[scheduler benchmarks](stackless-experiment.md): how much does executing a
long chain of already-composed shards cost, with no suspension or scheduling
inside the timed region?

**Initial baseline:** the 1.x release executes these small, hot shard chains
substantially faster than the unoptimized 2.0 prototype. Both 2.0 backends show
the gap. Sharing compiled programs and reducing instance memory remain useful,
but those advantages do not establish competitive instruction throughput.
This is a measured optimization priority, not a reason to abandon the
compiled/state split or a conclusion about Rust versus C++ in general.

**Follow-up:** the [builtin executor rerun](#builtin-executor-follow-up-2026-10-05)
reaches roughly 1.x speed for the measured Add chains and most cheap value
operations. Collection construction, table lookup and shared-sequence Push
still miss the 2× target. The original measurements below remain the baseline.

## Method

The [reproducible harness](../bench/vm-execution/README.md) generates the same
script for all three engines. Each batch times a `Repeat` inside one running
wire; no `Pause`, async work, mesh ticking, logging, parsing, compose or
instantiation is timed. The inner iteration reads a seed, runs a chain,
saves the result and increments a counter. After timing, full result equality
and the exact counter are checked. Every calibration, warmup and retained
sample must pass, or the run aborts.

Widths 0, 8, 64 and 256 distinguish fixed loop/sink costs from marginal chain
costs. The same instantiated body runs two warmup batches followed by three
retained batches. Three independent processes per engine/cell yield nine
retained samples. Job and engine order are shuffled deterministically. All
engines use the same calibrated iteration count in each cell. See the harness
README for calibration thresholds and the precise meaning of every workload.

Machine: Linux x86-64, Intel i9-14900KF, pinned to performance core CPU 2;
`intel_pstate` with the existing `powersave` governor. Frequency/turbo were not
locked. No workspace builds or other benchmark jobs ran concurrently with
measurements.
This is a single-machine result; minimum/maximum and median absolute deviation
are retained, not a claim of machine-independent cycle counts.

Builds:

- **1.x:** adjacent checkout at `37597927e`, freshly rebuilt with
  `cmake --build ../shards/build/Release --target shards -j 4`.
  GCC 16.2.1, Release, `SHARDS_INLINE_EVERYTHING=ON`, debugger and Tracy off.
  The core compile command uses `-O3 -march=broadwell`, inlining/unrolling
  flags and `-ffast-math -fno-finite-math-only`; no `-fsanitize` instrumentation
  flag. The checkout reported a dirty dependency submodule; this is recorded
  in metadata, not represented as a completely pristine source tree.
- **2.0 default:** runtime at `77dd307`, fresh
  `cargo build --release -p shards-cli`, pinned Rust 1.98.1, default release
  settings and features; no `output-checks`.
- **2.0 sensitivity build:** same source, ThinLTO, one codegen unit and
  `target-cpu=broadwell`. This tests whether stronger compiler settings close
  the gap; it is not a runtime optimization patch.

The two default builds have different compiler settings, intentionally
reflecting their existing release configurations. Inputs avoid overflow,
NaNs and rounding-sensitive cases. The sensitivity build narrows the target
CPU/build-optimization difference but does not equate all C++/Rust semantics.

## Initial baseline results

Median **ns per complete 256-motif iteration**, including the fixed
seed/result/counter/Repeat overhead. Lower is better. A motif is one shard
for Const/Get/Update/Add, two for Take/independent assignment, a call plus an
Add for Do, and a frontend expression for construction. Do not read this as
an equal-shard-count comparison across rows.

| Workload | 1.x | 2.0 stackful | 2.0 stackless | Stackless / 1.x |
|---|---:|---:|---:|---:|
| Integer Add | 290.4 | 3,923.2 | 3,884.2 | 13.37× |
| Float Add | 371.2 | 4,039.7 | 4,087.0 | 11.01× |
| Float4 Add (variable operand) | 362.2 | 4,831.8 | 4,829.4 | 13.33× |
| Integer Const | 122.6 | 2,446.6 | 2,445.4 | 19.95× |
| String Const (256 bytes) | 127.9 | 3,838.9 | 3,856.5 | 30.16× |
| Integer Get | 126.9 | 3,259.3 | 3,336.0 | 26.30× |
| Sequence Get (64 elements) | 200.3 | 4,643.0 | 4,686.0 | 23.39× |
| Get + table field Take | 860.4 | 11,064.9 | 11,854.6 | 13.78× |
| Do with one integer Add | 4,799.0 | 8,528.3 | 8,015.1 | 1.67× |
| Construct 4-element sequence | 1,400.8 | 11,076.2 | 11,210.2 | 8.00× |
| Construct 4-field table | 2,064.5 | 34,148.1 | 34,839.0 | 16.88× |
| Append to shared sequence | 690.9 | 9,926.9 | 10,436.3 | 15.11× |
| Independent integer assignment | 447.4 | 7,045.5 | 7,135.4 | 15.95× |
| Independent string assignment | 1,239.8 | 11,463.4 | 11,475.3 | 9.26× |
| Independent sequence assignment | 16,783.5 | 11,396.7 | 11,856.2 | 0.71× |
| Independent table assignment | 19,936.0 | 11,401.4 | 11,415.4 | 0.57× |

This confirms a large gap in cheap shard throughput. It also shows a real
benefit from 2.0's ownership choice: independent sequence/table assignment is
faster because sharing avoids copying the collection contents. These sizes
are examples, not a measured crossover curve over collection size.

The `update-*` chains are kept separately in the raw results: after the first
Update, 1.x forwards the destination's address and later updates become
self-assignments. The `assign-*` cases above explicitly re-read the original
source before **every** update. Treating these as equivalent copy benchmarks
would substantially misrepresent 1.x's behavior.

Raw measurements, calibration, summaries, fitted slopes and build provenance:
[main suite](../bench/vm-execution/results/2026-10-05-default/),
[independent assignment](../bench/vm-execution/results/2026-10-05-assign/).
All results and completion counts passed. Widths 0/8/64/256 and all retained
samples are available; the table does not select the fastest single sample.
Fits are descriptive only: the 1.x self-assignment sequence/table curves have
R² around 0.97, so a constant slope is a less complete description there.

### Compiler-settings sensitivity

Same 256-motif iterations, with nine retained samples per cell again:

| Workload | 1.x rerun (ns) | 2.0 default stackless (ns) | 2.0 tuned stackless (ns) | Tuned / 1.x rerun |
|---|---:|---:|---:|---:|
| const-int | 122.9 | 2,445.4 | 2,485.3 | 20.22× |
| const-string | 128.0 | 3,856.5 | 3,787.2 | 29.58× |
| get-seq | 199.1 | 4,686.0 | 4,699.5 | 23.60× |
| add-int | 288.4 | 3,884.2 | 3,866.7 | 13.41× |
| add-float4 | 363.1 | 4,829.4 | 4,794.9 | 13.20× |
| assign-seq | 17,029.1 | 11,856.2 | 10,748.1 | 0.63× |
| assign-table | 16,758.8 | 11,415.4 | 10,707.7 | 0.64× |

The stronger build does **not** close the dispatch/value-transfer gap. Its
stackful results are also retained in the [sensitivity data](../bench/vm-execution/results/2026-10-05-lto/).
This is not an attribution of every small difference to compiler flags: the
unchanged 1.x table-assignment binary also ran faster in the second suite
(about 16.8 µs versus 19.9 µs). Independent suite/build runs can differ beyond
within-run dispersion. The large hot-path gap and the collection-sharing
advantage persist in both suites.

Reproduce the sensitivity build and selection with:

```sh
CARGO_TARGET_DIR=/tmp/shards-vm-lto CARGO_PROFILE_RELEASE_LTO=thin \
  CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 RUSTFLAGS='-C target-cpu=broadwell' \
  cargo build --offline --release -p shards-cli
python3 bench/vm-execution/run.py ../shards/build/Release/shards \
  --shards2 /tmp/shards-vm-lto/release/shards2 --source1 ../shards --cpu 2 \
  --cases const-int const-string get-seq add-int add-float4 assign-seq assign-table \
  --widths 0 64 256 --out target/vm-execution/tuned-run
```

## What the baseline execution paths explain

These describe the initial baseline before the builtin executor below. They
are code/assembly observations, not a profiler's allocation of the measured
time to individual causes:

1. **1.x has specialized built-in dispatch.** Its
   `shards/core/runtime.cpp::shardsActivation` walks a null-terminated pointer
   array, calls `activateShardInline`, checks control flow and forwards the
   returned pointer. `shards/modules/core/inlined.cpp` directly handles
   `Const`, `Get`, variable operations and specialized arithmetic, among
   others. A constant can return its stored value's address; a local read can
   return its cell. Generic extension shards need not get these fast paths.
2. **2.0 pays for owned outputs and a general result envelope.** The current
   `Var` is 24 bytes/alignment 8 on this target, versus 1.x's 32-byte,
   16-aligned `SHVar`. However, `Flow`/`Step` are 32 bytes and their `Result`
   envelopes are 40 bytes. Smaller `Var` size alone does not predict the cost
   of forwarding it through an activation call. Release disassembly of
   `CompiledFlow<Stackless>::activate` shows an indirect call, result/tag
   handling, multiple stack value moves, and drop handling between nodes.
3. **Reference-counted reads are not pointer forwarding.** `Const` and `Get`
   return cloned values; strings, sequences and tables use `Arc`. The inspected
   release code contains `lock incq` in `Var::clone` and `lock decq` in flow
   value replacement. This is real work even without thread contention.
4. **Compose does not yet specialize 2.0 arithmetic as aggressively.** The
   shared `math::arith` checks runtime variants and supports mixed numeric
   shapes. Float vectors go through f64 components before becoming f32 vectors
   again; 1.x has dedicated vector activation cases. Integer overflow checks
   also differ. The arithmetic result includes these implementation/semantic
   differences, not just dispatch.
5. **Collection construction has different reuse behavior.** The same source
   expression lowers differently in each frontend. These cases include the
   resulting execution and allocation/reuse choices. They must not be reported
   as equal numbers of shard dispatches or as evidence about only enum layout.

The 2.0 stackful loop uses `iter().zip()`; the stackless loop uses explicit
indexing. Their similar results show that switching schedulers or blaming
iterator syntax does not account for the large observed gap. This is not an
isolated A/B proof that every iterator formulation compiles identically.

## Baseline decisions and limits

Keep the shared immutable compiled artifacts and per-instance state model.
Before treating the hot execution contract as settled, measure changes to:

- borrowed/pass-through results or stable output slots, with explicit lifetime
  rules across variable writes and suspension;
- compose-selected builtin activation paths, including typed arithmetic;
- the size and movement of activation results and avoidable clone/drop pairs;
- table construction reuse and compose-resolved field access;
- state erasure/downcast costs and state storage, separately from the above.

Do not introduce unchecked borrowing or raw-pointer lifetime assumptions just
because 1.x is faster here. Each candidate needs the same result checks, both
backends' acceptance suites, and reruns of the instance/scheduler benchmarks.
The current measurements do not isolate the contribution of `Any` downcasts,
bounds checks, allocator traffic or cache misses; no hardware-counter profile
was obtained.

This suite excludes many-instance scaling, creation/compose time, deep
suspension/resumption, I/O, graphics, browser/ESP32 execution, unique-buffer-only
mutation and generic host-shard dispatch. Table-field mutation is absent
because the matching 2.0 shard is not implemented. Hot repeated motifs favor
predictable branches and cache residency. Whole applications mix these costs
with work that may dominate them; no application-wide speed ratio is claimed.


## Baseline validation

All timed workloads passed their value and completed-work checks on all three
engines. The final generator reproduced every measured script byte-for-byte;
`scripts.sha256` in each result directory records their hashes. Parser tests
reject wrong results/counts, missing/extra samples and invalid durations.

Local repository checks passed: workspace check, formatting (including the
ESP32 workspace), all-target clippy with warnings denied, workspace tests,
docs-off core/IO metadata and catalog tests and frontend tests, release
nesting-limit tests, rustls-ring clippy, wasm core/frontend clippy, and the
prototype/metadata/frontend WASI test binaries under Node. The HTTP tests
needed loopback sockets enabled outside the restricted sandbox. No runtime
source or dependencies changed. CI results are separate from these local
checks.

## Builtin executor follow-up (2026-10-05)

The revised default release build reaches roughly 1.x throughput for the
measured integer, Float and Float4 Add chains. This is a runtime change,
not an LTO or fast-math build. The same `Var` enum still measures 24 bytes;
changing how values travel through the executor accounts for these gains.
The 2× target is **not** met across all collection workloads.

Both schedulers use one shared executor for compose-selected core operations.
The useful 1.x ideas implemented here are:

- **Typed builtin dispatch:** compose selects Const/Get/Set/Ref/Update/Inc,
  Int/Float/Float4 Add, Take and non-clearing Push. Rust implementation identity
  selects a builtin, so a host shard with the same name is not substituted.
- **Borrowed value forwarding:** constants and frame reads forward pointers
  within an uninterrupted segment. Assignment clones before replacing its
  destination, then forwards that destination; self-assignment skips the copy.
  The executor returns an owned snapshot before generic activation or suspension.
- **Reusable numeric storage:** explicit numeric variants use a call-local
  `MaybeUninit<Var>` slot that needs no destructor dispatch. General results
  use ordinary drop-managed scratch. Checked integer overflow is retained.
- **Direct vector arithmetic:** Float4 + Float4 adds `f32` lanes directly;
  the default x86-64 release emits packed `addps`, with no explicit intrinsics
  or target-specific source. Mixed scalar/vector inputs use the existing
  arithmetic rules. The [assembly excerpt](../bench/vm-execution/results/2026-10-05-inline/simd.txt)
  is tied to the binary hash in the measurement metadata.
- **Collection buffer reuse:** Seq.Make and Table.Make keep an output buffer
  in instance state and reuse it only when no saved output shares it. Shared
  outputs trigger a fresh allocation. Compiled artifacts remain immutable.

### Matched rerun

Same machine, CPU affinity, default release settings and complete harness
method as above: all 25 workloads, widths 0/8/64/256, three independent
processes and nine retained samples per engine/cell. No builds or other
benchmarks ran concurrently. All calibration, warmup, value and iteration
checks passed. The table again gives complete 256-motif iteration medians,
not isolated instruction latency or the fastest sample.

| Workload | 1.x ns | 2.0 stackful ns | 2.0 stackless ns | Stackless / 1.x |
|---|---:|---:|---:|---:|
| add-int | 294.9 | 190.3 | 186.2 | 0.63× |
| add-float | 368.8 | 357.0 | 357.6 | 0.97× |
| add-float4 | 369.0 | 372.0 | 369.9 | 1.00× |
| const-int | 124.3 | 126.5 | 127.5 | 1.03× |
| const-string | 139.3 | 131.0 | 128.4 | 0.92× |
| get-int | 125.6 | 199.4 | 194.1 | 1.55× |
| get-seq | 202.4 | 203.3 | 197.5 | 0.98× |
| update-int | 326.2 | 248.7 | 246.2 | 0.75× |
| assign-int | 440.2 | 579.6 | 571.2 | 1.30× |
| assign-string | 1,288.5 | 2,589.2 | 2,584.3 | 2.01× |
| assign-seq | 17,236.0 | 2,060.8 | 2,063.8 | 0.12× |
| assign-table | 17,127.1 | 2,058.2 | 2,053.0 | 0.12× |
| take-seq | 1,567.7 | 653.1 | 653.4 | 0.42× |
| take-table | 875.6 | 3,334.3 | 3,206.9 | 3.66× |
| do-int | 4,860.1 | 6,931.1 | 6,378.9 | 1.31× |
| push-shared-seq | 734.9 | 2,536.3 | 2,533.0 | 3.45× |
| make-seq | 1,380.0 | 10,760.3 | 11,399.0 | 8.26× |
| make-table | 2,012.0 | 10,975.7 | 11,398.4 | 5.67× |

[Full raw data, calibration, summaries and slopes](../bench/vm-execution/results/2026-10-05-inline/)
include the omitted Const/Get/Update variants. Source provenance is the
working tree based on `98a75c6`, containing this optimization; metadata does
not imply the binary came from that clean baseline. `runtime-source.sha256`
records the measured core source files (including the new executor), and
`scripts.sha256` records the generated scripts. This commit carries the
implementation and data together.

### Remaining costs and scope

Table Take still performs a BTreeMap key lookup. The 1.x implementation has
fixed-field indexing and a cached table-value path; these are candidates for
future compose-resolved storage/access work. A cached raw pointer needs an
explicit ownership/invalidation contract, not just the 1.x address trick.
Push still pays copy-on-write uniqueness checks and growth; construction
still goes through generic leaf activation with owned Arc results. The same
source also lowers differently for construction. The measurements do not
attribute every remaining nanosecond to one of these mechanisms.

Independent string assignment is around the 2× boundary; sharing still wins
strongly for the tested sequence/table assignments. The specialized math
coverage here is Add on the listed types, not every math operator or shape.
Generic host shards keep their existing APIs and dispatch. Compiled flows
now retain an extra instruction array; constructor buffers occupy per-instance
state. Neither program-size cost nor arbitrary application throughput is
established by the hot-chain results.

### Correctness and local validation

The raw-pointer accumulator is confined to one executor call over exclusive,
fixed-size frames. No arbitrary callback or suspension occurs inside it;
reads end before storage replacement, and the accumulator is reanchored
after writes. Output checks remain enabled in debug builds and with the
release `output-checks` feature. The lifetime contract is documented in
`inline.rs` and the shard contract.

The final implementation passed the complete local check set listed under
baseline validation, plus release core unit/acceptance tests with
`output-checks`. New shared tests cover snapshots across suspension and
nested writes, assignment aliases, and saved constructor outputs. Four
executor tests pass Miri on `nightly-2026-10-03`, including differential
owned-value execution, numeric scratch reuse, self-push and replacing a
collection after extracting an element. Miri is additional evidence, not a
proof of all executions. CI results are recorded separately from local checks.

The instance and depth examples were also rerun against an isolated build
of `98a75c6`, with default release settings, CPU 2 and three alternating
process rounds. `bench_depth` now uses 32 instead of 64 for its last depth
because the current compose limit is 48; the same one-line adjustment was
applied to the baseline example. All work counters matched.

| 10,000-instance workload | Baseline | Builtin executor |
|---|---:|---:|
| Stackful ns / instance tick | 299.2 | 279.3 |
| Stackless ns / instance tick | 119.6 | 92.1 |
| Stackful ready µs / instance | 3.28 | 3.16 |
| Stackless ready µs / instance | 0.90 | 0.79 |

This workload's per-instance state and measured heap remained unchanged
(696 bytes / 1.21 KiB stackful, 1,008 bytes / 1.47 KiB stackless). It does
not use the collection constructors and therefore says nothing about their
new buffer cost. Resume medians improved at most depths; stackless depth 32
increased from 535.1 to 546.6 ns/instance/tick (2.1%). This small three-run
sample is not evidence of zero regression on every workload.
[Commands, binary hashes and every output](../bench/vm-execution/results/2026-10-05-inline/scheduler-rerun.json)
are retained alongside the hot-chain data.
