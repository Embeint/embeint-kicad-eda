<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# glibc_trig fresh reference harness

Targeted CI check for `kicad_math::glibc_trig`, the portable port of glibc 2.39's baseline
SSE2 `sin`, `cos` and `sincos` that `MATRIX3x3::SetRotation` uses
(`port/findings/kimath-matrix-libm-sin-cos-glibc-port.md`). The workflow
`.github/workflows/rust-glibc-trig.yml` measures a fresh genuine reference from the locked glibc
package and compares the port with it, bit for bit, on 1,001,602 inputs. It does not replace
or relax the math unit's corpus gate in `rust-port.yml`, changes no golden or case, and
passing it does not make that unit green.

## Reference

`reference.lock.json` copies, from the reviewed PR55 candidate input lock
(`port/reference-env/inputs.lock.json` at `33535308…`, also at `00106ffe…`, SHA-256
`c4d1c9c0…`), the one package and the three file hashes needed:

- `libc6 2.39-0ubuntu8.12` amd64: snapshot URL, size 3,264,258, SHA-256 `d821b41f…`;
- its loader `ld-linux-x86-64.so.2` (`5fc09408…`), `libc.so.6` (`755a371e…`) and
  `libm.so.6` (`5dcc4d25…`, build ID `f772fc0c…`).

The workflow fetches that commit and `bind-lock` refuses unless the lock bytes hash to
`c4d1c9c0…` and every copied field agrees. The package SHA-256 is the trust anchor: it was
recorded by the reviewed lock, whose packages were checked against the signed Ubuntu snapshot
indices there. This harness does not verify any signature again; the source package's `.dsc`
signature and the signed `Sources` index were never verified for the port (see the finding).
Nothing from the PR55 image is adopted.

`glibc_trig_reference.py reference`:

1. checks the domain file (count, SHA-256, fixed part) written by the Rust test
   `reference_write_domain`;
2. downloads the `.deb` from the locked URL, with three bounded attempts for transient
   network/service errors. If those fail, it tries the same package path on Ubuntu's
   official archive. Either transport must supply the original locked size and SHA-256
   before `dpkg-deb -x` (data only; no maintainer script runs). Hash failures never trigger
   a fallback. The identity record includes the URL that supplied the bytes;
3. verifies the extracted loader, libc and libm (regular files, size and SHA-256);
4. builds `probe.c` with the host `gcc -O0`;
5. runs the probe only through the extracted loader: `ld-linux-x86-64.so.2 --inhibit-cache
   --library-path <libdir> probe …`, with the environment cleared to `PATH`, `LC_ALL=C` and
   `GLIBC_TUNABLES=glibc.cpu.hwcaps=-AVX,-AVX2,-FMA,-FMA4,-AVX512F` (no `LD_PRELOAD`,
   `LD_LIBRARY_PATH` or other loader override);
6. verifies the records: exactly 40 bytes per input, and each record's input word equal to
   the domain's, in order; then writes `reference-identity.json`.

`probe.c` refuses before its first call unless the only loaded objects are itself, the vDSO
and the extracted loader, libc and libm (so `/etc/ld.so.preload` or a system library cannot
take part), `sin`, `cos` and `sincos` resolve from that libm at the locked `__sin_sse2`,
`__cos_sse2` and `__sincos_sse2` offsets, and the MXCSR control bits are `0x1f80` and the x87
control word `0x037f`. Exception status flags raised by the calls are ignored; the control
bits are checked again after the last call. It writes `x, sin(x), cos(x), sincos_sin,
sincos_cos` as little-endian u64 for each input.

## Comparisons

All six use the same `reference.bin` and must report the full 1,001,602 records with 0
mismatches; `report` fails otherwise, including when a comparison did not run:

| Comparison | How |
| --- | --- |
| native debug, native release | `cargo test -p kicad-math --lib glibc_trig -- --include-ignored` (`reference_records`) |
| wasm32-wasip1 debug, release | the same library test binary under Node's WASI (`node_wasi_test.mjs`) |
| wasm32-unknown-unknown debug, release | `wasm-set-rotation` exports the public `SetRotation`; `check.mjs` checks `m[0][0]`, `m[0][1]`, `m[1][0]`, `m[1][1]` against `sincos` |

`reference_records` regenerates the domain inside each test binary and refuses a reference
whose input words, order or count differ. Emulated rows never stand in for this reference.
The fresh records, domain, logs, hashes and `report.json` are uploaded as the run's artifact
(also on failure); nothing is committed or published.

### Node runtime regression

The workflow pins Node **24.16.0** rather than using the runner's ambient Node.
[Run 45349354578](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45349354578)
at PR head `559d282a393e9ccb547c2700fb1ab98fe6d7e1d7` used Node 22.23.5 on
Linux x86-64. Five profiles completed 1,001,602 records with zero mismatches;
Node segfaulted during the optimized WASI `reference_records` test, leaving its
profile without a result. That run remains failed.

On macOS arm64, Rust 1.99.1 reproduced exactly the failing WASI module's SHA-256,
`698062cdf458bfdad2b8be9c94d773beecd5d496b7906731db267a1e39037601`.
With the same module and genuine reference bytes (SHA-256
`d3a0a9fb0d0431e08f8365a7ba2a16f310b26234d2e9d9f204ac2a7eca71b63f`),
Node 22.23.4 also exited 139 during that test, while Node 24.16.0 completed all
eight selected tests and all 1,001,602 records with zero mismatches. This isolates
a runtime-version-dependent crash; its underlying engine defect is not diagnosed.
The pin changes no Rust arithmetic, WebAssembly module, reference input or check.
Fresh Linux execution of the pinned runtime is required before merge; no retry,
profile exclusion or fallback turns the crash into a pass.

## Local checks

`python -I -B -m unittest discover -s port/glibc-trig` runs the static fail-closed tests (no
network, compiler or probe run). The probe needs Linux x86-64.
