# Escaped-key correction validation

Clean runtime source: `ec81fa0bd4096717730b1af3e0b5d5bb42393eeb`.
The source archive used `core.autocrlf=false` and `core.eol=lf` for this private
Linux test copy. The earlier 889 fixtures, corpus, goldens, evidence, portable
bodies and native adapter bodies remain byte-identical to `19e8384772`.

The C++ executable is explicitly **reused** from the qualified GCC 15 Release
`ff419ad71f` build, SHA-256
`b0b167cb40bfcb3b2f8fa6791f36742bdab8a560d791acccb875b105ce9b632f`.
Its hash and live embedded provenance were checked before use; that provenance
is retained here without relabelling its revision or CRLF reference export.
No fresh C++ build or raw-LF reference export is claimed by this test-only change.

Completed in `~/kicad-port/resistor-escaped-fix-ec81fa0b-r2`, holding the actual
shared build lock with at most eight compile jobs:

- Pinned west checkout verification, Rust formatting and warnings-denied Clippy
  for the shim and all its targets passed.
- All three `private_keys` process tests passed in debug and release, none
  ignored. The new test asserts actual escaped-dollar, escaped-colon and fully
  encoded key bytes, plus six mixed-spelling duplicate requests.
- The literal generator reproduced all 199 committed request bytes.
- The default C++ guard entry passed 1,088 complete private-key replies, byte for
  byte. Both Rust profiles matched all 1,088 exact replies: 65 successful controls
  and 1,023 refusals.
- The combined default script passed all 1,159 replies on C++, Rust debug and
  Rust release. Refusals remain separate from registered successful cases.
- The unchanged default-budget registered check passed all 1,050 cases in debug;
  release comparison also passed 1,050, with zero deferrals or mismatches. Both
  output hashes equal the historical proof and golden.

Commands, output, exact runner/executable hashes and full registered reports are
retained beside this file. The first setup attempt stopped before any test
because the prior resistor copy lacked west dependency storage; its log remains
`setup-attempt-1.log`. The successful new root copied and verified the existing
pinned foundation before Cargo. No earlier source/build tree was changed, and
the acquired lock was released.

This closes committed escape coverage and corrects the export claim described
in [the addendum](../resistors-escaped-key-and-export-addendum.md). It changes no
algorithm, registered case, golden, compiler qualification or phase-zero status.
The resistor record remains claimed pending extractor acceptance and final CI.
