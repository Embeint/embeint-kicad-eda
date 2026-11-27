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
[AUDIT.md](AUDIT.md). Fresh local correction evidence is in [VALIDATION.md](VALIDATION.md).
The unit stays partial (see the audit's gaps).

```text
python3 rust/kicad-core-app-include-shim/generate_cases.py [--check]
cargo test -p kicad-core -p kicad-math -p kicad-core-app-include-shim
kicad-difftest compare --partial --unit include.core.application.pure.p001 --root . \
    --oracle <oracle-build>/kicad-oracle --rust rust/target/debug/kicad-core-app-include-shim
python3 rust/kicad-core-app-include-shim/compare_controls.py --oracle <oracle> --rust <shim>
```

The corpus (`port/cases/include.core.application.pure.p001.jsonl`, 4,031 fixed-seed cases)
holds only requests inside the pinned headers' defined domain. Requests both sides must refuse
are in `port/oracle/fixtures/core-app-include-p001-controls.jsonl` (336) with the expected
error. A defined continuation request follows each refusal to check that the process
continues normally. `.github/workflows/rust-core-app-include-live.yml` builds the oracle,
compares both sets and continuations, and replays all 4,703 requests in import-free wasm32
against the run's native replies.

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
| `leak_at_exit` | `type` (`int`, `string`, `vector_int`, `map_string_int`, `shared_string`, `flaky`), `steps` | per-step `results` |
| `inplace_function` | `signature` (`i64(i32)`, `i64(i32,i32)`, `i64()`, `string(string)`), `capacity` (`"default"` or 64), `token`, `steps` | per-step `results`, `final_use_count` |
| `inplace_lifecycle` | `copies` (0 to 256), `steps` | per-step `{outcome, events}`, `final_events` |
| `isa` | `form`, `target`, `object` (`null` or `{class, value}`), `write` (`null` or `int`) | `result`, `class_of_calls`, `object` (the original's value afterwards) |

The throttle's construction takes `clock[0]`; each `Ready()` takes the next value. The oracle
wraps libstdc++'s `std::chrono::_V2::steady_clock::now()` (`-Wl,--wrap`) to supply them.

`leak_at_exit` steps, on two slots: `construct`, `created` (the private `m_instance` is set),
`identity` (all four accessors name one object), `read {via}`, `write {via, value}`,
`share {via}` (`shared_string`: store the case's shared pointer), `size` (`operator->`),
`destroy`, `use_count` (of the case's shared pointer), `release` (keep the pointer `Get()`
returns, then destroy the wrapper), `read_retained {index}` and `write_retained {index,
value}` (through a kept pointer, after its wrapper is gone). `via` is `get`, `deref`, `arrow`
or `convert`. `flaky` is a type whose default constructor throws while failures are pending:
`fail_next {count}` sets them, `attempts` reports the constructor calls, and an access whose
construction throws reports `{"exception": "construction failed"}`.

`inplace_function` steps, on four slots: `construct_empty`, `construct {callable}`,
`copy_construct {from}`, `move_construct {from}`, `assign {callable}`, `copy_assign {from}`,
`move_assign {from}` (a slot may be its own source), `destroy`, `bool`, `invoke {args}`
(`{"value": ...}` or `{"exception": "bad_function_call"}`), `use_count` (of the case's
token). Callables are genuine lambdas: `add`, `mul`, `shared` (captures the token), `plain`
(no capture) and the 48-byte `big` (64-byte capacity only).

`inplace_lifecycle` stores, in a default-capacity `INPLACE_FUNCTION<long long( int )>`, a
callable that records its special members (`construct N`, `copy N`, `copy-failed N`,
`move N`, `call N`, `destroy N live|moved-from`). Steps, on four slots: `construct_empty`,
`construct {id}` (rvalue), `construct_copy {id}` (lvalue), `copy_construct {from}`,
`move_construct {from}`, `assign {id}`, `assign_copy {id}`, `copy_assign {from}`,
`move_assign {from}` (a slot may be its own source), `destroy`, `bool`, `invoke {arg}` and
`copies {count}` (how many more copies succeed; the next throws). A failing copy reports
`{"exception": "copy failed"}`.

`isa` instantiates `IsA` and `dyn_cast` with a fixture hierarchy (`base`, `derived`,
`leaf`, `other`, `nullable` whose `ClassOf` accepts null, `multi` with a second base). Forms:
`isa_ptr` (`IsA<T>( const FIX_BASE* )`), `isa_ptr_type` (`IsA<T*>( const FIX_BASE* )`), `isa_ref`,
`dyn_cast_const`, `dyn_cast` (non-const; `write` stores through the result),
`derived_isa` and `derived_dyn_cast` (from a `FIX_DERIVED*`). A cast result is
`null` or `{value, same}`, `same` meaning it is the original object's part.

Refusals: `undefined: signed overflow` (`MIRRORVAL` in `int`/`int64_t`),
`undefined: chrono overflow` (`THROTTLE`), integers outside the type's values (`KICAD_T`
outside [-128, 127], `FLIP_DIRECTION` outside `int`), `callable too large for capacity`
(does not compile in C++), casts that do not compile (`cast does not compile`), and harness
limits and lifetimes (`slot not constructed`, `slot already constructed`,
`retained pointer not taken`, list sizes).
