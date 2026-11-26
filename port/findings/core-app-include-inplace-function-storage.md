# INPLACE_FUNCTION stores its callable in a box

Unit: `include.core.application.pure.p001`. Location: `include/core/inplace_function.h`.

## Difference

The C++ placement-constructs the callable in an `alignas( std::max_align_t )` buffer of
`Capacity` bytes and never allocates. Safe Rust cannot place a type-erased callable in inline
storage:

- `smallbox` 0.8.9 coerces to `dyn` on stable Rust only through its `smallbox!` macro, which
  expands to an `unsafe` block in the calling crate; under the port's `#![forbid(unsafe_code)]`
  that is error E0453 (checked).
- `stack_dst` 0.8.2 requires `typenum = "=1.17"`, which conflicts with the workspace's locked
  `typenum` 1.20.1, and stores the `dyn` metadata inside the buffer, which would change the
  usable capacity.

`kicad_core::inplace_function` therefore boxes the callable. Every operation's observable
behaviour (empty state, `std::bad_function_call`, copy, move leaving the source empty,
destruction once, reassignment destroying the old target) is the C++ one and is compared live.
The `static_assert`s on size (`DEFAULT_CAPACITY` = `4 * sizeof( void* )`) and alignment
(`MAX_ALIGN` = 16) are compile-time assertions in Rust too, so the same programs compile.

What differs: a box allocates (not for zero-sized callables) and an allocation failure aborts,
where the C++ cannot fail; and `moveFrom`'s destruction of the moved-from source has no Rust
counterpart (a Rust move leaves no source object).

## Next step

Decide whether to accept the box, or to allow one reviewed `unsafe` inline-storage container
for this type. Until decided, the unit stays `partial`.
