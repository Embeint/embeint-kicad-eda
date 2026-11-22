<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Resistor pilot local validation

Tested commit: `7384705afa6fcfdf17b2e7ae9b6abbeb4ed6081c`.
Reference pin: `4cf939778df04d2b747bb2c4714fc5014a7565c7`.
Host: Linux x86_64, GCC 15.2.0, glibc 2.43, Rust 1.99.1.
All builds used an owned Linux source/build copy under
`~/kicad-port/pilot-resistors`, the actual shared mkdir lock and at most eight
compiler jobs. No shared oracle or live KiCad tree was built or modified.

The unit remains **claimed and provisional**, dependent on #39. The a7 snapshot
lists 1,186 lines/41 definitions and no dependencies/gaps; independent review is
correcting its dispatch, active generated-definition inventory and wx coupling.
The structural/source-byte checks below do not establish semantic completeness
of that snapshot. Corrected ownership/dependencies must be confirmed and audited
before readiness. No phase-zero completion or green-unit claim is made.

## Checks

| Check | Result |
| --- | --- |
| Workspace `cargo fmt --all --check` | passed |
| Workspace `cargo clippy --locked --workspace --all-targets -j8 -- -D warnings` | passed |
| Workspace `cargo test --locked --workspace -j8` | 98 passed, 0 failed/ignored |
| `check_goldens_policy.py --wasm` | actual wasm builds passed for embeint-layout, embeint-web and kicad-resistors |
| Real CMake oracle build and ctest | 70 audited compilations; 913 unchanged exported files; 3 ctests passed, including 7 build-guard regressions |
| `validate_units.py` with source hashes | stored a7 graph's 1,570 records/accounting passed; semantic corrections remain pending |
| Registered `check_goldens_policy.py --ci` | all 199 requests matched live C++/Rust/committed golden, 0 ordinary errors/deferrals, unchanged 60-second process timeout |
| Golden review policy | passed against committed HEAD after the consolidation commit, using origin/rust as base |

The registered run reported phase zero **INCOMPLETE**, with 0 green units because
this record stays claimed and the strict switch remains false. Its extraction
flags retain `parse_complete=false`, `all_configurations_complete=false`, four
parse failures and the other stored coverage diagnostics. No flag or case was
removed to make a gate pass. Wasm was built; a wasm runtime comparison was not
performed.

## Successful work and legitimate reference outcomes

The 199 requests include 12 series cases spanning all representable decimal
exponents and i32 extremes, seven formatting cases containing 15,473 values,
constructor/default/parts probes, 12 helper cases, seven direct collector cases,
156 calculator cases, three real-reader identity cases and all 36 scale constants.
The calculator cases include nine caller-modified actual C++ E-series vectors;
their first element, ordering, zeros and early termination are preserved.

There were 148 successful public `Calculate` calls and 54 successful forced
private solutions. Eighteen actual `Empty solution collector` exceptions were
compared as observable results: 15 public calculations, two empty private searches
and one direct empty collector. The real collector's repeated `GetBest` copies
are compared completely. Invalid protocol/precondition/count refusals are tested
separately and are not counted as successful differential operations. In
particular, an empty custom table would index outside the C++ vector; an all-zero
table never satisfies its exit condition and has no finite returned result.

The reader probe uses real parser/string-reader objects in C++, and a documented
nullable borrowed reader-state co-port in Rust. Actual parser constructors remain
explicitly unported in their provisional owning UI unit. The getter never calls
them and no constructor success is fabricated.

Both baseline evidence artifacts are immutable. The complete artifact records
all 199 requests against the PR base's absent golden and includes every new reply,
both executable identities, exact compile/link commands, pinned source hashes,
generated-lexer regeneration/hashes, compiler/CMake/glibc identity and reason:
`port/goldens/evidence/pcb_calculator.application.pure.p001-baseline-complete.json`.
The earlier 198-case artifact is retained unchanged as historical evidence.
