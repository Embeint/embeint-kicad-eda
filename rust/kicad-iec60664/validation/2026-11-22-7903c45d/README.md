# Corrected extractor and nesting-bound validation

Tested runtime source: `7903c45d1cfdad45439608f244f8cfbe2a5a2d70`, with a clean
oracle export (`overlay_dirty_at_export=false`). This evidence and the notes in
the unit record are committed afterwards; they change no runtime file. The source
contains published PR #43 head `6fa797e4cdb687d2200bfded77cca404b65d5bce`,
corrected #39 head `19035296e129cf8640e89331bc9e8ad84f0fe38c` (merge `2a24c88fcd`)
and therefore origin/rust `4c6c1e38ce2dead8db8a3193e7525d19532523e9`: native
parser #45, JSONL Unicode/neutral wording #46 and the C++ 127-open-container
transport bound #47. #39 is still under independent review; this integration is
provisional until the parent accepts it.

## Scope proof

The canonical unit record in #39 `19035296` (blob
`d4983c3c8cd1bbfe65a6889048dbaf833228b96b`) is byte-identical to the historical
provisional `a7ec77be` record. It retains 1,943 physical lines, 28 definitions,
nine files, layer 0, `pure`, empty hard and soft dependencies and no coverage
gaps. The pilot's record differs from it only in `rust_module`, `tests`, `status`,
`claimed_by`, `claimed_at`, `notes` and the `[difftest]` table.

## What ran

`validate.sh` ran detached in a private Linux tree
(`~/kicad-port/iec-nesting-bound`), seeded by copying the retained
`~/kicad-port/iec-request-parser` tree, which was not modified. Rust and port
sources come from a `git -c core.autocrlf=false archive` of the tested commit
(`source-archive.sha256`). The oracle build acquired and released the shared
mkdir lock itself; the later steps took the same lock under a trap. Compiles were
capped at eight jobs.

- Oracle: clean 824-file pin, 60 compile commands, unchanged source, archive-link
  and floating-point (`contract=off`, no fast-math) audit; all five CTests pass.
- Workspace `cargo fmt --check`; clippy with warnings denied and tests for
  `kicad-difftest`, `kicad-iec60664` and `kicad-iec60664-shim` (including the
  real C++ request and nesting fixtures from #45/#47); wasm build of the engine
  and `check_goldens_policy.py --wasm` for every portable crate.
- Both actual processes pass **548 complete refusal envelopes**: the unchanged
  106-fixture contract plus 442 new nesting-bound fixtures. Each process also
  passes raw `-0`, signed-zero bits, unsigned finite bits, 256 steps and recovery.
- `validate_units.py`: all 1,801 unit records and source accounting valid.
- Seven policy tests pass. Their phase-zero COMPLETE/INCOMPLETE lines are
  synthetic fixtures, not a project claim.
- The registered `check_goldens_policy.py --ci` path runs all **14,190 live cases**
  against the actual oracle and committed golden: green, zero deferrals or
  failures. Oracle output (`3051d7e1...`) and Rust output (`819f74f4...`) hashes
  equal the f777c57b proof; the oracle binary changed because #47 changed the
  transport. No propose or bless ran. Phase 0 reports **INCOMPLETE**
  (`phase0-summary.json`; the 3.4 MB full report is identified by its hash).

`binary.sha256` holds the tested oracle, shim and runner identities.
`retained-artifacts.json` shows the corpus, golden, baseline evidence, original
input fixtures, engine sources and every earlier validation file are byte-identical
to `6fa797e4`.

## Nesting fixtures

Nested values with scalar, string and empty-container leaves sit in the input, a
step field, a top-level field, an unregistered unit's input, each identity, and a
step field before the identities. Array, object and both alternating shapes
are checked at 127 and 128 open containers; the array shape also at 126 and 129.
Escaped duplicate keys at the last accepted level remain refused. Expectations
follow from the constructed depth. Accepted requests reach the unchanged envelope
and adapter checks with their identities; refused requests give the null-identity
parse error. The comparator, numeric handling and domain checks are unchanged.

## Superseded attempt

A first run extracted a plain `git archive`, which applied the Windows checkout's
CRLF conversion to the case corpus; its live check passed but hashed the CRLF
file (`superseded-crlf-attempt.txt`). It is not used as evidence; the run above
repeated every step on exact committed bytes.

Golden policy against committed HEAD passes with bases `0ab6a0e8`, `a7ec77be` and
`19035296`, and `cases.py` reproduces the committed corpus byte for byte (both
checked from Windows Git after this commit). Independent review of #39, then
this PR's own review and hosted CI, must pass before merge. `PHASE0_REQUIRED`
stays false.
