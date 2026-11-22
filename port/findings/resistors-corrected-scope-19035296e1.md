<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Corrected resistor scope and retained reader co-port

This addendum supersedes the ownership assumptions in earlier resistor findings;
their historical cases, results and immutable evidence remain unchanged.
It integrates exact corrected #39 `19035296e129cf8640e89331bc9e8ad84f0fe38c`
and its `rust` dependency `4c6c1e38ce2dead8db8a3193e7525d19532523e9` through
formatted merge commits. The parent-pushed collector target fix and canonical
CTest at `1dd31a7493189ca694bc09020a3a84bf9fb6082e` remain intact.

The pure record now owns **1,126 lines / 40 definitions / seven files**, L0/pure,
with no hard or soft dependencies and no coverage gaps. Exact protected fields
match the corrected snapshot. Only its permitted port metadata/difftest are
reconciled. Status stays **claimed**; final independent graph review and parent
acceptance remain holds. No full phase-zero or green-unit claim is made.

| Owned path | Inclusive range |
| --- | --- |
| `pcb_calculator/common_data.h` | 1-62 |
| `pcb_calculator/eseries.cpp` | 1-102 |
| `pcb_calculator/eseries.h` | 1-177 |
| `pcb_calculator/pcb_calculator_utils.h` | 1-33 |
| `pcb_calculator/resistor_substitution_utils.cpp` | 1-508 |
| `pcb_calculator/resistor_substitution_utils.h` | 1-168 |
| `pcb_calculator/units_scales.h` | 1-76 |

All 40 exact USRs are unchanged from the prior implementation scope except the
sole removed `c:@S@PCB_CALCULATOR_DATAFILE_PARSER@F@GetReader#`. The
[machine-readable audit](resistors-corrected-scope-19035296e1.json) lists every
USR, retained Rust body, exact canonical range and pinned source hash. It covers
all E-series constructors/tables/scaling, Resistance defaults/comparisons,
engineering notation, collector callbacks/tie logic, four combination helpers
and every calculator state/build/buffer/search/2R/3R/4R continuation. The 36 scale
constants remain supported. No algorithm or owned branch was removed.

`datafile_read_write.h:1:60` and its inline getter at line 55 belong to
`pcb_calculator.application.thin.replacement.p001` (L0/thin-replacement).
`reader::DatafileParserReader::get_reader` and its minimal borrowed nullable
reader type remain an authorized co-port. Only that owner's notes are annotated;
its other metadata/protected fields and todo status remain canonical. Its other
definitions are not claimed implemented. Constructors and Parse belong to
`pcb_calculator.application.ui.p001` (L136/UI); existing Rust constructor APIs
still return explicit Unported errors naming that actual owner.

All three real reader identity cases remain within the existing 1,050-case
corpus. C++ still constructs actual parser/STRING_LINE_READER objects and calls
the original getter, with exact null/alias/distinct/rebinding/repeated identity
observations. No fake getter or constructor success is introduced. The reference
library closure, generated-lexer audit and archive linkage remain required.

Compiler qualification is **GCC 13/15 Release x86-64**. Independent review found
NaN payload differences under GCC O0 and Clang 18; their equivalence is not
claimed. Exact comparison and process budgets remain unchanged. No reblessing is
required because no input or golden output changes. Historical wasm execution
and original source/executable identities remain historical evidence; integration
validation records a fresh clean source/provenance identity separately.
