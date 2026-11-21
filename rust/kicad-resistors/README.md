<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Resistor pilot preparation

Provisional unit `pcb_calculator.application.pure.p001`, at KiCad
`4cf939778df04d2b747bb2c4714fc5014a7565c7`. The final #39 generated record is an
integration dependency. This preparation neither claims a unit nor supplies a
unit TOML, golden, green status or phase-zero completion claim. The final record
must confirm the entire measured 1,312-line/40-definition scope, L0, dependencies
and gaps before those steps. Accounted UI headers are interface references;
their GUI implementations are outside this portable engine.

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

| Owned definitions (40 provisional) | Portable bodies and differential observations |
| --- | --- |
| `ESERIES_VALUES`, eight named constructors, `ESERIES_IN_DECADE` (10) | Literal tables, stride/fallback branches, full i32 exponents, multiplication before division; every literal and scaled bit is returned. |
| `RESISTANCE` (1) | Explicit parts, omitted/default and empty parts; exact part order and value/name. |
| `operator<` overloads (2) | Real helpers compare both resistance and scalar values; value-only sorting retained. |
| Collector constructor, `Add2RLookupResults`, `GetBest`, `considerSolution`, `uniqueCount`, `betterCandidate` (6) | Real search callbacks plus direct collector access: improvement, epsilon tie, no improvement, unique/part/name tie breaking, empty collector and continuation counts. |
| `maybeEmbrace`, serial/parallel value and four resistance helpers (7) | Nested/top-level parentheses, arbitrary parts, all combinations and expression order. |
| `strValue` (1) | Three-significant-digit rounding, six-significant-digit `g` conversion, every unit boundary, finite extremes, non-finite/zero inputs. |
| Calculator constructor, `SetSeries`, `NewCalc`, `Exclude`, `Calculate`, `GetResults`, build, prepare1/2, find, calculate2/3/4 (13) | Default and repeated state, every series/decade, exclusions/reset, exact/near/tie/mixed targets, full buffers, all four 4R topologies and actual private helper calls. |

`findIn2RBuffer` retains both post-lower_bound edge branches. For a nonempty,
sorted, finite positive generated buffer the earlier min/max checks make those
two branches unreachable; corrupting sorted state would violate the C++ domain.
The collector's `!m_best_solution` test in its epsilon-tie arm is likewise retained:
an initially infinite best deviation cannot produce a finite epsilon tie before
an improvement installs a solution. Neither is removed for coverage convenience.

## Reproduce and validate

`python rust/kicad-resistors/cases.py` regenerates the public fixed-seed corpus.
The initial corpus is prepared before final extractor ownership confirmation.
Run fmt, clippy `-D warnings`, tests and `wasm32-unknown-unknown` checks for both
new crates (the shim is native), and the normal real-oracle build in an owned
WSL rsync tree with the shared lock and at most eight jobs. Use the existing
`kicad-difftest compare`, then `propose`/`bless` only after live agreement and
verified executable/provenance identity. No golden/evidence is supplied until
that has happened. A test run's final counts and tested commit must be recorded
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
GPL-3.0-or-later derived work. No additional source is vendored.
