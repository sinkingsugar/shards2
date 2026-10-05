# Shards 2.0

A new Rust implementation of the [Shards](https://github.com/fragcolor-xyz/shards) runtime.

**Status:** core prototype complete (2026-10-04). `crates/shards-core` implements the compiled/state split with two schedulers, **stackless (the default) and stackful**, both maintained, one per mesh. `crates/shards-io` adds real async I/O (`Http.Get`). The same acceptance suite passes on both schedulers, and on wasm for stackless. The core catalog and `Http.Get` have shared descriptions, argument decoding and structured compose diagnostics. Next: tables and type sets, then a language frontend slice measured against real scripts (see `docs/current-state.md`).

## Starting work

Start with [AGENTS.md](AGENTS.md) for working rules and [docs/current-state.md](docs/current-state.md) for settled decisions, review status and the next step. These are the entry points for a fresh session launched from this directory.

## Why a new implementation

In Shards 1.x, each shard instance holds its parameters, its compose output and its runtime state together. Running the same wire N times (spawned entities, server connections, `Expand` items) rebuilds and recomposes everything N times. 2.0 compiles a wire once into a shared, immutable artifact, and each instance owns only a small runtime state. Changing that shard contract is deep enough to justify a new core rather than migrating 1.x in place.

C++ libraries are still used, but called from Rust; in 1.x, C++ is the host.

## Documents

- [`docs/shard-metadata-and-compose.md`](docs/shard-metadata-and-compose.md): shared descriptions, catalog, argument decoding, and diagnostics; implemented for the full core catalog and `Http.Get`.

- [`docs/shards-2-compose-split.md`](docs/shards-2-compose-split.md): the core design, the 1.x audit behind it, what carries over from 1.x, and the validation plan.
- [`docs/prototype-shard-contract.md`](docs/prototype-shard-contract.md): the shard contract the prototype implements.
- [`docs/stackless-experiment.md`](docs/stackless-experiment.md): the two schedulers and the decision, shared shard APIs (`LeafShard`, `AsyncShard`), real I/O, and matched benchmarks against 1.x.
- [`docs/values-and-types.md`](docs/values-and-types.md): tables, float vectors and type sets for the frontend slice.
- [`docs/embedding.md`](docs/embedding.md): embedding Shards 2.0 in a Rust host and writing host shards.
- [`docs/surface-syntax-review.md`](docs/surface-syntax-review.md): the 1.x grammar reviewed for agent authoring, and the proposed parser decisions.
- [`docs/ai-first-roadmap.md`](docs/ai-first-roadmap.md): the overall strategy (reference copy; canonical in the 1.x repo).

## Results so far

Matched prototype benchmarks against 1.x (instance creation, memory per instance, steady state, resume depth, HTTP concurrency) are in `docs/stackless-experiment.md` and `docs/shards-2-compose-split.md` §5, with their limits. They support the instance-sharing design; they are not full-application performance claims.

```sh
cargo test --workspace                                                        # acceptance tests, both schedulers
cargo run --release -p shards-core --example bench_instances -- 1000 --stackless  # instance benchmark
cargo run --release -p shards-core --example bench_depth -- --stackless           # resume cost vs depth
cargo run --release -p shards-core --example bench_async                          # polling vs notification
bench/shards-1x/run.sh path/to/1.x/shards                                     # the same benchmarks on 1.x
bench/http-concurrency/run.sh path/to/1.x/shards                              # HTTP concurrency, 1.x vs 2.0
```

## Crates

- `crates/shards-core`: the runtime core: compose, both schedulers, lifecycle helpers, and the prototype shards (shared `LeafShard`/`AsyncShard` implementations, plus per-scheduler control flow).
- `crates/shards-io`: I/O shards (`Http.Get`) on a shared Tokio runtime, following 1.x's HTTP module. Native only; TLS via the `rustls-ring` or `native-tls` feature.
- `crates/shards-lang`: the language frontend: parser, lowering to wire definitions with a source map, `check`/`run`, and host-driven `Session` execution with state-preserving nested-wire reload on either scheduler.
- `crates/shards-cli`: the `shards2` command: `cargo run -p shards-cli -- check --json file.shs`, `run`, `watch`, `describe`, `search`. File watching and warm host sessions are covered in [the embedding guide](docs/embedding.md#5-warm-sessions-and-hot-reload).

## Development

ESP32, ESP32-S3 and ESP32-C3 firmware builds use ESP-IDF and the stackless
runtime. See [the ESP32 build guide](docs/esp32.md) for the example, toolchains,
CI artifacts and hardware-validation limits.

```sh
cargo check --workspace
cargo test --workspace
cargo fmt --all
cargo clippy --workspace --all-targets
```

The toolchain is pinned in `rust-toolchain.toml`.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this work, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
