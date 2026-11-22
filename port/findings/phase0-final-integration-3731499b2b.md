<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Phase zero final integration: final resistor head proof

Tested clean runtime: `3731499b2b0cacc2fa573d68994f81eb34a99522`, which merges
the final published #44 head `19e8384772bba1d02fc07889335f6656a354892e` (clean
resistor runtime `ff419ad71f`) into the integration already recorded in
[phase0-final-integration-d0cf06863c.md](phase0-final-integration-d0cf06863c.md).
The method, scripts, layers and resolutions are those of that record; this one
gives the results for the final head. Records are committed afterwards and change
no runtime file.

The strict gate stays off (`PHASE0_REQUIRED=false`), the resistor record stays
`claimed` and MD5/IEC60664 keep their provisional green records and graph hold
notes, pending parent acceptance of #39 (a parent fix of the generator capture
guard is in progress) and parent review of the pilots. No phase-zero claim is made.

## What the final head adds

#44 now carries the merged #48 request helper itself (its `rust/kicad-difftest`
is identical to `rust`), 889 literal serde private marker key fixtures with the
default `oracle-resistor-private-keys` CTest, Rust process replays in debug and
release, and the 960-reply combined request check (the original 71 as a byte
prefix). Its 1050 cases, goldens and evidence are unchanged. The only conflict
was the CMake test list; all tests of the three families are kept.

## Identity

`layer-identity.json`, with #44 = `19e8384772`: 2530 paths hold their owning
layer's blob. The resolved paths are the same ten (six shared files, four pilot
records); `test_iec60664_guards.py` is the one deliberate change; the only paths
without a pilot owner are the earlier integration records of this branch. All
1801 records keep #39's protected fields (1797 byte identical); the resistor
record is #44's published bytes and still `claimed`. Golden change policy is
valid against `rust`, #39, #42, #43 and the final #44 head.
`guard-mutations.log`: all four guard negative controls remain effective.

Raw export (`pin-identity.json`, `head-identity.json`): all 915 pinned reference
files equal their Git blobs; of 24124 HEAD files only
`thirdparty/argparse/tools/build.bat` differs, by its own CRLF attribute.

## Checks (WSL, own tree `~/kicad-port/phase0-final-3731499b2b`)

| Check | Result |
| --- | --- |
| Oracle audit | 915 pinned files, 75 compilations, contraction off, no fast-math, all archive links |
| CTest | 8/8 passed, default 60 s, including `oracle-resistor-private-keys` |
| Workspace fmt, clippy (warnings denied), tests | passed; 147 tests, none failed or ignored |
| Resistor shim tests in release | passed, including both private key replays |
| Portable wasm | embeint-layout, embeint-web, kicad-iec60664, kicad-md5, kicad-resistors |
| Resistor wasm parity probe | 1050 results agree with the C++ golden in Node |
| Policy tests | 7 passed (their synthetic COMPLETE is not project evidence) |
| Full source validator | 1801 records valid, no errors |
| Corpus regeneration | MD5 5973 and resistor 1050 in WSL; IEC60664 14190 from Windows Git against the blob |
| MD5 envelopes, both adapters | 194 complete refusals and 5 successes |
| IEC60664 envelopes, both adapters | 636 complete envelopes, raw -0, u64 bits, recovery |
| Resistor, oracle and debug/release shims | 7 collector refusals and the empty result; 889 private key envelopes (64 results, 825 refusals) byte-identical on the oracle and matched on both shims; 960 combined replies |
| Registered live check | **21213 cases** (5973 + 14190 + 1050), zero deferrals, 85 s |

The selected-package envelope copies and the registered executables are
identical, and each report's `rust_sha256`/`oracle_sha256` is the executable that
ran. Debug shim hashes differ from the preparation run even for unchanged
sources because debug information embeds the work-tree path; the release
resistor shim (`e5c6ba21...`) is identical in both runs.

| Unit | Oracle output = golden | Rust output |
| --- | --- | --- |
| MD5 | `9b8b9a76...` | `709084cd...`, unchanged |
| IEC60664 | `3051d7e1...` | `819f74f4...`, unchanged |
| Resistor | `8d1cc9e8...` | `5fb5dfb6...`, unchanged |

Oracle `79856288...` (embedded overlay revision is this runtime), provenance
`7de6a11b...`, source manifest `c7ec2c6d...`. GCC 15.2.0 Release x86-64, Rust
1.99.1, Node 22.22.1; the resistor NaN qualification remains GCC 13/15 Release
x86-64 only.

`reports/phase0.json`: **INCOMPLETE, 2/3** live green layer-0 units, 1801
records. Extraction coverage stays visible and is not claimed complete: parsing,
all configurations and the WSL gate incomplete; 4 parse failures; 63 uncompiled
application sources; 8,899,609 unresolved calls.

## Remaining holds

Parent acceptance of #39 after its generator capture guard fix, parent review of
#42, #43 and #44, their merge into `rust` and hosted CI. Then `rust` is merged
here, the resistor record and maintained notes are updated, `PHASE0_REQUIRED`
becomes true and the strict gate runs with `--require-phase0`.
