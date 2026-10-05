# Runtime performance and optimization headroom

Measured 2026-10-05, runtime implementation `f3f46e0`, checkout `bbabad1`.
All native binaries are release builds. This refresh covers the existing CPU VM,
instance/tick, nested-resume, simulated async and local HTTP benchmarks. It does
not measure a full application, graphics, physics, or embedded performance.

**Subsequent ownership fix:** the [inline segment follow-up](vm-execution-benchmarks.md#inline-segment-input-lifetime-review-follow-up)
fixes an additional quadratic retention case and reruns the complete VM suite.
Arithmetic remains near parity; 19/25 long-chain cases meet ≤2× (Get(Int) now
2.05×). The tables below describe the explicitly identified earlier snapshot;
the scheduler/HTTP refresh has not been repeated for the later fix.

**Compose specialization and assembly:** the [later compose-time cleanup pass](vm-execution-benchmarks.md#compose-time-scratch-release-selection)
removes redundant scratch checks. Its full VM rerun and the
[release assembly comparison](vm-assembly-comparison.md) identify remaining
opcode decoding and frame binding costs. The historical tables below retain
their original snapshot.

**Latest internal layout:** [explicit opcode tags and specialized Get offsets](vm-execution-benchmarks.md#explicit-opcode-tags-and-specialized-get-addressing)
were adopted after an alternating four-way experiment and full VM rerun.
Const(Int) is now 1.02× and Get(Int) 1.13× 1.x at width 256. The
[assembly follow-up](vm-assembly-comparison.md#implemented-follow-up-explicit-tags-and-specialized-get-offsets)
records correctness evidence, memory/code costs and limits. Historical tables
below are unchanged; no later scheduler/HTTP rerun is implied.

## Lessons from the VM optimization work

Conclusions at `12ef2f0`, after the assembly comparison, controlled layout
experiment and full VM rerun:

1. **1.x's small activations were deliberate engineering.** The inspected
   Const and Get handlers reduce to producing an address or loading a resolved
   pointer. Early resolution, compact dispatch and careful value forwarding
   matter when useful work is only an assignment or addition. A language choice
   or an optimizing compiler alone does not guarantee that execution shape.
   These are principles to retain without copying 1.x's instance architecture.
2. **Representation matters, but the evidence distinguishes which part.**
   Explicit opcode tags removed niche decoding; compose-selected Local/Mesh Get
   offsets removed repeated binding selection and index multiplication. The
   four-way experiment supports both changes. Public `Var` stayed unchanged,
   measured instruction size stayed 32 bytes, and persistent instance state did
   not grow. This supports the current foundation, not a claim that every
   representation choice is optimal. Function code and stack use did grow
   slightly; the [assembly report](vm-assembly-comparison.md#implemented-follow-up-explicit-tags-and-specialized-get-offsets)
   records those costs.
3. **Move decisions to compose when their inputs are known there.** Cleanup
   variants now encode whether scratch may own a value. Runtime still enforces
   frame bounds and integer overflow semantics. Shared compiled instructions
   contain offsets rather than pointers into a particular instance. The
   compiled/state split therefore supports specialization and instance sharing
   together.
4. **Ownership lifetime is part of performance correctness.** Two optimizations
   preserved output values but retained extra collection references, turning
   linear accumulation into quadratic copying. Constructor state now retains no
   output, and segments consume and release obsolete input. Regression tests
   check owner counts and allocation identity as well as output snapshots.
   Reuse must follow ownership becoming available, not prolong ownership just
   to retain an allocation.
5. **Optimize the measured operation.** Float4 already emits packed SIMD.
   Dispatch improvements helped cheap activations; collection construction,
   shared mutation and table access need different work. The existing
   passthrough-retention case (`acc | ExpectSeq` followed by Push) remains an
   investigation item, distinct from the two fixed regressions. Short mixed
   flows also need attention: long-chain wins do not imply every short case
   improves.

Keep the VM foundation and pursue collection ownership/access and short mixed
flows next. The latest results and remaining misses belong in the
[VM report](vm-execution-benchmarks.md#explicit-opcode-tags-and-specialized-get-addressing),
not a universal runtime speed ratio. Deeper stackless resume remains a separate
control-flow project. Future fusion must preserve lifecycle, source/error,
mutation and reload boundaries; the current one-instruction-per-node mapping
is not yet a general optimizing IR.

The [final independent review](../.agent-handoffs/reviews/2026-10-05-12ef2f0-claude-bc7c33.md)
found no issues within scratch lifetime, Get offset safety and benchmark-evidence
scope. It ran targeted native debug/release tests and Miri, checked source hashes
and recomputed reported tables from retained data. It did not rerun benchmarks,
the full check set, WASI or CI, or independently inspect the 1.x assembly side.
Earlier constructor/input-lifetime fixes have separate verification records.
This is bounded evidence for the design, not a whole-runtime safety or optimality
certificate.

## Results

The [complete VM suite](vm-execution-benchmarks.md#constructor-retention-review-follow-up)
was already rerun against this implementation: 25 workloads, widths 0/8/64/256,
both 2.0 schedulers and 1.x, nine retained samples per cell, verified outputs.
At width 256, stackless meets the ≤2× time target in 20 of 25 workloads:

| Workload | 2.0 stackless time / 1.x time (lower is better) |
|---|---:|
| Int Add | 0.75× |
| Float / Float4 Add | 0.99× / 1.02× |
| Constants | 0.82–1.37× |
| Get | 1.21–1.89× |
| Update | 0.52–0.90× |
| Assign sequence / table | 0.13× / 0.11× |
| Assign string | 2.12× |
| Construct sequence / table | 3.24× / 3.07× |
| Push to shared sequence | 3.42× |
| Take sequence / table | 0.47× / 3.72× |
| Do integer subflow | 1.31× |

These ratios describe long hot chains, not short flows or entire applications.
The report also retains zero-width costs and fitted slopes. Sharing semantics
help collection assignment; this is not a faster deep copy. Constructor reuse
applies to consumed input/consecutive constructors, not arbitrary invocations.

Fresh scheduler medians, ns per instance per tick (three process runs):

| Entity instances | 1.x | 2.0 stackful | 2.0 stackless |
|---:|---:|---:|---:|
| 100 | 70.3 | 110.0 | 92.6 |
| 1,000 | 185.8 | 133.6 | 90.5 |
| 10,000 | 637.5 | 294.6 | 95.4 |

The entity performs math, variable updates, a nested call and suspension.
Every run checked `instances × 500` completed iterations. At 10,000 instances,
2.0's spawn-through-first-activation cost was 0.79 µs/instance stackless and
3.11 µs stackful. Current RSS growth was 1.50 / 5.34 KiB per instance, respectively;
counted live heap was 1.47 / 1.21 KiB. These are fresh 2.0 creation/memory results;
1.x creation/memory was not rerun in this refresh.

Pure nested-resume medians, ns per instance per tick:

| Nested depth | 1.x | 2.0 stackful | 2.0 stackless |
|---:|---:|---:|---:|
| 1 | 131.1 | 121.8 | 103.7 |
| 4 | 138.9 | 120.7 | 141.6 |
| 16 | 164.5 | 123.2 | 317.7 |
| 32 | 166.2 | 134.9 | 561.8 |

All runs checked 1,000,000 resumes. Both harnesses now use depth 32 instead of
64, within 2.0's compose nesting limit. Historical depth-64 results are not
current executable coverage. Stackless explicitly re-enters suspended parents;
its depth-dependent cost remains visible.

Simulated async, 1,000 instances with 50-step latency: stackless notification
mode took 4.5 µs/tick versus 14.6 µs polling every tick; stackful took 5.8 versus
39.7 µs. All four modes completed 19,000 requests; notification reduced future
polls from 1,000,000 to 38,000. This compares 2.0 wake modes, not 1.x.

HTTP execution medians against the same local server with 50 ms/request delay:

| Requests | 1.x | 2.0 stackful | 2.0 stackless |
|---:|---:|---:|---:|
| 1 | 51.9 ms | 51.7 ms | 51.6 ms |
| 32 | 205.0 ms | 52.3 ms | 52.5 ms |
| 100 | 509.0 ms | 62.0 ms | 62.7 ms |
| 1,000 | 2,488.5 ms | 107.7 ms | 120.4 ms |

All 36 runs checked client successes and server completions against N. At 1,000,
the server saw 32 concurrent requests for 1.x versus 1,000 for both 2.0 backends;
client peak thread counts were 38 versus 5. This measures the different I/O
scheduling strategies and 1.x's worker-pool ceiling, not raw VM speed. Creation
is timed separately in the raw data; HTTP runs were not CPU-pinned.

CPU measurements used CPU 2 on the same i9-14900KF, Rust 1.98.1 and existing
1.x release binary as the VM report. Frequency was not locked. Processes ran
sequentially, with no concurrent builds/benchmarks. CPU rounds alternate the
2.0 backend order; 1.x precedes them each round. HTTP uses the existing grouped
runtime order. Three samples describe this machine, not statistical guarantees.
[Raw data and reproduction commands](../bench/runtime-overview/README.md).

## Are the architecture choices sound?

**Keep the compiled/state split, explicit bindings, shared leaf/async APIs and
both backends. Keep the executor representation open to change.** This is an
engineering assessment supported by these measurements, not proof that every
future optimization will be cheap.

- **Compose is the right specialization boundary.** Types, operands and frame
  bindings are resolved before activation; shared immutable compiled artifacts
  can hold optimized code once for every instance. The new typed builtin stream
  already reaches arithmetic parity without changing the public `Var` layout
  or requiring every shard author to write two implementations.
- **The instruction stream is a useful starting point, not a finished optimizing
  IR.** `CompiledFlow` currently keeps nodes and instructions in one-to-one
  correspondence; activation/lifecycle state is still indexed by node. Fusing
  instructions or flattening calls requires an explicit mapping back to logical
  nodes for cleanup, errors, source locations and reload boundaries. A lowering
  pass is feasible within this design, but is not implemented today.
- **Owned values at generic boundaries have a cost.** Builtins can borrow within
  a segment; generic shards return owned `Var`s, and suspension preserves owned
  input snapshots. More ownership-aware builtins and liveness-based elimination
  of temporary values fit the design. Borrowed outputs from arbitrary host
  shards would require an additional API/lifetime contract. Do not let borrowed
  frame pointers escape across callbacks, mutation or suspension. The constructor
  regression demonstrates why hidden retained aliases can destroy performance.
- **Table storage is a concrete data-layout target.** A known fixed shape could
  share its keys and address values by compose-resolved slots instead of a
  BTreeMap lookup. Sorted iteration, dynamic tables, snapshots and mutation must
  keep their defined semantics. Public `Var::Table` currently exposes the map
  representation, so this would affect host code, not just the private executor.
- **Stackless continuation traversal is the largest measured control-flow
  limitation.** An explicit continuation stack/program counter could avoid
  re-entering every suspended ancestor. That requires reworking control shards,
  unwinding, cancellation and reload handling; it is more than an inline tweak.
  The common leaf/async contracts and compiled/state split can survive that work.
  Keep stackful available for deeply suspended native flows in the meantime.
- **SIMD is available, but not the missing general solution.** The measured
  Float4 builtin already emits packed arithmetic. Table lookup, reference
  ownership and ancestor traversal need different optimizations. General loop
  vectorization would additionally require dependency/alias/effect information
  that the current instruction stream does not express.

Next experiments should measure fixed-shape table access and short mixed
builtin/host flows first, then prototype direct stackless resume with the same
cleanup/cancellation/reload tests. Fusion needs explicit read/write and error
ordering rules before transformations become general. Generic per-node state
boxing is another footprint opportunity. These are proposed follow-ups, not
features delivered by this benchmark refresh; no universal parity claim follows
from the current arithmetic results.
