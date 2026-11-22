<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Phase zero strict-gate candidate on the merged rust base

**Prospective, private validation.** This is not the hosted strict CI run that
accepts phase zero, and not a public gate activation. The candidate was
unpublished when it was tested.

Tested revision: `1e03934a39ec900988760fc73c9f66d02caa9a36`. It continues the
earlier candidate ([phase0-strict-candidate-57d9b5a449.md](phase0-strict-candidate-57d9b5a449.md),
which stays bound to `57d9b5a449`) with these commits:

| Commit | Change |
| --- | --- |
| `61cd15b423` | MD5/IEC60664 single-pilot gate statements labelled historical |
| `52a74cca54` | the `57d9b5a449` strict proof record |
| `b76c1d9a8b` | merge of `rust` `e3d4122a44` (#43 merged); ancestry only |
| `f8d4db536d` | PR scope and merge policy in `port/AGENTS.md` |
| `1e2af88b14` | #43 merged in the status docs |
| `6192834931` | merge of actual `rust` `cb4e5daa74` (#42 `8d3642a47f` and #44 merged); ancestry only |
| `1e03934a39` | notes of the three pure pilot records and maintained status docs |

Both rust merges kept the whole tree. `cb4e5daa74` has the exact reviewed #44
tree `dc8f6cddc7`; #39 (`9a48bb6636`), #42 and #43 are its ancestors. This record
is committed after the tested revision and changes no runtime path.

## Identity

`candidate-identity.json`:

- Against actual `rust` `cb4e5daa74`, the candidate differs only in:
  - the workflow flag (`PHASE0_REQUIRED: "true"`);
  - the merge policy in `port/AGENTS.md`;
  - the three pure pilot records (resistor status `green`; notes);
  - six maintained docs (`rust/PORT_PLAN.md`, the MD5 README/AUDIT/VALIDATION,
    the IEC60664 and resistor READMEs);
  - this branch's integration records.
- `port/oracle`, `port/tools`, `port/cases` and `port/goldens` are tree-identical
  to runtime `412b54a546`, and every Rust source file is too. Only the
  documentation listed above differs, and none of it is compiled.
- All 1801 protected fields equal #39 `328572624e`. The green records are
  exactly the three layer-0 pilots. The GetReader thin owner is `todo`, and its
  record is byte-identical to `rust`.
- Golden change policy is valid against `rust` `cb4e5daa74`, `e3d4122a44` and
  `9a48bb6636`, #42 `8d3642a47f`, #44 `5275f29be6`, `3efaeb15d3`, `52a74cca54`,
  `1e2af88b14` and the old base `1a2fb924d5`.

The raw export used `core.autocrlf=false` and `core.eol=lf` (`head-identity.json`,
`pin-identity.json`). All 915 pinned reference files equal their Git blobs; of
24360 exported files only `thirdparty/argparse/tools/build.bat` differs, by its
own CRLF attribute.

## Reused local oracle (epoch `412b54a546`)

No C++ was built or audited for this candidate, and no CTest ran. The oracle
`945b7b60...` from [phase0-final-integration-412b54a546.md](phase0-final-integration-412b54a546.md)
was reused after these checks (`reuse-identity.json`):

- binary SHA-256;
- live `--provenance` byte-equal to its build record;
- all 37 overlay file hashes equal the candidate's raw `port/oracle` bytes, with
  no file missing or extra;
- the candidate's 915 pinned reference hashes equal the embedded manifest
  (`dab50320...`).

Its embedded overlay revision remains `412b54a546`.

Separately, hosted CI qualified the merged #44 head. The
[rust-port run](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45015332781/job/137186313699)
at `5275f29be6` built and audited a fresh oracle (`9feca7f047d6...`, not the local
`945b7b60...`). It passed 8 CTests, all 21213 cases with zero deferrals and
unchanged output hashes, and 1801 valid records. Its phase-zero summary read
INCOMPLETE 2/3 because the resistor record was then `claimed` and the flag
`false`. That run was verified by the coordinator from its job log; it is cited
here, not reproduced.

## Checks (WSL, own tree `~/kicad-port/phase0-strict-candidate-1e03934a39`)

The shared lock was taken by mkdir without waiting and released afterwards.
Memory was checked first (29 GiB available). Cargo was capped at eight jobs, and
the target cache was copied read-only from the `412b54a546` root. The registered
runner and three native shims were rebuilt from the candidate sources, so each
report binds the executable that ran. Rust 1.99.1; the oracle is GCC 15.2.0
Release x86-64.

| Check | Result |
| --- | --- |
| Policy tests | 7 passed |
| Source validator | 1801 records valid, no errors |
| Candidate statuses | three green layer-0 records; workflow flag `true` |
| `check_goldens_policy.py --ci --require-phase0` | **21213 cases** at the default per-entry budget, including the 512 MiB MD5 carry; zero deferrals or failures; 89 s |

| Unit | Cases | Oracle output = golden | Rust output | Shim |
| --- | --- | --- | --- | --- |
| MD5 | 5973 | `9b8b9a76...` | `709084cd...`, unchanged | `8a1aae24...` |
| IEC60664 | 14190 | `3051d7e1...` | `819f74f4...`, unchanged | `61514d52...` |
| Resistor | 1050 | `8d1cc9e8...` | `5fb5dfb6...`, unchanged | `4541250c...` |

Runner `cda4f96c...`. Debug hashes differ between runs because debug information
embeds the work-tree path.

`reports/phase0.json`: **COMPLETE, 3/3** live green layer-0 units,
`strict_gate_enabled: true`, 1801 validated records. This is prospective and
private. Extraction coverage is still reported separately and is not part of
the gate:

- parsing, all configurations and the WSL gate are incomplete;
- 4 parse failures;
- 63 uncompiled application sources;
- 8,899,609 unresolved calls.

## Limits

- No C++ build, audit or CTest was run, and no workspace fmt, clippy or tests,
  wasm builds or envelope suites. Runtime code is identical to `412b54a546`,
  `7207de24ed` and actual `rust`, and their recorded results stay bound to their
  own tested revisions.
- The resistor NaN qualification remains GCC 13/15 Release x86-64 only.

## Evidence bytes

`phase0-strict-candidate-1e03934a39/files.sha256` verifies every payload as
committed. Every file is LF.

## Remaining holds

- an independent review of this candidate;
- the parent's go-ahead to push;
- the hosted strict CI run on the published head, which is the phase-zero
  acceptance authority.
