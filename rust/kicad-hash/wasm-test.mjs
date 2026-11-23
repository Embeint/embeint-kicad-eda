// SPDX-License-Identifier: GPL-3.0-or-later
// Run the wasm_parity example (built with KICAD_HASH_EXPECTED_DIGEST) in Node's wasm runtime.
import { readFileSync } from "node:fs";

const bytes = readFileSync(process.argv[2]);
const module = await WebAssembly.compile(bytes);
if (WebAssembly.Module.imports(module).length !== 0) {
    throw new Error("The portable parity probe must require no host imports");
}
const instance = await WebAssembly.instantiate(module, {});
if (typeof instance.exports.main !== "function") {
    throw new Error("Rust wasm probe has no main export");
}
const status = instance.exports.main(0, 0);
if (status !== 0) throw new Error(`wasm parity probe failed with status ${status}`);
console.log(`Every corpus reply in wasm equals the native digest; Node ${process.version}, V8 ${process.versions.v8}`);
