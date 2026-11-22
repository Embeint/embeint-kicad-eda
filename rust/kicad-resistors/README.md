<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Resistor pilot preparation

Unit `pcb_calculator.application.pure.p001`, at KiCad
`4cf939778df04d2b747bb2c4714fc5014a7565c7`. The local finalizer record contains
**1,186 physical lines and 41 owned definitions**, L0/pure, with no hard/soft
dependencies or gaps. The committed #39 head remains an integration dependency.
This preparation neither claims a unit nor supplies a unit TOML, green status
or phase-zero completion claim. The committed record must confirm the
complete local scope before those steps:

| Accounted path | Inclusive lines |
| --- | --- |
| `pcb_calculator/common_data.h` | 1–62 |
| `pcb_calculator/datafile_read_write.h` | 1–60 |
| `pcb_calculator/eseries.cpp` | 1–102 |
| `pcb_calculator/eseries.h` | 1–177 |
| `pcb_calculator/pcb_calculator_utils.h` | 1–33 |
| `pcb_calculator/resistor_substitution_utils.cpp` | 1–508 |
| `pcb_calculator/resistor_substitution_utils.h` | 1–168 |
| `pcb_calculator/units_scales.h` | 1–76 |

Interface declarations do not claim their out-of-unit bodies. The frame and
transline headers from the early audit are not owned by this final local unit.

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
are compared through the actual owned inline `GetReader` method. Stable caller
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
| `collector` | `target_bits`, `candidates` (up to 256 resistance inputs); returns best resistance and real continuation count, or the reference exception. |
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

| Owned definitions (41 local final) | Portable bodies and differential observations |
| --- | --- |
| `ESERIES_VALUES`, eight named constructors, `ESERIES_IN_DECADE` (10) | Literal tables, stride/fallback branches, full i32 exponents, multiplication before division; every literal and scaled bit is returned. |
| `RESISTANCE` (1) | Explicit parts, omitted/default and empty parts; exact part order and value/name. |
| `operator<` overloads (2) | Real helpers compare both resistance and scalar values; value-only sorting retained. |
| Collector constructor, `Add2RLookupResults`, `GetBest`, `considerSolution`, `uniqueCount`, `betterCandidate` (6) | Real search callbacks plus direct collector access: improvement, epsilon tie, no improvement, unique/part/name tie breaking, empty collector and continuation counts. |
| `maybeEmbrace`, serial/parallel value and four resistance helpers (7) | Nested/top-level parentheses, arbitrary parts, all combinations and expression order. |
| `strValue` (1) | Three-significant-digit rounding, six-significant-digit `g` conversion, every unit boundary, finite extremes, non-finite/zero inputs. |
| Calculator constructor, `SetSeries`, `NewCalc`, `Exclude`, `Calculate`, `GetResults`, build, prepare1/2, find, calculate2/3/4 (13) | Default and repeated state, every series/decade, exclusions/reset, exact/near/tie/mixed targets, full buffers, all four 4R topologies and actual private helper calls. |
| `PCB_CALCULATOR_DATAFILE_PARSER::GetReader` (1) | Direct nullable borrowed-reader return; real constructed C++ parser/reader objects, exact identity, aliasing, rebinding and repeated calls. |

`findIn2RBuffer` retains both post-lower_bound edge branches. For a nonempty,
sorted, finite positive generated buffer the earlier min/max checks make those
two branches unreachable; corrupting sorted state would violate the C++ domain.
The collector's `!m_best_solution` test in its epsilon-tie arm is likewise retained:
an initially infinite best deviation cannot produce a finite epsilon tie before
an improvement installs a solution. Neither is removed for coverage convenience.

## Reproduce and validate

`python rust/kicad-resistors/cases.py` regenerates the 198-case public fixed-seed corpus.
The initial corpus is prepared before final extractor ownership confirmation.
Run fmt, clippy `-D warnings`, tests and `wasm32-unknown-unknown` checks for both
new crates (the shim is native), and the normal real-oracle build in an owned
WSL rsync tree with the shared lock and at most eight jobs. Use the existing
`kicad-difftest compare`, then `propose`/`bless` only after live agreement and
verified executable/provenance identity. A baseline records that live agreement;
it does not publish a unit claim or complete the extractor integration. A test
run's final counts and tested commit must be recorded
separately; this source/case preparation is not a result claim.

## Dependencies and licences

All new code is GPL-3.0-or-later. `libm 0.2.16` is MIT and already locked in the
workspace; it is the only portable dependency. Native shim dependencies reuse
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
