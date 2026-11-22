<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Phase zero strict-gate candidate: prospective proof

**Prospective, private validation.** This is not hosted CI and not a public gate
activation. The candidate was unpublished when it was tested.

Tested revision: `57d9b5a449a3cb6f11f303abccdd34e09f8c353e`. It adds two commits
to the published integration head `3efaeb15d33edc6ac265fe75e6c0e420c5970024`:

| Commit | Change |
| --- | --- |
| `ebed0feb6b` | resistor pure record `claimed` to `green` (status and notes only) and its README status |
| `57d9b5a449` | `PHASE0_REQUIRED: "true"` in `.github/workflows/rust-port.yml` |

Later commits change only documentation and this record. `61cd15b423` labels
the MD5 and IEC60664 single-pilot gate statements as historical. They do not
change any runtime, case, golden, oracle or unit record path.

## Identity

`candidate-identity.json`:

- Apart from the `412b54a546` record, the candidate differs from runtime
  `412b54a546` only in the workflow, the resistor record and the resistor README.
  The README is not compiled into any crate.
- `port/oracle`, `port/tools`, `port/cases` and `port/goldens` are tree-identical
  to `412b54a546`; cases and goldens are also identical to `7207de24ed`.
- All 1801 protected fields equal #39 `328572624e` (the tree of `rust` at
  `9a48bb6636`).
- The green records are exactly the three layer-0 pilots. The GetReader thin
  owner stays `todo`.
- Golden change policy is valid against `rust` `9a48bb6636`, #42 `f0c16c4eb8`,
  #43 `0cbaf1d873`, #44 `9f40815c03`, `3efaeb15d3` and the old base `1a2fb924d5`.

The raw export used `core.autocrlf=false` and `core.eol=lf` (`head-identity.json`,
`pin-identity.json`). All 915 pinned reference files equal their Git blobs; of
24339 exported files only `thirdparty/argparse/tools/build.bat` differs, by its
own CRLF attribute.

## Reused oracle (epoch `412b54a546`)

No C++ was built or audited for this candidate, and no CTest ran. The oracle
`945b7b60...` built, audited and CTested in
[phase0-final-integration-412b54a546.md](phase0-final-integration-412b54a546.md)
was reused after these checks (`reuse-identity.json`):

- binary SHA-256;
- live `--provenance` byte-equal to its build record;
- all 37 embedded overlay file hashes equal the candidate's raw `port/oracle`
  bytes, with no file missing or extra;
- the candidate's 915 pinned reference hashes equal the manifest the binary embeds
  (`dab50320...`).

Its embedded overlay revision remains `412b54a546`. It is not relabelled to the
candidate.

## Checks (WSL, own tree `~/kicad-port/phase0-strict-candidate-57d9b5a449`)

The shared lock was taken by mkdir without waiting and released afterwards.
Memory was checked first (29 GiB available). Cargo was capped at eight jobs and
the target cache was copied read-only from the `412b54a546` root. The runner and
three shims were rebuilt from sources identical to the tested runtimes, so each
report binds the executable that ran. Rust 1.99.1, GCC 15.2.0 (Release x86-64
oracle).

| Check | Result |
| --- | --- |
| Policy tests | 7 passed |
| Source validator | 1801 records valid, no errors |
| Candidate statuses | three green layer-0 records; workflow flag `true` |
| `check_goldens_policy.py --ci --require-phase0` | **21213 cases** at the default per-entry budget, including the 512 MiB MD5 carry; zero deferrals or failures; 89 s |

| Unit | Cases | Oracle output = golden | Rust output | Shim |
| --- | --- | --- | --- | --- |
| MD5 | 5973 | `9b8b9a76...` | `709084cd...`, unchanged | `996360fc...` |
| IEC60664 | 14190 | `3051d7e1...` | `819f74f4...`, unchanged | `d8b9bb7f...` |
| Resistor | 1050 | `8d1cc9e8...` | `5fb5dfb6...`, unchanged | `6da5c83c...` |

Runner `8b0a0636...`. Debug hashes differ from earlier runs because debug
information embeds the work-tree path.

`reports/phase0.json`: **COMPLETE, 3/3** live green layer-0 units,
`strict_gate_enabled: true`, 1801 validated records. This is prospective and
private. Extraction coverage is still reported separately and is not part of
the gate:

- parsing, all configurations and the WSL gate are incomplete;
- 4 parse failures;
- 63 uncompiled application sources;
- 8,899,609 unresolved calls.

## Limits

- The workspace fmt, clippy and tests, the wasm builds and the envelope suites
  were not rerun here. Their results remain those of `7207de24ed` and `412b54a546`
  for identical runtime code.
- The pilot heads are merged as ancestry only. An actual merged `rust` head with
  #42, #43 and #44 has not been integrated or tested.
- The resistor NaN qualification remains GCC 13/15 Release x86-64 only.

## Evidence bytes

`phase0-strict-candidate-57d9b5a449/files.sha256` verifies all 19 payloads as
committed. Every file is LF; there is no CRLF manifest in this record.

## Remaining holds

- the merges of #42, #43 and #44 into `rust` and their current CI;
- an independent review of this candidate;
- normal integration of the merged `rust` head;
- an authorized push, then hosted strict-gate acceptance.
