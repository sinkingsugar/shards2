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
- `Maybe` without `Else` passes its input through.
- Code goes in wires on a mesh run by `@run(mesh FPS: n)`; loose code runs as the `root` wire when there is no `@run`.

## 4. Testing a host

Test host shards through scripts, on both schedulers, as `embedding.rs` does: `Program::load`, then `run::<Mesh>()` and `run::<StackfulMesh>()`, with `log::capture` for output. Use `check` for the compose errors your shards report. Run blocking shards against fakes of the host where possible; keep live runs for what only the real host can show.
