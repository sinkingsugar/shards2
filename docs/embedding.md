# Embedding Shards 2.0 and Writing Host Shards

How a Rust program (the host) adds its own shards and runs Shards scripts on them. The complete example, a host crate with a leaf shard, a blocking shard, a catalog and a script, is [`crates/shards-cli/tests/embedding.rs`](../crates/shards-cli/tests/embedding.rs). It is a test, so it stays correct; copy from it.

## 1. Crates

- `shards-core`: values, types, the shard traits, compose, and both schedulers.
- `shards-lang`: source to wire definitions, `check` and `run`.
- `shards-io` (native only): the shared Tokio runtime, `spawn` for async work and `spawn_blocking` for blocking work.

Depend on them by path or git. Host shards live in the host's own crate, and nothing in this repository needs to know about them.

## 2. A host shard

Every shard has one static description (`ShardDesc`). Its name, version, help, parameters, input and output come from there, and the decoder, catalog and documentation all read it. Prefix host shard names with a namespace (`Host.Reading`) so they cannot collide with core names; `Catalog::new` rejects duplicates, aliases included.

Choose the trait by what the shard does:

| The shard | Implement | Notes |
|---|---|---|
| Returns at once (reads, computes, converts) | `LeafShard` | One implementation for both schedulers. "Leaf" means it never suspends, not that it does little work. |
| Does slow or blocking work (a long scan, a blocking library call, human-paced input) | `AsyncShard`, with `shards_io::runtime::spawn_blocking` | The work runs on the blocking pool. Only the waiting instance suspends; other wires keep running. |
| Waits on async I/O | `AsyncShard`, with `shards_io::runtime::spawn` | Race every await against the cancellation token. |

Host shards should not need the backend-specific control-flow traits.

### Parameters

Declare parameters as `ParamDecl`s: name, help, accepted forms (`Forms::LITERAL`, `VARIABLE`, `FLOW`, ...), literal types and requirement (`Required`, `Optional`, `Default(...)`, `Variadic`). The decoder enforces all of that before compose runs.

For a value that can be a literal or a variable (a `Target: pid` the script sets at runtime), use `Operand::compose_arg` in compose. It resolves the variable and reports unknown or possibly-uninitialized ones as located errors. Then call `Operand::get(ctx)` at activation. Check the returned type in compose. For a parameter declared `Requirement::Optional`, use `Operand::compose_optional_arg`, which returns `None` when the script did not give it; `compose_arg` is for required and defaulted parameters.

### Compose

Compose runs once per wire shape, and its result is shared by every instance. It must be a pure function of the arguments, the input type and what it reads through `ComposeCtx`:

- **Never touch the host in compose** (process memory, files, devices, the clock). Do that at activation.
- **Return precise output types.** A fixed record is `Type::fixed_table([...])`, so `r.key` composes to that field's type and a typo is a compose error with suggestions. A value that may be unknown is `Type::union([T, Type::none()])`; at runtime it is an explicit `none`, never a guess. Every key of a fixed table is present at runtime.
- **A shard that produces a value without using its input** declares `InputDesc::Ignored` and accepts any input type, so it can start a statement anywhere.
- **Report problems as structured diagnostics** (`Error::Diagnostic(Diagnostic::new(Phase::Compose, kind, code, message).shard(..).param(..))`). The frontend adds file, line, column and the occurrence path.

### Activation

- **Leaf shards** return `Flow::Next(value)`. Runtime failures are `Error::Activation(message)`. `Maybe` can catch them; otherwise they end the instance.
- **Async shards** start one operation per activation from owned inputs: no borrows of frames or of `ctx`. The adapter polls it on later ticks.
- **Blocking work cannot be interrupted from outside.** `spawn_blocking` passes a cancellation token: check it at safe points or wire it to the library's own cancel hook. Cancelling the instance cancels the token. Never leave external state half-changed when stopping early.
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
- **Logging:** `Log` writes to standard output. `shards_core::log::capture` collects the lines a run logs on the current thread, which is useful in host tests.

The language is the 1.x syntax with the changes in [surface-syntax-review.md](surface-syntax-review.md) and the deviations listed in [current-state.md](current-state.md). The ones scripts hit most:

- `And`/`Or` become `All(...)`/`Any(...)`.
- `;` comments are rejected.
- Int and Float mix in arithmetic and comparisons.
- `f"..."` strings and `t.key` / `s.0` paths are supported.
- `Maybe` without `Else` passes its input through.
- Code goes in wires on a mesh run by `@run(mesh FPS: n)`; loose code runs as the `root` wire when there is no `@run`.

## 4. Testing a host

Test host shards through scripts, on both schedulers, as `embedding.rs` does: `Program::load`, then `run::<Mesh>()` and `run::<StackfulMesh>()`, with `log::capture` for output. Use `check` for the compose errors your shards report. Run blocking shards against fakes of the host where possible; keep live runs for what only the real host can show.

## 5. Warm sessions and hot reload

Use `shards_lang::Session` when the host already owns its event loop and
long-lived services. Keep connections and other expensive session resources
in the host, and have shards look them up during activation. Replacing a
script then reuses those services; setup inside a script's `Once` runs again.

```rust
use std::collections::HashMap;
use shards_core::{Catalog, Mesh};
use shards_lang::{Session, Source};

let catalog = Catalog::new(&[shards_core::shards::CATALOG]).unwrap();
let defines = HashMap::new();
let mut session = Session::<Mesh>::new();
match session.reload(Source::new("live.shs", "40 | Add(2)"), &catalog, &defines) {
  Ok(finished) => { /* consume old instances' cancellation/cleanup outcomes */ }
  Err((source, diagnostics)) => { /* render diagnostics against rejected source */ }
}
// In the host loop, call once per frame; this does not sleep.
for finished in session.tick() {
  println!("{}: {:?}", finished.wire, finished.outcome);
}
// Submit new source through reload between ticks. On shutdown:
let finished = session.stop();
```

`Session::<StackfulMesh>` has the same API on native platforms. `SessionHost`
extends the frontend's `Host` trait with cancellation of all instances;
custom implementations must obey its deferred-activation and cleanup contract.

The replacement boundary is the **whole program**:

- Parse, lower and compose all wires, including unreachable ones as `check`
  does, on a candidate mesh. Rejected edits return located diagnostics and
  leave the current execution and its tick count untouched.
- Schedule the candidate entries without instantiating or activating shards.
  After successful preparation, cancel every old entry and spawned child,
  attempt every cleanup, release the old mesh, then install the candidate.
  The new revision starts only on the next host tick.
- Locals, mesh variables, `Once`, and coroutine continuations restart. There
  is no implicit state migration. Changing a called wire recompiles its
  callers; removed definitions cannot linger in the replacement mesh.
- Reload returns the old instances' outcomes, including cleanup failures.
  A cleanup failure does not roll back the replacement. Instantiation and
  activation failures in the new revision are reported by `tick`, without
  restoring an already-cancelled revision.
- `tick` returns all newly finished entry and child outcomes exactly once.
  It drains records each tick and retains no outcome history. At `Iterations`,
  it cancels remaining work and returns those outcomes in the same call.
  The host owns pacing; `frame_interval()` exposes `FPS` as a suggested delay.
- `stop` cancels and releases the revision; dropping the session also cancels
  work but cannot return cleanup errors. A fresh compose cache per revision
  bounds cache retention across edits. The process-wide type registry still
  interns types for the process lifetime.

Reload is synchronous: compilation pauses ticking on that host thread. It
is not a zero-latency or hard real-time operation. Cancellation drops pending
futures before replacement activation, but it does not wait for detached
workers to exit or undo external effects. Host operations must cooperate
with cancellation; a host requiring strict worker quiescence must enforce
that in its service before allowing a new operation.

The offline test `reload_keeps_host_service_warm_and_cancels_pending_operations`
in `crates/shards-cli/tests/embedding.rs` exercises a fake host service on both
schedulers: rejected edits preserve the pending operation; accepted edits
cancel it and reuse the same service; dropping the session releases pending
work. Hosts can use the same pattern with recorded inputs to test scripts
without live I/O.

### Watching a file

```sh
cargo run -p shards-cli -- watch live.shs
cargo run -p shards-cli -- watch --stackful live.shs key:value
```

The CLI polls contents every 100 ms and requires two identical samples before
trying a revision, so atomic saves and same-size edits work. Invalid source
or a read error leaves the previous execution running and is reported once
until the observed contents/error change. A watcher stays alive after a
program finishes, reaches `Iterations`, or fails, ready for the next edit.
`FPS` controls ticking; without it the watcher uses a 16 ms frame interval.
It checks for edits even when the script requests a very low frame rate.
Ctrl-C, `q` followed by Enter, or stdin EOF cancels the current execution and
exits. Watch mode reports runtime failures as they occur; its successful
shutdown exit code is not a claim that every revision succeeded.

This command watches one source file. Dependency watching, asynchronous
compilation, cross-revision compose reuse, explicit state migration and a
network serving protocol are deferred. A host can trigger `Session::reload`
from its own watcher or command channel without using the CLI.
