# Values and Types for the Frontend Slice

**Status:** §2-§4 implemented 2026-10-04 (`var.rs`, `types.rs`, `describe.rs`); §5 follows with the frontend. See [current-state.md](current-state.md).
**Scope:** the value and type additions the frontend slice needs before real scripts can lower onto the core: tables, the missing float vectors, and type sets. It extends contract §3 (types) and §9 (values) in [prototype-shard-contract.md](prototype-shard-contract.md); it does not settle the final `Var` layout, which still waits for measurements.

## 1. Why now

Both reviews of the prototype (2026-10-04) named the value and type model as the largest untested part, and the scripts of the first external host confirm it. They depend on:

- **fixed-shape tables** whose `t.key` access composes to the field's type, and which reject an unknown key at compose (host shards return fixed records, and scripts pass table literals to host shards);
- **Float2, Float3 and Float4** values (screen points, positions, poses);
- **"value or none"** fields: an unknown reading is an explicit none, never a guess.

Almost every 1.x test script also uses tables. Lowering source onto a model without them would either stall the frontend or force rushed type decisions, so these land first, as explicit work.

## 2. Values

`Var` gains:

| Variant | Payload | Notes |
|---|---|---|
| `Float2` | `[f64; 2]` | 1.x layout: two 64-bit floats. |
| `Float4` | `[f32; 4]` | 1.x layout: four 32-bit floats (`Float3` stays `[f32; 3]`). |
| `Table` | `Arc<BTreeMap<Arc<str>, Var>>` | String keys only, in key order. |

Decisions:

- **Table keys are strings.** 1.x allows any value as a key; nothing in the scripts we target needs that, and string keys keep tables cheap to hash, compare and print. Non-string keys can be added later as a separate key kind without changing string-keyed code.
- **Key order is sorted.** A `BTreeMap` gives deterministic iteration, printing, equality and hashing, so tables can appear in parameter literals, which are part of the compose cache key. 1.x insertion order is not preserved; scripts that depend on it are out of scope.
- **Copy on write.** Like `Seq`, a table is shared behind an `Arc`; a shard that changes it clones only when shared (`Arc::make_mut`).
- **Identity equality** (bitwise floats) stays the rule for `PartialEq`/`Hash`, as for the other variants. The language's `Is` is a separate comparison.
- Integer vectors, `Color`, `Bytes` and objects remain deferred until a ported script needs them.

## 3. Types

`TypeDesc` gains `Float2`, `Float4`, `Table(TableType)`, `Union(Vec<Type>)` and `Never`.

### 3.1 Tables

```rust
pub struct TableType {
  /// Known keys and their value types, sorted by key.
  pub keys: Vec<(Arc<str>, Type)>,
  /// The value type of any other key; `None` for a fixed table.
  pub rest: Option<Type>,
}
```

One description covers the three shapes in use:

- **Fixed (struct) table:** known keys, `rest: None`. Every key is present; reading a key that is not listed is a compose error. This is the 1.x `fixedStructTable` behavior hosts rely on.
- **Homogeneous table:** no known keys, `rest: Some(T)`. `Table` with `rest: Some(Any)` is "any table".
- **Mixed:** known keys plus a `rest` type.

A key whose value may be missing is typed as a set with `None` (`Float4 | None`), not as an optional key. Every listed key is present at runtime, so an unknown value is an explicit none, never a guess.

A table literal's type (`Var::type_of`) is the fixed table of its keys.

### 3.2 Unions

`Union` (named `Set` until 2026-10-05; renamed so "set" stays free for a possible runtime set value). Unions are canonical at construction (`Type::union`): nested unions are flattened, members sorted by a structural order (`structural_cmp`: stable across runs, unlike registry indices, and unambiguous, unlike printed forms, which can coincide for distinct tables) and deduplicated; a one-member union is that member, a union with `Any` is `Any`, and `Never` members drop out. Equal unions intern to the same handle.

`Never` is the type of something that produces no value: `Stop`, and a flow that reaches it. Every type accepts it, so a branch or predicate ending in `Stop` fits; compose still checks the unreachable shards after it.

A sequence literal with mixed element types now types as `[A | B]` instead of `[Any]`.

### 3.3 Acceptance

`Type::accepts(expected, actual)` is the one subtyping rule, used for parameter literals, variable updates and shard input checks. Exact equality is still used where identity matters (cache keys).

- `Any` accepts everything; `Any` as an *actual* type is accepted only by `Any` (an untyped value must be narrowed, as with 1.x `Expect*`). `Never` as an actual type is accepted by everything.
- Checking a runtime value against a type uses `Type::admits(&Var)`, which interns nothing (`value.type_of()` would intern every new shape in the never-freed registry). `Spawn`, `set_var` and `Expect*` use it.
- Expected set: some member accepts the actual type. Actual set: every member is accepted.
- `[E]` accepts `[A]` when `E` accepts `A` (values are immutable or copy-on-write, so covariance is sound).
- Expected table `X` accepts actual table `Y` when:
  - every key of `X` is in `Y`, with an accepted type (a key only covered by `Y.rest` may be missing at runtime, so it is rejected);
  - every other key of `Y` is accepted by `X.rest`, and fails if `X` is fixed;
  - if `Y` has a `rest`, `X.rest` exists and accepts it.
- Otherwise, the descriptions must be equal.

### 3.4 Static names

`TypeName` (const-friendly names in shard descriptions) gains `Float2`, `Float4`, `Seq` (any sequence) and `Table` (any table). `TypeName::matches` uses `accepts`. Diagnostics report 1.x `SHType` codes: Float2 11, Float4 13, Seq 56, Table 57; a set reports -1, with its full printed form in `name`.

### 3.5 Printing

`Float2`, `Float4`; `[T]` for sequences; `{a: Int b: String}` for a fixed table (keys that are not plain names are quoted: `{"two words": Int}`, also when printing values), `{a: Int ...: Any}` with a rest type, `{...: Any}` for any table; `A | B` for sets, parenthesized inside a sequence element or table value when needed for clarity (`[(Int | None)]`).

## 4. The type registry

The process-wide registry stays (scoped registries are deferred), with two changes from the second review:

- **No copying under the lock.** Descriptions are allocated once and never freed (the registry already kept them for the process lifetime); `Type::desc()` returns `&'static TypeDesc` instead of cloning a description that may now hold a key list.
- **Readers do not serialize.** The registry is behind an `RwLock`: compose-time type checks and printing take a shared read lock; only interning a new description writes.

A lock-free read path is a later option if compose profiling shows the read lock.

## 6. Numbers mix

Agreed 2026-10-05. 1.x refused `Int` with `Float` in arithmetic and comparisons, which forced workarounds like `Math.Add(0.0)` and `IsMore(0.0)` against `IsMore(0)` in real scripts. In 2.0:

- **Arithmetic** (`Math.Add`, `Math.Subtract`, `Math.Multiply`, `Math.Divide`):
  - Int with Int stays Int. Overflow and division by zero are activation errors, and division truncates (`5 | Div(2)` is 2).
  - Int with Float, in either order, gives a Float (`5 | Div(2.0)` and `5.0 | Div(2)` are 2.5).
  - A float vector with a number, in either order, applies to each component.
  - Vectors of one size work per component; different sizes are a compose error.
  - Compose computes the result type from the two operands.
- **Comparisons** (`IsLess`, `IsMore`, `IsMoreEqual`, `IsLessEqual`) take Int and Float mixed, compared by value. `Is`, `IsNot` and `IsAny` treat `1` and `1.0` as equal.
- **Not changed:** variables keep their type, so a Float variable does not accept an Int through `Set`/`Update`, and parameter literals still follow the declared types.

## 7. Open decisions: table storage and value sets

Raised in review (2026-10-05) against 1.x, whose tables are a `flat_map` over a `stable_vector` so `cloneVar` could copy into an existing destination without allocating.

- **Copy semantics stay copy-on-write.** Passing a table or assigning it only bumps a reference count; 1.x's main reason for `cloneVar` does not exist here.
- **Storage is a prototype choice.** The likely change is a sorted flat vector (`Arc<Vec<(Arc<str>, Var)>>`): one allocation to build, binary search to read, same ordering, equality and hashing. Two further gains to measure:
  - a shard that rebuilds a table every tick (`Table.Make`) reusing its previous output when nobody else holds it (`Arc::get_mut`), which is 1.x's allocation-free steady state without copy semantics;
  - `Take` on a fixed table resolving the key to an index at compose.
- **Decide with a benchmark:** small fixed records, repeated same-shape updates, retained snapshots followed by mutation, and growing dynamic tables, comparing time and allocations, `BTreeMap` against a flat vector. The final `Var` layout (contract §9) is decided alongside.
- **Stable references to table entries** (a 1.x requirement) are not needed by value reads or compose-resolved field access; do not add them without a use.
- **A runtime set value** (removed from 1.x in 2024) waits for a ported script that needs one. Its membership rules must be settled first: whether `1` and `1.0` are one element, how NaN behaves, and iteration order. `Var`'s bitwise identity equality is deliberately not the language's equality, so it cannot be used as is.

## 8. Not in this step

- How the frontend lowers `t.key` and `s.0` (the reading shard computes the key's type from a fixed table; that shard lands with the frontend).
- Shards over tables (`Take`, `Count`, table literals with variables inside). These follow with the frontend's first shards.
- Recursive types, objects, integer vectors, bytes, the final `Var` layout.
