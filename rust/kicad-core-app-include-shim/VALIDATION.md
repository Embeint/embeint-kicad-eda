<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Review-correction validation, 27 November 2026

The unit remains **partial**, the PR remains draft, and no golden is created. This is
local diagnostic evidence, using GCC 15.2 / glibc 2.43 in WSL, Rust 1.99.1 and Node
22.22.1. Fresh GCC 13 / glibc 2.39 hosted qualification and independent review of the
corrected implementation are still required.

Accepted `rust` revision `70888b572202033a6522f2f271b10cc908f74dfe` was merged with
merge commit `80e7fdad4e5879bfe2c005f55d5ea5377172e1b6`. Workspace members, lock entries,
oracle registrations, the throttle clock wrapper and the ellipse co-port were preserved.

## Fresh reference and comparisons

The final oracle was rebuilt in `~/kicad-port-appinclude-r3/work` from 918 **raw Git
blobs**, exported using native Windows Git `cat-file --batch` at pinned revision
`4cf939778df04d2b747bb2c4714fc5014a7565c7`. This avoids Windows archive conversion:
the older r2 oracle's source had CRLF conversions. Its preliminary results were repeated
against the new raw-blob build. The source-manifest verification and linkage audit passed
(137 compilations, strict FP flags), as did all 15 oracle dispatch/input checks.

| Comparison | Result |
| --- | --- |
| Public corpus, including lifecycle, casts, retained lifetime and retry | 4,031 exact, zero deferred |
| Separate refusal controls | 336 exact refusals |
| Defined request after each refusal, in the same process | 336 successful continuations, oracle/Rust exact |
| Import-free wasm32 versus newly executed native replies | All 4,703 requests exact (4,031 + 336 + 336) |
| `libs.core.application.pure.p001` caller | 6,227 exact |
| `libs.core.include.core.application.pure.p001` caller | 669 exact |
| Ellipse caller, local diagnostic toolchain | 2,112 exact, 12 double-result differences, 100 deferred |

The ellipse differences remain a local qualification limit. The Rust math implementation
has no changes relative to reviewed head `b48b95f893125e53b6f4bb484156d2abee43dcd5`;
the preserved r2 comparison also reports those 12 differences. They are not counted as
passes or suppressed with tolerances. The previously hosted ellipse result (2,124 exact,
100 deferred) does not qualify the new head.

Final raw-blob oracle SHA-256:
`24ae74fc1bfd1a22d494570ffd3eaaf5a7f284e9a8ca55451bb3ce784929faf4`.
The local report, compile commands, source manifest and logs are retained in the r3
directory; older validation directories and private runtime builds were not overwritten.

## Portable checks and dependency review

- Scoped core, math, adapter and ellipse tests pass. The five existing ignored math tests
  remain ignored. Rustfmt and Clippy with `-D warnings` pass for the scoped packages.
- Portable core/math checks pass for wasm32 and the workspace's Rust 1.85 minimum version.
- The deterministic generator `--check` passes (4,031 public cases, 336 controls).
- Unit records/source accounting validation passes with `--skip-source-hashes` for the
  historical extraction graph; this is not a claim to have re-extracted current sources.
- The vendor archive SHA-256 and 16 retained upstream files match the published archive
  byte for byte, excluding the documented module registration and sentinel fix. Original licences and the
  original manifest are retained; local buffer code is GPL-3.0-or-later.
- Vendor tests pass: five new buffer tests, 33 upstream tests and 29 doctests; one existing
  doctest remains ignored. No heap fallback feature is enabled in `kicad-core`.
- Miri nightly 2026-11-26 with strict provenance passes five buffer tests (including zero,
  odd and full capacities, pointer payloads, moves and destruction) and three portable
  callable storage/lifecycle tests. Two leak/retry tests pass under Miri with intentional
  leak checking disabled for those tests.

The buffer review found that exposed struct padding, and a union with uncovered tail
padding, can lose stored bytes during a Rust move. Miri reproduced the latter failure.
The final overlapping union fields jointly cover the entire representation, and unused
metadata bytes are initialized. The replacement sentinel check also uses the actual
metadata width, avoiding double destruction if a payload destructor panics in replacement.
That safe public dependency path has a dedicated panic regression test. The
portable crates retain `forbid(unsafe_code)`; the dependency review covers the safe
constructor, dereference and destruction paths used by this port. See
[the patch review](../vendor/stack_dst/EMBEINT-PATCH.md).
