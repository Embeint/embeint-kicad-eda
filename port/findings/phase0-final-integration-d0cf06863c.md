<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Phase zero final integration: preparation proof

Tested clean runtime: `d0cf06863cf64eb07b11787d1a6dd2826c04d47e`. These records
are committed afterwards and change no runtime file (`port/oracle` is untouched,
so the embedded overlay hashes stay those of the tested revision).

This is **preparation**. The strict gate stays off (`PHASE0_REQUIRED=false`),
the resistor record stays `claimed`, and no phase-zero claim is made. MD5 and
IEC60664 keep their published provisional green records and graph hold notes.

## Layers

| Layer | Head | Role |
| --- | --- | --- |
| rust | `1a2fb924d539d63a68f5696191907dc2aa6d6370` | base, includes merged #45, #46, #47, #48 |
| #39 | `d2bdddb904116348e474389dd577cb99ac0c8790` | corrected V5 graph, generator refresh, 1801 records |
| #42 | `c5ec29e70d07506ac3fb8b8b5c1b6f5102c7ef3e` | MD5 pilot |
| #43 | `fc4718302716baf33e9b764e3bce30c2727f2706` | IEC60664 pilot |
| #44 | `084115f06cb1cb73fe6bc04f90f715d3384cfb3e` | resistor pilot, current published head for preparation |

Each was merged with a merge commit in that order; no history was rewritten.
The final #44 head replaces `084115f06c` by a further merge before final
validation.

## Resolutions

Only places where pilots added entries side by side conflicted. All are unions:

- `port/oracle/registry.cpp`: `RegisterMd5`, `RegisterIec60664`, `RegisterResistors`.
- `port/oracle/CMakeLists.txt`: the actual `kimath`, `iec60664-reference`,
  `resistor-reference`, `fmt` and `calculator-reader-reference` targets, with
  the reader's generated lexer, `--gc-sections` from #44 and the audit
  dependency of every target. CTests: protocol, both build-guard suites,
  dispatch, MD5, IEC60664 and resistor inputs.
- `port/oracle/pin.py`: the union of the source paths; **915** pinned files
  measured (#44 alone 913, #43 alone 824).
- `port/oracle/audit.py`: the whole eight-target closure, the IEC60664
  source-identity check, the kimath, IEC60664, resistor/fmt and reader archive
  link checks, and the byte-checked regenerated lexer.
- `rust/Cargo.toml`, `rust/Cargo.lock`: the existing workspace plus the three
  portable engines and three native shims.
- `rust/kicad-difftest`: #44's side was only its older rust base (it has no
  changes of its own there), so the merged #48 helper and tests are kept.

`port/oracle/test_iec60664_guards.py` is the one non-merge change. Its fixture
described the single-pilot closure, so after the union two checks failed and the
missing-compilation check passed only because other targets were absent. The
fixture now supplies the complete other closures, and a new check requires the
other closures while IEC60664 is present. `guard-mutations.log` shows each
negative control fails when its audit check is removed (IEC60664 target, source
identity and archive link; resistor target).

## Identity

`layer-identity.json` (from `scripts/identity_proof.py`) classifies every path
by the layer whose blob differs from all bases it could inherit from:

- 2505 paths hold exactly their owning layer's blob; none changed without an owner.
- Resolved paths: the six shared files above and the four pilot records
  (MD5, IEC60664, resistor pure, GetReader thin owner). Each record is byte
  identical to its pilot's published record; the #39 records for them did not
  change between `19035296e1` and `d2bdddb904`.
- All 1801 records keep every protected field of #39; 1797 are byte identical.
  The thin owner differs only in its GetReader co-port notes, still `todo`.
- All cases, goldens, golden evidence, guard fixtures and validation records of
  the three families are their published bytes.

## Byte policy

On this Windows host the repository's `text=auto` attributes follow `core.eol`,
which is CRLF natively. `core.autocrlf=false` alone still exported 8017 HEAD
files and 877 of the 915 pinned files with CRLF. `scripts/export.py` therefore
runs Git (including the unchanged `prepare.py`) with `core.autocrlf=false` and
`core.eol=lf`, as a Linux checkout does. `pin-identity.json`: all 915 exported
reference files equal their Git blobs, none missing or extra.
`head-identity.json`: of 24073 HEAD files only
`thirdparty/argparse/tools/build.bat` differs, by its own CRLF attribute; it is
outside the pin and the build. Earlier pilot evidence retains its recorded
exports; it is not rewritten. Here the oracle outputs of all three families equal
the committed goldens under raw sources.

## Checks (WSL, own tree `~/kicad-port/phase0-final-d0cf06863c`)

The tree was extracted from the raw archive; the pinned solidcore checkout
`7a588fa2` and Cargo cache were copied read-only from `md5-corrected-graph`.
The oracle build and every later step held the shared mkdir lock (trap
released); compiles at most eight. GCC 15.2.0 Release x86-64, Rust 1.99.1,
Node 22.22.1. `run.log` is the full log.

| Check | Result |
| --- | --- |
| Oracle audit | 915 pinned files, 75 compilations, contraction off, no fast-math, all archive links |
| CTest | 7/7 passed, default 60 s |
| Workspace fmt, clippy (warnings denied), tests | passed; 135 tests, none failed or ignored |
| Portable wasm | embeint-layout, embeint-web, kicad-iec60664, kicad-md5, kicad-resistors |
| Resistor wasm parity probe | 1050 results agree with the C++ golden in Node |
| Policy tests | 7 passed (their synthetic COMPLETE is not project evidence) |
| Full source validator | 1801 records valid, no errors |
| Corpus regeneration | MD5 5973 and resistor 1050 in WSL; IEC60664 14190 from Windows Git against the blob (`iec-corpus-check.log`) |
| MD5 envelopes, both adapters | 194 complete refusals and 5 successes |
| IEC60664 envelopes, both adapters | 636 complete envelopes, raw -0, u64 bits, recovery |
| Resistor envelopes, oracle, debug and release shims | 7 collector refusals and the empty result; 71 complete replies |
| Registered live check | **21213 cases** (5973 + 14190 + 1050), zero deferrals, 85 s |

Each shim was built as a selected package before its checks; the registered run
rebuilt each again. The envelope copies (`adapters.sha256`) and the registered
executables (`registered-binaries.sha256`) are identical, and every report's
`rust_sha256` and `oracle_sha256` equals the executable that ran.

| Unit | Oracle output = golden | Rust output |
| --- | --- | --- |
| MD5 | `9b8b9a76...` | `709084cd...`, as in its earlier evidence |
| IEC60664 | `3051d7e1...` | `819f74f4...`, as in its earlier evidence |
| Resistor | `8d1cc9e8...` | `5fb5dfb6...`, as in its earlier evidence |

Oracle `e5b01380...` (its embedded overlay revision is this runtime), provenance
`883d79ee...`, source manifest `e5c8c171...`.

`reports/phase0.json`: **INCOMPLETE, 2/3** live green layer-0 units (resistor
claimed), 1801 records. The extraction coverage stays visible and is not
claimed complete: inventory and compile database complete; parsing, all
configurations and the WSL gate incomplete; 4 parse failures; 63 uncompiled
application sources; 8,899,609 unresolved calls. The resistor NaN qualification
remains as recorded: GCC 13/15 Release x86-64 only, not GCC O0 or Clang 18.

## Remaining holds

Acceptance of the running independent #39 review, the final published #44 head,
parent review of #42, #43 and #44, and hosted CI. After the parent confirms them,
the final head is merged, the resistor record and the maintained notes are
updated and `PHASE0_REQUIRED` becomes true; the strict gate then runs here
with `--require-phase0`.
