<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# kicad-poisson-shim

Native JSONL adapter for unit `libs.kimath.src.geometry.poisson_disk.application.pure.p001`
(`POISSON_DISK::ToroidalUnitTile`, ported in `kicad_math::poisson_disk`). It mirrors
`port/oracle/shims/kimath_src_poisson_disk.cpp` field for field, with the same validation order
and refusal messages. IEEE doubles travel as 16 lower-case hex digits of their bits.

## Operations

`tile`: `{"op":"tile","min_dist":<bits>,"seed":<u32>,"max_grid_cells":<0..4194304>,
"max_samples":<0..4194304>,"emit":"samples"|"digest"}`. Result `{"count":N,"samples":[[x,y],...]}`
in the function's order, or `{"count":N,"digest":<FNV-1a 64 over every sample's x then y bits,
little-endian>,"first":[...],"last":[...]}` with up to eight samples at each end. Refusals
(`invalid_input`):

| Message | Contract |
| --- | --- |
| `undefined grid size` | `aMinDist` passes the early-return guard (NaN does) and `ceil( sqrt( 2 ) / aMinDist )` is not below 2^31, so the reference's `(int)` conversion is undefined |
| `grid limit` | `gridN * gridN` exceeds `max_grid_cells` |
| `sample limit` | the tile has more than `max_samples` samples (the port stops there; the C++ adapter refuses from the reference's count) |

The early-return domain (`aMinDist <= 0` or `> 0.5`) returns an empty tile without consulting
the limits, as upstream does. Neither side returns a partial tile.

`rng`: `{"op":"rng","seed":<u32>,"draws":[spec,...]}` with up to 64 specs and 65,536 draws in
total from one `mt19937`, in order: `{"kind":"u32","count":n}` (engine outputs),
`{"kind":"int","min":"<u64>","max":"<u64>","count":n}` (`uniform_int_distribution<size_t>`,
decimal strings) and `{"kind":"real","min":<bits>,"max":<bits>,"count":n}`
(`uniform_real_distribution<double>`, finite `min < max` only). This checks the Boost.Random
port directly against the pinned Boost headers, beyond the ranges the tile reaches.

## Evidence

- `generate_cases.py` writes the public corpus (inputs only) and the refusal controls
  `port/oracle/fixtures/poisson-disk-controls.jsonl`; `--check` verifies both are current.
- `kicad-difftest compare --partial` runs the corpus on the oracle and this shim.
- `compare_controls.py` runs every control on both programs in one process each (every refusal
  identical and followed by a successful continuation) and probes exact sample limits.
- `examples/corpus_parity.rs` replays the corpus and the controls in import-free wasm32 against
  this revision's native C++ replies (`KICAD_POISSON_WASM_REFERENCE`), run by `wasm-test.mjs`.
