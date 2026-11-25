# VECTOR2 / VECTOR3 differential adapter

Owns `libs.kimath.include.math.application.ui.p002` at upstream
`4cf939778df04d2b747bb2c4714fc5014a7565c7`. The canonical portable implementation is
`kicad-math/src/vector.rs`; this native-only crate handles protocol/I/O. The paired C++
adapter is `port/oracle/shims/vector.cpp` and calls the actual unmodified pinned headers.
Neither adapter loads recorded outputs. See `../kicad-math/TYPE_SUPPORT.md` for all 57
USR mappings, branch audit, supported types and remaining qualification gaps.

Each input has `dim` (integer 2 or 3), `type`, `op`, `a` (2 or 3 components).
Types are `i32/i64/f64` in 2D and `i32/u32/f64` in 3D. Integral components are exact JSON
integers in the component range; double components are lowercase 16-digit IEEE-754 hex
strings, including signed zeros, infinities and quiet/signalling NaN payloads. Results
use the same encoding; no JSON NaN/null substitution or decimal rounding.

| Operations | Extra fields | Observable output |
| --- | --- | --- |
| `new`, `copy`, `default` | none | `{value: vector}` |
| `assign` | none | `{value, state, same_ref}` from actual assignment return |
| `traits` | none | `{min, max}` in extended_type |
| `compare` | `b` | eq/ne; 2D also four relational operators, lex compare, min/max vectors and whether each returned the first reference |
| `dot`, `cross` | `b` | scalar or 3D vector |
| 2D `operator_dot`, `add`, `sub`, `squared_distance` | `b` | scalar/vector |
| 2D `distance` | `b` in **extended_type**, not coord_type | double |
| `norm`, 2D `squared_norm`, `neg`, `perpendicular`, `format`, `stream` | none | scalar/vector/string |
| 2D `mul_scalar`, `scalar_mul`, `add_scalar`, `sub_scalar`, `resize` | `scalar` in coord_type | vector |
| 2D `divide`, `add_float`, `sub_float` | `scalar` double bits | vector (KiROUND paths are deferred) |
| 2D `add_assign`, `sub_assign`, `mul_assign` | `b` | `{value, state, same_ref:true}` |
| 2D `add_assign_scalar`, `sub_assign_scalar`, `mul_assign_scalar` | either `scalar` **or** `alias: "x"/"y"` | same, with live const-reference alias reads |
| 3D `mul_assign_scalar`, `div_assign_scalar` | either `scalar` or `alias: "x"/"y"/"z"` | same, scalar captured by value |
| 3D `normalize`, `set_all` | `set_all` takes `scalar` | `{value: returned copy, state: mutated vector, same_ref:false}` |
| `cast`, 2D `cast_call` | `target` (one of that dimension's types) | constructor / `operator()<U>` conversion |
| double-only 2D `equals` | `b`, optional `epsilon` double bits | bool; actual omitted-default call in C++ |
| double-only 2D `ki_round`, `ki_round_xy` | none | VECTOR2I; distinct real upstream overloads |

Unknown/missing/extra input fields are rejected. This first adapter directly instantiates
same-type vector arithmetic; mixed common-type functions exist in the library but are not yet
qualified by native calls. Fresh classic-locale streams only: custom flags, precision, width,
locale, failed streams and chaining remain unqualified. No stream newline is added. Integer
format/stream outputs are implemented; double formatting is an explicit owned deferral.

Signed UB is checked at every intermediate, not only at the final mathematical result.
Unsigned multiplication and addition wrap in u32. VECTOR2 signed squared norm saturation is
**not** a dot-product guard. Float-to-integer casts check truncated representability; VECTOR2
clamps first, VECTOR3 does not. VECTOR2L's rounded `double(INT64_MAX)` bound is not treated as
representable. Defined KiROUND NaN/infinity results are never rejected as cast UB. VECTOR2
floating division by zero is allowed and reaches KiROUND, unlike integral VECTOR3 division.
The adapter does not impose the rescale comment's numerator <= denominator as a precondition.

`generate_cases.py` regenerates the fixed-seed synthetic corpus and separate UB controls.
The 9,582 corpus cases include all exposed operations, signed extremes, cast boundaries,
scalar aliases, signed zeros, tiny/huge floats, infinities and payload-bearing sNaN/qNaN pairs.
It computes inputs only. `port/oracle/vector-refusals.jsonl` has 30 UB controls; they are not
success cases or goldens. Both guard tests require each refusal and a succeeding next request. The shared 12 malformed-input controls also check component ranges, alias types and strict input shape.
Rust direct tests separately check partial mutation when a later checked component fails;
there is no claim about a C++ post-UB state.

Run locally with bounded jobs:

```sh
cargo test --locked --manifest-path rust/Cargo.toml -p kicad-math -p kicad-vector-shim -p kicad-hash -j2
cargo clippy --locked --manifest-path rust/Cargo.toml -p kicad-math -p kicad-vector-shim --all-targets -j2 -- -D warnings
cargo build --locked --manifest-path rust/Cargo.toml -p kicad-math --target wasm32-unknown-unknown -j2
python3 port/oracle/test_vector_inputs.py rust/target/debug/kicad-vector-shim
```

Hosted Linux rebuilds/audits the real oracle, runs its refusal test, then executes the complete
corpus using `kicad-difftest compare --partial`. The unit has no golden: deferrals remain
explicit, exact successful comparisons remain mandatory, and it cannot count as green.
Building for wasm is evidence of portability, not native/wasm runtime parity; no wasm vector
execution or per-site floating qualification is claimed in this unit yet.

The initial 4,103-case hosted pass is retained under
`validation/2026-11-24-f98485d7/`: 3,173 exact comparisons, 930 deferrals, zero failures.
It is tied to that historical head. The expanded 9,582-case corpus and later changes require
a fresh hosted result; no count from the initial run certifies them.

The later `validation/2026-11-24-ff389f87/` snapshot records 9,186 native cases,
8,513 exact comparisons and 673 explicit deferrals, with zero failures. It qualifies the
unique-NaN arithmetic implementation at that head; subsequent API/reference observations
and accepted-base integration remain pending their own live run.
