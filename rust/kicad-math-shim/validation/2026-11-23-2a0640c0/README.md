<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Local validation of the review fixes (not qualified)

Tested code: implementation `0915a81f7d` and corpus `2a0640c08e` (7,460 cases; the first
3,485 are byte-identical to the qualified corpus, `c21be36d…`). This is a macOS arm64 smoke
run, not the qualified Linux x86-64 platform. No golden exists or was proposed.

## Checks passing locally (Rust 1.99.1)

- `cargo fmt --all --check`; `cargo clippy --locked --workspace --all-targets -- -D warnings`.
- `kicad-math` tests (9) and `kicad-math-shim` tests (7).
- `check_goldens_policy.py --wasm`: all six portable crates build for `wasm32-unknown-unknown`.
- `validate_units.py`: all unit records and source accounting valid.
- The refactored Rust shim reproduces its previous replies for the original 3,485 cases byte
  for byte (`1c91b2ef…` before and after).
- `test_refusals.py`: 260 refusal probes (520 requests), byte-identical complete replies from
  the smoke oracle and Rust, 0 failures.

## Smoke oracle versus Rust, by group (`local-summary.json`)

The smoke oracle is built from the genuine adapter and `protocol.cpp`, with Apple clang and
libc++. Zero error replies on either side.

| Group | Identical | Different | Explanation of differences |
| --- | ---: | ---: | --- |
| original | 3,482 | 3 | Apple libc NaN printing (2), Apple `__sincos_stret` vs `cos` (1), as before |
| int | 185 | 0 | |
| stream-state | 82 | 197 | Every differing token is classified: NaN sign not printed by Apple (163), libc++ `%F` for uppercase fixed (56), Apple normalizing subnormal `%a` (20). None unexplained. |
| set-rotation-variant | 9 | 1 | Apple `sincos` vs `sin` on one angle |
| mixed-nan | 1,875 | 1,626 | The open mixed-NaN finding (compiler operand order); not resolved by this revision |

On the qualified platform the stream-state results depend on the libstdc++ runtime for
uppercase `fixed` infinities and NaNs (see `port/findings/kimath-matrix-stream-libstdcxx-lwg4084.md`).
The hosted gate's pre-GCC-15 runtime is expected to agree with the port; GCC 15 is not.

## Native versus wasm32-wasip1 (Node 24 WASI)

All groups are identical except `set_rotation`: 24 of the original angles and 6 of the 10
variant-sensitive angles differ (host libm versus musl-derived libm). On this host the
mixed-NaN group happens to agree, because both run on the same ARM hardware NaN rules. wasm
leaves NaN payload propagation unspecified, so that agreement is not portable.

## Not done

No Linux run of this revision: the shared Rancher VM failed earlier and is not to be used
for emulated rebuilds. The WSL qualification thread covered exact head `8dd2dc0b`. This
revision's integer and stream-state cases need a qualified run, and the mixed-NaN
regressions will fail it until the recorded decision is made.
