<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Corrected live hash baseline

[Native run 45348544983](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45348544983)
tested exact `a051ef7c7aaafb42029c9c28aae97b7df832201f` on Ubuntu 24.04 x86-64,
GCC 13.3 and Rust 1.99.1, using the recorded SSE2 tunable and measured runtime
files. It matched all 2,641 cases in debug/release, all 441 guards and 1,983
sensitive requests per profile, and actual wasm. The oracle audit covered all 78
compilations; all eight CTests passed.

The workflow performed genuine live propose/bless/check twice with the same
oracle and Rust binaries: incrementally from the original 1,315 replies, and
from the empty official-rust baseline. Both checks matched the same new golden.
It did not push changes. Parent import is byte-exact from artifact `14899532747`;
complete ZIP SHA-256 is
`665d7e6d0efe983bf309b894e7c1c74f19c0a4942917ea2268d074acdeec2cf5`.

The incremental record preserves 979 values, corrects 321 invented MMH3 byte
snapshots, adds 1,341 valid generated cases and removes exactly the fifteen
undefined reads retained as permanent refusal fixtures. It also records every
position change. Both evidence files enumerate the exact old/new reply lines;
no tolerance, input-domain exclusion or fabricated upstream output is used.
All prior evidence and the input corpus remain unchanged.

`audit.json` records parent verification. `reports-summary.json` contains derived
summaries and SHA-256 bindings for six full runner reports, whose every ordered
case outcome passed the committed policy verifier. Full reports and the native
oracle/object remain in the CI artifact. `bindings.txt`, `runtime.json`,
`provenance.json` and `compile_commands.json` are verbatim artifact bytes.
`SHA256SUMS` covers every file committed in this directory except itself.

The parent rehashed the included native oracle and measured adapter object.
Native Rust binaries were not included; their identities were recorded and
rechecked by the workflow runner at each live action, not independently rehashed
after download. This evidence does not qualify the entire reference image or
arbitrary other compiler/runtime variants. The unit remains partial; current-head
CI and final review must pass before readiness.
