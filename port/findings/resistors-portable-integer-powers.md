<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Portable decimal powers need glibc compatibility

The first live 190-case comparison failed 19 cases. All calculator searches,
expressions, parts, masks, state transitions and complete buffers agreed. The
failures were confined to standalone decade bits and engineering notation:
FDlibm `pow(10, integer)` differs from the Linux glibc result at some exponents,
and negative logarithms produce a different NaN sign. No mismatched golden was
created or blessed.

This unit never needs a noninteger exponent: E-series exponents are i32, and
formatting explicitly floors log10 before subtracting two. The portable port
uses correctly rounded decimal literals for powers -323 through 308, with the
reference's two exceptions at 23 (`0x44b52d02c7e14af7`) and 210
(`0x6b88557f31326bbc`). Exponents below/above the range retain zero/infinity.
The negative-logarithm formatting branch preserves the observed `nanL` string.
All other rounding, expression order and the six-digit g-format conversion stay
unchanged. This is a specialized implementation for the actual exponent domain,
not a returned-value normalization or a comparison tolerance.

With these changes, all 190 cases passed the existing live exact comparator.
Four further helper boundary probes brought preparation to 194 cases; the standard
exact comparator also passed that entire corpus. A complete per-value diagnostic
agreed on every reply (about 8 MB per executable), measured
at 0.277 seconds for C++ and 0.498 seconds for Rust. These are local warm-process
measurements, not hosted CI timings. Both the real C++ archive/fmt linkage audit
and fmt/clippy/tests/portable wasm checks passed before this corpus extension.
The final reproducible baseline must rerun the standard compare/propose/bless
pipeline at its tested commit and include the full live executable/provenance
identity. The final generated scope record remains pending; no unit status or
phase-zero completion is claimed here.
