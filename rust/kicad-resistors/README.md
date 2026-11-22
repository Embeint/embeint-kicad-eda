<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Resistor pilot

Unit `pcb_calculator.application.pure.p001`, at KiCad
`4cf939778df04d2b747bb2c4714fc5014a7565c7`. Corrected #39 snapshot
`19035296e129cf8640e89331bc9e8ad84f0fe38c` records **1,126 physical lines,
40 owned definitions and seven files**, L0/pure, with no hard/soft dependencies
or gaps. Its exact source/functions/ranges/accounting are retained alongside
this pilot's port metadata and difftest registration. #39 is merged into `rust`
as `9a48bb6636`, whose tree equals the reviewed `328572624e`; every protected
field of this record is unchanged there. The parent accepted the owned
implementation and the corrected scope, and this pilot (#44) is merged into
`rust` as `cb4e5daa74`, so the status is `green` (see
[Acceptance status](#acceptance-status)). A passing pilot does not establish
phase-zero completion.

| Accounted path | Inclusive lines |
| --- | --- |
| `pcb_calculator/common_data.h` | 1–62 |
| `pcb_calculator/eseries.cpp` | 1–102 |
| `pcb_calculator/eseries.h` | 1–177 |
| `pcb_calculator/pcb_calculator_utils.h` | 1–33 |
| `pcb_calculator/resistor_substitution_utils.cpp` | 1–508 |
| `pcb_calculator/resistor_substitution_utils.h` | 1–168 |
| `pcb_calculator/units_scales.h` | 1–76 |

`GetReader` and `datafile_read_write.h:1:60` moved out of the pure record into
`pcb_calculator.application.thin.replacement.p001` (L0/thin-replacement). The
getter at header line 55 remains implemented as a documented reader/type co-port,
with all three real reader identity cases retained. Its owning thin unit's notes
record the co-port; that unit's other bodies and todo status are unchanged.
Parser constructors and Parse remain in `pcb_calculator.application.ui.p001`
(L136/UI); the supplied Rust constructor APIs remain explicitly Unported.
Neither the getter nor its header is counted among these 40 pure definitions.
Interface declarations do not claim out-of-unit bodies; frame/transline headers
are outside the pure unit.

`kicad-resistors` owns its calculator state and performs no I/O. The separate
`kicad-resistors-shim` is a native JSONL process (`portable = false`). No reference
source is edited. All floats, including non-finite values and signed zero, use
JSON **unsigned integer u64 IEEE754 bits** in both adapters. Results retain
resistance values, part order, names, optional presence, mutation order and
exceptions. There is no difftest tolerance. KiCad's `epsilon = 1e-12` remains
only in its selection/exclusion/tie algorithm.

## Reference compilation and test access

The CMake `resistor-reference` static library compiles pinned `eseries.cpp`
directly. Its second translation unit,
`port/oracle/reference/resistors_access.cpp`, includes the **unmodified pinned**
`resistor_substitution_utils.cpp` before defining test accessors. This is necessary
to name the source-local `SolutionCollector` and static helper functions. It is
one compilation of the real source, not a second implementation, source copy or
replacement algorithm in the JSON shim. The accessor object and eseries object
are linked from `libresistor-reference.a`; the shim calls those real definitions.
Both original source bytes and all accessor/header/build inputs are hashed in
provenance. `thirdparty/fmt` is exported from the same pin and actually compiled
and linked. Strict FP flags and all existing kimath/source/provenance guards stay
enabled. Audit regressions refuse absent reference compilation, missing archive
links and an accessor that omits the pinned source include.

`calculator-reader-reference` additionally compiles the actual unchanged
`datafile_read_write.cpp`, `dsnlexer.cpp`, `richio.cpp` and `exceptions.cpp`.
The upstream `TokenList2DsnLexer.cmake` generates the actual lexer from pinned
keywords. The audit independently regenerates both outputs in a temporary
directory and compares every byte without repairing a corrupted output; both
generated hashes and the generation command enter provenance. Every original
source/header and generator input is exported and hashed. Function/data sections
and linker garbage collection discard unused GUI/parser bodies; no GUI code is
ported into Rust and no application is launched. Missing real compilation or the
reader archive link is a refusal regression.

Getter cases construct real `PCB_CALCULATOR_DATAFILE_PARSER` objects using the
original constructor and real `STRING_LINE_READER` instances. Null, aliased and
distinct equal-content readers, repeated calls and protected-reader rebinding
are compared through the actual thin-owned inline `GetReader` method. Stable caller
indices encode pointer identity, not pointer addresses or guessed values.
`reader::DatafileParserReader` is the minimal borrowed reader-state co-port; its
getter returns the actual current nullable reference, without copying a reader
or taking ownership. `from_lexer_state`/`bind_lexer_state` are explicit host
state boundaries, not implementations of parser constructors or PushReader.
Both actual parser constructors remain explicit `Err(Unported { ... })` APIs
owned by `pcb_calculator.application.ui.p001`; the getter never calls them.

Private member pointers use standard C++ explicit instantiation: the access
checks do not apply to explicit-instantiation arguments ([temp.explicit]). No
`#define private public`, header rewrite or duplicated C++ algorithm is used.
The accessor permits forcing each real 3R/4R helper even when public `Calculate`
short circuits. It also calls the real collector with arbitrary candidate part
counts, reaching its otherwise unreachable part-count tie breaker. Its identity
callback counts actual collector continuation invocations; the separate search
cases exercise every real callback expression in `calculate2/3/4RSolution`.

The portable sort follows libstdc++ introsort, including equal-value order,
median selection, heap fallback and final insertion. A different sort can change
the two entries chosen by lower_bound, and thus expressions and part order. The
Linux oracle is the comparison target; MSVC sorting is not qualified here.

FDlibm's general `pow` differs by a bit from the Linux glibc reference for some
decimal exponents. This unit only uses integer exponents. `eseries::power10`
therefore uses portable decimal literals covering every representable power,
preserving glibc's two deviations from correctly rounded decimal conversion at
23 and 210 explicitly. Exponents outside that range return the same zero/infinity.
The corpus compares every returned bit across the whole representable range and
full-i32 extremes, for all eight series. `str_value` also preserves glibc's
positive-NaN spelling for a negative logarithm. These are reference compatibility
behaviours, not comparator normalization or accuracy improvements.

## JSONL input contract

Every request uses the existing exact `{unit,case,input}` envelope. All counts
below are transport bounds, not changes to the reference algorithm. Unknown or
missing keys are refused. Arrays and names are checked before allocation/iteration.

| `input.op` | Fields and result |
| --- | --- |
| `series` | `series: i32`, `decades: [i32]` (up to 1,024); returns literal base values, all eight actual named constructors when selected, and scaled value bits for each exponent. Unknown selectors use the actual E1 fallback. |
| `format` | `values_bits: [u64]` (up to 4,096); returns complete `strValue` strings. |
| `resistance` | `values: [resistance input]` (up to 256); returns constructor results. |
| `helpers` | `first`, `second` resistance inputs; `text` (up to 4,096 UTF-8 bytes), `symbol` (one ASCII byte); returns real unique counts, both better-candidate decisions, both comparison overloads, parentheses and all four simple/full series/parallel combinations. |
| `collector` | `target_bits`, `candidates` (up to 256 resistance inputs); returns two complete repeated `GetBest` copies and the real continuation count, or the reference exception. |
| `calculator` | `actions` (up to 64); returns each action's return value and state immediately afterward. |
| `reader` | `readers` (up to 16 `{text,source}` objects, each string at most 4,096 bytes), `parsers` (up to 16 nullable reader indices), `actions` (up to 64 `{op,parser,reader?}`). `get` observes and `bind` supplies surrounding reader state. Every result returns actual reader identity, identity with the initial reader, and repeated-get identity. |
| `unit_scales` | Returns the exact bits of all 36 literal unit conversion macros in the accounted `units_scales.h`, retaining each C++ expression order. |

A resistance input is `{value_bits,name,parts_bits?}`. Names have at most 4,096
UTF-8 bytes; parts have at most 256 entries. Absent or empty parts exercise the
actual constructor default. Test-only `clear_parts: true` subsequently clears the
public vector, permitting defined empty-vector helper tests. Constructor inputs
can contain all IEEE values. A sorted parts vector containing NaN and two distinct
non-NaN numbers violates `std::sort`'s strict-weak-order precondition and is refused
before sorted collector/helper probes; NaN with at most one numeric value is
allowed. This restriction does not apply to plain constructor transport.

Calculator actions:

| `op` | Fields/behaviour |
| --- | --- |
| `state` | Observe state and results without initialization. |
| `set_series` | `series: u32`; preserves the complete setter range and defers indexing until a later operation. Calculator indexing supports E1 through E24 only. |
| `new` | `target_bits`; calls `NewCalc`, resets masks and results, and retains old buffers exactly as C++ does. |
| `exclude` | `values_bits` (up to 1,024); calls real exclusions in order. NaN and nonmatches remain valid before `NewCalc`. |
| `prepare` | Calls real 1R and 2R buffer builders; an empty resulting buffer is defined. |
| `calculate` | Calls the complete real public calculation. |
| `solution` | `level: 2/3/4`; force the actual private search. Empty 3R/4R collectors return their real exception. |
| `find` | `values_bits` (up to 1,024); returns both complete real adjacent entries, including NaN and boundary handling. |
| `build` | `series: i32`; calls the real private table builder with the actual E-series constructors, including the larger standalone series. |
| `build_values` | `values: [u16]` (up to 256); mutates an actual C++ `ESERIES_VALUES` vector and calls the real private builder. Nonstandard first values, unsorted/repeated members, zeros and zero-first vectors are retained. Empty vectors would index outside the C++ vector; all-zero vectors never reach its exit condition. Both are transport refusals, not passing reference exception cases. |
| `buffers` | Returns every 1R/2R entry, in actual sorted order. |

Each state includes series, target bits, every mask flag, both buffer counts and
all three optional results (each complete resistance). Dedicated buffer probes
add the entire private buffer contents. No entries are sampled or normalized.

Bounds on calculator tables (10 ohm to 1 Mohm, E1 through E24) are source
behaviour. Standalone E-series and the private builder accept larger series.
An uninitialized matching exclusion, invalid series indexing, or lookup in an
empty buffer is a harness `invalid_input` failure because the C++ would access
outside a vector. These are not legitimate reference refusals and do not count
as passing differential cases. `SolutionCollector::GetBest` really throws
`logic_error("Empty solution collector")`; the adapter catches that as an
observable **result**, retaining any state mutations before it. It is distinct
from a deferred or failed harness operation.

## Definition and branch audit

| Owned definitions (40 in the corrected snapshot) | Portable bodies and differential observations |
| --- | --- |
| `ESERIES_VALUES`, eight named constructors, `ESERIES_IN_DECADE` (10) | Literal tables, stride/fallback branches, full i32 exponents, multiplication before division; every literal and scaled bit is returned. |
| `RESISTANCE` (1) | Explicit parts, omitted/default and empty parts; exact part order and value/name. |
| `operator<` overloads (2) | Real helpers compare both resistance and scalar values; value-only sorting retained. |
| Collector constructor, `Add2RLookupResults`, `GetBest`, `considerSolution`, `uniqueCount`, `betterCandidate` (6) | Real search callbacks plus direct collector access: improvement, epsilon tie, no improvement, unique/part/name tie breaking, empty collector and continuation counts. |
| `maybeEmbrace`, serial/parallel value and four resistance helpers (7) | Nested/top-level parentheses, arbitrary parts, all combinations and expression order. |
| `strValue` (1) | Three-significant-digit rounding, six-significant-digit `g` conversion, every unit boundary, finite extremes, non-finite/zero inputs. |
| Calculator constructor, `SetSeries`, `NewCalc`, `Exclude`, `Calculate`, `GetResults`, build, prepare1/2, find, calculate2/3/4 (13) | Default and repeated state, every series/decade, exclusions/reset, exact/near/tie/mixed targets, full buffers, all four 4R topologies and actual private helper calls. |

Separate co-port: `PCB_CALCULATOR_DATAFILE_PARSER::GetReader` (thin owner above)
returns nullable borrowed reader identity. Real constructed C++ parser/readers
exercise null, aliasing, rebinding and repeated access without fabricated layouts.

`findIn2RBuffer` retains both post-lower_bound edge branches. For a nonempty,
sorted, finite positive generated buffer the earlier min/max checks make those
two branches unreachable; corrupting sorted state would violate the C++ domain.
The collector's `!m_best_solution` test in its epsilon-tie arm is likewise retained:
an initially infinite best deviation cannot produce a finite epsilon tie before
an improvement installs a solution. Neither is removed for coverage convenience.

## Reproduce and validate

`python rust/kicad-resistors/cases.py` regenerates the 1,050-case public fixed-seed
corpus; `--check` verifies its bytes without writing. The original 199 request
lines retain SHA-256
`ee77e9687eb15556fa222c0c3b57e86bf2be7523a8c8c465a2eb9548f7103d15`.
Decimal conversion replaces host-libm exponentiation; explicit historical input
bits at exponents 23 and 126 preserve the published Windows corpus. Exponent
210 uses correctly rounded decimal conversion. This input compatibility table
is separate from the engine's measured C++ `pow` table. The generator's tests
pin the original prefix, historical powers and literal integer `-0` requests.

The additional cases include the complete 28-by-28 IEEE operand matrix (signed
zeros, subnormal/normal extremes, infinities and signed quiet/signaling NaNs),
64 fixed-seed NaN payload pairs, explicit formatting classes and raw `-0` tokens.
`fp.rs` explicitly implements the observed pinned GCC 13/15 Release x86-64 semantics:
a NaN operand is quieted with its sign and payload retained, choosing the second
operand when both are NaNs; an invalid operation without an input NaN produces
x86's negative indefinite NaN. Ordinary Rust arithmetic does not guarantee
this payload choice. Constructor values and parts remain verbatim. Formatting
also spells these measured exceptional cases explicitly for wasm portability;
neither adapter nor comparator normalizes returned bits. GCC O0 and Clang 18
NaN payload equivalence is not claimed; independent review observed differences.

Both owned adapters use explicit typed guards with matching complete diagnostic
envelopes. `kicad-resistors-shim/tests/requests.py` exercises malformed fields,
validation order, raw integer `-0` versus floating `-0.0`, full unsigned domains,
finite numbers outside integer storage ranges, parser overflow before identity
extraction, and successful recovery. Run it with `--oracle PATH` and one or more
`--rust PATH` arguments. The native shim calls the reviewed common
`kicad_difftest::request::parse` API before extracting identities (reviewed #45
at `8f5f45729c733d8f1695dda9ee6d953f9bb4d400`, now merged into `rust` as
`f72e08a883509c0bf451b00905718a9a83249fb1`); the exact comparator retains arbitrary
numeric tokens. Finite numbers outside integer storage ranges are parsed and
then checked against each operation's actual domain.

Merged #48 (`rust` at `1a2fb924d539d63a68f5696191907dc2aa6d6370`) builds request
values directly from the validated exact tree, so objects whose keys spell
serde's private `Number` or `RawValue` markers stay ordinary objects, as in the
C++ adapter. `port/oracle/fixtures/resistor-private-keys.*.jsonl` hold 889 literal
request envelopes and the complete replies recorded from the pinned C++ adapter:
both marker keys (combined and with near-miss spellings) carrying
encoded valid/invalid JSON, numeric and overflow strings and nonstring values, at
the envelope, identities, input root, operation, every bits field, the series
setter, custom vectors, collector target and candidates, reader and helper
fields. Duplicate keys and literal overflow remain null-identity refusals;
plain strings stay strings; 64 successes are interleaved for recovery. The
canonical CTest `oracle-resistor-private-keys` requires byte-identical C++
replies, `tests/private_keys.rs` replays them through the built shim process and
`tests/requests.py` appends them after its original 71 cases. These refusals are
adapter controls, not registered difftest cases. The historical authoring script's
escaped-key constants were plain keys, so its claimed escape coverage was absent.
The separate `resistor-private-keys-escaped.*.jsonl` fixtures add the independent
review's 199 literal escaped-dollar, escaped-colon, fully encoded and mixed-spelling
duplicate requests. All default guards include both sets: 1,088 private-key
requests, or 1,159 with the original 71 controls. The escaped request generator
and raw-spelling assertions prevent silent decoding of the fixtures.

The source manifest and binary provenance in the historical `ff419ad71f` proof
are exact, but 875/913 reference files contain CRLF conversions of the raw Git
blobs; the proof's LF claim is withdrawn. See
`port/findings/resistors-escaped-key-and-export-addendum.md`. The separately
recorded combined integration uses a raw-LF export and reproduces the same output.

The `nonfinite_parity` example reuses the pure operation conversions from the
native shim, embeds the complete case/golden files and compares every result,
including state, expression and part order, in a wasm runtime. After a matching
live baseline exists, build it with `cargo build -p kicad-resistors --example
nonfinite_parity --target wasm32-unknown-unknown --release` and run
`node rust/kicad-resistors/wasm-test.mjs PATH_TO_WASM`. It requires no host imports
and performs no filesystem/process/thread operations. A wasm build alone does
not certify bit parity; record actual runtime results separately.

Source checkpoint `af47b0e73160ed7672846d8b9f329b08465a38d4` passed workspace
fmt/clippy, 110 tests, native debug/release and registered 1,050-case exact
comparisons, 63 complete request-envelope checks, every portable wasm build and
all 1,050 operation results in Node's wasm runtime. The original 199 request
bytes/results and both historical evidence files remain unchanged. See
[`resistors-review-fixes-af47b0e731.md`](../../port/findings/resistors-review-fixes-af47b0e731.md)
for all five review fixes, executable identities, timings and coverage limits.
Fresh complete evidence records the standard live `propose`/`bless`/`check`
sequence against an absent base golden. At that checkpoint the unit stayed
claimed and phase zero was incomplete.

Corrected-scope integration at clean source
`1b42b8368eaf369102b8fc2f58a8af3428eb6a66` passed 112 workspace tests, four
actual oracle CTests (including the parent collector-target refusal fix), 71
complete native envelopes, all 1,050 registered live cases, portable wasm builds
and the corrected 1,801-record source validator. No case, golden or historical
evidence byte changed and no reblessing was needed. Fresh source/provenance and
executable identities are in
[`resistors-corrected-integration-19035296e1.md`](../../port/findings/resistors-corrected-integration-19035296e1.md).
Final independent #39 review and parent graph acceptance were holds at that point.

## Acceptance status

#39 passed its independent reviews and is merged into `rust` as `9a48bb6636`. The
parent accepted:

- the owned 40-definition implementation and the corrected scope;
- the NaN qualification, which covers GCC 13/15 Release x86-64 only (no GCC O0,
  Clang or NaN payload claim);
- the 1,050-case native and wasm parity and the request protocol reviews.

The unit is therefore `green`. The GetReader thin owner stays `todo`.

The phase zero integration (#49) checked this unit together with the MD5 and
IEC60664 pilots in one oracle and workspace. Runtime `7207de24ed` passed all
1,050 registered cases among 21,213, with zero deferrals or failures, and 1,159
complete replies on the oracle and both shim profiles; see
[`phase0-final-integration-7207de24ed.md`](../../port/findings/phase0-final-integration-7207de24ed.md).

All three pilots are merged into `rust`. This one (#44) merged as `cb4e5daa74`,
whose tree equals the reviewed head `5275f29be6`, after its independent reviews
and passing head CI. The hosted
[rust-port run](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45015332781/job/137186313699)
at that head built and audited a fresh oracle (`9feca7f0...`), passed 8 CTests
and all 21,213 registered cases with zero deferrals. The record was still
`claimed` and the flag `false` in that run, so its phase-zero summary read
INCOMPLETE 2/3. The strict hosted CI run of the combined integration is the
phase-zero acceptance authority and is pending. Green alone does not establish
phase-zero completion. All earlier evidence above stays as recorded.

## Dependencies and licences

All new code is GPL-3.0-or-later. `libm 0.2.16` is MIT and already locked in the
workspace; it is the only portable library dependency. The parity example uses
the existing MIT/Apache-2.0 `serde_json` closure as a dev-dependency only.
Native shim dependencies reuse
the existing `serde_json` and `kicad-difftest` closure documented in their README
and `rust/MCP.md` (GPL-compatible MIT/Apache-2.0, with existing transitive notices).
Pinned KiCad resistor sources are GPL-3.0-or-later. Pinned fmt is MIT
(`thirdparty/fmt/LICENSE`), built
from the exported pin rather than a system package. The libstdc++ sort adaptation
is GPL-3.0-or-later with the GCC Runtime Library Exception, compatible with this
GPL-3.0-or-later derived work. Its original FSF, Hewlett-Packard and Silicon
Graphics notices are retained in `src/sort.rs`; the GPL text is at the repository
root (`LICENSE.GPLv3`) and the exception is in `COPYING.RUNTIME`. Native reader
support uses KiCad's GPL-2.0-or-later sources under GPL-3.0-or-later and pinned
fast_float headers (MIT OR Apache-2.0 OR BSL-1.0; all three notices exported).
No additional dependency source is vendored into the portable crate.
