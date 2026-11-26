<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Depth follow-up validation, 2026-11-26

Local evidence for the removal of the fixed 64-level depth limit, at tested
revision `7d375e39ad830ae72707955adde97ccb2f2ddaf9` (implementation `5bb53cf441`
merged with accepted `rust` at `a862a3eb8e`). The earlier
`../2026-11-24-48a80874/` evidence is historical and unchanged.

* `compare.json`: kicad-difftest strict report, 259 compared, 0 deferred,
  0 failures, 0 unprocessed.
* `provenance.json`: staged source and overlay hashes, compiler, flags and the
  native binary hash. The stage was copied from the clean working tree (CRLF
  checkout normalized to committed LF bytes) rather than exported by
  `native/prepare.py`; the pinned headers were checked equal to the pin blobs.
* `coverage-summary.json`: gcov over the instantiated pinned header after the
  run. The counts equal the earlier revision's: the deep programs reach the
  same header lines at greater depth rather than new lines.
* `summary.json`: counts, measured root levels, binary and source hashes.

Hosted x86-64 CI and a separate review are pending. No golden is committed.
