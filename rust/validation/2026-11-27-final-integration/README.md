<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Manual final integration, 27 November 2026

All 12 reviewed implementation PRs in this cycle are merged into `rust`:
#74, #76, #77, #78, #82, #83, #84, #86, #87, #85, #88 and #81.
Claim-only PRs #89 and #90 are unfinished and are not included.

Final accepted commit: `98a12c1123efc4904e0f0c5a59299452a7a15e5d`. Tree: `dbfb66a55e0854cc8436d8c500617940c935c1ea`.
Every accepted merge tree was checked equal to its manually tested tree.
SEG used its successful exact-head hosted checks and fallback preflight;
subsequent combined trees ran the final CI commands manually, as requested.

| Final check | Result |
| --- | --- |
| Genuine pinned C++ comparison | 140,550 public cases across 27 units; 135,782 exact, 4,768 explicitly deferred, zero failures or unprocessed cases |
| Unit validation and strict phase-zero gate | 1,801 unit records; COMPLETE |
| Rust workspace tests | 453 passed, zero failed, 5 ignored, across 135 suites |
| Workspace formatting and clippy | Pass with warnings denied |
| Minimum supported Rust | 1.85 workspace/all-target checks and layout wasm check pass |
| Generated corpora | Every applicable `rust/*/generate_cases.py --check` passes |
| Portable wasm builds | Pass |
| Existing import-free corpus replay workflows | 16 pass against fresh native/oracle replies |
| Refusal controls and golden evidence policy | Pass; golden results unchanged by integration |
| Layout | Python/native/wasm parity, wasm lint, loader tests and required-runner backend tests pass |
| Standalone libm harness | Formatting and locked debug/release wasm builds pass |

The manual reference root was reconstructed from the unchanged locked Ubuntu
inputs. All 249 package archives and recorded compiler/runtime file hashes
were verified. It runs GCC 13.3, glibc 2.39, libstdc++ 14, wxWidgets 3.2.4 and
Rust 1.99.1, with the SSE2 dispatch tunables used in CI. The verified official Node 22.22.1 distribution runs the
wasm VM; it does not supply the C++ runtime.
This records manual validation, not GitHub-hosted check results or qualification
of a newly adopted reference image.

The final source snapshot uses the accepted `source_binding.archive_git_blobs`
exporter: raw Git blobs, verified object IDs and an independently checked archive
inventory. Windows `git archive` had converted some LF corpus files to CRLF;
that export-only issue was corrected before the final complete rerun. No corpus
content, generator behavior or golden was changed to accommodate it.

Full hypot supersedes the finite-only vector wiring: all 2,912 arc cases now
compare. Ellipse has 2,224 public cases (2,124 exact and 100 logging deferrals)
and 249 controls. Fifteen programs whose newly available continuation reaches
an existing undefined-domain guard moved intact from public cases to controls;
all 2,473 original requests remain covered. See the accepted findings
`port/findings/hypot-arc-integration.md` and
`port/findings/ellipse-full-hypot-integration.md`.

Partial units retain their owners and gaps. This run does not repeat the full
seven-function native/WASI/wasm libm qualification; corrected hypot's hosted
qualification completed before integration, and backend arithmetic is unchanged.
It also does not qualify live KiCad GUI/MCP workflows.

[verification.json](verification.json) records accepted/tested commits and trees,
commands and exit codes, runtime identity, per-unit case/output/binary hashes,
deferred owners and hashes of the retained full logs/reports. The full local
logs and reports remain in the private `rust-final-integration` validation
workspace; this compact artifact is not a golden baseline.
