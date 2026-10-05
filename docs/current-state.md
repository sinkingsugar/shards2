# Current state and next work

Checkpoint: 2026-10-05, implementation through the handoff-record reviews of 2026-10-05 (after `40e67d5`). Check git history and the working tree for later changes. The public repository starts from a squashed snapshot of 2026-10-05; commit hashes cited in the docs and handoff records before it refer to the private archive of the earlier history. This file is the short handoff for a fresh session; detailed contracts live in the linked documents.

## Settled decisions

Review exchange between sessions uses tracked records in [.agent-handoffs/](../.agent-handoffs/README.md), through the shared [review-handoff skill](../skills/review-handoff/SKILL.md). Reviews, implementation responses and independent verification are separate records; invoke the skill in the receiving session to pick them up.

- Rust hosts the runtime; shards implement Rust traits. External Rust crates can be compiled into a custom runtime. A stable binary-plugin/foreign-language shard ABI is deferred.
- Compose produces shared, immutable compiled artifacts; each instance owns its runtime state. Compose purity is a trusted contract, with recorded dependencies (including absent lookups) rechecked before cache reuse.
- Stackless is the default. Both stackless and stackful are maintained, with one backend per mesh and no cross-backend `Do`. Real-workload tracing informs optimization; it no longer gates this decision.
- Most shard authors use one `LeafShard` or `AsyncShard` implementation. Core control-flow shards share compose logic but have backend-specific execution.
- Static metadata owns the public argument contract. Shared decoding owns forms, literal types and defaults; shard compose owns bindings, contextual relationships and nested-flow validation. Do not rebuild the full 1.x automatic parameter system.
- 1.x is a semantic reference, not a requirement for one-to-one architecture or feature parity.
- Frontend: a new crate that owns its sources (no 1.x runtime dependencies) lowers to `WireDef`/`ShardDef`. Source spans live in a frontend side table keyed by the structural occurrence path that diagnostics carry, never in the cache keys. The parser is hand-written (lexer plus recursive descent) for agent-quality errors and spans; `;` comments are rejected, `And`/`Or` only compose inside conditions, and the top level is declarations with `@run` as the entry ([surface-syntax-review.md](surface-syntax-review.md) §5).
- Tables have string keys in sorted order; fixed tables reject unknown keys at compose; "maybe missing" is a `T | None` set, not an optional key ([values-and-types.md](values-and-types.md)).

Sources: [shard contract](prototype-shard-contract.md), [core design](shards-2-compose-split.md), [scheduler experiment](stackless-experiment.md), [metadata design](shard-metadata-and-compose.md), [values and types](values-and-types.md).

## Implemented

- Shared compose cache, frame slots, definite-initialization checks, both schedulers, and shared lifecycle helpers for cleanup and rollback. Finished execution data is released; `take_outcome` retires completed instance records.
- Shared leaf/async adapters, polling and notification wake modes, and native `Http.Get` through an external Tokio runtime. Local-server tests exercise pending-request cancellation and nonblocking mesh progress.
- Every core catalog shard, including test instrumentation, and `Http.Get` has a `ShardDesc`. The explicit catalog exposes JSON index/detail/search; parameter declarations power decoding and documentation. Prose can be compiled out with the `docs` feature.
- **Frontend (`shards-lang`).** A hand-written lexer and recursive-descent parser with spans on every node, error recovery, and messages in language terms (`missing \`}\` for the flow opened at 1:9 (found \`)\` at 3:1)`). It applies the review's decisions: `;` and `null` rejected with fixes, number forms, mixed paths, `{}` resolved by the parameter's declared forms, lowercase shard names and parameter names diagnosed. Lowering maps literals, variables, assignments (`=` `Ref`, `>=` `Set`, `>` `Update`), shard calls with named and positional arguments, nested flows, wire references, `@name` script arguments and the top-level declarations (`@wire` with `Looped`, `@mesh`, `@schedule`, `@run` with `FPS`/`Iterations`; loose code becomes the `root` wire when there is no `@run`). A source map keyed by occurrence path locates compose errors, including inside called wires. `check` returns the 1.x `{ok, file, diagnostics}` envelope; `run` drives either scheduler. Paths (`t.a.0`) lower to `Take`; f-strings to `Seq.Make` + `String.Format`; computed sequence and table elements to `Seq.Make`/`Table.Make`; a computed parameter value is computed first in a `SubFlow` that sets a temporary (`%n`), so the input flows past it; `>>` is `Push`. Core gained `Ref`, `SubFlow`, `Take` (fixed-table key errors with suggestions), `Push`, `Seq.Make`, `Table.Make`, `String.Format`, variadic parameters (`Requirement::Variadic`), source-syntax `Display` for values, `Phase::Parse` and `did_you_mean`. Local measurement: 137 of the 166 1.x test scripts parse clean, 27 fail only on `;` comments and 2 on `null`, both rejected by decision; the external host's scripts parse clean. Most 1.x scripts do not lower yet (missing shards).
- **Step-5 shards** (each described, in the catalog, tested through source on both schedulers): control flow `If`, `Match` (cases form), `Maybe`, `All`/`Any` (variadic conditions: Bool literals, variables or flows; short-circuit), `Repeat` `Until:`/`Forever`, `SubFlow`; `Log` (capturable per thread), `Stop`, `Count`, `Not`, `Is`, `IsNot`, `IsMore`, `IsLessEqual`, `IsAny`, `IsNone`, `IsNotNone`, `Time.Now`; `Math.Subtract/Multiply/Divide/Dec/Abs/Round/Floor/Ceil/Length`; `ToString`, `ToInt`, `ToFloat`, `ToHex`, `ParseInt`, `ParseFloat`, `ToFloat2/3/4`, `Expect*`. Shard aliases follow 1.x (`Add`, `Sub`, `Mul`, `Div`, `Inc`, `Dec`, ...); canonical names are 1.x's (`Math.Add`). Int and Float mix in arithmetic and comparisons ([values-and-types.md](values-and-types.md) §6). `And`/`Or` are rejected with the `All`/`Any` rewrite ([surface-syntax-review.md](surface-syntax-review.md) §7.1).
- Values and types ([values-and-types.md](values-and-types.md) §2-§4): `Var` has Float2, Float4 and string-keyed tables; types have fixed and open tables, canonical type sets and one acceptance rule (`Type::accepts`), used by argument decoding and `Set`/`Update`. The type registry hands out `&'static` descriptions under a read lock. No shard reads tables yet (`Take`, `Count` and key access come with the frontend).
- Structured compose/argument diagnostics retain useful 1.x field names and basic type codes, and carry an occurrence path (wire, shard index, parameter) from the composed wire down to the failing shard, through nested flows, `Do` and `Spawn`. Input mismatches also say where the input came from, skipping shards that pass it through ([metadata design](shard-metadata-and-compose.md) §6). Source locations need the frontend's side table and are not implemented yet.
- Stackless core tests run on wasm32-wasip1 under Node WASI. This is not browser integration; `shards-io` is native-only. Wasm panic-abort does not provide native panic isolation.

Benchmarks are matched prototype results, not full-runtime performance guarantees. See the experiment document for numbers and limitations. The HTTP concurrency gain also reflects replacing 1.x's blocking worker-pool path with Tokio tasks; it is not evidence that one scheduler beats the other.

## Review checkpoint

The [Codex handoff verification](../.agent-handoffs/verifications/2026-10-05-d395707-codex-8c9d.md) at `d395707` verifies the Push/Once, post-Stop typing and unused-cycle fixes. F4 remains partially open: draining now visits records once, but linear membership checks against retained entry IDs can still make retirement quadratic when many entries finish while another keeps running. Required local checks passed; see the record for evidence and scope.

The metadata implementation and catalog expansion were reviewed through `765ced1`. Findings about structured variable errors, declaration defaults, sequence type codes, overflowing Pause durations, and child-diagnostic ownership through Do/Spawn are fixed, with regression coverage committed. CI was reported passing on Linux, macOS and wasm.

Two architecture reviews followed (2026-10-04). Both agree the direction holds and the frontend is the right next step. Their findings and how they are handled:

- **[P2] The public API defaulted to stackful** although stackless is the documented default. Fixed: the crate-root `Mesh` is the stackless scheduler, the stackful one is `StackfulMesh` (native only), and compose types (`CompiledWire`, `ComposeCtx`, `ComposeCache`, `CompiledFlow`) no longer default their backend. A test pins the root `Mesh`.
- **The value and type model is the biggest untested piece** (no tables, no type sets). Taken up first: [values-and-types.md](values-and-types.md).
- **Compose errors carried no occurrence path**, so "Add failed" could not say which `Add`. Fixed: diagnostics carry a structural path; the frontend maps it to source.
- **Measure the port with real scripts**, not hand-written programs, and **ship a minimal `check --json`** to exercise the agent repair loop. Both are in the acceptance below.
- **Keeping two schedulers gets more expensive** as control-flow shards are added (1.x has `If`, `Match`, `Maybe`, `ForEach`, `Map`, `Expand`, `TryMany`, `Branch`, `Step`, `Detach`, ...). Rule: a control-flow shard lands on both backends in the same commit, with parity tests in the shared suite. Watch the count; if stackless deep resume is made cheap, revisit whether stackful still earns its cost.
- **Places where the code does not match the design yet** (fine for the prototype, listed so they are not mistaken for decisions):
  - `Do` composes its sub-wire inline at every call site (`ComposeCtx::compose_inline`), bypassing the cache; design §3.2 shares identical sub-wires.
  - Instance state is one heap allocation per shard (`Vec<Box<dyn Any>>` in `flow.rs`); the design calls for one allocation per instance, sized at compose.
  - The type registry is process-wide. Fixed: descriptions are no longer copied under the lock and readers share an `RwLock` ([values-and-types.md](values-and-types.md) §4); scoped registries remain deferred.
  - The HTTP client cache is process-wide and the compose cache is unbounded. Scoped services and cache lifetimes are needed for multiple hosts and live editing, not for the first parser slice.
- **Keep the process lighter.** Benchmark summaries live in [stackless-experiment.md](stackless-experiment.md) and design §5 only; other files link there. For porting, short notes here plus tests.

A third round (Astra and a four-reviewer Opus pass, 2026-10-05) covered `19a6d9d..41be25b`. All findings are fixed with regression tests on both schedulers:
- **Once and Maybe:** `Once` counted as done after a failure caught by `Maybe`, exposing an unassigned variable. Now it is done only when a run completes.
- **Frontend temporaries:** they collided when a wire was inlined by `Do` with different input types. Each `%` `Ref` now declares a fresh slot.
- **The suggested `If(All(a b) ...)` rewrite did not compile.** A flow parameter now takes a shard call, a variable or a literal as a one-shard flow.
- **Spawned-instance failures were dropped.** They are now reported, fail the run, and their records are retired every tick.
- **Numbers:** Int/Float equality and ordering lost precision above 2^53 (now exact). Equality's compose check rejected `[1] Is [1.0]` (now recursive). `ToInt` rejected values near the range limits.
- **`@run`:** tiny FPS values panicked, and `Iterations: 0` ran one tick. Both are validated now.
- **Types:**
  - Union canonicalization depended on printed forms (now structural, and keys are quoted).
  - `Expect*` interned a type per value, and `Spawn`/`set_var` compared exact types. All three now use `Type::admits`.
- **`check`:** it skipped unreferenced wires, and ignored scheduled wires without `@run`. Both are now reported.
- **`Push`:** it grew forever in a looped wire. It now has `Clear`, as in 1.x, through iteration-scoped locals.
- **Parser and lexer:** an uppercase path key swallowed the rest of the path, `{x: 1 y:}` dropped `y`, deep nesting crashed, and some 1.x escapes were rejected; `0x1g`, the `x-1` hint and `Times: Action:` are fixed too.
- **Misc:** computed elements ran before plain reads; a variable assigned in `Until` was rejected in `Action`; CLI argument order mattered; `log::capture` did not restore itself after a panic.
- **Added:** a `Never` type (`Stop`), so a branch or condition ending in `Stop` composes.

A follow-up review of those fixes found new problems, now fixed:
- `Push` clearing emptied a sequence whose declaring `Push` had not run, for example inside `Once`. It now clears in its own first run of each iteration, using a per-instance iteration counter (`LeafCtx::iteration`). This is not 1.x's rule; see the deviations below.
- `Do` chains bypassed the parser's depth limit and crashed the stackful scheduler. Compose now limits flow nesting, including wires run through `Do` or `Spawn` (`MAX_FLOW_DEPTH` = 48), and the parser counts brackets, braces and parentheses.
- The shard after a `Stop` gets a None input, but later shards keep their real types.
- `check` composes from reachability, so unreachable wire cycles are reported.
- The runner drains finished spawned instances in one pass per tick.

A third look found the stackful scheduler's 128 KB coroutine stack too small in debug builds for flows nested near the limit through `If`/`All`/`When`/`Repeat`. Debug builds now use 1 MB stacks; release keeps 1.x's 128 KB. A test runs a chain at the limit through each control shard on both schedulers, and CI also runs it in release. `Maybe` without `Else` now passes its input through, as in 1.x. `Match`'s pass-through on no match stays as a listed deviation.

Table storage and runtime sets were discussed; they are open decisions in [values-and-types.md](values-and-types.md) §7.

### Deviations from 1.x

Deliberate differences a ported script can hit:
- **Syntax:** `;` comments and `null` are rejected. `And`/`Or` are rejected, with the `All`/`Any` rewrite given. `Sub` is `Math.Subtract`; the run-a-flow shard is `SubFlow` (1.x `_SubFlow`).
- **Numbers:** Int and Float mix in arithmetic and comparisons. Int division truncates.
- **Tables:** keys are strings, and iteration is in sorted key order.
- **`Push` with `Clear`** starts the sequence over on the declaring `Push`'s first run in each loop iteration, so later pushes in the same iteration grow it. In 1.x the declaring `Push` clears on each of its runs, so when it is inside a loop body only the pushes after its last run remain; other `Push`es to the same variable append in both.
- **`Match`** with no matching case passes the input through; 1.x raises an error.
- **`Once`** runs again on the next activation after a failure.
- **`check`** composes wires that nothing references, with no input.

Use the repository tests and CI configuration to revalidate the current checkout. Previous `/tmp` review crates and conversation history are not prerequisites.

## Next: a frontend driven by real scripts

The first real user is an external host project (private) that runs Shards scripts over its own shards through the 1.x Rust bindings. Its scripts are small, fixed and exercise what a real host needs: struct tables, float vectors, nested control flow, two looped wires on one mesh, and slow shards that suspend only their own wire. Its shards are plain calls (`LeafShard`) or cancellable blocking calls (`AsyncShard`); none needs a per-backend implementation. They stay in that project, which also tests that an external crate can extend a custom runtime. This repository does not name or describe private downstream projects.

Order of work:

1. ~~**Values and types**~~: done (see Implemented).
2. ~~**Occurrence paths in diagnostics**~~: done (see Implemented).
3. **`shards-lang` crate: done for the external host's subset** (see Implemented). Paths, f-strings, computed elements and parameter values, and `>>` lower onto core shards. Still rejected explicitly: `#(...)`, enums, `@define`/`@template`, non-string table keys. Labeled flows as the canonical form come with the formatter. Measured locally: every remaining problem `shards2 check` reports on the external host's scripts is a missing shard (core ones listed in step 5, plus the host's own).
4. **CLI: done (minimal).** `shards2 check [--json] [--stackful] file key:value`, `run`, `describe`, `search`, `catalog` (`crates/shards-cli`). Human output renders `file:line:column`, the source line with a caret, suggestions and the occurrence path. Still open: the inferred type at every shard (review §7.4), `-I` include paths. Baseline on the 1.x check harness (`SHARDS=target/debug/shards2 ../shards/shards/tests/check/run.sh`, run locally 2026-10-04): 12 of 26 assertions pass. The failures come from shards not ported yet (`Log`, `Take`), `-I`, and `;` comments in the cases.
5. **Shards for the external host's scripts: done** (see Implemented). Measured locally after this step: `shards2 check` on those scripts reports only the host's own shards and two `And` uses (each with the `All(...)` rewrite). The 1.x check harness passes 20 of 26 assertions (it was 12). Next: the host ports its own shards to 2.0 as an external crate, guided by [embedding.md](embedding.md).

Acceptance, tracked as a count of scripts that pass on both native backends:

- the 1.x `check` eval cases (`../shards/shards/tests/check/cases`) for diagnostics: phase, kind, location, actual/expected types and repair aids;
- a named first batch of small core 1.x test scripts (`../shards/shards/tests/*.shs`), chosen when the grammar runs;
- the external host's scripts composing (`check`) and running on 2.0, beside its 1.x engine, until its own tests and a live run pass.

Existing runtime, metadata, docs-off and wasm checks stay green throughout.

## Deferred work

Variable declaration/write metadata; cache normalization; recursive types, objects and bytes; broader module ports; browser fetch/event-loop integration; binary extensions; graphics and physics. The graphics/physics 1.x baseline is needed before those ports, not before frontend work. Runtime tracing and deep-resume optimization remain useful follow-ups, not prerequisites to maintaining both schedulers.

The [AI roadmap](ai-first-roadmap.md) is a reference copy of the 1.x strategy. Its older proposals must be read alongside the settled 2.0 decisions above.
