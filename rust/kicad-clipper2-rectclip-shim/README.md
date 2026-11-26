# Clipper2 rectangle clipping shim

Native JSONL adapter for `kicad_clipper2::rectclip`, the port of unit
`thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p003`
(`thirdparty/clipper2/Clipper2Lib/src/clipper.rectclip.cpp`). It answers the same requests
as the C++ adapter `port/oracle/shims/clipper2_rectclip.cpp`.

## Protocol

`{"op": ..., ...}` inside the shared envelope. Points are `[x, y, z]` integers (`USINGZ`),
rectangles `[left, top, right, bottom]`, locations the `Location` enumerator values 0..4.

- `rect_clip`, `rect_clip_lines`: `{"rect", "calls": [paths, ...]}` constructs one
  `RectClip64`/`RectClipLines64` and calls `Execute` once per entry of `calls`;
  the reply is `{"results": [paths, ...]}`.
- The file's helpers, each with its own fields: `get_location`, `is_horizontal`,
  `get_segment_intersection`, `get_intersection`, `get_adjacent_location`,
  `heading_clockwise`, `are_opposites`, `is_clockwise`, `get_edges_for_pt`,
  `is_heading_clockwise`, `has_horz_overlap`, `has_vert_overlap`, `path1_contains_path2`,
  `start_locs_are_clockwise`. Reference parameters (`loc`, `ip`) are passed in and returned.

Requests carry at most 65536 points and 64 calls in both programs.

## Where the oracle's answers come from

- `rect_clip`/`rect_clip_lines` run the genuine `RectClip64`/`RectClipLines64` of the
  pinned clipper2 archive; `get_segment_intersection` and `start_locs_are_clockwise` call
  the archive's exported functions.
- The other helpers are `inline` in the source file, so the archive need not emit them.
  The adapter calls them in `port/oracle/reference/clipper2_rectclip_copy.cpp`, a second
  compilation of the same unmodified pinned source with the same flags and `USINGZ`, with
  namespace `Clipper2Lib` renamed in that translation unit only. This is the pinned code,
  compiled again; it is not the archive's object code.
- Undefined behaviour is decided by the pinned source itself. Every request first runs in a
  forked child on `port/oracle/reference/clipper2_rectclip_guard.cpp`, the same source
  compiled with UBSan in trap mode (signed overflow including `__int128_t`,
  float-cast-overflow, bounds, null, ...) and `_GLIBCXX_ASSERTIONS` (`std::vector`
  subscripts). A trap there is `undefined behaviour: <op>`; a replay over 20 s or 4 GiB is
  `harness limit: <op>`. No coordinate range is fenced off: inputs anywhere in `int64_t`
  run if the source is defined on them.

The Rust port refuses at the same expressions (`ClipperError::Undefined`) and maps its
`RectClipBudget` limit (`ResourceLimit`) to `harness limit: <op>`.

## Evidence

- `generate_cases.py` writes the public fixed-seed corpus (`port/cases/<unit>.jsonl`) and
  the refusal controls (`port/oracle/fixtures/clipper2-rectclip-controls.jsonl`);
  `--check` verifies both. Controls never enter the corpus.
- `kicad-difftest compare` compares every case exactly; the unit has no deferrals.
  `compare_controls.py` requires identical refusals from both programs.
- `examples/corpus_parity.rs` replays the corpus and controls in import-free WebAssembly
  against the newly executed native replies (`.github/workflows/rust-clipper2-rectclip-live.yml`).
