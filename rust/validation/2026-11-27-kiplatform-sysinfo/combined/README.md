<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Accepted Unix system-info integration

PR #90 was accepted on `rust` at `b98201bc6b4c9e90cf725a0ab62b29d7b7b37a42`.
Reviewed/tested head `397629f3ec38ed1df8951cca4a5edae8a4444573` and accepted
commit have the exact same tree, `bd5c03643276b1156c87bad6186b53e1e2d822b6`.
GitHub explicitly refused rebase; the permitted normal merge was then used.

The original Sol 6.1 source review and a fresh exact-head integration review
found no actionable findings. The latter checked all 41 nonshared blobs, the
workspace/lockfile union, source closure, registrations, both clock wrappers,
unit statuses and co-port notes. Only accepted rust was imported.

Manual final CI was authorized by the user and ran on the unchanged locked
GCC 13.3/glibc 2.39/libstdc++ 14/wxWidgets 3.2.4 runtime. This records manual
results separately from GitHub status. Fresh hosted jobs on the combined head
were still running at merge; earlier-head hosted results are recorded in the
[parent evidence](../README.md).

| Check | Observed result |
| --- | --- |
| Registered-unit global gate | 29 units, 140,712 cases: 135,944 exact, 4,768 explicit deferrals, zero failures/unprocessed |
| Previously accepted 28 units | Every input and C++/Rust reply hash unchanged |
| Workspace tests | 460 passed, zero failed, 5 ignored across 147 suites |
| fmt, clippy with warnings denied, Rust 1.85 all-target check | Pass |
| Fresh audited C++ oracle and CTests | Pass; 15/15 tests |
| Policy | 36 exact; 25 matching refusal controls with continuations |
| System info | 126 exact; 67 matching refusal controls with continuations |
| Portable builds and both platform wasm replays | Pass; import-free wasm equals fresh C++ replies |
| Source and runtime binding | 4,133 raw Git-exported files reverified after checks; locked runtime files reverified |

Oracle SHA-256:
`87048341fafbcb64070e4c9ad26a11fd0eab5fb623c566efa5197a0eba56f408`.
Policy reply SHA-256:
`c79a22a5f355b31b9c51058cb7765be4c57483c3284f3035dcc0abdd34c0abe5`.
System-info reply SHA-256:
`b470406aea31b9622f9a44fc16083ef2a341dcaca560e601de9e48e10aa69e34`.

Both platform units remain partial without a golden. All C++ numeric fixture
fields are initialized before observation. Bounded Linux fixture domains,
other platform representations and unbounded allocation failure are not
qualified. This does not qualify live GUI/MCP work or repeat the full libm
qualification. [verification.json](verification.json) retains commands, counts,
source/runtime/provenance bindings, binary/reply/log hashes and both reviews.
[accepted.json](accepted.json) binds the accepted commit and merge decision.
