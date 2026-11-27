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
| `IsA<T>( const I* )` | null pointer (no `ClassOf` call); `T::ClassOf` true/false | ported | `isa-*` (`isa_ptr`, `isa_ptr_type`, `derived_isa`): null objects (0 calls), every class against every target |
| `IsA<T>( const I& )` | `T::ClassOf( &aObject )` | ported | `isa-*` (`isa_ref`) |
| `dyn_cast<Casted>( From )` | `ClassOf` true: `static_cast`; false: `nullptr`; a null pointer reaches `ClassOf` | ported (`dyn_cast`, `dyn_cast_mut`) | `isa-*` (`dyn_cast_const`, `dyn_cast`, `derived_dyn_cast`): accepted and rejected casts, null (1 call; `FIX_NULLABLE` accepts it and yields null), identity of the result with the original object, writes through the result |

`IsA` and `dyn_cast` are templates over the class they test. Their evidence uses a generic
fixture hierarchy compiled into both adapters with its own `ClassOf` predicates (on a class
tag, counting calls): `FIX_BASE`, `FIX_DERIVED : FIX_BASE`, `FIX_LEAF : FIX_DERIVED`, the
sibling `FIX_OTHER`, `FIX_NULLABLE` (whose `ClassOf` accepts a null pointer) and
`FIX_MULTI : FIX_SIDE, FIX_BASE` (its C++ `static_cast` from `FIX_BASE*` adjusts the
pointer). The oracle instantiates the pinned templates with them: `IsA<T>( const FIX_BASE* )`,
`IsA<T*>( const FIX_BASE* )`, `IsA<T>( const FIX_BASE& )`, `IsA<T>( const FIX_DERIVED* )`,
`dyn_cast<const T*>( const FIX_BASE* )`, `dyn_cast<T*>( FIX_BASE* )` and
`dyn_cast<T*>( FIX_DERIVED* )` (up, identity and down casts; the casts that do not compile
are refused). Each reply carries the template's result, the number of `ClassOf` calls, whether
a cast result is the original object's part (C++: equal to `dynamic_cast`, independent of the
templates) and the original object's value after a write through a non-const result. Rust
implements `ClassOf` for the same classes (`static_cast_mut` gives the mutable projection) and
calls `kicad_core::typeinfo`. (Passing a non-const pointer to `IsA` selects
`IsA( const I& )` with `I` deduced as the pointer, a better match than the qualification
conversion to `const I*`, and `ClassOf( &aObject )` then does not compile for these
`ClassOf( const FIX_BASE* )` signatures; the cases pass const pointers. Rust's `is_a` takes
`Option<&I>`.) KiCad's production hierarchies (`EDA_ITEM`, `PNS::ITEM`
subclasses) and their `ClassOf` belong to the units that own them and are not qualified here.

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
| `Get()` | first call: `new T()` (allocate, then value-initialize) inside `call_once`; a throwing `T()` (or `bad_alloc`) leaves the flag unset, so the next call retries; later calls return the object | ported | `leak-*` for `int`, `std::string`, `std::vector<int>`, `std::map<std::string, int>`, `std::shared_ptr<std::string>`; `leak-lifetime-flaky-*`: a `FLAKY` whose constructor throws on demand, through every accessor, with the attempt count |
| `operator*`, `operator->`, `operator T&` | `Get()` | ported | every accessor creates on first use; `identity` |
| destructor (implicit) | never destroys or frees `T` | ported | `shared_string`: `use_count` stays raised after `destroy`; `release` + `read_retained` / `write_retained`: the object stays readable and writable through the pointer `Get()` returned after the wrapper is destroyed |
| deleted copy/move | | compile-time | the Rust type is neither `Clone` nor `Copy` |

The object lives in its own heap allocation that is never freed (`Vec::try_reserve_exact`,
then `Vec::leak`), so it survives the wrapper as the C++ object does; `LeakAtExit::release`
destroys the wrapper and hands back the surviving `&'static mut T`, the counterpart of a
pointer retained from `Get()`. Initialization fails where `new T()` fails: the allocation is
reported (`InitFailure::Allocation`, no abort) and a failing constructor
(`try_get_with`, `InitFailure::Construction`) releases the allocation; either way the wrapper
stays uninitialized and the next access tries again. `std::call_once` makes concurrent first
accesses create one object; the Rust accessors take `&mut self` (one owner, as for the other
ported `include/core` synchronization). `T: Default` value-initialization (`get` and the
accessors) has no typed constructor-failure channel; fallible constructors use `try_get_with`.

### inplace_function.h

| Symbol | Branches | Status | Evidence |
| --- | --- | --- | --- |
| `INPLACE_FUNCTION()` | empty | ported | `construct_empty` |
| `INPLACE_FUNCTION( F&& )` / `assign` | rvalue: move-construct into the storage; lvalue: copy-construct (may throw); `static_assert`s | ported (`from_callable`, `try_from_copy`); asserts compile-time | `construct` for every callable kind of four signatures, two capacities; `lifecycle-*`: `construct` (construct, move, destroy the moved-from temporary), `construct_copy` (copy or failed copy) |
| copy constructor / `copyFrom` | empty source: nothing; else `COPY` (may throw: nothing constructed) | ported (`try_clone`) | `copy_construct` with live, empty and moved-from sources; `use_count`; lifecycle events, failing copies |
| move constructor / `moveFrom` | empty source: nothing; else `MOVE`, then `aOther.reset()` destroys the moved-from source | ported (`take`) | `move_construct`; lifecycle events `move N`, `destroy N moved-from` |
| `operator=( const& )` | `this == &aOther`: nothing; else copy first (a throwing copy leaves `*this` unchanged), reset, move in | ported (`try_copy_assign`, `try_copy_assign_self`) | `copy_assign`, including self (63 public self-assignments now call the portable self branch) and failing copies that keep the old target |
| `operator=( && )` | `this == &aOther`: nothing; else reset, move | ported (`move_assign`, `move_assign_self`) | `move_assign`, including self (59 public self-assignments) |
| `operator=( F&& )` | temporary, reset, move; lvalue `F`: the temporary copies first | ported (`assign_callable`, `try_assign_copy`) | `assign`; lifecycle `assign`, `assign_copy` |
| destructor / `reset` | `m_manage`: destroy, clear | ported | `destroy`; `final_use_count` / `final_events` after the remaining slots are destroyed in slot order |
| `operator bool` | `m_invoke != nullptr` | ported | `bool` |
| `operator()` | empty: `std::bad_function_call`; else invoke | ported | `invoke` on empty and live wrappers |

**Storage.** The callable is constructed inside the wrapper, as in C++: `CAPACITY` payload
bytes at 16-byte alignment in `stack_dst::Value<dyn _, MaxAlignBuf<CAPACITY>>` (the
workspace's reviewed `stack_dst` 0.8.2 patch, `rust/vendor/stack_dst/EMBEINT-PATCH.md`),
with a separate metadata word after the payload standing for `m_invoke`/`m_manage`. No
wrapper storage operation allocates; a unit test checks that a stored callable's address lies inside the
wrapper. The C++ `static_assert`s are separate compile-time assertions on the Rust callable
type (`size_of <= CAPACITY`, `align_of <= 16`); they establish the Rust bound, not that a
C++ compiler would lay out an equivalent lambda identically.

**Lifecycle.** `kicad_core::inplace_function::Callable` carries the callable's special
members: `move_construct` (the move constructor; the wrapper then drops the moved-from
source as `moveFrom` resets it), `copy_construct` (the copy constructor, returning
`CopyFailure` where the C++ throws) and `Drop` (the destructor). Rust closures have no move
constructor and are relocated, which no program can observe; their copy is `Clone`. The
`inplace_lifecycle` cases store the adapters' `PROBE`, a callable that records each special
member, and compare the event sequence of every operation with the oracle's: e.g. a wrapper
move is `move N` then `destroy N moved-from`, an rvalue assignment is `construct`, `move`
(into the temporary), `destroy` of the old target, `move`, `destroy` of the moved-from
storage and finally of the moved-from argument, and a copy assignment whose copy fails is
`copy-failed N` with the old target kept.

The callables of the `inplace_function` cases are genuine lambdas in both adapters (`add`,
`mul`, a capture-less `plain`, a `shared` lambda capturing a copy of the case's shared token,
and a 48-byte `big` lambda for the 64-byte capacity). The token's `use_count` (C++
`std::shared_ptr`, Rust `Rc`) counts copies and destructions, not moves. The QA scenarios of
the pinned `qa/tests/common/test_inplace_function.cpp` are cases `inplace-qa-*`.

## Differences that no case can observe, stated

- **Closure moves.** A Rust closure has no move constructor; the wrapper relocates it and
  there is no moved-from closure to destroy. Callables with observable moves implement
  `Callable::move_construct` and get the C++ sequence (`inplace_lifecycle`).
- **Wrapper size.** The Rust wrapper is the payload, the metadata word and an `Option`
  discriminant; the C++ is the payload and two function pointers. Layout is not an
  observable of any operation and is not claimed equal.
- **Return conversion.** `std::is_invocable_r_v<R, ...>` lets a callable return a type
  convertible to `R`; Rust callables return `R` itself.
- **Allocation failure.** `LEAK_AT_EXIT`'s allocation failure path is ported and reported,
  but neither adapter can make an allocation fail, so no case reaches it.
- **Two storage invariants, reported as typed errors.** `INPLACE_FUNCTION`'s storage
  construction cannot fail after its compile-time size and alignment assertions, and
  `LEAK_AT_EXIT`'s leaked storage holds the one object just pushed. Neither is reachable,
  and neither panics: placement returns `StorageInvariant` (with `CopyFailure` combined into
  `CopyError` on the copy paths), propagated through every constructor and assignment, and
  `LEAK_AT_EXIT` reads its object back with `first_mut` and returns `InitFailure::Storage`.
  A failure keeps the C++ order: what is built before a reset is built first, so the
  destination is unchanged, as with `CopyFailure`. The Rust adapter refuses such a request
  instead of encoding a C++ exception, since no C++ exception stands for either; no case or
  control reaches one. `clippy::unreachable` is denied in every port crate.

## Remaining gaps (the unit stays partial)

Fresh raw-blob local validation, buffer safety probes and exact counts are recorded in
[VALIDATION.md](VALIDATION.md), including the local ellipse caller's remaining differences.

1. **Qualification.** Local evidence is GCC 15 / glibc 2.43 (WSL); the qualified reference is
   GCC 13 / glibc 2.39 (hosted `ubuntu-24.04`), which this PR's workflow runs. The new
   lifecycle, lifetime and `IsA`/`dyn_cast` domains need that hosted run and a new
   independent review before the unit can be considered for green.
2. **Production `ClassOf`.** `IsA`/`dyn_cast` are qualified through the fixture hierarchy;
   each production hierarchy's `ClassOf` is qualified by its owning unit when ported.

## Callers

`ELLIPSE::Mirror` (unit `libs.kimath.src.geometry.ellipse.application.ui.p001`) uses
`FLIP_DIRECTION` and was re-run: `cargo test -p kicad-ellipse-shim` and its corpus. Other
callers of `MIRROR`, `MIRROR_AXIS`, `KICAD_T`, `IsA`, `dyn_cast`, `INPLACE_FUNCTION`
(`libeval_compiler.h`), `THROTTLE` (`wx_progress_reporters.h`) and `LEAK_AT_EXIT` are in
units that are not ported yet; no `UNPORTED` stub names a symbol of this unit.
