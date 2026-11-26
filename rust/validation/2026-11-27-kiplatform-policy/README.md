<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Unix policy manual reference evidence

Reviewed/tested PR #89 head: `4f482553db5ded162af1c3d35552ef51ca731a78`.
Tree: `6b2c6b73c1c481faf1f1f90f4e43ccf980fba205`. Independent Sol 6.1 source review found no findings at
this head. The parent supplied the missing target-environment evidence below.
These are manual results, not GitHub-hosted check statuses or qualification of
a new reference image. Hosted CI remains pending at this recording.

| Check | Result |
| --- | --- |
| New policy corpus | 36 exact results, zero deferrals, zero failures |
| Policy refusal controls | 25 identical refusals, each followed by a successful call |
| Policy wasm replay | All 36 fresh C++ replies agree with import-free wasm |
| Full registered-unit gate | 140,586 cases across 28 units: 135,818 exact, 4,768 explicit deferrals, zero failures/unprocessed |
| Existing accepted units | All 27 prior case hashes and C++/Rust output hashes unchanged |
| Full workspace tests | 456 passed, zero failed, 5 ignored across 141 suites |
| Formatting, clippy, portable wasm builds | Pass, with clippy warnings denied |
| Rust 1.85 workspace/all-target check | Pass |
| Generator, source audit and unit records | Pass |
| Pinned C++ build and CTests | Pass; actual policy symbols and adapter call relocations verified |

The unchanged locked runtime uses GCC 13.3, glibc 2.39, libstdc++ 14 and
wxWidgets 3.2.4, with Rust 1.99.1 and CI's SSE2 dispatch tunables. The original
249 package-archive checks remain recorded, and the locked compiler/runtime
files were reverified after this run. The source snapshot contains
4,114 raw Git-exported files; each was checked again after validation.
The pinned policy.cpp is compiled unchanged, and both adapter relocations call
its real functions. The 36-case corpus is not a substitute constant oracle.

Both Unix functions preserve the ignored key parameter. They return
`PBool::NotConfigured` and `None`, respectively. The accepted signed-wchar
`WxString` model is reused, and only PBOOL is co-ported in the header owner's
notes; that header unit stays todo.

The unit remains partial and no golden is blessed. The 4096-unit shim bound,
inherited WxString capacity limit and excluded Windows wchar representation
remain qualification limits. This evidence does not qualify live GUI/MCP work
or repeat the complete seven-function libm qualification.

[verification.json](verification.json) records exact source/tree/runtime
bindings, commands, result counts, per-unit input/output/binary hashes, source
review disposition and hashes of the retained local logs. Full logs/reports
remain in the parent's private manual reference workspace.
