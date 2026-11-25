// SPDX-License-Identifier: GPL-3.0-or-later
// Runs a wasm32-wasip1 libtest binary under Node's WASI (preview1).
//   node node_wasi_test.mjs TEST.wasm HOST_DATA_DIR [libtest args...]
// HOST_DATA_DIR is preopened as /data. KICAD_GLIBC_LIBM_* variables are forwarded with any
// host path under HOST_DATA_DIR rewritten to /data. Exits with the test binary's status.
import { readFileSync } from 'node:fs';
import { resolve, relative, sep } from 'node:path';
import { WASI } from 'node:wasi';

const [wasmPath, dataDir, ...testArgs] = process.argv.slice(2);
const hostData = resolve(dataDir);
const env = {};
for (const [key, value] of Object.entries(process.env)) {
  if (!key.startsWith('KICAD_GLIBC_LIBM_')) continue;
  const rel = relative(hostData, resolve(value));
  env[key] = rel && !rel.startsWith('..') && !rel.includes(':') ? '/data/' + rel.split(sep).join('/') : value;
}
const wasi = new WASI({
  version: 'preview1',
  args: ['test.wasm', ...testArgs],
  env,
  preopens: { '/data': hostData },
  returnOnExit: true,
});
const module = await WebAssembly.compile(readFileSync(wasmPath));
const instance = await WebAssembly.instantiate(module, wasi.getImportObject());
const status = wasi.start(instance);
process.exit(status ?? 0);
