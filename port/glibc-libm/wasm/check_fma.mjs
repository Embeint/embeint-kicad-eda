// SPDX-License-Identifier: GPL-3.0-or-later
// Compares the import-free wasm fma with genuine 32-byte (x, y, z, fma) records.
import { readFileSync } from 'node:fs';
const [modulePath, recordsPath, countText] = process.argv.slice(2);
const module = await WebAssembly.compile(readFileSync(modulePath));
if (WebAssembly.Module.imports(module).length !== 0) throw new Error('wasm has imports');
const instance = await WebAssembly.instantiate(module, {});
const data = readFileSync(recordsPath);
const count = Number(countText);
if (data.length !== count * 32) throw new Error('record count');
let failures = 0;
for (let i = 0; i < count; i++) {
  const b = i * 32;
  const x = data.readBigUInt64LE(b), y = data.readBigUInt64LE(b + 8), z = data.readBigUInt64LE(b + 16);
  const got = BigInt.asUintN(64, instance.exports.fma_libm(x, y, z));
  const want = data.readBigUInt64LE(b + 24);
  if (got !== want && failures++ < 10) {
    console.log(`fma x ${x.toString(16)} y ${y.toString(16)} z ${z.toString(16)} got ${got.toString(16)} want ${want.toString(16)}`);
  }
}
console.log(`glibc_libm fma: ${count} records, ${failures} mismatches`);
process.exit(failures === 0 ? 0 : 1);
