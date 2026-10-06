# ESP32 firmware builds

The [firmware example](../examples/esp32/) embeds `shards-core` and
`shards-lang` in an ESP-IDF application. It parses an included script, runs it
on the stackless scheduler, checks its result and suspension, and prints
`Shards ESP32 smoke test passed: 42` with the tick count and its stack and heap
low-water marks to the serial console.

| Chip | Rust target | Toolchain |
|---|---|---|
| ESP32 | `xtensa-esp32-espidf` | Espressif Rust `1.97.0.0` (`esp`) |
| ESP32-S3 | `xtensa-esp32s3-espidf` | Espressif Rust `1.97.0.0` (`esp`) |
| ESP32-C3 | `riscv32imc-esp-espidf` | `nightly-2026-10-03` |

This uses Rust `std` on ESP-IDF, not bare-metal `no_std`. The coroutine mesh
and its `corosensei` dependency are excluded on ESP-IDF, as on WASI: the
crates' `build.rs` sets `cfg(stackful)` only where it exists. The
desktop backends remain unchanged. `shards-io`, the desktop CLI, networking,
GPIO and other peripheral shards are outside this initial build target.

## Build

Follow the [Rust on ESP toolchain guide](https://docs.espressif.com/projects/rust/book/getting-started/toolchain.html)
and install the native ESP-IDF prerequisites (CMake, Ninja, Python with venv
support, libclang, libudev and libusb development packages on Linux).
The build downloads ESP-IDF `v5.5.5` and its C toolchain into `.embuild`.
Allow several GB of disk space and network access for the first build.

For ESP32 and ESP32-S3:

```sh
cargo install espup --version 0.17.1 --locked
cargo install ldproxy --version 0.3.4 --locked
espup install --toolchain-version 1.97.0.0 --targets esp32,esp32s3
. "$HOME/export-esp.sh"
cd examples/esp32
MCU=esp32 cargo +esp build --locked --release --target xtensa-esp32-espidf
```

For ESP32-S3, use `MCU=esp32s3` and `--target xtensa-esp32s3-espidf`.
For ESP32-C3:

```sh
rustup toolchain install nightly-2026-10-03 --profile minimal --component rust-src
cargo +nightly-2026-10-03 install ldproxy --version 0.3.4 --locked
cd examples/esp32
MCU=esp32c3 cargo +nightly-2026-10-03 build --locked --release --target riscv32imc-esp-espidf
```

When `shards-core` or `shards-lang` gain or change a dependency, refresh the
example's lockfile with `cargo update -w` in `examples/esp32`: the firmware
builds with `--locked`, and root workspace builds do not touch that lockfile.

Run Cargo **from `examples/esp32`** so it reads the local `.cargo/config.toml`.
The example is a separate workspace with a committed lockfile; desktop
commands at the repository root do not build or install ESP-IDF. Explicit
`+toolchain` selectors override the repository's desktop toolchain pin.
Generated SDK configuration lives under the target's build directory. Keep
any optional user-supplied `sdkconfig` specific to the chip being built.

## Flash and verify

Install [espflash](https://github.com/esp-rs/espflash), connect a board matching
the target, and run from the example directory (ESP32 shown):

```sh
espflash flash --monitor target/xtensa-esp32-espidf/release/shards-esp32
```

The CI artifacts contain an ELF, the lockfile and SDK configuration. Flash the
ELF with `espflash`, which supplies the bootloader and partition table; it is
not a raw flash image. Confirm the success message on the serial console.

Without a board, run the firmware in Espressif's QEMU fork (upstream QEMU lacks
the ESP machines). Put `espflash` and the `qemu-system-xtensa` (ESP32, ESP32-S3)
or `qemu-system-riscv32` (ESP32-C3) binary from a
[QEMU release](https://github.com/espressif/qemu/releases) on `PATH`; the Linux
builds need the SDL2, slirp, pixman, libgcrypt and GLib shared libraries. Then,
from the example directory:

```sh
../../scripts/esp32-qemu.sh esp32s3 target/xtensa-esp32s3-espidf/release/shards-esp32
```

The script merges bootloader, partition table and app into a flash image, boots
it, prints the serial log and fails unless the success line appears before a
crash or the timeout (60 s by default; a third argument changes it).

The example reserves 8 KiB for its shallow main task. Acceptance suites run
sequentially on temporary tasks: 48 KiB for core, metadata, host-contract and registry, 128 KiB for
frontend tests and 96 KiB for trampoline depth tests. Each suite prints its stack low-water
mark and frees the task before the next suite. The success line reports the main
task's stack and heap low-water marks. Stack depth follows the code path, so
the emulator measures it as a board would; heap figures depend on the chip's
RAM layout and enabled components. Measured in QEMU on 2026-10-05 (commit
`5bdb7e2`, release build):

| Chip | Main stack used | Heap min free |
|---|---|---|
| ESP32 | 5,552 of 65,536 B | 228,032 B |
| ESP32-S3 | 5,768 of 65,536 B | 320,248 B |
| ESP32-C3 | 5,140 of 65,536 B | 259,692 B |

The smoke script is shallow, so 64 KiB is generous for it. It is not evidence
that the desktop nesting limit fits on a device: deeper scripts use more stack
while parsing and composing, and real workloads need their own measurement.
Firmware uses `panic = "abort"`: panics terminate the application and do not
provide desktop per-instance panic isolation. Shard documentation prose is
disabled through both dependency paths; parameter contracts are retained.

## CI and validation limits

[ESP32 CI](../.github/workflows/esp32.yml) links release firmware for all three
chips on pull requests and pushes to `main` that touch the core, the frontend,
the example or the root manifest, and on manual dispatch. Each matrix job uploads
its ELF and the SDK configuration of that build; the ESP32-C3 job also lints the
example with clippy. Each job then boots its firmware in Espressif's QEMU
(pinned release, checksum-verified) with `scripts/esp32-qemu.sh` and requires
the success line, so ESP-IDF startup, the main-task stack budget and the
smoke script's run are checked on all three chips. Emulation does not cover
real timing, flashing, radio or peripheral behavior, and QEMU does not
enforce every hardware limit; physical-board execution is still a separate
manual check, and no hardware result is claimed here. The native and WASI
jobs continue to test runtime semantics in depth.

The build layout follows the official
[ESP-IDF Rust template](https://github.com/esp-rs/esp-idf-template/tree/master/cargo)
and Rust's [ESP-IDF target documentation](https://doc.rust-lang.org/rustc/platform-support/esp-idf.html).

## M4 acceptance build

CI builds with `--features acceptance`. The example build script derives a direct runner from `shards-core/tests/{prototype,metadata,host_contract,registry,trampoline}.rs` and `shards-lang/tests/lang.rs`, preserving test cfg attributes and using the default mesh. The regular smoke example remains available without this feature. Device tests omit panic-unwind-only behavior (ESP-IDF aborts on panic), native filesystem watching and host allocator instrumentation. Deep-call tests obey the device's default invocation limit of 32 and use a smaller over-limit source to fit the heap. The success marker is printed only after all enabled suites finish; a panic or missing marker fails QEMU.

The M4 shared-suite gate exposed two limits hidden by the smoke script: a 64 KiB task stack overflowed in recursive compose on Xtensa, and the 200-wire rejection fixture exhausted the C3 lexer heap. A 96 KiB main stack was tried first (superseded by the per-suite tasks below); ESP rejection fixtures use 80 wires (still beyond the unchanged compose limit of 48), while native stress inputs remain unchanged. Exact-limit control-flow and trampoline depth tests are unchanged. The table above predates this expanded suite; new low-water marks require its QEMU success output.

The first 96 KiB main-stack attempt exhausted heap in the 100-instance case; temporary suite tasks avoid reserving that stack during heap-heavy tests. Nesting acceptance uses diagnostics-only `Program::compose`, which no longer builds a full tooling occurrence report only to discard it. Dedicated analysis tests still exercise those reports. The smoke program and catalog are released before acceptance begins.

The frontend task budget was raised from 96 to 128 KiB after S3 detected a stack overflow in the existing 30-level nested-source case. Main uses 8 KiB (the measured shallow smoke needs under 6 KiB), retaining more heap for the suites. Arena initialization now reserves the known subtree size and exact state/child capacities to avoid doubling allocations; device verification remains necessary.
