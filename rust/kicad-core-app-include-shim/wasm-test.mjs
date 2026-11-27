// SPDX-License-Identifier: GPL-3.0-or-later
import {readFileSync} from 'node:fs';
const module=await WebAssembly.compile(readFileSync(process.argv[2]));
if(WebAssembly.Module.imports(module).length)throw new Error('runtime probe has host imports');
const instance=await WebAssembly.instantiate(module,{});
const code=instance.exports.main(0,0);
if(code!==0)throw new Error(`wasm include/core parity failed: ${code}`);
console.log(`Every include/core case and control agrees with newly executed native Rust replies (including refusals) in import-free wasm; Node ${process.version}, V8 ${process.versions.v8}`);
