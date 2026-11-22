<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Phase zero final integration: literal escaped MD5 key follow-up

Tested clean runtime: `412b54a54637b44966cb8babc4a7bd4fc6a4c4c1`. It merges the
corrected #42 head `c9266a341336baafdbe6536e1b935b5457c3b27d` (validated source
`da17dc54dc`, descendant of the previously integrated `c5ec29e70d`) into the
recorded runtime `7207de24ed` and its record commit `f7d98b47cb`, by a normal
merge.

This is an additive affected-unit proof. The results in
[phase0-final-integration-7207de24ed.md](phase0-final-integration-7207de24ed.md)
remain those of `7207de24ed`; they are not relabelled to this runtime. That
record, the earlier records and their scripts and roots are unchanged.

The strict gate stays off (`PHASE0_REQUIRED=false`), the resistor record stays
`claimed`, and MD5/IEC60664 keep their provisional green records.

## What #42 adds

An exact-head review found that three nonduplicate "escaped" MD5 fixtures used
plain marker keys. #42 appends three true literal escaped keys and a successful
recovery to the default guard (`test_md5_inputs.py`, run by the
`oracle-md5-inputs` CTest). It also adds a shim test that checks their raw and
decoded spellings, and a validation directory with byte-preserving
`-text -whitespace` attributes. The production shim, shared helper, engine,
corpus, goldens and record are unchanged.

## Identity

`supplemental-identity.json`:

- All 29 paths #42 changed since `c5ec29e70d` hold its exact blobs.
- No other path changed since `f7d98b47cb`. That covers 87 integration evidence
  paths (including the `7207de24ed` record), all 1801 unit records, the unions
  and the #39 and #44 deltas.
- The MD5 record equals #42's.
- The guard's historical 199 checks are an exact prefix in raw request and
  expected reply bytes. All 203 equal #42's committed `requests.jsonl` and
  `expected.jsonl`.
- Golden change policy is valid against `rust`, #39, the new #42 head, #43, #44,
  the published integration head and `f7d98b47cb`.

`layer-identity.json` uses #42 = `c9266a3413`. 2663 paths hold their owner's blob
and the same ten paths are resolved. `test_iec60664_guards.py` is still the one
deliberate change. 1797 records are byte-identical to #39 and no protected field
differs.

The raw export used `core.autocrlf=false` and `core.eol=lf`. All 915 pinned
reference files equal their Git blobs, and the pinned reference archive is the
one of `7207de24ed`. Of 24318 exported files, only
`thirdparty/argparse/tools/build.bat` differs, by its own CRLF attribute.

## Checks (WSL, own tree `~/kicad-port/phase0-final-cont-412b54a546`)

The tree was extracted from the raw archive. The solidcore checkout and Cargo
target cache were copied read-only from this proof's own `7207de24ed` root. The
oracle was built fresh (73 objects), because its embedded overlay revision and
hashes change with the runtime. The shared lock was taken by mkdir without
waiting and released afterwards. Compiles were capped at eight. GCC 15.2.0
Release x86-64, Rust 1.99.1.

| Check | Result |
| --- | --- |
| Oracle audit | 915 pinned files, 75 compilations, contraction off, no fast-math |
| CTest | 8/8 passed at default 60 s, including `oracle-md5-inputs` with the literal escapes |
| Workspace fmt; MD5 shim clippy (warnings denied) | passed |
| MD5 shim tests | 11 passed, none ignored, including `literal_escaped_marker_keys_preserve_complete_refusals` |
| Native shim and portable `kicad-md5` wasm build | passed |
| Default MD5 guard, both actual adapters | 203 complete replies: 197 refusals, 6 successes |
| Transcripts | fresh oracle and shim stdout equal #42's committed `oracle.stdout.jsonl` and `rust.stdout.jsonl` byte for byte, so the 199-line prefixes are unchanged |
| #42 payload checksums | 23 entries verify in the LF tree |
| Source validator | 1801 records valid |
| Registered MD5 unit | **5973 cases** at the default per-entry budget, including the 512 MiB carry; green, zero deferrals or failures; 25 s |

| Unit | Oracle output = golden | Rust output |
| --- | --- | --- |
| MD5 | `9b8b9a76...` | `709084cd...`, unchanged |

Oracle `945b7b60...` (embedded overlay revision is this runtime), provenance
`631d6022...`, MD5 shim `16e82698...`, runner `357b7d6c...`. The registered
report's `oracle_sha256` and `rust_sha256` are these executables.

The IEC60664 and resistor engines, shims, cases and goldens are unchanged from
`7207de24ed`, so their envelopes and the full 21213-case run were not repeated.
Their results there are not reattributed to these binaries; the default CTest
above did run their oracle-side guards against this oracle. The resistor NaN
qualification remains GCC 13/15 Release x86-64 only. Phase zero stays
INCOMPLETE 2/3, as recorded for `7207de24ed`.

## Evidence bytes

`files.sha256` verifies every committed file here. Windows-written reports are
LF. The tested source manifest is kept only as `source-manifest.original.json.gz`
(`gzip -n`, byte-exact CRLF). It decompresses to `dab50320...`, the identity in
the live provenance, `summary.json` and `pin-identity.json`.

## Remaining holds

Separate bounded review of the #42 correction, parent acceptance and merge of
#39 and the pilots into `rust`, and hosted CI. Then `rust` is merged here, the
resistor record and notes are updated, and `PHASE0_REQUIRED` becomes true for a
final combined proof and review with `--require-phase0`.
