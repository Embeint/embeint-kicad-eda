# Object-preserving request helper validation

Tested runtime source: `32b88a6cc47f53f18f5152e955eaf83b5b93a782`, with a clean
oracle export (`overlay_dirty_at_export=false`). This evidence and the notes are
committed afterwards and change no runtime file.

The source is published #43 head `aec2056e8c013464412e0e058ce525fd4f66ee58` plus:

- `5057210bb6`, a merge of #48 head `1ec85a446741c9903963a7282eedddeea06e34e2`
  (runtime `50e3f24874ea430770d59e10aa3db095dec4f757`). #48 has had a clean
  independent review, and the parent authorised integrating it as a reviewed
  stacked dependency. It is **not** merged into rust. Its shared request helper
  builds the `Value` from the exact parse tree, so ordinary objects keyed like
  serde_json's private `Number`/`RawValue` markers stay objects. Finite-number
  checks and the 127-container bound are retained. This topic does not edit that
  helper, the exact comparator or any portable code.
- `32b88a6cc4`, which removes the pending gating. `test_iec60664_inputs.py`, and
  therefore the `oracle-iec60664-inputs` CTest, holds both real processes to
  every fixture set. The `reserved_marker_keys_match_the_shared_oracle_contract`
  shim test is no longer ignored. A new shim test checks that number and raw
  markers at the root, input, unit, case, a step, a binary64 value field and
  nested array elements parse as ordinary one-key objects with their exact string
  contents, including escaped key spellings. None of these requests succeeds.

The fixture bytes did not change: 106 original, 442 nesting and 88 reserved.
`retained-artifacts.json` compares 49 files with `aec2056e`, covering all
fixtures, the corpus, the golden, the baseline evidence, the engine sources, the
unit record and every earlier validation file.

## What ran

`validate.sh` ran in a new private tree, `~/kicad-port/iec-object-preserve`. It
was seeded by copying `~/kicad-port/iec-nesting-bound/repo`, which was read but
not modified. Sources came from a `core.autocrlf=false` archive of the tested
commit. The oracle build and the later steps each took the shared mkdir lock and
released it (the later steps under a trap). Compiles were capped at eight.

- Oracle audit: 824 pinned files, 60 compile commands, `contract=off`, no
  fast-math. All five CTests pass with the default 60 s timeout.
- `cargo fmt --check`; clippy with warnings denied; tests for `kicad-difftest`
  (including #48's request and real C++ transport tests), `kicad-iec60664` and
  `kicad-iec60664-shim` (6 passed, none ignored). The engine wasm build and
  `check_goldens_policy.py --wasm` pass.
- **636 complete envelopes on both actual processes**, with identical replies.
  The raw `-0`, signed-zero bits, unsigned finite bits, 256-step and recovery
  controls also pass.
- 1,801 unit records valid; seven policy tests pass.
- The registered `check_goldens_policy.py --ci` passes all **14,190 live
  cases**, green, with zero deferrals. Oracle output `3051d7e1...` and Rust output
  `819f74f4...` are unchanged. Phase 0 reports **INCOMPLETE** (1/3).

`binary.sha256`: oracle `2ca9833f...` (same C++ source; the embedded overlay
revision differs), shim `68e04538...` and runner `d93f8485...`. Both of the
latter changed because the shared helper changed.

From Windows Git, committed-HEAD golden policy passes against `0ab6a0e8`,
`a7ec77be`, `19035296` and current rust `4c6c1e38`, and `cases.py` and the
fixture generators reproduce the committed bytes.

Holds remain: #48 merging into rust (MCP/bundle CI pending at integration),
corrected #39 acceptance, independent review and hosted CI for this PR.
`PHASE0_REQUIRED` stays false.
