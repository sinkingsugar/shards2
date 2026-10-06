# Embedding Shards 2.0 and Writing Host Shards

How a Rust program (the host) adds its own shards and runs Shards scripts on them. The complete example, a host crate with a leaf shard, a blocking shard, a catalog and a script, is [`crates/shards-cli/tests/embedding.rs`](../crates/shards-cli/tests/embedding.rs). It is a test, so it stays correct; copy from it.

## 1. Crates

- `shards-core`: values, types, the shard traits, compose, and both schedulers.
- `shards-lang`: source to wire definitions, `check` and `run`.
- `shards-io` (native only): the shared Tokio runtime, `spawn` for async work and `spawn_blocking` for blocking work.

Depend on them by path or git. Host shards live in the host's own crate, and nothing in this repository needs to know about them.

## 2. A host shard

Every shard has one static description (`ShardDesc`). Its name, version, help, parameters, input and output come from there, and the decoder, catalog and documentation all read it. Prefix host shard names with a namespace (`Host.Reading`) so they cannot collide with core names; `Catalog::new` rejects duplicates, aliases included.

Declare `effects: Effects` and `lifetime: Lifetime` from `shards_core::signature` in the description. Effects describe potential suspension, I/O, time, randomness and unknown behavior; use `Effects::NONE` only for operations with none of these. `ShardDesc::undocumented` defaults to unknown effects and lifetime. This is a trusted host contract. Stateless describes semantic lifetime: temporary state retained during a suspended operation does not make a shard stateful; state remembered across invocations does.

Compose records mesh reads through `ComposeCtx::read_var` (also used by `Operand`) and writes through `mark_initialized`. A read-modify-write must use both. `var` is only a binding lookup and records no read. Use the compose helpers for child flows and wires so their effects and mesh access are included. `CompiledWire::signature()` exposes exact composed types and inferred mesh access; `ShardDesc::signature()` exposes native metadata through the same owned-or-borrowed view.

Choose the trait by what the shard does:

| The shard | Implement | Notes |
|---|---|---|
| Returns at once (reads, computes, converts) | `LeafShard` | One implementation for both schedulers. "Leaf" means it never suspends, not that it does little work. |
| Does slow or blocking work (a long scan, a blocking library call, human-paced input) | `AsyncShard`, with `shards_io::runtime::spawn_blocking` | The work runs on the blocking pool. Only the waiting instance suspends; other wires keep running. |
| Waits on async I/O | `AsyncShard`, with `shards_io::runtime::spawn` | Race every await against the cancellation token. |

Host shards should not need the backend-specific control-flow traits.

### Parameters

Declare parameters with `ParamDecl::new(name, help, forms, types, requirement)`: accepted forms (`Forms::LITERAL`, `VARIABLE`, `FLOW`, ...), types (`TypeName`s) and requirement (`Required`, `Optional`, `Default(...)`, `Variadic`). When a `TypeName` list cannot say the type (`[Int]`, `Float4 | None`, a table with given keys), declare the full type instead: `ParamDecl::new_typed(name, help, forms, requirement, || Type::seq(Type::int()))`. It needs no `TypeName` list; the catalog derives the type code. The decoder enforces forms, requirements and literal types before compose runs.

For a value that can be a literal or a variable (a `Target: pid` the script sets at runtime), use `Operand::compose_arg` in compose. It resolves the variable, reports unknown or possibly-uninitialized ones, and checks the variable's type against the declaration (the full type, or the type list), all as located errors. Then call `Operand::get(ctx)` at activation. For a parameter declared `Requirement::Optional`, use `Operand::compose_optional_arg`, which returns `None` when the script did not give it; `compose_arg` is for required and defaulted parameters.

### Compose

Compose runs once per wire shape, and its result is shared by every instance. It must be a pure function of the arguments, the input type and what it reads through `ComposeCtx`:

- **Never touch the host in compose** (process memory, files, devices, the clock). Do that at activation.
- **Return precise output types.** A fixed record is `Type::fixed_table([...])`, so `r.key` composes to that field's type and a typo is a compose error with suggestions. A value that may be unknown is `Type::union([T, Type::none()])`; at runtime it is an explicit `none`, never a guess. Every key of a fixed table is present at runtime.
- **Input types are enforced from the description.** `InputDesc::Types(&[...])` is checked before your compose runs, with a located `input-type-mismatch`; do not repeat it. When a type list cannot say the input (a record with given keys, `[Int]`), declare the full type with `InputDesc::Typed(record_type)`, a `fn() -> Type`, and a record missing a key fails `check` instead of failing at runtime. `InputDesc::Any` leaves the input to your compose. A shard that produces a value without using its input declares `InputDesc::Ignored` and accepts any input type, so it can start a statement anywhere.
- **Produced values are checked against the compose output type** in debug builds (and in release with shards-core's `output-checks` feature): a leaf or async shard whose value drifts from the type it declared, for example a missing table key, fails the instance with a message naming the shard.
- **Report problems as structured diagnostics** (`Error::Diagnostic(Diagnostic::new(Phase::Compose, kind, code, message).shard(..).param(..))`). The frontend adds file, line, column and the occurrence path.

### Activation

- **Leaf shards** return `Flow::Next(value)`. Runtime failures are `Error::Activation(message)`. `Maybe` can catch them; otherwise they end the instance.
- **Async shards** start one operation per activation from owned inputs: no borrows of frames or of `ctx`. The adapter polls it on later ticks.
- **Blocking work cannot be interrupted from outside.** `spawn_blocking` passes a cancellation token: check it at safe points or wire it to the library's own cancel hook. Cancelling the instance cancels the token. Never leave external state half-changed when stopping early.
- **Read and build values through the accessors**, not the storage: `Var::as_str`, `Var::as_seq` (a `&[Var]`) and `Var::as_table` give borrowed views; a `Table` has `get`, `len`, `contains_key` and `iter`/`keys`/`values` in sorted key order. Build tables with `Table::builder().with("key", value).build()`, `collect()` from `(key, Var)` pairs, or `Var::table([...])`; derive a changed table with `table.into_builder()`, which takes the storage when it is not shared. The table storage is private and will change (struct tables, golden-path.md §7.3), so nothing outside `shards-core` can depend on it.
- **Keep per-instance state in `State`**, not in `Compiled`. Session-wide resources (an open process handle, a connection) belong to the host's own services, looked up at activation by a key the script passes.

## 3. The catalog and scripts

Build one catalog from the core list and the host's list:

```rust
Catalog::new(&[shards_core::shards::CATALOG, HOST_CATALOG])
```

Add `shards_io::CATALOG` if scripts use `Http.Get`. Then:

- **Check without running:** `shards_lang::check::<shards_core::Mesh>(Source::new(path, text), &catalog, &defines)`. It returns the 1.x `{ok, file, diagnostics}` JSON envelope (`to_json()`), and `shards_lang::render` prints a diagnostic for humans.
- **Run:** `Program::load(source, &catalog, &defines)`, then `program.run::<shards_core::Mesh>()`. The default is the stackless scheduler; `StackfulMesh` is the other one. The report has each entry wire's outcome, plus failures of spawned instances.
- **Script arguments:** `defines` maps `name` to a string, read as `@name` in scripts.
- **Logging:** `Log` writes to standard output; values print as text: whole floats without `.0`, other floats exact (1.x rounded to six digits). A host shard can log with `shards_core::log::emit`. `shards_core::log::capture` collects the lines a run logs, including lines logged by work it started through `shards_io` on other threads, which is useful in host tests.

The language is the 1.x syntax with the changes in [surface-syntax-review.md](surface-syntax-review.md) and the deviations listed in [current-state.md](current-state.md). The ones scripts hit most:

- `And`/`Or` become `All(...)`/`Any(...)`.
- `;` comments are rejected.
- Int and Float mix in arithmetic and comparisons.
- `f"..."` strings and `t.key` / `s.0` paths are supported.
- `Maybe` without `else` passes its input through.
- Code goes in wires on a mesh run by `@run(mesh fps: n)`; loose code runs as the `root` wire when there is no `@run`.

## 4. Testing a host

Test host shards through scripts, on both schedulers, as `embedding.rs` does: `Program::load`, then `run::<Mesh>()` and `run::<StackfulMesh>()`, with `log::capture` for output. Use `check` for the compose errors your shards report. Run blocking shards against fakes of the host where possible; keep live runs for what only the real host can show.

## 5. Warm sessions and hot reload

Use `shards_lang::Session` when the host owns its event loop and long-lived
services. Keep connections and other expensive resources in the host and
look them up during shard activation. Both native schedulers support the
same API; stackless also supports WASI and ESP-IDF.

There are two replacement modes:

- **`reload_preserving`** keeps unchanged callers, locals, `Once` state,
  mesh values and suspended execution. Edited nested `Do` bodies are selected
  at their next call boundary. This is what `shards2 watch` uses.
- **`reload`** explicitly restarts the whole program on a fresh mesh. All
  script and mesh state, including host-declared mesh-variable schemas, is
  discarded. Host services owned outside the mesh survive either mode.

Both modes parse, lower and compose the candidate before committing. A
rejected edit returns its source and located diagnostics; the current
execution continues unchanged. Compilation is synchronous and pauses the
host loop on that thread.

### Preserving a caller while editing a nested wire

```shards
@wire(step { 10 | Log })
@wire(main {
  Keep(counter 0)
  Inc(counter) | Log
  Do(step)
} looped: true)
@mesh(m)
@schedule(m main)
@run(m fps: 30)
```

Changing `10` to `20` in `step` keeps `main` running: its counter continues,
its `Keep` state is retained, and its next `Do(step)` uses the edited body.
This works through multiple nested `Do` calls, including a child called
repeatedly inside a parent that never returns.

An in-flight call pins the body it started with until it returns. A pending
async operation is neither cancelled nor overlapped with its replacement by
an accepted nested edit. If the edited call never returns, that call never
adopts the edit; request a full restart to cancel it. Each nested call has
its own boundary. An unchanged parent body can select a new child on its
next child call; an edited parent body already in flight retains its old
call sites, so rearranged descendants cannot be mistaken for old ones.

Replacement must preserve a retained call site's input/output types, local
slot layout, and definite-initialization contract. Type or binding changes,
including new locals inside the changed callee, reject the edit with
`reload-incompatible`. This is checked before any running instance changes.
Diagnostics name the specific incompatible change. Added or changed locals point
to their declaration, including declarations inside nested flows; removed locals
fall back to the affected wire because their declaration is absent. The message
also explains that `r` in watch performs a full restart.
There is no arbitrary local-state migration or in-place coroutine rewriting.
Use a full restart for incompatible edits. Changing the caller's own body
restarts that caller and its locals, so stable initialization and counters
should live in an unchanged caller above the editable `Do` body.

Allowing appended locals in an edited callee is a follow-up. `Do` currently
shares its caller's frame: a new callee slot can shift slots used later by the
caller, so merely accepting a larger layout is insufficient. Supporting this
needs stable existing bindings, safe growth of retained instance frames and
initialization rules while old calls remain in flight.

Other lifetime rules:

- Unrelated wires keep their execution, including pauses and pending I/O.
  Changed scheduled roots restart; removed schedules are cancelled.
  Retained instances keep scheduler order; new/restarted entries append.
  Reordering `@schedule` alone does not reorder retained instances.
- Unchanged nested `Do` bodies retain their shard state too. Their `Once`
  blocks do not repeat merely because a different body was edited.
- `Spawn` remains a static compiled dependency. Editing its target can
  restart the code that owns that dependency. Spawned instances are detached:
  compatible ones survive independently, while changed/removed ones are
  cancelled; new spawns run the new code. The candidate also composes retained
  children's input specializations, even if new entries no longer spawn those
  types. Pending old calls can still spawn work belonging to their pinned code.
- Unchanged completed/stopped entries remain finished, so saving a different
  wire does not repeat setup effects. Failed entries stay stopped until the
  next accepted reload, when they retry. Spawned failures are reported but
  are not independently retried by the session.
- Every replaced state gets cleanup attempted once. At a nested boundary,
  cleanup and instantiation run when the next call starts. A failure there
  is a runtime failure (`Maybe` can catch instantiation errors); it does not
  roll back the committed revision. Terminal cleanup still visits the other
  states if cleanup panics on native builds.

### Driving a session and retaining explicit mesh values

```rust
use std::collections::HashMap;
use shards_core::{Catalog, Mesh, Var};
use shards_lang::{Session, Source};

let catalog = Catalog::new(&[shards_core::shards::CATALOG]).unwrap();
let defines = HashMap::new();
let mut mesh = Mesh::new();
mesh.declare_var("shared-counter", Var::Int(0), true);
let mut session = Session::with_mesh(mesh);
match session.reload_preserving(
    Source::new("live.shs", "Inc(shared-counter)"), &catalog, &defines,
) {
  Ok(finished) => { /* consume cancelled/replaced instances' outcomes */ }
  Err((source, diagnostics)) => { /* render against the rejected source */ }
}
// In the host loop, once per frame; tick does not sleep.
for finished in session.tick() {
  println!("{}: {:?}", finished.wire, finished.outcome);
}
// Submit edits through reload_preserving between ticks. On shutdown:
let finished = session.stop();
```

`Session::with_mesh` accepts an idle, host-configured mesh. Preserving reload
keeps its mesh-variable frame and schema unchanged, so values survive even
when a scheduled root restarts. Scripts access declared mesh variables with
the usual reads and updates. Creating/removing/changing the type of a mesh
variable during preserving reload is not supported. To reset such a session,
create a fresh configured mesh and session; `reload` alone starts from the
default empty mesh schema.

`tick` returns every newly finished entry and child outcome once and retires
the records. A session keeps no outcome history, only bounded entry status.
At `iterations`, remaining work is cancelled and reported in that tick.
An accepted reload resets the revision's tick budget and updates its `fps`;
`frame_interval()` exposes the suggested delay, with pacing owned by the host.
`stop` cancels everything and releases the mesh. Dropping the session also
cancels work, but cannot return cleanup errors.

`ReloadHost` extends `SessionHost` for custom hosts that support isolated
candidate compilation and validated installation. Compiled artifacts remain
immutable. Inline revision selection belongs to a mesh; each active `Do`
state retains an `Arc` to its selected immutable body. Candidate caches are
fresh per revision. Old code remains only while referenced by retained roots
or call state, not in a cumulative revision history. The process-wide type
registry still interns types for the process lifetime.

Cancellation on root replacement, stop, or full restart drops pending
futures, but does not join detached blocking workers or undo external
effects. Such operations must cooperate with cancellation; a host requiring
strict worker quiescence must enforce it in its service before admitting new
work. Preserving an in-flight `Do` avoids restarting that call, but does not
solve every host-side cancellation race.

Offline tests in `crates/shards-cli/tests/embedding.rs` verify warm service
reuse, cancellation on full reload, and pending-operation completion across
preserving reload on both schedulers. The shared frontend suite verifies
caller counters, unchanged `Once` state, deep call boundaries, mesh values,
incompatible edits, cleanup failures, and retained spawned specializations.
Hosts can use the same pattern with recorded inputs to test without live I/O.

### Watching a file

```sh
cargo run -p shards-cli -- watch live.shs
cargo run -p shards-cli -- watch --stackful live.shs key:value
```

The CLI polls contents every 100 ms and requires two identical samples before
trying a revision. Atomic saves and same-size edits work. Invalid source,
incompatible changes or read errors preserve the current execution and are
reported once until observed contents/error change. A watcher stays alive
after completion, the iteration limit or runtime failure; failed wires are
explicitly reported as stopped until the next successful reload/restart.
`fps` controls ticking; without it the watcher uses a 16 ms interval and
continues checking edits even when the script requests a very low frame rate.

Enter `r` to validate the current file and explicitly restart all script
execution in the same process. Enter `q`, send Ctrl-C, or close stdin to
cancel the execution and exit cleanly. Successful shutdown does not mean
every revision ran successfully.

Embedding hosts can reuse the same watcher through `shards_lang::FileWatcher`.
It owns content comparison, the two-sample save check, rejected-revision
suppression, preserving reload and tick pacing. It installs no keyboard,
signal, logging or async-runtime handlers. For a blocking host loop:

```rust
use shards_core::{Catalog, Mesh};
use shards_lang::{FileWatcher, Session, WatchControl, WatchEvent};
use std::collections::HashMap;
use std::sync::mpsc;

let catalog = Catalog::new(&[shards_core::shards::CATALOG]).unwrap();
let mut session = Session::<Mesh>::new(); // or Session::with_mesh(...)
let (commands, input) = mpsc::channel::<WatchControl>();
// Hand `commands` to your UI/input thread. Send Restart or Stop as needed.
FileWatcher::new("live.shs").run(
    &mut session, &catalog, &HashMap::new(),
    || input.try_recv().unwrap_or(WatchControl::Continue),
    |event| match event {
        WatchEvent::Rejected { source, diagnostics } => {
            for d in diagnostics { eprint!("{}", shards_lang::render(&d, &source)); }
        }
        WatchEvent::ReadError(error) => eprintln!("{error}"),
        WatchEvent::Reloaded { finished, .. }
        | WatchEvent::Tick(finished)
        | WatchEvent::Stopped(finished) => {
            for f in finished { println!("{}: {:?}", f.wire, f.outcome); }
        }
    },
);
```

`Tick` fires after every session tick, even with no outcomes. `Reloaded` fires
after successful installation, before that revision's first tick. `Rejected`
carries the rejected source for diagnostics; `ReadError` leaves the session
running. `Restart` validates even unchanged/rejected file contents immediately,
bypassing save stability. `Stop` cancels the session and emits `Stopped`.
For a host that already owns an event loop, call `FileWatcher::poll` instead:
it performs due reads/reloads/ticks without sleeping and returns the suggested
delay. That host calls `Session::stop` on shutdown. Both APIs run file reads,
compilation and callbacks synchronously on the driving thread.

This watches one source file. Dependency watching, asynchronous compilation,
general state migration, cross-revision cache reuse and a network serving
protocol remain deferred. An embedding host can trigger either reload mode
from its own watcher or command channel.
