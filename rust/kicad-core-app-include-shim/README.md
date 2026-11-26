<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Application include/core p001 shim

Native JSONL adapter for unit `include.core.application.pure.p001`: the application
`include/core` headers `inplace_function.h`, `leak_at_exit.h`, `mirror.h`, `throttle.h` and
`typeinfo.h` at KiCad `4cf939778df04d2b747bb2c4714fc5014a7565c7`. The portable port is in
[`kicad-core`](../kicad-core) (`typeinfo`, `throttle`, `leak_at_exit`, `inplace_function`) and
[`kicad-math`](../kicad-math) (`mirror`, with `FLIP_DIRECTION`); the C++ counterpart is
[`port/oracle/shims/core_app_include_p001.cpp`](../../port/oracle/shims/core_app_include_p001.cpp),
which compiles the unmodified pinned headers. Both adapters validate in the same order and
refuse with the same messages. The per-symbol audit, differences and gaps are in
[AUDIT.md](AUDIT.md). The unit stays partial (see the audit's gaps).

```text
python3 rust/kicad-core-app-include-shim/generate_cases.py [--check]
cargo test -p kicad-core -p kicad-math -p kicad-core-app-include-shim
kicad-difftest compare --partial --unit include.core.application.pure.p001 --root . \
    --oracle <oracle-build>/kicad-oracle --rust rust/target/debug/kicad-core-app-include-shim
python3 rust/kicad-core-app-include-shim/compare_controls.py --oracle <oracle> --rust <shim>
```

The corpus (`port/cases/include.core.application.pure.p001.jsonl`, 3,599 fixed-seed cases)
holds only requests inside the pinned headers' defined domain. Requests both sides must refuse
are in `port/oracle/fixtures/core-app-include-p001-controls.jsonl` (293) with the expected
error. `.github/workflows/rust-core-app-include-live.yml` builds the oracle, compares both
sets, and replays them in import-free wasm32 against the run's native replies.

## Operations

Every input is an object with `op`; unknown or missing fields refuse with `unexpected or
missing field`. Integers are JSON integers; `double`s are the 16 lower-case hex digits of their
bits. Strings are at most 64 KiB; step lists at most 256.

| `op` | Fields | Result |
| --- | --- | --- |
| `enumerators` | | every `KICAD_T` enumerator as `[name, value]`, in order |
| `typeinfo` | `value` in [-128, 127] | `base_type` and the ten `Is*Type` predicates |
| `mirror_val`, `mirror` | `type` (`int`, `int64_t`, `double`), `point`, `ref` | `value` (`MIRRORVAL`) or `point` (`MIRROR`) |
| `mirror_vector` | `type` (`VECTOR2I`, `VECTOR2L`, `VECTOR2D`), `point`, `ref` (pairs), `direction` (any `int`) | `point` |
| `mirror_axis` | `direction` (any `int`) | `degrees` of `MIRROR_AXIS` |
| `throttle` | `interval_ms` (`int64`), `clock` (1 to 4,096 `steady_clock` nanosecond values) | `results` of `Ready()` for `clock[1..]`, the private `last`, `clock_calls` |
| `leak_at_exit` | `type` (`int`, `string`, `vector_int`, `map_string_int`, `shared_string`), `steps` | per-step `results` |
| `inplace_function` | `signature` (`i64(i32)`, `i64(i32,i32)`, `i64()`, `string(string)`), `capacity` (`"default"` or 64), `token`, `steps` | per-step `results`, `final_use_count` |

The throttle's construction takes `clock[0]`; each `Ready()` takes the next value. The oracle
wraps libstdc++'s `std::chrono::_V2::steady_clock::now()` (`-Wl,--wrap`) to supply them.

`leak_at_exit` steps, on two slots: `construct`, `created` (the private `m_instance` is set),
`identity` (all four accessors name one object), `read {via}`, `write {via, value}`,
`share {via}` (`shared_string`: store the case's shared pointer), `size` (`operator->`),
`destroy`, `use_count` (of the case's shared pointer). `via` is `get`, `deref`, `arrow` or
`convert`.

`inplace_function` steps, on four slots: `construct_empty`, `construct {callable}`,
`copy_construct {from}`, `move_construct {from}`, `assign {callable}`, `copy_assign {from}`,
`move_assign {from}` (a slot may be its own source), `destroy`, `bool`, `invoke {args}`
(`{"value": ...}` or `{"exception": "bad_function_call"}`), `use_count` (of the case's
token). Callables are genuine lambdas: `add`, `mul`, `shared` (captures the token), `plain`
(no capture) and the 48-byte `big` (64-byte capacity only).

Refusals: `undefined: signed overflow` (`MIRRORVAL` in `int`/`int64_t`),
`undefined: chrono overflow` (`THROTTLE`), integers outside the type's values (`KICAD_T`
outside [-128, 127], `FLIP_DIRECTION` outside `int`), `callable too large for capacity`
(does not compile in C++), and harness limits and lifetimes (`slot not constructed`,
`slot already constructed`, list sizes).
