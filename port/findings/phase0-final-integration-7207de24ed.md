<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Phase zero final integration: reviewed #39 and #44 heads

Tested clean runtime: `7207de24edea9f509e5e4e243418f1488a71cc77`. It continues
the published integration `8928097f950c0113e63bc46910c6221cefe1a89d` (still an
ancestor) with two normal merges:

| Merge | Layer head | Previously integrated |
| --- | --- | --- |
| `8cf2d84a3e` | #39 `328572624e382ad8c55bd1d45b207b740c2c5258` | `d2bdddb904` |
| `7207de24ed` | #44 `2c0817d575b26c0660a6ce0ad497fc3ee6cc3a6e` | `19e8384772` |

#42 `c5ec29e70d` and #43 `fc47183027` stay as already integrated. This record is
committed after the tested runtime and changes no runtime file. The method is
the one in [phase0-final-integration-d0cf06863c.md](phase0-final-integration-d0cf06863c.md),
extended by the extractor suite, the escaped-key generator check and the guard
negative controls run in the LF tree. Earlier records, scripts and proof roots
are unchanged; this proof used new script, log and root names.

The strict gate stays off (`PHASE0_REQUIRED=false`), the resistor record stays
`claimed`, and MD5/IEC60664 keep their provisional green records and graph hold
notes. No phase-zero claim is made.

## What the two heads add

#39 adds the generator capture guard and guarded recapture, the binding of
generator queries to the verified Ninja executable and the freezer fix that
keeps tool symlink invocation names (37 extractor tests). Its retained capture
evidence (runtime `c1e8a467da`, refresh identity `2b4eca5d...`, graph,
inventory, manifest and the D2 ownership maps) is integrated as its published
blobs. It was not re-extracted, reparsed or rebound to the later freezer code.

#44 adds 199 literal escaped marker key requests and replies to the default
private key guard (`oracle-resistor-private-keys` CTest) and the Rust process
test, and the export qualification addendum. Its original 889 fixtures and the
960-reply prefix of the combined check are unchanged.

## Identity

Both merges were clean. `supplemental-identity.json`: all 112 paths #39 changed
since `d2bdddb904` and all 17 paths #44 changed since `19e8384772` hold the
owners' exact blobs. The two sets do not overlap, and no other path of the
published integration changed. That covers the 58 earlier integration evidence
paths, the six shared unions (registry, CMake, pin, audit, Cargo manifest and
lock) and the corrected IEC60664 guard fixture.

`layer-identity.json` uses #39 = `328572624e` and #44 = `2c0817d575`:

- 2638 paths hold their owning layer's blob.
- The resolved paths are the same ten as before: six shared files and four pilot records.
- `test_iec60664_guards.py` is the one deliberate change.
- The only paths without a pilot owner are this branch's earlier integration records.

All 1801 unit IDs equal #39's. 1797 records are byte-identical to #39, and no
protected field differs. The other four (MD5, IEC60664, resistor and the
GetReader thin owner) are their owners' published bytes. The resistor record is
still `claimed`. Golden change policy is valid against `rust`, #39, #42, #43, #44
and the published integration head.

Raw export (`pin-identity.json`, `head-identity.json`) used `core.autocrlf=false`
and `core.eol=lf`. All 915 pinned reference files equal their Git blobs, none
missing or extra. Of 24264 exported files, only `thirdparty/argparse/tools/build.bat`
differs, by its own CRLF attribute.

## Checks (WSL, own tree `~/kicad-port/phase0-final-cont-7207de24ed`)

The tree was extracted from the raw archive. The pinned solidcore checkout and
the Cargo target cache were copied read-only from `md5-corrected-graph`, so Cargo
rebuilt only what changed. The oracle was built fresh in a new directory (73
objects). The oracle build held the shared mkdir lock through `build.sh`, and
the later steps held it themselves; it was released afterwards. Compiles were
capped at eight. GCC 15.2.0 Release x86-64, Rust 1.99.1, Node 22.22.1, extractor
Python 3.14.4 with libclang. `run.log` is the full log.

| Check | Result |
| --- | --- |
| Oracle audit | 915 pinned files, 75 compilations, contraction off, no fast-math, all archive links |
| CTest | 8/8 passed, default 60 s, including `oracle-resistor-private-keys` with the escaped fixtures |
| Workspace fmt, clippy (warnings denied), tests | passed; 149 tests in 34 suites, none failed or ignored |
| Resistor shim tests in release | passed, including the private and escaped key replays |
| Portable wasm | `check_goldens_policy.py --wasm` passed |
| Resistor wasm parity probe | 1050 results agree with the C++ golden in Node |
| Policy tests | 7 passed (their synthetic COMPLETE is not project evidence) |
| Extractor suite | 37 passed, none skipped, including `test_freeze_cli_preserves_tool_symlink_invocation_names` |
| Full source validator | 1801 records valid, no errors |
| Generators | MD5 5973 and resistor 1050 regenerate exactly; 199 escaped-key requests reproduce; IEC60664 14190 from Windows Git against the blob (`iec-corpus-check.log`) |
| Guard negative controls | all four remain effective (`guard-mutations.log`) |
| MD5 envelopes, both adapters | 194 complete refusals and 5 successes |
| IEC60664 envelopes, both adapters | 636 complete envelopes, raw -0, u64 bits, recovery |
| Resistor, oracle and debug/release shims | 7 collector refusals and the empty result; 1088 literal private key envelopes (889 + 199; 65 results, 1023 refusals) on the oracle and both shims; 1159 combined replies |
| Registered live check | **21213 cases** (5973 + 14190 + 1050) at the default per-entry budget, including the 512 MiB MD5 carry case; zero deferrals or failures; 86 s |

Each shim was built as a selected package before its envelope checks. The
envelope copies (`adapters.sha256`) and the registered executables
(`registered-binaries.sha256`) are identical. Each report's `rust_sha256` and
`oracle_sha256` is the executable that ran.

| Unit | Oracle output = golden | Rust output |
| --- | --- | --- |
| MD5 | `9b8b9a76...` | `709084cd...`, unchanged |
| IEC60664 | `3051d7e1...` | `819f74f4...`, unchanged |
| Resistor | `8d1cc9e8...` | `5fb5dfb6...`, unchanged |

Oracle `426c476c...`; its embedded overlay revision is this runtime. Provenance
`bbbee8d6...`. These binaries belong to `7207de24ed`, not to earlier runtimes.
The release resistor shim (`e5c6ba21...`) is byte-identical to the one in the
earlier integration and #44 runs. Debug hashes differ because debug information
embeds the work-tree path. The resistor NaN qualification remains GCC 13/15
Release x86-64 only; nothing is claimed for GCC O0 or Clang.

`reports/phase0.json`: **INCOMPLETE, 2/3** live green layer-0 units, 1801
records. Extraction coverage stays visible and is not claimed complete:

- parsing, all configurations and the WSL gate are incomplete;
- 4 parse failures;
- 63 uncompiled application sources;
- 8,899,609 unresolved calls.

## Evidence bytes

`files.sha256` verifies the committed bytes of every file in this directory.
Reports written on Windows were normalized to LF before hashing. Their content
describes other files, not themselves. The tested source manifest is kept only
as `source-manifest.original.json.gz` (`gzip -n`, byte-exact, CRLF as
`prepare.py` wrote it on Windows). It decompresses to SHA-256 `0774d3fe...`,
which is the identity in the live oracle provenance, `summary.json` and
`pin-identity.json`. The 915 reference files themselves are raw Git bytes.

## Remaining holds

The parent's acceptance of #39 and of the pilots, their merge into `rust`, and
hosted CI. Then `rust` is merged here, the resistor record and maintained notes
are updated, `PHASE0_REQUIRED` becomes true and the strict gate runs with
`--require-phase0` after a final combined proof and review.
