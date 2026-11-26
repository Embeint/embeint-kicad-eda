// SPDX-License-Identifier: GPL-3.0-or-later
// Compares the import-free wasm pow with genuine 24-byte (x, y, pow) records.
import { readFileSync } from 'node:fs';
const [modulePath, recordsPath, countText] = process.argv.slice(2);
const module = await WebAssembly.compile(readFileSync(modulePath));
if (WebAssembly.Module.imports(module).length !== 0) throw new Error('wasm has imports');
const instance = await WebAssembly.instantiate(module, {});
const data = readFileSync(recordsPath);
const count = Number(countText);
if (data.length !== count * 24) throw new Error('record count');
let failures = 0;
for (let i = 0; i < count; i++) {
  const b = i * 24, x = data.readBigUInt64LE(b), y = data.readBigUInt64LE(b + 8);
  const got = BigInt.asUintN(64, instance.exports.pow_libm(x, y));
  const want = data.readBigUInt64LE(b + 16);
  if (got !== want && failures++ < 10) console.log(`pow x ${x.toString(16)} y ${y.toString(16)} got ${got.toString(16)} want ${want.toString(16)}`);
}
console.log(`glibc_libm pow: ${count} records, ${failures} mismatches`);
process.exit(failures === 0 ? 0 : 1);
