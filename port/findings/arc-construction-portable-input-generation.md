<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Arc construction generator host-libm dependency

PR #85 head `207f67b2cb11cb1b51a24ed37a692be8b3109d3c` failed
`arc-construction-live` in run 45739086562, job 139639373879. Fmt,
clippy, unit tests and the portable build passed. The next step rejected the
public corpus as stale before any live oracle comparison.

The generator used Python's host `math.sin` and `math.cos` to construct raw
binary64 tangent inputs. On the Windows generator those agree with the committed
corpus; on Linux they differ in some last bits. The generator also used host
binary64 pow for random magnitudes and hypot for four small QA normalizations.
These are input construction dependencies, separate from the Rust algorithms
being compared.

The generator now evaluates input sine/cosine with 120 Taylor terms at 100 decimal
digits for angles bounded by seven radians. Input decimal powers of ten also use
100 decimal digits. Each converts once to binary64. The four small integer QA
norms use correctly rounded square root. No platform transcendental function
chooses public input bits. The branch mirror still only labels coverage; neither
it nor the input helpers supply oracle answers.

Regeneration retains all 2,912 case IDs, operations and families. Only tangent
bits in 19 fixed-seed random cases change; QA and special-value inputs, all 20
controls, runtime Rust/C++ bodies, guards, deferrals and unit status are unchanged.
No golden is added. Prior corpus comparisons remain historical evidence, not
validation of these changed inputs.

Canonical LF corpus hashes:

- Prior SHA-256: `65c257e018d7514b129901e1b8f7b2ae2aa85bffa09532661724c96dc8acdda4`.
- New SHA-256: `12ef1c73bb994b642ce8d520742f3d4ed3113508b203077df72a89b9cc19576f`.

Changed IDs:

- `tangent-random-0-10`.
- `tangent-random-0-17`.
- `tangent-random-0-25`.
- `tangent-random-0-27`.
- `tangent-random-0-37`.
- `tangent-random-0-56`.
- `tangent-random-0-58`.
- `tangent-random-1-38`.
- `tangent-random-1-44`.
- `tangent-random-2-24`.
- `tangent-random-2-26`.
- `tangent-random-2-49`.
- `tangent-random-3-7`.
- `tangent-random-3-15`.
- `tangent-random-3-42`.
- `tangent-random-4-14`.
- `tangent-random-4-20`.
- `tangent-random-4-24`.
- `tangent-random-4-36`.

Windows and WSL Linux generator `--check` now both pass against identical bytes;
the unchanged old generator fails on Linux. The five arc shim tests pass. A fresh
replay against the existing genuine local oracle gives 2,892 exact results,
20 explicit vector-owner deferrals and zero failures; all 20 controls match.
This oracle is GCC 15/glibc 2.43 with the CI SSE2 tunables, not the qualified
GCC 13/glibc 2.39 reference. Fresh hosted live comparison and CI on the corrected
published head remain required.

The updated corpus and all harness controls also pass import-free WebAssembly
replay against freshly executed native Rust replies under Node 22.22.1.
