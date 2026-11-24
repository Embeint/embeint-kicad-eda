<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Local validation of 2f0d6b14 (not qualified)

Tested code: `2f0d6b142b7cc908342c1c918e0a12885da99369`, unit
`libs.kimath.include.math.application.pure.p001`, 3,485 cases
(`cases_sha256` in `local-summary.json`). None of this is a qualified run. Linux x86-64 is
the qualified numerical platform, and no golden has been blessed yet.

## Passing locally (macOS 26 arm64, Rust 1.99.1)

- `cargo fmt --all --check`; `cargo clippy --locked --workspace --all-targets -- -D warnings`.
- `cargo test --locked --workspace`: everything passes, except that four `embeint-mcp`
  board-client tests fail under macOS's long `TMPDIR` (`path must be shorter than SUN_LEN`
  for a Unix socket). With `TMPDIR=/tmp`, all 53 `embeint-mcp` tests pass. This crate is not
  touched by this change.
- `python port/tools/check_goldens_policy.py --wasm`: all six portable crates, including
  `kicad-math`, build for `wasm32-unknown-unknown`; `kicad-math-shim` is native
  (`portable = false`).
- `python -m unittest discover -s rust/kicad-difftest/tests -p test_policy.py`: 7 tests pass.
- `python port/tools/validate_units.py`: all unit records and source accounting valid.

## Smoke oracle comparison

The adapter `port/oracle/shims/kimath_math.cpp`, the real `port/oracle/protocol.cpp` and a
throwaway driver were compiled with Apple clang
(`-std=c++20 -O2 -fno-fast-math -ffp-contract=off`) against `git archive` of the pin. It
used an empty `config.h` and a two-macro `wx/debug.h` stand-in, needed only by the
uninstantiated `VECTOR3::Normalize`. This is not the audited oracle build. Exact comparison
with the native Rust shim:

- 3,482 cases identical, zero error replies on either side.
- `stream-default-nan-with-infinity`, `stream-values-3`: Apple's libc prints a NaN with its
  sign bit set as `nan`, where glibc prints `-nan`. The port follows glibc, the qualified
  platform.
- `set-rotation-22` (angle NaN `fff8000000000000`): clang fused `sin`+`cos` into Apple's
  `__sincos_stret`, which keeps the NaN sign; Apple's separate `cos`, called by Rust,
  returns `7ff8000000000000`.

`test_refusals.py` passes all 187 refusal probes (374 requests) with byte-identical complete
replies from both programs.

## Native versus wasm

The same shim was built for `wasm32-wasip1` and run under Node 24's WASI. 3,461 cases are
identical to native and 24 differ, all `set_rotation` (libm `sin`/`cos`). See
`port/findings/kimath-matrix-libm-sin-cos.md`.

## Linux attempt

`../../validate_linux.sh` was started in a disposable `ubuntu:24.04` `linux/amd64` container
(QEMU user-mode emulation on the Rancher Desktop VM, which is shared with other workers).
The container was lost during package installation when the Docker daemon dropped it
(`container-attempt.log`). No oracle was built and no result came from that attempt. It was
not retried, to avoid loading a VM that other workers are using.

The qualified live comparison is the hosted `Rust port harness and phase zero` workflow on
the PR. `kicad-difftest check` runs the full oracle-versus-Rust comparison before it reads
the golden. The first baseline (propose/bless) still requires a Linux x86-64 run of
`validate_linux.sh bless`, or the equivalent WSL procedure in `port/oracle/README.md`.
