<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# IEC60664 calculator parity pilot

`kicad-iec60664` is a portable, dependency-free translation of `IEC60664` at
KiCad `4cf939778df04d2b747bb2c4714fc5014a7565c7`. It preserves the reference's
IEC60664-1:2020-05 tables and computation order. It makes no safety certification
claim. It uses the reference's scalar units directly, including the different
voltage scales used by different helpers; it performs no unit conversion.

Unit: `pcb_calculator.calculator_panels.application.pure.p001`. **Provisional
dependency snapshot:** independent #39 review requires graph corrections for
virtual destruction, active generated definitions and wx aliases. Corrected
ownership, scope and layers must be reconfirmed by the parent before merge; none
is frozen by this pilot. No scope is reduced to preserve L0. The #39
record at `a7ec77beffd3b2e5d60409423fa28b6c116c6e00` (blob
`d4983c3c8cd1bbfe65a6889048dbaf833228b96b`) accounts for **1,943 physical lines
across nine files, with the same
28 IEC60664 definitions**, layer 0, pure, no hard/soft dependencies and no coverage
gaps. The actual Rust module is `kicad_iec60664::iec60664`; root reexports preserve
existing callers. All paths below are in `pcb_calculator/calculator_panels/`:

| File | Inclusive lines | Ownership |
| --- | --- | --- |
| `iec60664.cpp` | 1–1355 | calculator bodies |
| `iec60664.h` | 1–137 | calculator state, enums, inline bodies and declarations |
| `panel_board_class.h` | 1–45 | interface only |
| `panel_color_code.h` | 1–44 | interface only |
| `panel_electrical_spacing.h` | 1–43 | interface only |
| `panel_electrical_spacing_iec60664.h` | 1–50 | interface only |
| `panel_electrical_spacing_ipc2221.h` | 1–48 | interface only |
| `panel_eseries_display.h` | 1–156 | interface only, including UI colour constants/member declarations |
| `panel_galvanic_corrosion.h` | 1–65 | interface only |

These are the finalizer's full ranges, including the additional headers; the
provisional 1,648-line audit is superseded. `panel_r_divider.h` is outside this
final unit. Every listed header span was audited against the pin. No GUI
implementation is ported: the panel methods and `CORROSION_TABLE_ENTRY`
constructor have declarations here and bodies in their own UI units. UI colour
constants and member declarations in the E-series display describe that panel's
interface. None is referenced by an IEC60664 body or is a separate owned function;
no GUI type/constant co-port is needed by this calculator.

The state owns only scalar values. Construction reproduces all member initializers;
destruction, like the empty C++ destructor, releases no external resource. The
Rust category newtypes retain all signed 32-bit enum values, including unnamed
values which exercise default arms. The unused `WAVEFORM` enum is retained too.
No I/O, unsafe code, global state, allocation, FFI, or unported call exists in the
engine. The native `kicad-iec60664-shim` has explicit `portable = false` metadata.

## Reference build

The oracle adapter compiles the unchanged pinned `iec60664.cpp` into
`iec60664-reference` and links that archive into the existing small KiCad oracle.
`pin.py` extends the export by exactly that `.cpp` and its `.h`; neither the
application nor a GUI panel is compiled. The source/flag audit additionally
requires the calculator compile target and archive link. Every reference/overlay
object and link retains `-fno-fast-math -ffp-contract=off` and the existing source
hash and flag rejection checks. The live provenance records the extended source
manifest, target commands and overlay hashes.

The shim calls public methods directly. Standard C++ explicit-instantiation access
(C++20 [temp.spec], access checking exception for explicit instantiation) obtains
member pointers for all nine private helpers and the input state. This changes
neither pinned source nor class layout and uses no access-control compiler flag
or `private` macro. The dormant CTI and RMS helpers can therefore be checked
directly against the live oracle. Helper default values (`aDistIso = 100`,
`aField = FIELD::INHOMOGENEOUS`) are audited against the pinned declaration and
passed explicitly by the C++ member-pointer adapter; Rust receives `None` for the
corresponding omitted JSON arguments. There is no inaccessible-helper gap.

## JSONL contract

The normal unit/case/input envelope contains `input: {"steps": [...]}`. Each case
constructs an independent calculator, executes 1 to 256 ordered steps and returns
an ordered array. Each element is `{"value": ..., "state": {...}}`, including
the complete 16-field state **after** that operation. Public getters supply the
three calculated distances. Setters do not implicitly recompute.

Every binary64 input is a JSON **u64 integer containing its IEEE754 bits**. All
binary64 results and state fields use the identical encoding (`std::bit_cast`
and `f64::to_bits`). This retains signed zero, subnormals and every output bit,
without depending on decimal formatters. No tolerance, rounding, arithmetic
normalization or numeric coercion is applied. Inputs must encode finite numbers;
unnamed enums are signed 32-bit integers. Wrong fields/types, non-finite input,
empty/oversized step lists and unknown operations are harness errors. They are
not the reference's legitimate negative/sentinel results.

| Operation | Arguments beyond `op` | `value` |
| --- | --- | --- |
| `state` | none | null |
| `categories` | none | all named enum values, including waveform |
| `set` | `field`, `value` | null |
| `compute` | none | actual boolean return |
| `impulse` | none | binary64 bits |
| `groove` | `pd`, optional `distance` bits | binary64 bits |
| `altitude` | `voltage` bits (altitude scalar) | binary64 bits |
| `rms` | `voltage` bits, optional `field` | binary64 bits |
| `cti` | `voltage` bits (CTI scalar) | material-group integer |
| `transient` | `voltage` bits, `pd`, `field` | binary64 bits |
| `peaks` | `voltage` bits, `field` | binary64 bits |
| `basic_creepage` | `voltage` bits, `pd`, `mg` | binary64 bits |
| `clearance` | `pd`, `field`, `altitude` bits | binary64 bits |
| `creepage` | `pd`, `mg` | binary64 bits |

Setter fields are `altitude`, `frequency`, `rated_voltage`, `rms_voltage`,
`peak_voltage`, `transient_voltage` (binary64 bits); `pollution_degree`,
`material_group`, `field`, `insulation_type`, `overvoltage_category` (enum integer);
and `pcb_material` (boolean). The source exposes no coated setter.

## Corpus and audit

Run `python rust/kicad-iec60664/cases.py` from the Windows checkout or a normal
Linux Git checkout; `--check` verifies byte-for-byte reproducibility. It reads
the source at the pin with Git, not the Rust implementation. Cases include
`nextafter` below/at/above every source threshold, all named enum combinations,
unnamed enum defaults, both PCB paths, every preferred reinforcement transition,
scaled peak/transient boundaries, extreme finite values and signed zeros,
frequency below/at/above 30 kHz, repeated Compute and mutation sequences, and
128 fixed-seed sequences. Cases contain only public generated scalars.

| Upstream symbols | Implemented behavior | Live case families | Gap |
| --- | --- | --- | --- |
| constructor/destructor, 12 setters, three distance getters | every initial value; direct mutation; independent instances; empty destruction | `default-*`, `mutation-*`, `fixed-seed-state-*`, all step snapshots | none |
| `GetMinGrooveWidth` overload | nonpositive refusal, division below 3, every PD/default, argument default | `groove-*`, `compute-*` | none |
| `GetClearanceAltitudeCorrectionFactor` | all 11 threshold rows and out-of-range sentinel | `altitude-*`, `compute-altitude-*` | none |
| `GetClearanceForRMSVoltage` | both fields, every table entry, repeated thresholds, invalid field, default argument | `rms-*` | none; dormant helper called directly |
| `GetClearanceToWithstandTransientVoltage`, `GetClearanceToWithstandPeaks` | all field/PD tables, ordered PD fallthrough, refusals/defaults | `transient-*`, `peaks-*` | none |
| `GetRatedImpulseWithstandVoltage` | all OV tables, rated-voltage state, refusals/default | `impulse-*`, `mutation-*` | none |
| `GetMaterialGroupFromCTI` | all four descending thresholds and fallback | `cti-*` | none; dormant helper called directly |
| `GetBasicCreepageDistance` | PCB disabling order; all material/PD branches and tables; division tail and sentinel | `creepage-pcb*` | none |
| `ComputeClearanceDistance` | preferred reinforcement ladder/fallback, peak multiplier, sentinel propagation, ordered maximum then altitude multiplication | `clearance-*`, `reinforced-scaled-*`, `compute-*` | none |
| `ComputeCreepageDistance`, `Compute` | basic lookup, reinforcement doubling including sentinel, clearance/creepage adjustment, groove last, true return | `compute-*`, `mutation-*`, `fixed-seed-state-*` | none |

The `#if 0` coating/frequency refusals remain inactive. `Compute` never returns
false at this pin. Reference sentinel multiplication, out-of-range altitude
effects, duplicate thresholds and the final RMS rows returning 80 are preserved.
Unreachable duplicate condition bodies have the same literal table entries; a
case cannot distinguish them from the earlier identical condition. They are not
deleted or described as separately executed branches. The corpus distinguishes
positive numerical results from legitimate negative/zero outcomes. A true
Compute return alone does not establish a usable positive result.

The engine adds no dependency. The native adapter reuses workspace serde_json
and kicad-difftest; their locked dependency licences are already recorded in
`rust/MCP.md` and `rust/kicad-difftest/README.md` (GPL-compatible). Pinned
calculator sources and this derived work are GPL-3.0-or-later. No standards table
is replaced with a newer standard or third-party implementation.

## Local validation

Tested implementation revision: `d15389ec258f931b15ddc71114fc8b93ae5a3e0f` on
WSL Ubuntu with Rust 1.99.1. The oracle audit records 824 pinned files and 60
translation units. All four CTests pass, including missing reference compilation,
substituted target source and missing actual archive link refusals. No pinned
source or access-control/FP compiler flag was changed.

Workspace `cargo fmt --all --check`, `cargo clippy --locked --workspace
--all-targets -j8 -- -D warnings`, `cargo test --locked --workspace -j8`, the seven
golden-policy tests and `check_goldens_policy.py --wasm` pass. Every portable
workspace crate builds for wasm, including this engine. `test_shims.py` checks
29 input refusals per live adapter, maximum step count, EOF without newline and
stream recovery. These harness refusals are outside the golden corpus.

Live difftest `compare`, `propose`, `bless`, then `check` agree on all **14,189
cases / 47,101 complete state snapshots**, with zero deferrals or failures. The
baseline evidence retains actual live oracle provenance and both binary hashes;
the oracle was exported with a clean overlay at the tested revision. The first
baseline has null old values for every case, with a reason explaining the exact
bit protocol and code-parity purpose. No mismatch was blessed. Golden change
policy must run against committed HEAD after the baseline commit, using the
intended PR base. Reproduce the numerical counts with
`python rust/kicad-iec60664/summarize.py`:

| Scalar operation | Positive | Negative | Zero |
| --- | ---: | ---: | ---: |
| altitude factor | 40 | 2 | 0 |
| basic creepage | 3420 | 4140 | 0 |
| clearance computation helper | 1968 | 96 | 0 |
| creepage computation helper | 162 | 78 | 0 |
| groove width | 26 | 72 | 14 |
| rated impulse | 125 | 134 | 0 |
| peak withstand clearance | 203 | 337 | 0 |
| RMS clearance | 681 | 777 | 0 |
| transient withstand clearance | 850 | 455 | 0 |
| total | 7475 | 6091 | 14 |

There are 21 categorical CTI calls: three each return groups I, II, IIIa and IIIb;
nine return NOT_INSULATING. Every one of the 4,069 Compute calls returns true;
3,316 yield three positive distances and 753 have a nonpositive distance. These
counts describe reference behavior, not design approval. The corpus also checks
29,417 setters, 13 state reads and the named-category inventory.

The pilot PR provisionally depends on #39 and targets `rust`; the parent must
confirm its corrected generated scope, then review and merge #39 first.
The unit's local green metadata records exact implementation evidence. Hosted
pilot CI and independent review remain pending merge gates. Local exact agreement
and stored goldens do not establish the full three-pilot phase-zero gate or
all-configuration extraction completeness. `PHASE0_REQUIRED` remains false.

## Adapter review corrections

Revision `0659b51c6564a98868841f655b4dadc710e15c5a` accepts literal integer
negative-zero bits, preserves the full unsigned domain, and aligns complete
identity/refusal envelopes and validation order. Both real processes pass 38
shared refusal fixtures. All 14,190 live cases pass compare/propose/bless/check;
the original 14,189 golden replies remain byte-identical. Original immutable
evidence is retained beside a new complete initial-baseline artifact.
[Commands, reports and actual provenance](validation/2026-11-22-0659b51c/README.md)
record the clean tested overlay, five CTests and affected Rust checks.

The native adapter adopts `kicad_difftest::request::parse` from reviewed PR #45
commit `8f5f45729c733d8f1695dda9ee6d953f9bb4d400`. It rejects overflowing numeric
tokens before extracting identities, returning the complete invalid-request
envelope with null identities and `invalid JSON or duplicate key`. The shared
106-fixture contract retains raw literals for overflowing floats and 400-digit
integers in nested, unknown and identity fields, plus finite out-of-range,
underflow and float inputs that must reach envelope/domain validation. Process
checks also retain accepted integer `-0`, signed-zero bits and unsigned finite
bit patterns. The exact comparator, portable engine, corpus and all successful
golden replies are unchanged.

PR #45 and corrected graph scope/layer review remain dependencies, alongside
hosted CI and independent review; this does not establish the phase-zero gate.
