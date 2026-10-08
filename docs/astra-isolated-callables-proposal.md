# Astra proposal: isolated callables and direct-resume execution

> **Superseded by [golden-path.md](golden-path.md)** (2026-10-06). Kept as background for the reasoning; where they differ, the golden path wins.

Date: 2026-10-06
Author: GPT-6 Astra (Pure)
Status: independent proposal for review with Fable, then synthesis by Opus. NOT the final golden path and NOT an implementation claim.

## 0. Mandate and evidence

Giovanni authorizes breaking compatibility with previous Shards. The goals are explicit scope, typed runtime parameters, predictable state lifetimes, better agent-facing diagnostics, fixed-record storage, and an eventual stackless-only runtime without ancestor replay on resume.

This proposal is deliberately complete enough to implement after approval. Where the conversation disagreed, it selects a design and labels the choice. Do not merge alternatives opportunistically while implementing. Do not edit the running implementation on the authority of this draft before the final synthesis.

Source inspected: `compose.rs`, `var.rs`, `types.rs`, `flow.rs`, `describe.rs`, `shards/leaf.rs`, `stackless/mod.rs`, `stackless/shards.rs`, `reload.rs`; README, current-state, core design/contract, embedding, syntax review and runtime performance overview. The present stackless executor resumes from the root and recursively re-enters parents; see `stackless/mod.rs:293-326` and the nested `activate` calls in `stackless/shards.rs`. `Do` shares caller locals (`compose.rs:439-461`). The current `Var::Table` exposes an Arc-wrapped BTreeMap. These observations ground the proposal, not a fresh full code review.

Tool limitations: no shell execution was available. Git status/log, Cargo tests, Miri, benchmarks and CI were not run. A readable clone reflog mentions `9e2c2b7`, but that is not verification of current HEAD or a clean worktree. Existing repository measurements remain attributed to their recorded snapshots. All new syntax and tests below are proposed, not currently executable.

### Decision summary

1. Script-defined callables use ordinary shard-call syntax and typed named runtime parameters.
2. `@fn` is invocation-local by default. `Stateful: true` explicitly selects per-call-site persistent instances. This is Astra's proposed compromise, not unanimous prior agreement.
3. `@wire` is a scheduled/spawned process, not an implicitly callable shared-scope block.
4. All named bodies have isolated local scope. Anonymous control blocks are lexical. Delete `Do`; do not add `Inline` or a macro system in this change.
5. Separate mutable declaration from update. No implicit sequence declaration/clearing in `Push`.
6. Purity is separate from isolation and state lifetime. Unknown host effects are never assumed pure.
7. Independently addressed frames and immutable code revisions are required. No pointers into another invocation's locals survive suspension.
8. Replace recursive nested activation with an iterative continuation engine. Keep stackful as a temporary semantic oracle, then remove it after explicit gates.
9. Fixed records, scoped metadata lifetime and language authoring evaluation are distinct workstreams, not prerequisites for named arguments.

## 1. Signature

Every native or script operation exposes a common tooling view:

- definition identity and implementation/revision identity;
- pipeline input type;
- ordered named parameters: type, required/defaulted, runtime evaluation form;
- pipeline output type;
- invocation-local or persistent-instance lifetime;
- mesh reads and writes, with types;
- effects: may suspend, may fail, external I/O, nondeterminism, observable persistent state, unknown;
- source and documentation when available.

The signature is not a promise of termination, bounded allocations, or automatic serialization.

### Declaration syntax selected for this draft

```shards
@fn(Scale
  Input: Float
  Params: {Factor: Float}
  Output: Float
  {
    Math.Multiply(Param(Factor))
  })

3.0 | Scale(Factor: 2.0) | Log
```

Output: `6` under current human-text float formatting.

`Input`, `Params`, and `Output` are required for `@fn` in the first slice. Use `Params: {}` for no parameters. This avoids defining polymorphic inference and generic type syntax at the same time as call semantics. Compose verifies the declared types. Effects are inferred and displayed; optional declared bounds reject a body exceeding them. Supporting omitted input/output annotations later must not change lifetime or scope semantics.

`Param(Factor)` reads a typed immutable parameter. This is an intentional syntax choice: public parameter labels remain uppercase like native shard parameters, while ordinary local names remain lowercase. It avoids making a bare uppercase parameter look like a shard call. A local alias is explicit: `Param(Factor) = factor`. The final synthesis may choose a different parameter-binding spelling, but must resolve this ambiguity explicitly.

Calls are `value | Scale(Factor: gain)`, not `Call(scale ...)`. Calls with no arguments may use the existing no-parentheses shard form. User/native names and aliases share collision checking. Reject ambiguous duplicate names; never silently override native operations.

Do not force owned script definitions into today's fully static `ShardDesc` using process-lifetime leaks. Introduce a common descriptor view over static native descriptions and owned script descriptions. Existing native decoding rules should be reused, with a script-call lowering node responsible for invocation.

### Native trust boundary

Native implementations declare conservative effect/lifetime contracts. Metadata is trusted host code, not a sandbox proof. An undeclared native effect is `unknown`. Script summaries are the transitive union of body and argument-expression effects. Recursion remains rejected, allowing dependency-order checking rather than a recursive effect fixed point in this milestone.

Native allocation buffers are not automatically observable persistent state. A native node must explicitly promise whether reuse can affect future script-visible outputs. Without that contract, treat reuse as potentially stateful.

## 2. Scope and argument rules

Named callables see only:

1. their pipeline input and immutable `input` binding;
2. declared parameters through `Param`;
3. their own lexical locals and, when allowed, persistent slots;
4. explicitly declared mesh access through mesh operations;
5. registered native operations, subject to host admission policy.

There is no lookup into caller locals and no missing-local fallback into the mesh. Reserve `input`; redeclaration is an error. Reading `input` always gives the invocation-entry value, not the changing pipeline accumulator.

Anonymous bodies of `If`, `When`, `Repeat`, and similar controls inherit lexical scope. A declaration inside a block is block-local; `Update` can reach a mutable enclosing local. Names cannot shadow another visible local, parameter alias, or persistent slot in this first version. Sibling disjoint blocks may reuse a name. A block-local declaration initializes on each fresh block entry, not on resume. No declaration escapes through a conditional or loop body.

### Argument evaluation

- First resolve and validate supplied labels against the parameter schema.
- Evaluate supplied argument expressions once in source order, left to right.
- Each expression receives the original call's pipeline input. Outputs of previous argument expressions do not become later arguments' pipeline inputs.
- Expressions use the caller's lexical scope; their own temporary block declarations do not escape.
- Successful argument values are owned snapshots stored until invocation completion. No borrowed frame references are stored.
- Argument expressions may suspend. Preserve their cursor and already computed values; do not re-evaluate them on wakeup.
- Do not enter or instantiate the callee until all arguments succeed. Earlier argument side effects are not rolled back if a later argument fails.
- First-slice defaults, if provided, are typed literals only. They do not run hidden computations.
- Callee parameters remain immutable throughout suspension. A mesh change does not change a previously bound argument.
- No `Needs` captures and no automatic `Writes` copy-back. Return values and explicit caller `Update` perform value transfer.

Runtime argument values do not enter compose-cache keys. Types and declared compile dependencies do. Different inputs such as Factor 2 and Factor 3 reuse the same compiled Scale body.

### Shared mesh values

Proposed explicit operations: `Mesh.Get("name")`, `Mesh.Set("name")`. Named definitions declare `Uses: {name: Type}` and/or `Mutates: {name: Type}`. `Mutates` grants write permission only; a read-modify-write requires both. Calls compose against the host mesh schema and transitive effect requirements. A function cannot obtain undeclared access merely by calling another function that accesses the mesh.

Mesh bindings in reusable code are callee-relative requirement indices. Link each instance to host slots after checking types and permissions. Do not cache caller-specific absolute mesh offsets in a supposedly portable function body.

## 3. State and lifetime

### Ordinary function

`@fn` defaults to `Stateful: false`:

- Every invocation starts with uninitialized locals, then binds input/parameters.
- Locals and native activation state survive suspension only for that invocation.
- They are cleaned up and released on return, failure, stop, restart, or cancellation as applicable.
- `Keep`, persistent `Once`, and calls to stateful components are rejected.
- I/O and suspension are allowed unless a stronger effect contract forbids them.
- Memory storage can be pooled, but pooling must not preserve semantic state. Initially instantiate/cleanup native state per invocation; optimize only with an explicit reset/reuse contract.

### Explicitly stateful callable

```shards
@fn(Counter
  Input: None
  Params: {}
  Output: Int
  Stateful: true
  {
    Keep(n 0)
    n | Math.Add(1) | Update(n)
  })
```

Each static occurrence of Counter inside a persistent owner has one distinct component instance. Calls share immutable compiled code, never persistent slots. Repeated executions of the same occurrence reuse that instance. Separate root instances get separate component trees.

Ordinary locals are still invocation-local even in a stateful callable. Only `Keep` slots and explicitly persistent native/child component state survive invocation completion. The signature makes the distinction visible.

Stateful calls are allowed in wires and other stateful callables, not ordinary functions. Recursion and simultaneous re-entry of one component instance are rejected. Shared instances addressed by several callers are deferred; do not simulate them with implicit mesh-variable conventions.

A stateful invocation failing does not roll back already committed Keep/mesh writes. Its transient activation state must reset before another invocation. If a native component cannot recover after failure, it marks its instance poisoned and subsequent entry fails until explicit reset. Do not silently reconstruct it and replay initialization effects.

### Keep

`Keep(n 0)` declares mutable persistent storage with a literal initializer. It is only allowed at the top lexical level of a wire or stateful callable, outside loops and branches. Initializers run as instance initialization, not as ordinary pipeline computations; the source occurrence passes its pipeline input through. Literal-only initialization avoids unresolved questions about repeated runtime arguments, suspension, initialization failure and captured resources. Later computed initialization needs its own specification.

`Keep` storage lasts until instance reset/destruction. Initializer or persistent-layout changes require a reset policy during reload. Do not pretend identical slot types imply compatible state meaning.

No new `Once` idiom is needed for declaring variables. Broader one-time effect initialization can be designed separately; it is not automatically replaced by Keep.

### Wires

`@wire` owns a scheduled/spawned process and persistent component tree. Each root loop iteration has fresh ordinary locals and retains Keep/component state. `Restart` unwinds current invocations and starts a fresh root iteration while retaining the process's persistent state. `Stop` terminates the root process. `Return` exits the nearest named invocation (or finishes the root iteration if at its root). These are explicit continuation signals, not hidden context flags.

Suspension is not an exit and must not trigger logical cleanup. Cleanup remains exactly once per successfully instantiated state lifetime, with all cleanup attempts made even if one panics. Cancellation does not undo external I/O or forcibly join a non-cooperating blocking worker.

## 4. Purity

`Pure: true` is an optional checked contract, independent of `Stateful`:

- no observable persistent state;
- no mesh reads/writes;
- no external I/O;
- no nondeterministic observations;
- no suspension in the first strict pure subset;
- no unknown effects.

Local mutation and deterministic checked failure are allowed. Purity means deterministic results/failures from inputs under declared semantics, not totality. A scope-isolated function calling `Time.Now` is not pure. An unknown native operation cannot pass a Pure check. Calling compose twice is at most a nondeterminism probe, not an enforcement mechanism.

## 5. Syntax cleanup

Selected forms:

| Form | Meaning |
|---|---|
| `value = x` | Declare immutable block-local binding; output remains value |
| `value | Var(x)` | Declare initialized mutable block-local binding; error if already visible |
| `value | Update(x)` | Update an existing mutable binding; error if absent or immutable |
| `value | Push(xs)` | Append to an existing mutable sequence; outputs the original appended input |
| `Keep(x literal)` | Declare initialized persistent mutable storage in an allowed owner |

`Push` neither declares storage nor clears it implicitly. Initialize a per-iteration sequence explicitly with `[] | Var(xs)` inside that iteration. Empty sequences must remain usable: infer a consistent element type from writes within the lexical scope where practical; until that analysis exists accept `[Any]` for an unannotated empty sequence rather than inventing unsound narrowing. Add explicit local type annotations in a follow-up if real scripts need them.

Delete `Do`, `Set`, `Ref` as an alternate spelling of immutable bind, and assignment glyphs `>=`, `>`, `>>`. Keep comparisons as named operations. No compatibility aliases or migration framework are required. Remove/update existing examples and tests in the implementation milestone.

`Match` is exhaustive or declares `Default`. Initially prove finite literal coverage only for Bool and None (and unions of supported finite cases). Int, Float, String and arbitrary predicates require a default. Do not confuse case-output union typing with coverage proof. Unmatched pass-through is possible only through an explicit default body, such as `{input}`. Equality, NaN and duplicate-case rules must use the language comparison semantics, not cache-key bit identity. Diagnose provably duplicate/unreachable literal cases.

All Float2/3/4 components become f32; scalar Float remains f64. Conversion is a numerical semantic change, not just an enum/layout edit. Update parser literals, constructors, arithmetic, comparisons, printing, type metadata, host APIs, cache hashing and tests together. Include representability/rounding edge cases and non-finite values; do not change scalar arithmetic implicitly.

## 6. Lowering, caching and data representation

### Calls

A compiled body contains callee-relative bindings, parameter slot layout, local layout, effect summary, code, and logical-node/source mappings. Each call site contains argument evaluation code and a reference to a compiled specialization, not a cloned body tied to caller slots.

Cache identity includes definition structure/revision, native catalog implementation identity, input/parameter types, and recorded semantic dependencies/policy. Hash collisions require structural equality. Runtime argument values and caller local names/layouts are excluded. This is compose once per compatible specialization/cache scope, not exactly once globally forever.

Use a single unresolved-definition registry so native and source operations can be discovered uniformly. Source spans and call-site diagnostic paths remain outside semantic cache keys. Body-sharing must not make two call sites report the same erroneous location.

### Frames

Separate invocation frame, persistent component state and mesh frame. Independently address each invocation through a stable handle and callee-relative slots. A Vec/arena may back frames, but retain handles, not raw pointers into relocatable storage. Generation-tag reused slots so stale continuation/wake references cannot target a new invocation.

No assumption that a private namespace in a single flattened frame solves reload. The implementation must permit a new callee frame layout without relocating a retained caller's slots.

### Fixed records

Hide table representation behind an owned Table API. Fixed records use a shared sorted shape plus indexed values; open tables retain a dynamic-map path. This is struct-like field indexing, not a promise of C-compatible packed layout or unboxed typed fields.

Constant-key access on a known fixed shape lowers to a slot. Dynamic-key access and sorted iteration remain supported. Preserve snapshots, copy-on-write value semantics, equality and hashing independent of storage kind. The same key/value content must compare and hash identically across fixed and dynamic representations when used as values/cache inputs. No silent identity-based equality.

Unknown keys in fixed records remain compose errors. Runtime host values must be validated against shapes at an appropriate boundary; an unchecked wrong shape cannot turn a slot access into wrong-field access.

Named call parameters lower directly to slots and do not depend on record allocation. Multi-value outputs can use records after this representation work.

### Metadata ownership

Do not add another process-lifetime shape leak on top of the existing type registry. Define program/session or explicit host-owned registry lifetimes, and keep descriptions alive while referenced by retained code/values. Structural compatibility across registry boundaries must not rely on raw numeric IDs. Existing global type interning needs a separate migration plan; until implemented, report it as a remaining retention limit, not solved by fresh compose caches.

## 7. Direct-resume stackless engine

### Feasibility and complexity target

Shards does not require an OS coroutine stack per process. It DOES require information about suspended calls, loops, handlers and owned values. Store that explicitly as VM continuation data.

Target: resuming and re-suspending the same leaf performs O(1) ancestor-dispatch work regardless of parent depth. Entering D calls or completing/unwinding D real continuations still costs O(D). Do not promise constant-time full completion or zero stack-like data structures.

Current nested `FlowState` objects hide child state inside parent `Any` boxes. Merely remembering a leaf index does not solve access: traversing that object tree is still O(depth). The runtime must own directly addressable execution frames/continuations.

### Proposed execution protocol

Use a central iterative trampoline with runtime-owned frame and continuation stores. Conceptual actions:

- Advance(output): advance the current sequence/program counter.
- Enter(child, input, bindings, return_continuation): allocate/select a child frame and transfer control.
- Wait(wait_registration): park the process with its active frame handle.
- Complete(value): deliver to the saved parent continuation.
- Signal(Return/Stop/Restart/Error): unwind to the specified boundary/handler.

These names are internal design vocabulary, not required public Rust identifiers.

Control shards request child execution; they never recursively call child `activate`. A continuation records the parent frame, next phase/program counter, required original input or partial result, loop state, handler boundaries, and immutable code owner. On child completion, dispatch the parent continuation exactly once. On Wait, park without touching ancestors.

Control examples:

- If enters its predicate; its continuation selects one branch once. Waking inside the branch does not re-run the predicate.
- Repeat stores limit/counter and original input; child completion advances the loop phase. Waking inside its body does not increment or re-evaluate the loop guard.
- Maybe installs a handler boundary. A child error unwinds abandoned transient frames, then enters Else once. It does not catch cancellation as an ordinary script error.
- A named call binds a new private invocation frame and captures the selected immutable body revision until return.

Leaf/async authoring APIs remain conceptually one implementation. Async work stores an owned future/operation in addressable state. Wakers mark a process ready; the process resumes its stored active frame. Late wakes after cancellation/completion are harmless through instance/generation validation. Preserve both polling and notification modes initially. Do not scan ancestors to locate the future.

Reused control frames must reset transient state centrally on non-Suspend exits. Cleanup of owned child state must be centralized too: changing execution to iterative form while retaining recursively nested ownership teardown would leave a separate deep-stack hazard. Keep current depth limits until composition, instantiation and cleanup have independently been checked; direct resume alone does not justify lifting them.

### Ownership and scheduling

Frames own live input snapshots and partial arguments. Release dead values when no continuation needs them, not only at process completion. Keep a regression for generic passthrough retention as well as builtin retention. Persistent component state lives outside transient continuation entries.

A process can execute synchronously until completion, voluntary suspension, or an instruction budget safe point. Initially preserve existing pacing; add bounded safe-point budgeting with parity tests if needed. A budget cannot preempt a blocking native leaf: host code must still obey the nonblocking contract.

Builtin instruction segments retain their current safety rule: borrow frames only within the segment; no callbacks, suspension, frame relocation or revision replacement during the borrow. Export owned values at generic/suspension boundaries. Adding a direct-resume engine is not permission to retain raw frame pointers across calls.

### Removing stackful

The desired final runtime is stackless only. Deletion is the LAST scheduler milestone, not the first patch.

Before deleting:

1. Run old stackless, stackful and direct-resume engines against applicable unchanged semantics, checking outputs, scheduling traces, cancellation and cleanup. For deliberately new semantics, use explicit expected traces rather than old behavior as authority.
2. Add a structural counter test: depth 1/4/16/32 pending-leaf re-polls execute zero ancestor handlers. This is the primary evidence that the algorithm changed.
3. Benchmark pending re-poll separately from full leaf completion/unwind. Also benchmark short uninterrupted mixed flows, many instances, allocations, resident/live heap, and creation time.
4. Proposed performance gate for approval: direct-resume depth-32 pending-leaf median <=1.25x stackful on the same workload; representative short-flow median regression <=10% against the current stackless baseline. Retain repeated alternating samples and disclose noise. If a gate fails, explain it and seek approval rather than deleting tests or changing workloads to pass.
5. Run native debug/release, docs-off, WASI, supported ESP-IDF CI and targeted Miri coverage. All lifecycle tests must pass.
6. Then remove stackful dependencies, exports, backend-generic plumbing that is truly redundant, CLI switches, and parity-only scaffolding. Retain semantic regression tests and archived benchmark evidence.

No state serialization, cross-machine migration or reverse execution is promised merely because continuations are explicit. Native Any state, futures, devices and external effects need separate contracts.

## 8. Reload

Compile/validate candidates without mutating running instances. Admission includes input/output/parameter compatibility and host effect permissions. Increased effects require re-admission; an unchanged type signature is not sufficient.

An invocation pins its entire selected body and descendant binding graph. It does not switch nested call definitions halfway through that invocation. Newly started invocations select the accepted revision at their boundary. This deliberately chooses simpler whole-invocation consistency over the current ability for an unchanged long-running parent to adopt child edits internally; list it as a new semantic decision.

Transient functions can add/remove/reorder private locals freely for NEW invocations if their public contract is compatible. In-flight invocations keep old code and old layouts. The caller does not restart.

For persistent components, first implementation policy:

- unchanged definition/state identity retains the instance;
- changed component definitions reject preserving reload by default;
- an explicit host reload option permits reset of changed components at their next inactive boundary;
- the reset cleans up old persistent state exactly once, instantiates new state, and does not reset the caller;
- an active old invocation finishes first; a never-returning invocation needs cancellation/restart to adopt the revision;
- no arbitrary Keep/native-state migration or automatic matching by slot index.

Rejecting a candidate preserves running code and state. Failure during post-commit initialization is a runtime failure, not rollback of external effects or the accepted program revision.

Use definition identity and owner/call-site identity separately. Never map reordered occurrences in an edited body onto old persistent instances by index alone. Keeping old graphs pinned until completion provides a conservative baseline.

## 9. Acceptance suite

These are target-language fixtures, NOT tests run in this session. During implementation, commit them under a new frontend suite directory, plus Rust host tests for lifecycle/internals. Diagnostic assertions pin phase, code, source span, symbol and expected/actual type; snapshot human text for readability but do not make every punctuation choice the semantic ABI.

### A. Runtime parameters and body sharing

```shards
@fn(Scale Input: Float Params: {Factor: Float} Output: Float {
  Math.Multiply(Param(Factor))
})
3.0 | Scale(Factor: 2.0) | Log
3.0 | Scale(Factor: 4.0) | Log
```

Expected logs: `6`, `12`. Instrumentation: one compiled Scale specialization, distinct invocation locals. Changing runtime Factor does not increment compose count.

### B. No caller capture

```shards
@fn(Bad Input: Int Params: {} Output: Int { Math.Add(secret) })
10 = secret
1 | Bad
```

Expected compose diagnostic: `unknown-local`, symbol `secret`, span at the reference inside Bad. Suggested message: `unknown local 'secret' in Bad; pass it through Params`. The caller declaration cannot make this compile.

### C. Fresh ordinary locals at the same occurrence

```shards
@fn(Fresh Input: None Params: {} Output: Int {
  0 | Var(n)
  n | Math.Add(1) | Update(n)
})
Repeat(Times: 3 Action: { none | Fresh | Log })
```

Expected logs: `1`, `1`, `1`. Instrument initialization and cleanup once per invocation, not on each resume.

### D. Explicit persistent instances

```shards
@fn(Counter Input: None Params: {} Output: Int Stateful: true {
  Keep(n 0)
  n | Math.Add(1) | Update(n)
})
Repeat(Times: 3 Action: {
  none | Counter | Log
  none | Counter | Log
})
```

Loose root code is wrapped in a process as today. Expected logs: `1`, `1`, `2`, `2`, `3`, `3`. Separate scheduled root instances restart both counters at 1. Without `Stateful: true`, Keep produces `persistent-state-not-allowed` at Keep.

### E. Misspelled update and duplicate declaration

```shards
0 | Var(counter)
1 | Update(coutner)
```

Expected: `unknown-local` at `coutner`, suggestion `counter`, no new slot created.

```shards
0 | Var(counter)
1 | Var(counter)
```

Expected: `duplicate-binding` at the second declaration, related span at the first. Updating an immutable binding produces `immutable-binding`.

### F. Explicit matching

```shards
true | Match([true {1} false {0}]) | Log
```

Expected log: `1`; coverage proven.

```shards
2 | Match([1 {10}])
```

Expected: `non-exhaustive-match` at Match; message requests Default for Int.

```shards
2 | Match([1 {10}] Default: {input}) | Log
```

Expected log: `2`, pass-through is explicit.

### G. Stable parameters across suspension

```shards
@fn(Delayed Input: Int Params: {Amount: Int} Output: Int {
  Pause(0.0)
  Math.Add(Param(Amount))
})
@wire(main Uses: {gain: Int} {
  3 | Delayed(Amount: {Mesh.Get("gain")}) | Log
})
@mesh(m)
@schedule(m main)
@run(m)
```

Host fixture configures mesh m with mutable Int gain. Set gain to 2 before entry and 100 while suspended. Expected result: `5`, not `103`. Argument evaluation count is one. A separate fixture suspends inside an argument expression and verifies previous arguments are not replayed.

### H. Argument ordering

Register a test-only Host.Mark that appends a label to an event trace and returns its input. Call a two-parameter function with `First: {Host.Mark("first")}` and `Second: {Host.Mark("second")}` on input 7. Expected trace first, second; both parameters equal 7. Suspend in Second and verify First is not repeated. Fail Second and verify the callee never instantiates.

### I. Purity and transitive admission

Pure function calling Time.Now fails `effect-not-allowed`; Pure calling a test native operation with no effect metadata fails `unknown-effect`. An ordinary isolated function may call a declared I/O operation. Mesh access missing Uses/Mutates fails `undeclared-mesh-access`, including when indirect through another function.

### J. Lifecycle and direct resume

Rust fixtures construct nested If/Repeat/Maybe/functions around a controllable async leaf. Verify:

- pending polls invoke zero ancestor handlers at depths 1, 4, 16, 32;
- predicates and side effects run once at entry, not per wake;
- completion executes precisely the necessary parent continuations;
- error reaches the nearest Maybe after abandoned transient cleanup;
- Return targets the nearest callable, Stop the root, Restart a fresh root iteration;
- cancel visits all successfully instantiated state exactly once, even after one cleanup panic where unwinding is supported;
- stale wakes cannot resume a cancelled frame or a recycled instance;
- allocation/owner counts do not grow with repeated suspend/resume;
- one waiting process does not block another ready process.

### K. Reload

Run a caller counter while a stateless callee is suspended. Submit a compatible revision adding private locals and changing its arithmetic. Old call finishes with old result; next call uses new result; caller counter continues and caller bindings do not move. Reject an output/parameter contract change atomically. Changed persistent component rejects by default; explicit reset policy resets only that component at an inactive boundary. Track old code destruction after its last owning invocation/instance goes away.

### L. Records and vectors

Cross-representation record equality/hash; sorted iteration; snapshot isolation; known-key slot access; unknown-key compose error; wrong-shape host rejection. Float2 rounding is explicitly f32 and matches Float3/4 component rules. Repeat record-shape reload edits and report registry growth; do not claim bounded reclamation until implemented and tested.

## 10. Implementation sequence and completion gates

Each stage is a separately reviewable change. Maintain both existing backends until the scheduler-removal gate; updating this stable repository rule is part of the approved final plan, not an incidental edit.

1. **Contracts/tooling:** common native/script descriptor view, conservative effect metadata, checked declaration schema, per-occurrence input/output JSON. No unsafe new purity assumptions. Keep current host APIs working where not deliberately replaced.
2. **Isolated invocation slice:** parser/lowering for @fn, named arguments, private frames, recursion rejection, cache sharing, fresh invocation lifecycle, direct shard-style calls. Land tests A-C, E, G-I on both backends.
3. **Explicit persistent mode and syntax:** Stateful, Keep, lexical lifetime rules, no Do/ambient captures, declaration/update/Push semantics, exhaustive Match. Tests D/F and failure/reset cases. Remove old syntax and update repository fixtures directly.
4. **Reload:** separate interface and frame-layout compatibility, pinned invocation graphs, component reset policy. Test K with controlled pending operations.
5. **Direct-resume engine:** iterative controls, directly addressable frames, centralized unwind/cleanup. Preserve the previous engine as oracle during the experiment; land structural counter tests before performance claims.
6. **Values:** opaque tables, fixed shapes and f32 vectors. Can proceed independently after the relevant API design is reviewed; not a prerequisite for stages 2-5. Add ownership/allocation scaling tests and resolve metadata lifetime explicitly.
7. **Authoring evaluation:** 30-50 fixed tasks across available models, with generated catalog reference and checker. Record semantic correctness via tests, first-pass check success, repair count, tool calls, tokens and latency. Include baseline language/old syntax comparisons when executable. Separate syntax failures from missing library features. Do not treat check success alone as task success or tiny samples as universal evidence.
8. **Stackful retirement:** after section 7 gates, remove stackful and simplify APIs. Re-run the full repository check set appropriate to the new architecture and CI targets.

Refresh `docs/current-state.md`, README, embedding and core contract after each approved milestone. Remove contradictory present-tense historical claims; keep detailed measurements and prior review records in their existing dedicated locations. Re-read checkout status and recent commits before implementation; preserve unrelated work. If dependencies change, refresh the ESP-IDF lockfile as AGENTS.md requires.

## 11. Decisions for final synthesis

Astra recommends these choices; they are not disguised consensus:

- `@fn` plus explicit `Stateful: true`, rather than every function implicitly having persistent lifetime or introducing an explicit shared-instance API now.
- `Param(Factor)` to resolve uppercase label versus lowercase-local syntax cleanly.
- Required boundary types in the first slice, effects inferred conservatively.
- Ordinary functions may suspend/do I/O; Pure is a separate checked subset.
- Whole-invocation revision pinning rather than descendant hot swapping inside old invocations.
- Persistent code edits reject by default and offer explicit callee-only reset, not inferred native-state migration.
- No macro/template feature in this scope.
- Stackless-only as the destination, gated by structural correctness and measured workloads, not a day-one deletion.

The final document should resolve these against Fable's proposal explicitly. Keep one executable contract for Claude Code; do not leave contradictory lifetime rules in parallel specification files.
