# Clipper2 core and engine header shim

Native JSONL adapter for `kicad-clipper2`, the port of unit
`thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p001`
(`clipper.core.h`, `clipper.engine.h`). It answers the same requests as the genuine pinned
C++ adapter `port/oracle/shims/clipper2_core_engine.cpp`, which compiles the unmodified
headers with KiCad's public `USINGZ` definition and links the pinned `clipper2` archive.

## Protocol

`{"op": ..., ...}` inside the shared envelope. `int64_t` values are JSON integers; doubles
are their 16 lowercase hex digit bit patterns, so NaN payloads, signed zeros and infinities
are compared exactly; points are `[x, y, z]`, rectangles `[left, top, right, bottom]`.
Typed operations take `"type": "i64" | "f64"`; conversions take `"to"` and `"from"`.
A `Clipper2Exception` thrown by `DoError` is a result, `{"thrown": what, ...}` with any
reference parameter (`error_code`, `precision`) it updated first. `std::bad_function_call`
is `{"thrown": "std::bad_function_call"}`. Pointers never reach the output: engine records
report `*_null`/`*_self`/`*_arg` identities and `PolyPath` nodes are numbered by creation.

## Evidence

- `generate_cases.py` writes the public fixed-seed corpus
  (`port/cases/<unit>.jsonl`) and the undefined-behaviour controls
  (`port/oracle/fixtures/clipper2-core-engine-controls.jsonl`); `--check` verifies both.
  Controls cover signed overflow, out-of-range `double` to `int64_t`/`__int128_t`
  conversions (including each `ClipperD` `Add*` group scaled by `clipperd_execute`), the pinned
  `StripeIndex` arithmetic, out-of-range `PolyPath` indexing and use of destroyed nodes. Both
  programs must refuse them identically (`compare_controls.py`); they never enter the corpus.
- `clipper64_execute` hands caller coordinates to the pinned engine, whose undefined cases
  belong to the engine source units. Both programs run it only within Clipper2's declared
  range `MIN_COORD..MAX_COORD` and refuse other coordinates with
  `outside the guarded engine domain` (a harness limit, also recorded as controls). The
  `StripeIndex` guard in this shim mirrors the adapter's and computes no result; the port
  still defers `StripeIndex::BuildFromPath`.
- Trees are built under the host `Budget` `TREE_BUDGET` (the root plus one node per
  operation, 2^22 points); Execute replies carry the adapter's `"tree"` serialization.
- `kicad-difftest compare --partial` compares every corpus case with the live oracle exactly.
  Deferred cases name `ClipperBase::AddPaths`/`StripeIndex::BuildFromPath`
  (`...src.thirdparty.pure.p001`) or `ClipperBase::ExecuteInternal` (`...p002`).
- `examples/corpus_parity.rs` replays the corpus and controls in import-free WebAssembly and
  requires the newly executed native replies (`.github/workflows/rust-clipper2-live.yml`).
