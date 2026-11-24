<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Resistor integer-power reference migration to glibc 2.39 SSE2

The user authorized this migration on 2026-11-24 after reviewing the thirteen
changed cases. It selects the pinned glibc 2.39 SSE2 result for this unit; it
does not adopt PR55's entire candidate environment or waive another failure.
The original [integer-power finding](resistors-portable-integer-powers.md) and
all historical golden evidence remain unchanged.

## Observed behavior

The genuine C++ `ESERIES_IN_DECADE` uses runtime `std::pow(10, exponent)`.
The selected glibc runtime gives `pow(10,126)` as `0x5a17a2ecc414a040`;
host-default FMA and the historical Rust table/golden give
`0x5a17a2ecc414a03f`. Both are defined behavior. This is a reference selection,
not an upstream-source correction or a comparison tolerance.

Native candidate CI [45323785738](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45323785738)
at `33535308d33e019f04af6e089b14ca593ea84ccb` reproduced 1037 exact resistor
cases, thirteen failures and zero deferred/unprocessed cases. Its 76 strict
compilations, eight oracle CTests and twelve probe checks passed. The trace
identified the locked SSE2 implementation; the corpus remained failed.
KiCad stays pinned at `4cf939778df04d2b747bb2c4714fc5014a7565c7`.

Exactly these existing case IDs change, without additions/removals/reordering:
`series-0` through `series-8`, `series--1`, `series--2147483648`,
`series-2147483647`, and `format-boundaries-3`. They contain 317 changed numeric
bit fields and three format strings, which cross rounding boundaries from
102/500/1000 to 101/499/999. Every integer exponent from -323 through 308 is
covered by the existing corpus. Exceptions at 23 and 210 remain unchanged.

## Implementation and contract

The portable table changes only exponent 126. Arithmetic, formatting algorithms,
inputs and exact comparisons stay unchanged. CI starts reference processes with
`GLIBC_TUNABLES=glibc.cpu.hwcaps=-AVX,-AVX2,-FMA,-FMA4,-AVX512F`.
`port/oracle/check_resistor_reference.py` refuses another platform, loader
override, tunable or hashed libm/libc/loader binary, checks the oracle's resolved
libraries, and makes a dynamic, non-folded `pow(10,126)` call. Its explicit runtime
hashes equal PR55's existing glibc 2.39 lock, without consuming an open topic
branch. Changed distribution bytes fail closed and require renewed review.

The preflight checks identity/configuration and a sensitive call; it is not a
new trace proof or whole-image qualification. The fresh 1050-case live C++/Rust
comparison is still mandatory. The CI loader setting also reaches the other
registered units; those must pass their existing unchanged exact goldens.

## Golden generation and validation

The manual `migrate_resistor_sse2` workflow input defaults to false and never
enables on PR events. The one-time helper uses the existing live
`propose`/`bless`/`check` sequence. It refuses a changed corpus/old golden,
unexpected case IDs/positions/types/field counts, or a runtime report for a
different oracle. The reason records the runtime-preflight digest. Blessing
requires repeated live equality and matching live oracle provenance. A new
immutable evidence file is written; no historical evidence is edited. The job
uploads generated files for review and never commits or pushes them.

Fresh native generation, full strict CI, actual wasm execution and independent
review are pending at the initial implementation commit. The earlier private
Mac debug experiment matched all 1050 retained genuine CI oracle replies and
passed nineteen tests; that was retained-reference replay, not fresh C++
execution or permission to bless a mismatch. New results will be recorded
before this PR is marked ready.

Matrix NaN/SetRotation, hash/Bézier NaN/locale, other libm paths, cross-host
qualification and PR55 adoption remain separate work.
