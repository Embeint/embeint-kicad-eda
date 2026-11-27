# INPLACE_FUNCTION inline storage and callable lifecycle

Unit: `include.core.application.pure.p001`. Location: `include/core/inplace_function.h`.

## Problem (first port, reviewed)

The first port boxed the callable. The pinned header states that the wrapper "stores the
target in a fixed inline buffer and never allocates on the heap"; the box allocated, and an
allocation failure aborted. Its wrapper moves also relocated the box, so a callable's move
constructor and the destruction of the moved-from source (`moveFrom`: `OP::MOVE`, then
`aOther.reset()`) had no counterpart, and the shared-pointer counts the cases used cannot see
moves. Copies were `Clone`, which cannot report the exception a C++ copy constructor may
throw, so the strong guarantee of copy assignment was untested. Self-assignment cases skipped
the call in the Rust adapter.

## Route evaluated

- `smallbox` 0.8.9: its stable coercion is the `smallbox!` macro, which expands to `unsafe`
  in the calling crate (E0453 under `#![forbid(unsafe_code)]`). Rejected.
- Borrowed caller storage, or an `Option<F>` per concrete type: not a type-erased, owning,
  copyable and reassignable wrapper for heterogeneous callables. Rejected.
- `stack_dst` 0.8.2 (`MIT OR Apache-2.0`): its safe `Value::new_stable` stores any
  `U: Trait` inline as `dyn Trait`; the `unsafe` stays inside the dependency. Two obstacles,
  both resolved by a reviewed local patch (`rust/vendor/stack_dst/EMBEINT-PATCH.md`):
  its `typenum = "=1.17"` pin (kept only for rustc 1.36) conflicts with the workspace's
  locked typenum 1.20, and it stores the `dyn` metadata inside the buffer. The patch relaxes
  the pin and adds `MaxAlignBuf<BYTES>`: `BYTES` payload bytes at 16-byte alignment plus a
  separate metadata word, so the metadata never takes payload capacity.

## Resolution

`kicad_core::inplace_function` stores the callable in
`stack_dst::Value<dyn _, MaxAlignBuf<CAPACITY>>` inside the wrapper; no operation allocates
(a unit test checks the callable's address lies within the wrapper). The size and alignment
`static_assert`s stay separate compile-time assertions on the Rust callable type.

`Callable` makes the special members explicit: `move_construct` (the move constructor; the
wrapper drops the moved-from source afterwards, as `moveFrom` does), `copy_construct`
(fallible, `CopyFailure`) and `Drop`. The wrapper's copy operations are fallible
(`try_clone`, `try_copy_assign`, `try_from_copy`, `try_assign_copy`) and keep the old target
when the copy fails; `try_copy_assign_self` and `move_assign_self` are the `this == &aOther`
branches, which the adapter now calls for the 63 copy and 59 move self-assignments of the
existing corpus. Rust closures, which have no move constructor, are relocated.

The `inplace_lifecycle` operation compares, event for event, a callable that records its
construction, copies, failed copies, moves, calls and destruction (`PROBE` in the C++
adapter, `Probe` in Rust) through every constructor and assignment operator.

## State

Implemented; local oracle comparison and review evidence are in the unit's PR. The unit stays
`partial` until the hosted GCC 13 run and a new independent review.
