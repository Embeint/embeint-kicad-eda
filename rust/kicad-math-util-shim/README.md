<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Math util JSONL adapter

Native runner for `libs.kimath.src.math.util.application.ui.p001`. Portable code
is in `kicad-math/src/math_util.rs`. See [AUDIT.md](AUDIT.md) for source behavior,
platform profiles, wx host design and downstream handoff. Status is **partial**.

Each request is `{"unit":"libs.kimath.src.math.util.application.ui.p001",
"case":"name","input":{...}}`; identity and schema checks follow the existing
strict oracle protocol. Inputs have exactly the fields below. JSON integer
literals retain all bits; floats, booleans and out-of-range integers are refused.

| op | Other fields | Result |
| --- | --- | --- |
| `hypot_wide` | `x`, `y`: full u64 | `{"value":<u64>}` |
| `rescale_i32` | `numerator`, `value`, `denominator`: full i32 | `{"value":<i32>}`; zero denominator refused |
| `rescale_i64_int128` | Same, full i64 | `{"value":<i64>}`; Linux int128 profile; zero denominator refused |
| `log_debug` | `format`: string | Explicit unported `kimathLogDebug`; no formatting or wx effects executed |
| `log_overflow` | `bits`: 16 lowercase hex digits for binary64, `type_name`: string | Explicit unported `kimathLogOverflow`; no wx effects executed |

The main corpus contains **10,656 arithmetic cases**: 2,504 hypot and 4,076 of
each rescale width. It includes full-width boundary grids, both unsigned wrap
transitions, halfways, odd/negative divisors, narrowing, amplification and
fixed-seed generated inputs (LCG seed `0x64a17e0328bd591f`). The separate explicit
control fixture contains **411 refusals and 7 host deferrals**. Each is followed
by a successful arithmetic request in the process test, for 418 continuations.

This separation follows the existing harness: ordinary oracle errors and native
oracle deferrals are failures in a numerical difftest. Controls therefore run as
CTest `oracle-math-util-inputs`, Rust shim unit tests, and the hosted script below.
None are counted as successful arithmetic comparisons. Logging remains a gap even
when numerical comparison reports zero deferrals. No golden is proposed or blessed.

```sh
python3 rust/kicad-math-util-shim/generate_cases.py --check
cargo test --locked --manifest-path rust/Cargo.toml -p kicad-math -p kicad-math-util-shim -j2
cargo build --locked --manifest-path rust/Cargo.toml -p kicad-difftest -p kicad-math-util-shim -j2
cargo build --locked --manifest-path rust/Cargo.toml -p kicad-math-util-shim --release -j2
bash rust/kicad-math-util-shim/hosted_validation.sh ORACLE_BUILD_DIR OUT_DIR
```

The existing `rust-port.yml` builds the real pin's kimath library and this C++
adapter, runs native CTest controls, then automatically builds the registered
shim and compares every arithmetic case live. No workflow settings or source pin
changes are needed. The optional hosted script adds release comparison, byte
comparison between Rust profiles, controls through both programs and binary hashes.
[VALIDATION.md](VALIDATION.md) distinguishes completed evidence from pending runs.
