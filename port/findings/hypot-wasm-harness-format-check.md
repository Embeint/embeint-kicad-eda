<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Hypot standalone wasm harness formatting gate

PR #88 head `980bcf964ae55a64e3368d20afaab040a3b339d6` failed
`glibc-libm` in run 45738836395, job 139638548913. The complete retained job
log identifies `cargo fmt --manifest-path port/glibc-libm/wasm/Cargo.toml --check`
as the first failing command. All pinned source hashes and coefficient checks
and all 12 Python harness tests had passed immediately before it.

Rustfmt requires the `glibc_trig` module declaration before `hypot` in the
standalone harness. The declarations are reordered by cargo fmt; the hypot
comment now stays beside its own declaration. There are no arithmetic, guard,
input-corpus, expected-result, source-hash or qualification-policy changes.
No numerical comparison ran in that failed hosted job. Its later report failure
correctly rejects the missing comparisons; it is not evidence of a numerical
mismatch and has not been waived.

Accepted rust `7a984a76e8073b7b6cdc01895da4bae126f05641` was also imported
conflict-free before publishing the correction. Merge commit
`81b91ee19c57ac9627e287dbd24ba53f2084072f` has precomputed tree
`f6ed7ba34c12386e6b6f0fec029c159f30f348bb`. The staged import exactly equals
the accepted delta; the previously reviewed hypot runtime is unchanged.

Local checks on the correction:

- Standalone wasm and kicad-math formatting pass.
- The 12 Python reference/harness tests pass, including rejection of missing
  comparisons, wrong domain hashes and altered transport bytes.
- Kicad-math clippy with warnings denied passes.
- Locked standalone import-free wasm debug and release builds pass. Each executes
  all 1,607,712 existing genuine glibc 2.39 hypot records with zero mismatches.
  The reference SHA-256 is
  `2972d4008f433f2e052824625af0136c63abaa9cbb10ac854f40ab97d8aaa3cd`.

Fresh final-head hosted qualification remains required for native, WASI and
import-free wasm debug/release across all seven functions. The unit stays
partial and no golden is added.
