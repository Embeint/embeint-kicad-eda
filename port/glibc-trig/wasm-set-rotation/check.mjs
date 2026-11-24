// SPDX-License-Identifier: GPL-3.0-or-later
// node check.mjs MODULE.wasm reference.bin EXPECTED_RECORDS
// For every fresh genuine record (x, sin, cos, sincos_sin, sincos_cos) checks the
// wasm32-unknown-unknown SetRotation entries: m[0][0] = sincos_cos, m[0][1] = sincos_sin with
// the sign flipped, m[1][0] = sincos_sin, m[1][1] = sincos_cos. Refuses a record count other
// than EXPECTED_RECORDS; exits 1 on any mismatch.
import { readFileSync } from 'node:fs';

const [wasmPath, referencePath, expected] = process.argv.slice(2);
const { instance } = await WebAssembly.instantiate(readFileSync(wasmPath), {});
const entry = instance.exports.set_rotation_entry;
const data = readFileSync(referencePath);
const records = data.length / 40;
if (!Number.isInteger(records) || records !== Number(expected)) {
  console.error(`refused: ${data.length} bytes is not ${expected} records`);
  process.exit(1);
}
const SIGN = 1n << 63n;
const hex = (v) => BigInt.asUintN(64, v).toString(16).padStart(16, '0');
let mismatches = 0;
for (let r = 0; r < records; r++) {
  const base = r * 40;
  const x = data.readBigUInt64LE(base);
  const s = data.readBigUInt64LE(base + 24);
  const c = data.readBigUInt64LE(base + 32);
  const arg = BigInt.asIntN(64, x);
  const got = [0, 1, 3, 4].map((i) => BigInt.asUintN(64, entry(arg, i)));
  const want = [c, s ^ SIGN, s, c];
  if (got.some((g, i) => g !== want[i]) && mismatches++ < 20) {
    console.log(`x ${hex(x)} got ${got.map(hex)} want ${want.map(hex)}`);
  }
}
console.log(`wasm32-unknown-unknown set_rotation: ${records} records, ${mismatches} mismatches`);
process.exit(mismatches === 0 ? 0 : 1);
