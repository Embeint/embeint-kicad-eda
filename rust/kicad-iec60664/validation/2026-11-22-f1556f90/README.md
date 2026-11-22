# Pending reserved marker key validation

Tested runtime source: `f1556f9080971a8a34d87a5cc28baf081853a8bc`, with a clean
oracle export (`overlay_dirty_at_export=false`). It is the
[7903c45d validation](../2026-11-22-7903c45d/README.md) source plus test-only
changes: `port/oracle/iec60664-reserved-key-guards.jsonl` (88 raw-literal
fixtures), their generator and Rust-side wiring in `test_iec60664_inputs.py`, and
an ignored shim test. No runtime code, comparator, corpus, golden or earlier
evidence changes (`retained-artifacts.json` compares 37 files with `1c2c3aaf`).
The 7903c45d evidence remains the record for that source.

## Why the set is pending

Independent review of #42 found that `kicad_difftest::request::parse` lets
serde_json read ordinary objects keyed `$serde_json::private::Number` or
`$serde_json::private::RawValue` as its private markers. The pinned C++ transport
and adapter see plain objects. The parent will fix the shared helper in a
separate reviewed PR, and this topic does not edit it. The fixtures hold the
C++ expectations, which the oracle passes now. The Rust process is held to them
only with `--include-pending-reserved` and the ignored
`reserved_marker_keys_match_the_shared_oracle_contract` test. Enable both when
the shared fix is adopted.

Against the current helper the Rust shim diverges on **50 of 88** fixtures. In
25 of those it returns a false success; the other 25 are parse or numeric
refusals where C++ returns an adapter domain error. The fixtures cover number and
raw markers in setter, groove, enum and boolean values, and in op, field, step,
steps, input, unit, case and the root. They include escaped-key, malformed,
overflowing, deep, extra-key and near-miss variants, and markers that both sides
already treat alike (step/input siblings, top-level extra, unknown unit).

## What ran

The same `validate.sh` flow as 7903c45d, in the same private tree, from a
`core.autocrlf=false` archive of the tested commit. The oracle build took the
shared mkdir lock and released it, and so did the later steps, under a trap. A
first attempt stopped at an informational divergence step because of a quoting
error in that step; its earlier steps had passed. The run recorded here repeated
every step.

- Oracle audit (824 pinned files, 60 compile commands, FP flags) and all five
  CTests pass. `oracle-iec60664-inputs` now includes the reserved set.
- fmt, clippy with warnings denied, and tests for `kicad-difftest`,
  `kicad-iec60664` and `kicad-iec60664-shim` pass, with the reserved test
  reported as ignored. The engine wasm build and `--wasm` policy also pass.
- Oracle: **636** complete envelopes (106 + 442 nesting + 88 reserved). Rust
  shim: **548** (106 + 442), and its replies to those and to the controls equal
  the oracle's.
- 1,801 unit records valid, seven policy tests pass, and the registered live
  check passes **14,190** cases with zero deferrals. Its output hashes match
  7903c45d. The shim binary hash (`16fc6163...`) is also unchanged. The oracle
  hash differs because its embedded provenance names the new overlay revision.
  Phase 0 reports **INCOMPLETE**.

The #43 merge hold remains: the shared reserved-key parser fix, corrected #39
acceptance, independent review and hosted CI. `PHASE0_REQUIRED` stays false.
