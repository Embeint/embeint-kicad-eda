<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# kimath header hash: the 8c54103 baseline is superseded as qualification

Unit `libs.kimath.include.application.pure.p001`, PR #51.

The following artifacts remain committed, unchanged, as history. They are **not**
qualification evidence for the unit and must not be reused as such:

- `port/goldens/evidence/libs.kimath.include.application.pure.p001-baseline.json`
  (first baseline, 1315 cases, oracle output sha256 `e1b95812a96a314af616bb1f7a21a56961df7b0d67a692bffea0f2bec891f4b9`);
- the golden lines it blessed, until a reviewed reblessing replaces them;
- `rust/kicad-hash-shim/validation/2026-11-23-7cc11f2e/` and the 2026-11-23
  section of `rust/kicad-hash-shim/VALIDATION.md`.

Reasons (independent Sol 6.1 review of head `8c54103a7d9a888419e663abb54082c3370fee0a`):

1. The oracle adapter wrote zero to all 16 private `MMH3_HASH::blocks` bytes
   after each real constructor call. The pinned constructors leave them
   uninitialized. Every one of the 336 MMH3 cases therefore serialized invented
   initial bytes, and 15 cases computed results from never-written bytes
   ([reclassification](kimath-header-hash-mmh3-unwritten-reads.md)). The
   agreement was with the adapter's repaired object, not with the reference.
2. `HASH_128::ToString` honours the global C++ locale's `numpunct<char>`
   grouping; the corpus exercised only the classic locale, and the Rust port had
   no locale input ([locale record](kimath-header-hash-tostring-locale.md)).
3. The five owned `bezier_curves.h` bodies had neither implementation nor an
   executable deferral.
4. Identity: the recorded executable hashes `b723f41c…` (oracle) and
   `44d2a535…` (Rust) are the `docker exec` wrapper scripts, not the binaries.
   The binaries were `kicad-oracle` sha256 `6e65e38510f5f3ca34f08b9ebd970afbbe4e0edfef2bef389ca4dc3f2fff99e3`,
   debug Rust `15377b904b11f75fb467d47b9dbb26bc69e980a9628fd53b2abffe80b369954f`
   and release Rust `2874a24e435fc16c95ac3436a69355326d74d0493bcbd9ec1c560d131a1a97ae`.
   The wrappers do not identify the image, runtime libraries or the mutable named
   container they invoked. The run was under QEMU user emulation, not a native
   Linux host.

What the old run still shows, without being qualification: on the 979 non-MMH3
cases and on the defined arithmetic of the MMH3 cases, the integer
implementation agreed with the adapter's outputs. The corrected implementation
leaves all of those values unchanged.

Next step: a live, native Linux x86-64 oracle build of the corrected adapter,
an exact comparison of the defined corpus, then a new reviewed blessing whose
reason cites this record. Neither the old evidence nor its golden is edited.
