<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Graph adoption integration evidence

Reviewed head: `d3d65cb5ecf2f9f67b6b46e6a71b205e2db52d9c`.
Reviewed tree: `73ca919b04fab75da9b2250f40b722a22d4bde7a`.
Accepted base: `b98201bc6b4c9e90cf725a0ab62b29d7b7b37a42`.

The original adoption source review and fresh Sol 6.1 integration review report
no actionable findings. Fresh independent checks verify all 1,801 metadata
records, two splitter regenerations, measurement and both drift generators;
95 Windows tests discovered, 81 passed and 14 skipped. Default validation and
golden policy pass. Strict validation retains exactly the known incomplete
extraction diagnostic. The graph and aggregation code remain unchanged.

The user-authorized manual path uses the unchanged locked GCC 13/glibc 2.39
reference runtime. All runtime, harness, corpus, golden and workflow files are
byte-identical to the source-bound accepted Unix integration; the native binaries
and C++ oracle are reused, not rebuilt. Source accounting and the global gate
run on this exact refreshed head. Previous workspace, controls and wasm results
remain evidence on the unchanged source, not newly executed checks here.

The runtime-only raw Git export contains 4,230 files. Its complete original
C++ inventory is supplied separately through read-only references to the
preserved extraction source, each normalized hash checked against the committed
inventory. The global gate uses its default source root. Earlier attempts
stopped because this supplementary source context was absent there; no source
check or comparison was weakened. A separate initial script BOM error occurred
before execution. Another attempt exposed the missing default rust/target
mapping for reused native binaries; the final run supplies and prechecks every
registered binary. These attempts remain recorded in verification.json.

The offline isolated preflight 45867748826 was cancelled and replaced by
existing hosted fallback 45868907057 on the exact head. Manual results are not
GitHub check statuses. Incomplete parsing/configuration coverage remains; no
unit status, golden, pin, runtime body or corpus changes in graph adoption.

PR #94 rebase-merged as `70888b572202033a6522f2f271b10cc908f74dfe`.
The accepted tree equals the reviewed tree exactly. All 29 units' input and
C++/Rust output hashes equal the previous accepted baseline. The fresh manual
gate reports 140,712 cases, 135,944 exact comparisons, 4,768 explicit deferrals
and zero failures/unprocessed; all 15 CTests pass. No tolerance or golden changed.

At the merge decision all applicable completed hosted jobs passed, including
every live workflow and MSRV. The offline preflight was explicitly replaced by
successful hosted fallback 45868907057; downloaded source/workflow SHAs match
`d3d65cb5ec` and outcome is success. The hosted rust-port job was still running.
The user-authorized manual result is recorded separately from that status.

[verification.json](verification.json) records commands, final reports and log
hashes, earlier scaffolding stops, unchanged-source binding, runtime checks and
hosted fallback artifact hashes. [review.json](review.json) records the exact
independent review; [accepted.json](accepted.json) binds the accepted tree.
