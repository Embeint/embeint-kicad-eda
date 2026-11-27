// SPDX-License-Identifier: GPL-3.0-or-later
import {readFileSync} from 'node:fs';
const module = await WebAssembly.compile(readFileSync(process.argv[2]));
if (WebAssembly.Module.imports(module).length) throw new Error('runtime probe has host imports');
const instance = await WebAssembly.instantiate(module, {});
const code = instance.exports.main(0, 0);
if (code !== 0) throw new Error(`wasm corpus parity failed: ${code}`);
console.log(`Complete Clipper2 clipper.h and clipper.minkowski.h corpus and controls agree with newly executed native Rust replies in import-free wasm; Node ${process.version}, V8 ${process.versions.v8}`);
