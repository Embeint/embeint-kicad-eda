<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# stack_dst 0.8.2: Embeint compatibility patch

`kicad-core` stores `INPLACE_FUNCTION`'s callable inline, as the C++ does, through
`stack_dst::Value<dyn _, MaxAlignBuf<CAPACITY>>`. The workspace takes this crate through
`[patch.crates-io]` in `rust/Cargo.toml`; nothing else in the workspace uses it.

## Source

- crates.io `stack_dst` 0.8.2, archive SHA-256
  `207b6c7f45471c8ad50acdf69a83931ce3b4166deba7b9dea435205374717629`
  (upstream commit `2fc26a9088192e98565cc3c94f3a419d9d8e81bf`,
  <https://github.com/thepowersgang/stack_dst-rs>).
- Upstream licence `MIT OR Apache-2.0` (`LICENSE-MIT`, `LICENSE-APACHE`, `LICENCE`),
  compatible with the port's GPL-3.0-or-later. The new `embeint_inline.rs` is
  GPL-3.0-or-later (`LICENSE-GPL-3.0-or-later`); the patched package declares
  `(MIT OR Apache-2.0) AND GPL-3.0-or-later`.
- `.gitattributes` keeps every vendored file byte-exact (CRLF included): `src/`, `tests/`,
  `README.md`, the original licence files and `Cargo.toml.orig` equal the archive's files,
  except for the documented module registration in `src/lib.rs` and sentinel fix in
  `src/value.rs`.

## Changes

1. **`Cargo.toml`** (rewritten from `Cargo.toml.orig`): `typenum = "1.17"` instead of
   `"=1.17"`. Upstream pins 1.17 only so the crate still builds on rustc 1.36; the
   workspace requires rustc 1.85 and already locks typenum 1.20 through `hybrid-array`,
   and Cargo cannot resolve the conflicting compatible-version requirements. `publish = false`, `edition = "2015"`
   (the upstream default), `rust-version = "1.51.0"` (its `const_generics` floor).
   `kicad-core` uses the crate with `default-features = false`.
2. **`src/embeint_inline.rs`** (new) and a documented module registration in
   `src/lib.rs`: `MaxAlignBuf<BYTES>`, a `#[repr(C, align(16))]` union with at least `BYTES`
   payload bytes followed by one separate 16-byte metadata word, and its element type
   `MaxAlignWord`. `Value` writes the object at the start of the buffer and the `dyn`
   metadata in its last word, so every `U` with `size_of::<U>() <= BYTES` and
   `align_of::<U>() <= 16` fits and the metadata never takes payload space. Two overlapping
   C-layout union fields put the aligned word before or after the byte array. Their data
   regions jointly cover every byte, so union moves preserve payload and metadata, including
   pointers and the rounded capacity that a safe `Value` caller can use. A single struct
   or a union with uncovered tail padding can lose those bytes during a move. The complete
   union is zeroed initially, including unused bytes of the metadata word copied when
   rebuilding pointers. The crate's word arithmetic
   would accept objects up to the padded size; `kicad-core` enforces the exact `BYTES`
   limit itself, at compile time.

3. **`src/value.rs::is_replace_sentinel`**: inspect only the pointer metadata bytes that
   `replace_stable` marks, instead of the whole rounded buffer word. With a 16-byte word
   and a smaller vtable pointer, the old check misses the sentinel if a callable's
   destructor panics during replacement, allowing a second destruction. The new buffer's
   public safe `Value` use must remain sound in this case, even though the port itself
   does not call `replace_stable`. A panic/destruction-count regression test covers it.

No other upstream code is changed or removed.

## Review notes

The upstream code is `unsafe` internally (type erasure needs it); the port's own crates keep
`#![forbid(unsafe_code)]` and use only the safe constructors (`Value::new_stable`) and
`Deref`/`DerefMut`/`Drop`. Reviewed for that use:

- `new_stable` checks that the coercion closure returns the same address and size
  (`check_fat_pointer`), copies the object's bytes into the buffer and `forget`s the
  original; `Drop` runs `drop_in_place` exactly once; `Deref` rebuilds the fat pointer from
  the buffer's start and the stored metadata. Moving a `Value` is a plain move of its
  buffer: the stored object never refers to its own address (Rust values are relocatable).
- The alignment check (`AlignmentValid`) compares `align_of::<U>()` with the element type's
  alignment (16 here), so the object's start, the buffer's start, is suitably aligned.
- `replace_stable`, `Stack` and `Fifo` are not used.
- The metadata is stored through `*const ()` words and rebuilt with a byte copy, preserving
  pointer provenance. Miri checks the new buffer and the portable wrapper's storage,
  observable moves/destruction, and failed-copy tests. This covers the paths the port uses;
  it is not a claim that every public API in the dependency has been independently audited.

`src/embeint_inline.rs` has its own tests (`cargo test --manifest-path
rust/vendor/stack_dst/Cargo.toml`): the layout, a full 32-byte 16-aligned payload stored
inline, moves, odd capacities, pointer-bearing payloads and exactly-once destruction. The
upstream tests (`tests/`) pass unchanged with the relaxed typenum. The new buffer and the
portable wrapper's three storage/lifecycle tests also pass under Miri (nightly 2026-11-26),
including the replacement panic regression.
