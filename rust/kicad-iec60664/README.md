<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# IEC60664 calculator parity pilot

`kicad-iec60664` is a portable, dependency-free translation of `IEC60664` at
KiCad `4cf939778df04d2b747bb2c4714fc5014a7565c7`. It preserves the reference's
IEC60664-1:2020-05 tables and computation order. It makes no safety certification
claim. It uses the reference's scalar units directly, including the different
voltage scales used by different helpers; it performs no unit conversion.

Provisional unit: `pcb_calculator.calculator_panels.application.pure.p001`.
The final extractor record is required before publishing a claim or green status.
The expected scope is `iec60664.cpp:1:1355`, `iec60664.h:1:137` and interface-only
spans `panel_electrical_spacing_iec60664.h:1:50`,
`panel_electrical_spacing_ipc2221.h:1:48`, `panel_r_divider.h:1:58` in
`pcb_calculator/calculator_panels/`. No GUI implementation is ported. Those panel
headers declare methods owned by other UI units; none of the calculator's bodies
calls them, so no GUI type co-port is necessary.

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
| constructor/destructor, 12 setters, three distance getters | every initial value; direct mutation; independent instances; empty destruction | `default-*`, `mutation-*`, `fixed-seed-state-*`, all step snapshots | none in implementation; live validation pending |
| `GetMinGrooveWidth` overload | nonpositive refusal, division below 3, every PD/default, argument default | `groove-*`, `compute-*` | live validation pending |
| `GetClearanceAltitudeCorrectionFactor` | all 11 threshold rows and out-of-range sentinel | `altitude-*`, `compute-altitude-*` | live validation pending |
| `GetClearanceForRMSVoltage` | both fields, every table entry, repeated thresholds, invalid field, default argument | `rms-*` | live validation pending |
| `GetClearanceToWithstandTransientVoltage`, `GetClearanceToWithstandPeaks` | all field/PD tables, ordered PD fallthrough, refusals/defaults | `transient-*`, `peaks-*` | live validation pending |
| `GetRatedImpulseWithstandVoltage` | all OV tables, rated-voltage state, refusals/default | `impulse-*`, `mutation-*` | live validation pending |
| `GetMaterialGroupFromCTI` | all four descending thresholds and fallback | `cti-*` | live validation pending |
| `GetBasicCreepageDistance` | PCB disabling order; all material/PD branches and tables; division tail and sentinel | `creepage-pcb*` | live validation pending |
| `ComputeClearanceDistance` | preferred reinforcement ladder/fallback, peak multiplier, sentinel propagation, ordered maximum then altitude multiplication | `clearance-*`, `reinforced-scaled-*`, `compute-*` | live validation pending |
| `ComputeCreepageDistance`, `Compute` | basic lookup, reinforcement doubling including sentinel, clearance/creepage adjustment, groove last, true return | `compute-*`, `mutation-*`, `fixed-seed-state-*` | live validation pending |

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

Validation and immutable evidence will be recorded after the real build and
live comparison; stored cases alone establish no parity or phase-zero gate.
