<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# include.core.application.pure.p001: source audit

Unit `include.core.application.pure.p001` owns the application `include/core` headers
`inplace_function.h`, `leak_at_exit.h`, `mirror.h`, `throttle.h` and `typeinfo.h` at KiCad
`4cf939778df04d2b747bb2c4714fc5014a7565c7` (37 USRs, 1,046 physical lines). The five
headers in this checkout are byte-identical to the pin (CR stripped). Ports:

| Header | Rust |
| --- | --- |
| `typeinfo.h` | `kicad_core::typeinfo` |
| `throttle.h` | `kicad_core::throttle` |
| `leak_at_exit.h` | `kicad_core::leak_at_exit` |
| `inplace_function.h` | `kicad_core::inplace_function` |
| `mirror.h` | `kicad_math::mirror` (it needs `EDA_ANGLE` and `VECTOR2`) |

`kicad_core::app_include_core` re-exports the four `kicad-core` modules. The C++ adapter is
[`port/oracle/shims/core_app_include_p001.cpp`](../../port/oracle/shims/core_app_include_p001.cpp);
it compiles the unmodified pinned headers and only decodes, scripts and encodes.

## Per-symbol and per-branch audit

Status: **ported** (body and every branch ported, with successful oracle evidence),
**ported, no evidence** (body ported; no genuine successful operation can be run yet),
**compile-time** (a C++ compile-time property, enforced at Rust compile time).

### typeinfo.h

| Symbol | Branches | Status | Evidence |
| --- | --- | --- | --- |
| `enum KICAD_T` (115 enumerators, -1 to 113) | values: every integer in [-128, 127] ([dcl.enum]/8) | ported | `enumerators` case: every name and ordinal; all 256 values |
| `static_assert( S3D_CACHE_T == 104 )` | | compile-time | `const _: () = assert!` |
| `BaseType` | 10 case groups + `default` | ported | `typeinfo-*`: all 256 values |
| `IsNullType` | `aType <= 0` (all negatives, including non-enumerators) | ported | all 256 values |
| `IsGeneratedTableType` | 2 enumerators | ported | all 256 values |
| `IsInstantiableType` | `IsNullType` early return; 4 `false` groups; `default` | ported | all 256 values |
| `IsEeschemaType`, `IsPcbnewType`, `IsSingleLayerType`, `IsGerbviewType`, `IsPageLayoutEditorType`, `IsMiscType` | case lists + `default` | ported | all 256 values |
| `IsTypeCorrect` | short-circuit `\|\|` of the six predicates | ported | all 256 values |
| `IsA<T>( const I* )` | null pointer (no `ClassOf` call); `T::ClassOf` | ported, no evidence | see gaps |
| `IsA<T>( const I& )` | `T::ClassOf( &aObject )` | ported, no evidence | see gaps |
| `dyn_cast<Casted>( From )` | `ClassOf` true: `static_cast`; false: `nullptr` | ported, no evidence | see gaps |

Integers outside [-128, 127] are refused (`integer out of range: value`): converting them to
`KICAD_T` is undefined ([expr.static.cast]/10). Non-enumerators inside the range (for
example -128 to -2 and 114 to 127) are valid values and take each `default` branch; they are
public cases.

### mirror.h

| Symbol | Branches | Status | Evidence |
| --- | --- | --- | --- |
| `enum class FLIP_DIRECTION` (underlying `int`) | `LEFT_RIGHT`, `TOP_BOTTOM`, any other `int` | ported | `mirror_vector`/`mirror_axis` with 0, 1, 2, -1, 42, `INT_MIN`, `INT_MAX`, random |
| `MIRRORVAL<T>` (`int`, `int64_t`, `double`) | `-( aPoint - aMirrorRef ) + aMirrorRef` | ported | `mirror_val-*`: integer boundary grid, 22 x 22 special doubles (signed zeros, subnormals, infinities, quiet/signalling NaNs with payloads), random |
| `MIRROR( T&, const T& )` | assigns `MIRRORVAL` | ported | `mirror-*` |
| `MIRROR( VT&, const VT&, FLIP_DIRECTION )` (`VECTOR2I`, `VECTOR2L`, `VECTOR2D`) | `LEFT_RIGHT`: x; else: y | ported | `mirror_vector-*`, including an overflowing coordinate that the chosen branch does not touch |
| `MIRROR_AXIS` | `LEFT_RIGHT`: `ANGLE_90`; else `ANGLE_0` | ported | `mirror_axis-*` |

Undefined and refused: every signed overflow of `aPoint - aMirrorRef`, its negation, or the
final addition (`undefined: signed overflow`, 220 controls: 80 each for `MIRRORVAL` and
`MIRROR` over `int` and `int64_t`, 60 for `VECTOR2I`/`VECTOR2L`). For `double` the compiled
reference evaluates `aMirrorRef - ( aPoint - aMirrorRef )`, identical for every non-NaN value
but different in which NaN survives; the port evaluates that form with `kicad_math::fp`
(`port/findings/core-app-include-mirrorval-compiled-subtraction.md`: 419 cases differed with
the source form, 0 with the compiled one).

`FLIP_DIRECTION` was co-ported first by the ellipse unit with its two enumerators only. It
now lives in `kicad_math::mirror` with its whole domain (`Other(NonEnumerator)`, built only
by `FlipDirection::from_int`, so each value has one representation) and is re-exported as
`kicad_math::ellipse::FlipDirection`. `ELLIPSE::Mirror` tests only for `LEFT_RIGHT`, so a
non-enumerator now takes its `TOP_BOTTOM` branch exactly as in C++; the ellipse adapter
still constructs only the two enumerators.

### throttle.h

| Symbol | Branches | Status | Evidence |
| --- | --- | --- | --- |
| `THROTTLE( milliseconds )` | `m_last = now() - aInterval` | ported | `throttle-*` (zero, negative and extreme intervals) |
| `Ready()` | `now - m_last >= m_interval`: update and `true`; else `false` | ported | `throttle-*`: equal times, exactly one interval, one nanosecond short, backward clocks, `INT64` extremes; reply includes the private `m_last` and the clock call count |

Host observation: `steady_clock::now()` enters the portable code explicitly through
`throttle::SteadyClock`, one call per `now()`. The oracle supplies the same scripted values
to the genuine compiled bodies by wrapping libstdc++'s out-of-line
`std::chrono::_V2::steady_clock::now()` with `-Wl,--wrap`, only while a throttle case is
armed (otherwise it forwards to the real clock). The adapter reads `m_last` through the
explicit-instantiation access rule to guard, and to report; it never writes it.

Undefined and refused (`undefined: chrono overflow`): the milliseconds-to-nanoseconds
conversion of the interval, `now - aInterval`, and `now - m_last` overflowing `int64`.

### leak_at_exit.h

| Symbol | Branches | Status | Evidence |
| --- | --- | --- | --- |
| `LEAK_AT_EXIT()` | `m_instance( nullptr )` | ported | `created` is false after construction |
| `Get()` | first call creates `new T()` (value-initialized); later calls return it | ported | `leak-*` for `int`, `std::string`, `std::vector<int>`, `std::map<std::string, int>`, `std::shared_ptr<std::string>` |
| `operator*`, `operator->`, `operator T&` | `Get()` | ported | every accessor creates on first use; `identity` |
| destructor (implicit) | never destroys `T` | ported | `shared_string`: `use_count` stays raised after `destroy` |
| deleted copy/move | | compile-time | the Rust type is neither `Clone` nor `Copy` |

`std::call_once` makes concurrent first accesses create one object; the Rust accessors take
`&mut self` (one owner, as for the other ported `include/core` synchronization). A throwing
`T()` or `std::bad_alloc` leaves the flag unset so a later access retries; `T::default()`
cannot report failure, so that path has no Rust counterpart (see gaps).

### inplace_function.h

| Symbol | Branches | Status | Evidence |
| --- | --- | --- | --- |
| `INPLACE_FUNCTION()` | empty | ported | `construct_empty` |
| `INPLACE_FUNCTION( F&& )` / `assign` | stores the callable; `static_assert`s | ported; asserts compile-time | `construct` for every callable kind of four signatures, two capacities |
| copy constructor / `copyFrom` | empty source: nothing; else `COPY` | ported | `copy_construct` with live, empty and moved-from sources; `use_count` |
| move constructor / `moveFrom` | empty source: nothing; else `MOVE` and reset the source | ported | `move_construct`; source then `false` and `bad_function_call` |
| `operator=( const& )` | `this == &aOther`: nothing; else copy first, reset, move in | ported (self case: see differences) | `copy_assign`, including self |
| `operator=( && )` | `this == &aOther`: nothing; else reset, move | ported (self case: see differences) | `move_assign`, including self |
| `operator=( F&& )` | temporary, reset, move | ported | `assign` |
| destructor / `reset` | `m_manage`: destroy, clear | ported | `destroy`; `final_use_count` after the remaining slots are destroyed |
| `operator bool` | `m_invoke != nullptr` | ported | `bool` |
| `operator()` | empty: `std::bad_function_call`; else invoke | ported | `invoke` on empty and live wrappers |

The callables are genuine lambdas in both adapters (`add`, `mul`, a capture-less `plain`, a
`shared` lambda capturing a copy of the case's shared token, and a 48-byte `big` lambda for
the 64-byte capacity). The token's `use_count` (C++ `std::shared_ptr`, Rust `Rc`) observes
every copy, move and destruction of a stored callable. The QA scenarios of the pinned
`qa/tests/common/test_inplace_function.cpp` are cases `inplace-qa-*`.

## Differences that no case can observe, stated

- **Storage.** The C++ constructs the callable in an inline buffer and never allocates. Safe
  Rust cannot type-erase into inline storage (it needs `unsafe`; `smallbox`'s stable
  coercion is a macro that expands to `unsafe` in the caller, refused by
  `forbid(unsafe_code)`, and `stack_dst` pins `typenum = 1.17`, incompatible with the
  workspace). The callable is boxed: an allocation (none for zero-sized callables) whose
  failure aborts. Capacity and alignment remain compile-time requirements, checked against
  the C++ limits (`DEFAULT_CAPACITY` = 4 pointers, `MAX_ALIGN` = 16), so exactly the C++
  programs that compile, compile.
- **MOVE then DESTROY of the source.** `moveFrom` move-constructs the callable and then
  destroys the moved-from source. A Rust move relocates the callable; there is no moved-from
  object to destroy. Only a callable with an observable moved-from destructor could tell.
- **Self-assignment.** `fn = fn` and `fn = std::move( fn )` take the `this == &aOther` branch
  and do nothing. Rust's exclusive borrows cannot express these calls, so the adapter makes
  no call for them; the oracle runs the genuine C++ branch and the replies agree.
- **Return conversion.** `std::is_invocable_r_v<R, ...>` lets a callable return a type
  convertible to `R`; Rust callables return `R` itself.

## Remaining gaps (the unit stays partial)

1. **`IsA` and `dyn_cast` have no successful-operation evidence.** Every real `ClassOf`
   belongs to a `UI` unit that is still `todo` (the `EDA_ITEM` subclasses in `eeschema/`
   and `pcbnew/`, the `PNS::ITEM` subclasses, e.g. `PNS::SEGMENT::ClassOf` in
   `pcbnew.router.application.ui.p021`), and none of their headers is in the oracle closure
   (`pns_via.h` pulls `padstack.h`; the `EDA_ITEM` classes pull `eda_item.h` and wx). No
   stand-in hierarchy is used. The bodies are ported against a `ClassOf` trait; the owners
   implement it when they are ported, and their cases exercise these templates.
2. **Allocation failure.** `new T()` in `LEAK_AT_EXIT::Get` and the absence of any
   allocation in `INPLACE_FUNCTION` are not mirrored (see above).
3. **Qualification.** Local evidence is GCC 15 / glibc 2.43 (WSL); the qualified reference
   is GCC 13 / glibc 2.39 (hosted `ubuntu-24.04`), which this PR's workflow runs.

## Callers

`ELLIPSE::Mirror` (unit `libs.kimath.src.geometry.ellipse.application.ui.p001`) uses
`FLIP_DIRECTION` and was re-run: `cargo test -p kicad-ellipse-shim` and its corpus. Other
callers of `MIRROR`, `MIRROR_AXIS`, `KICAD_T`, `IsA`, `dyn_cast`, `INPLACE_FUNCTION`
(`libeval_compiler.h`), `THROTTLE` (`wx_progress_reporters.h`) and `LEAK_AT_EXIT` are in
units that are not ported yet; no `UNPORTED` stub names a symbol of this unit.
