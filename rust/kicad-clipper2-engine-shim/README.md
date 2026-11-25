# Clipper2 engine source shim (clipper.engine.cpp lines 1-2271)

Native JSONL adapter for `kicad_clipper2::engine_p001`, the port of unit
`thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p001`. It answers the same requests as the
pinned C++ adapter `port/oracle/shims/clipper2_engine_p001.cpp`.

## The oracle

Most of these bodies are private `ClipperBase` members or file-local `inline`/`static`
functions, which no other translation unit can call. The C++ adapter therefore compiles the
pinned, unmodified `clipper.engine.cpp` into its own translation unit with the identifier
`Clipper2Lib` renamed (so its symbols never meet the linked archive's), with the oracle's
`-fno-fast-math -ffp-contract=off` and KiCad's public `USINGZ`. Private members are reached
through explicit template instantiation, which C++ exempts from access checks. Calls into lines
2272 onward (`CheckJoinLeft`, `CheckJoinRight`, `SwapPositionsInAEL`, `Split`) run the real C++
there; the Rust port returns typed deferrals to unit `...src.thirdparty.pure.p002`.

## Protocol

`{"op": ...}` inside the shared envelope. Points are `[x, y, z]`, doubles 16 lowercase hex
digits.

- `stripe_index` (`source` `path`|`ring`, `path`, `queries`, `ys`): builds a `StripeIndex`
  and reports its private members, the queries and `YToStripe`.
- Pure helpers: `get_dx`, `top_x`, `area_triangle`, `pts_really_close`, `is_odd`,
  `intersect_list_sort`, `loc_min_sorter`, `scanline`.
- `engine` (`steps`): up to 512 steps on one `ClipperBase` and one
  `ReuseableDataContainer64`, then the whole state: vertex arrays, minima, the minima iterator,
  flags, scanlines, AEL, SEL, every edge, every output record and ring, and the Z callback
  calls (the recorder sets `pt.z = base + calls`). Edges are handles in registration order
  (`new_active`, then edges newly reachable from `actives_` and `sel_` after each step);
  output records are list positions; vertices are `["b"|"r", list, index]`.

Steps: `add`, `add_path`, `add_reuseable_data`, `clear`, `clean_up`, `reset`, `settings`,
`insert_scanline`, `pop_scanline`, `pop_local_minima`, `insert_local_minima`, `new_active`,
`set_active`, `insert_left_edge`, `insert_right_edge`, `push_horz`, `pop_horz`,
`extract_from_sel`, `insert_before_sel`, `wind_closed`, `wind_open`, `contributing`,
`valid_ael_order`, `local_min_poly`, `local_max_poly`, `join_outrec_paths`, `add_out_pt`,
`start_open_path`, `intersect_edges`, `swap_outrecs`, `uncouple`, `prev_hot_edge`,
`matching_loc_min`, `maxima_pair`, `curr_y_maxima`, `next_vertex`, `prev_prev_vertex`,
`trim_horz`, `update_edge`, `top_x`, `set_z`, `edges_adjacent`, `new_outrec`, `set_path`,
`set_owner`, `real_outrec`, `valid_owner`, `clean_collinear`, `fix_self_intersects`,
`do_split_op`, `area`, `point_count`, `very_small_triangle`, `swap_sides`,
`update_outrec_owner`, `dispose_out_pts`, `clean_path`, `valid_closed_path`,
`reverse_out_pts`, `invalid_path`, `duplicate_op`, `dispose_out_pt`, `point_in_op_polygon`,
`contains`, `contains_indexed`, `horz_seg_sorter`.

## Refusals

Both programs refuse, with identical envelopes:

- Undefined behaviour in the reference (`undefined behaviour: <op>`): signed overflow and
  out-of-range conversions (precise guards in the pure helpers, `stripe_index`, `new_active`
  and `top_x`), null dereferences and loops that never end.
- Harness rules (`invalid input: ...`): unknown or deleted handles, a stale minima iterator,
  clearing a container a clipper still borrows, list misuse that would corrupt the AEL/SEL,
  self ownership, short rings, inconsistent output records, and the engine coordinate domain:
  steps that do coordinate arithmetic run only while every vertex and output point lies in
  Clipper2's declared `MIN_COORD..MAX_COORD`. Adding paths, `Reset`, minima and scanline
  steps only compare coordinates and accept the whole `int64_t` range.

## Evidence

- `generate_cases.py` writes the corpus (`port/cases/<unit>.jsonl`) and the controls
  (`port/oracle/fixtures/clipper2-engine-p001-controls.jsonl`); `--check` verifies both.
- `kicad-difftest compare --partial` compares every case exactly; `compare_controls.py`
  requires identical refusals; `examples/corpus_parity.rs` replays both in import-free wasm.
