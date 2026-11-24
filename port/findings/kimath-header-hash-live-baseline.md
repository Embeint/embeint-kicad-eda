<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Corrected hash/Bézier live baseline

The first hash golden included invented zero bytes for unwritten MMH3 storage
and preceded the owned Bézier implementation and compiled-runtime corrections.
It cannot certify the current corpus. Earlier evidence remains immutable.

Fresh hosted [run 45348544983](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45348544983)
at `a051ef7c7aaafb42029c9c28aae97b7df832201f` passed live propose/bless/check for
all 2,641 cases, both incrementally and against official rust's empty unit
baseline. The same oracle and Rust binaries were rehashed at every action; both
final checks matched golden SHA-256
`1ef5a58c552ee2898575b7563807e4a8cf5325eae78d226244e7ca58623e08d9`.

The original 1,315 requests account for 979 unchanged values, 321 corrections of
invented MMH3 byte snapshots and fifteen undefined-read refusals. Those fifteen
requests remain byte-exact refusal fixtures. There are 1,341 additional valid
generated cases. Both evidence files enumerate exact lines, identities and order
changes; no defined input is excluded and no mismatch is blessed.

Debug/release comparisons, 441 guard requests and 1,983 sensitive requests per
profile, actual wasm, all 78 strict oracle compilations and eight CTests passed.
The actual adapter object matched the originally measured bytes, and linked
libstdc++ matched the measured runtime. The full-object guard rejects changed
non-arithmetic effects as well as instruction-order drift.

[Evidence and parent audit](kimath-header-hash-live-baseline/2026-11-24-a051ef7c/README.md)
identify source, binaries, artifact hashes and verification limits. The unit stays
partial; this does not adopt or qualify the entire reference-environment
candidate. Current-head CI and final review remain required after import.
