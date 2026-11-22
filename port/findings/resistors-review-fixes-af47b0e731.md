<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Resistor pilot review fixes and validation

Tested source: `af47b0e73160ed7672846d8b9f329b08465a38d4`.
Reviewed previous head: `84dbbc9d8eaa757675ffb7f593a48e6d96ead42a`.
Actual C++ pin: `4cf939778df04d2b747bb2c4714fc5014a7565c7`.
Linux x86_64, GCC 15.2.0, glibc 2.43, Rust 1.99.1. The own WSL
`pilot-resistors` source/build trees used the shared mkdir lock with traps and
at most eight compilation jobs. The completed jobs released their acquired lock.

All five findings have implementation and focused regressions. No valid domain
was removed and no exact-comparison rule was changed. The unit remains
**claimed and provisional** under the corrected #39 ownership hold. The a7
snapshot still records 1,186 lines, eight files and 41 definitions, L0/pure with
no dependencies/gaps; those protected fields are unchanged, not newly certified.
The #39 owner is correcting further source-audit findings. Any newly owned
definitions/dependencies require a new audit and honest completion before merge.

## Review findings

| Finding | Actual change | Regression and observed result |
| --- | --- | --- |
| Two distinct NaN payloads disagree | `rust/kicad-resistors/src/fp.rs` explicitly preserves the pinned GCC helper's second-NaN preference, quiet bit, sign and payload; generated invalid operations without input NaNs retain the measured x86 negative indefinite. Serial/parallel callbacks and zero-first custom builder scaling use these operations. Exceptional `strValue` spellings are explicit for wasm. Stored constructor/part bits remain verbatim. | `tests/nonfinite.rs`; complete 28-by-28 IEEE matrix, 64 generated payload pairs and explicit formatting classes. All four combinations agree with actual compiled C++ in debug/release and wasm. The separate 848-request reference probe checked 652 NaN-input requests and 132 generated-invalid combination results. No arbitrary payload canonicalization occurs. |
| Raw JSON integer `-0` rejected | Native `operations.rs::unsigned` accepts a nonnegative signed integer as well as full u64. Floating `-0.0` remains distinct and follows the C++ refusal. | Two literal-token cases in the 1,050-case corpus, unsigned unit tests and `tests/requests.py` check bits and u32 series; full u64/u32 domains retained. |
| Malformed diagnostics differ | Explicit typed `Field`/`Text`/`Boolean`/`Integer` guards in the owned C++ adapter align with Rust guards; resistance and collector validation order follows C++. Original pinned library bodies remain unchanged. | Native tests and complete-envelope request comparisons cover missing/wrong op, reader text type, clear-parts boolean, missing initial solution level and multiple-invalid-field priority. Complete messages and identities agree, not only error kinds. |
| Numeric overflow parsed before domain dispatch | Native dispatch calls reviewed `kicad_difftest::request::parse` before extracting identities. Shared `exact::parse` remains unchanged. | Full null-identity `invalid_request` envelopes for overflow in bits, unit/case, nested unused input, extra envelope fields and root scalars. Finite outside-u64 numbers, underflow, long finite tokens and full-u64 inputs still parse and then follow operation-domain validation. All 63 focused native requests agree in both Rust builds. |
| Host-dependent generator powers | `cases.py` uses decimal conversion plus explicit historical input bits at 23/126, separately from the engine's glibc pow table. No host-libm exponentiation remains. | Windows and Linux regeneration have the same full SHA-256. `test_cases.py` pins historical boundaries and the original prefix; native Cargo `tests/generation.rs` executes regeneration with `--check` in normal workspace tests. |

PR #45 was tested at reviewed commit
`8f5f45729c733d8f1695dda9ee6d953f9bb4d400` and is now merged into `rust` as
`f72e08a883509c0bf451b00905718a9a83249fb1`. The reviewed helper/test contents are
byte-identical. It is a satisfied infrastructure dependency; #39 remains the
provisional scope dependency. The tested source checkpoint is retained, without
rewriting its identity for commit-message cosmetics.

## Completed gates

| Gate | Actual result |
| --- | --- |
| Workspace fmt and clippy | `cargo fmt --all --check`; `cargo clippy --locked --workspace --all-targets -j8 -- -D warnings` passed |
| Workspace tests | 110 passed, zero failed/ignored, including shared-parser C++ fixture and deterministic generator checks |
| Reference build/audit | 70 audited compilations, 913 unchanged pinned exports; strict `-fno-fast-math -ffp-contract=off`; three ctests passed including seven guard regressions |
| Native exact comparison | 1,050/1,050 C++ versus Rust debug and release, zero errors/deferrals; unchanged 60-second process budget |
| Native complete envelopes | 63 focused raw-token, malformed-input, overflow/fallback and recovery requests matched C++ and both Rust executables |
| Portable builds | Every discovered portable crate built for `wasm32-unknown-unknown`: embeint-layout, embeint-web, kicad-resistors |
| Actual wasm runtime | All 1,050 complete operation results matched the live C++ fixture in Node 22.22.1 / V8 12.4.254.21-node.35, without host imports |
| Standard evidence flow | Fresh absent-golden root; live `compare`, `propose`, reason, `bless`, `check`; 1,050 complete initial-baseline changes, all old values null |
| Registered CI entrypoint | All 1,050 cases matched live reference, Rust and golden; stored 1,570-record source accounting passed |

The corpus contains 12 series, nine formatting (15,502 values), one constructor,
860 helper, seven collector, 157 calculator, three reader and one scale request.
There are 148 successful public calculations and 54 successful forced private
solutions. Eighteen actual Empty solution collector exceptions are compared as
results (15 calculations, two private searches, one direct collector). Separate
invalid-input refusals are not counted as successful algorithm work.

One complete-corpus measurement took C++ 0.301754 s, Rust debug 2.607326 s and
Rust release 0.475170 s. These are observed local timings, not a general benchmark.
All stay below the unchanged process budget; no expensive case was dropped.

## Executable and immutable evidence identities

| Artifact | SHA-256 |
| --- | --- |
| Actual C++ oracle | `6bc97c0160e724de4a8106683e3983d8936c7902cfa5ce731f041b6450b81184` |
| Rust debug shim | `cc9b410fcb8a0463f8acc5fb5825459b7e882b51737c79cae3b73946ff3e0862` |
| Rust release shim | `61fa3cd93085f9f85aa5becabdc41d7a6a1a14c1bb6235acbca96b96a59d0c11` |
| Wasm parity executable | `2a492466ad5a3ffe64ba443b34d0020dd82a22b8a81216357597231ef874e4a2` |
| Complete 1,050-case input | `58dd1186c9f10c8efb54bc90102c71508c8e852c0b8c08d437380b112fe3c597` |
| Complete initial-baseline evidence | `2983c643c643fb6f2ba00c85ae6edd4ed13f77669cafd09e66ad50fe488f9479` |

The live oracle's `--provenance`, built provenance and evidence provenance match
exactly: overlay revision is the tested source, dirty-at-export is false, and all
native source/link/generated inputs remain recorded. Retained executable bytes
were rehashed against the actual reports. No algorithm rerun was needed to
inspect those completed results.

The new immutable artifact is
[`pcb_calculator.application.pure.p001-review-fixes-complete.json`](../goldens/evidence/pcb_calculator.application.pure.p001-review-fixes-complete.json).
It covers the entire initial baseline against both intended base `0ab6a0e8635421998a6471d1001fbf7b44c81e71`
and a7 dependency `a7ec77beffd3b2e5d60409423fa28b6c116c6e00`, where no unit golden
exists. It is not a delta-only blessing against the old 199-case local golden.
All original 199 request bytes, golden reply bytes and exact results remain
unchanged. The historical 198-case and 199-case complete evidence artifacts remain
byte-identical to the previously published head.

The [inspection summary](resistors-review-fixes-af47b0e731.json) records hashes and
counts. The [retained logs and reports](resistors-review-fixes-af47b0e731/review-prep.log)
include prep, baseline, final, driver, native comparison, registered gate and
runtime reports. Their completion markers were checked together with actual
case/golden/evidence bytes, provenance and executables rather than trusted alone.

## Limits and remaining hold

Wasm execution covers the portable operations and complete successful corpus;
native transport parsing/refusals use the separate 63-request envelope suite.
GCC/x86-64 is the measured reference for NaN propagation and libstdc++ sorting;
no other C++ compiler/architecture or wasm engine is certified by this run.
Actual parser constructors remain explicitly Unported in their provisional
owning UI unit; real C++ reader fixtures and the portable borrowed getter remain
covered without fabricating constructor success.

The registered report remains **phase0 INCOMPLETE, 0/3 green**, with
`PHASE0_REQUIRED=false`. Stored parse/configuration diagnostics remain visible.
Passing the structural a7 graph check does not certify its semantic omissions.
Corrected #39 scope, ownership, dependencies and layer must be reconfirmed and
any additional owned work completed; a new independent review of the published
fixes remains required. No merge or readiness claim follows from this evidence.
