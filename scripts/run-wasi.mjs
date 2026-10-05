// Runs a wasm32-wasip1 binary (e.g. a test binary built with
// `cargo test --target wasm32-wasip1 --no-run`) under Node's built-in WASI.
// Usage: node scripts/run-wasi.mjs <file.wasm> [args...]
import { readFile } from "node:fs/promises";
import { WASI } from "node:wasi";
import { argv, exit } from "node:process";

const [file, ...args] = argv.slice(2);
if (!file) {
  console.error("usage: node scripts/run-wasi.mjs <file.wasm> [args...]");
  exit(2);
}
const wasi = new WASI({ version: "preview1", args: [file, ...args], env: {}, returnOnExit: true });
const module = await WebAssembly.compile(await readFile(file));
const instance = await WebAssembly.instantiate(module, wasi.getImportObject());
exit(wasi.start(instance));
