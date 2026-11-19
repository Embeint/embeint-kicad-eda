# Merged-main installed qualification: 2026-11-19

Merged fork main `b630c7bc794b854ce4823f9ca810fde38f032241` was checked against
`embeint/main` before any edits. The checkout was clean. The complete Git archive,
fresh frozen MCP, development `.deb`, new installed runtime and raw evidence are
outside all source checkouts. Earlier skill 1.1.0 and native receipts remain
unchanged. This is bounded Linux x86_64 WSL/Xvfb qualification, not a release or a
native Windows qualification.

## Source and artifact identity

| Input/output | Identity |
| --- | --- |
| Archived merged main | `b630c7bc794b854ce4823f9ca810fde38f032241` |
| Full source archive SHA-256 | `3c2039c54d2963bfe2fa2440b0208d6a79e45fd7a429c9385e9eec15eb9af4cb` |
| Frozen executable SHA-256 | `af81300cb473fb232e9c36e7cceb58762ae99fec8ccec77c004cf96fdf9bcbd4` |
| Build-info SHA-256 | `7f4ac592d150fd23fa8d65fce5e39fcea6b78b8a3b22764b5e4c1189ac2035f1` |
| Schema SHA-256 | `214ce118ab3b9bfa24fc06818000fb6443c8a4340976875bcc0569f96d9b5115` |
| Skill | `kicad-house-style` 1.1.1 |
| Skill content SHA-256 | `111b920c10f3a865b3ff25ef1fec3e3b80339ea62466e43648f4301c3c841c7a` |
| Discovery | 112 tools, 145 registered commands |
| Package version | `10.99.0~dev+d880.loader1.mcp.b630c7b` |
| `.deb` SHA-256 | `2d3684dbe3d65cfc1680310dfa02c3286b699fa396865ecc178992dd1980b27c` |
| Initial helper restriction implementation | `e1aa1b6d36dd9e6f279c1891b272510324e36abc` |
| Native save fix | `9af0ff166ab15bc3adf27848cde067461c7fee6e` |
| Final scripted qualification implementation | `318dad0670d6e215e347800c6c2b4d5aa11076c8` |

The frozen build uses Python isolation and an exact archive. Its manifest retains
the caller-supplied archive revision and unknown dirty state; it does not claim
Git verified that extracted directory. Adapter source hashes, protocol hashes,
dependency versions, skill versions and per-file/content hashes remain in
`build-info.json`. The helper qualification commit adds tests and runner guidance only:
every adapter-root Python file matches the archived main after CRLF/LF
normalization, and no bundled skill or protocol changed.

The initial main qualification uses native binaries from the retained `d880b3be5dbf30003e4b0069262417d0278bf528` build
plus the exact four-file loader patch, measured as
`10.99.0-qualification-d880-loader-patch-dirty`. **This initial run did not rebuild native main.** All 897 manifest files and the retained source archive were
verified. All 4,852 files under `api`, `common`, `eeschema`, `include`, `kicad` and
`pcbnew` in that native archive match merged main after CRLF/LF normalization.
Each seed/reopened manager's loaded install modules, private dependencies and
system libraries were checked or hashed; the native prefix is mounted at its
actual `/opt/embeint-kicad` build prefix. The native archive remains
`native-source-d880-supplied-loader.tar.gz`, SHA-256
`60c2e8b89fcd96b904ea3f7c20e586932e873ed1e3b2a29038f6541db4c742cb`.

The initial new `.deb` combines these verified retained native files with the newly
frozen main MCP using the committed package tooling. Packaging used the retained
read-only Poppler/dpkg overlay; it installed no host dependencies. The separate
Ubuntu 26.04.1 runtime root is a full filesystem copy of the prior dependency
root, excluding its application and disposable home/tmp/run state. Confined dpkg
installed the new package; `dpkg --audit` was empty. It is a fresh application
installation on retained dependency inputs, not a newly downloaded OS build.

Both complete corresponding source archives are retained beside the package.
The native archive includes the exact loader patch, build sources, LICENSE and
third-party tree. The merged-main archive includes MCP, protocols and LICENSE.
The package and standalone frozen directory contain the GPL text at
`libexec/mcp/licenses/KiCad-GPL.txt` and bundled Python/dependency notices in that
licenses directory. Ubuntu dependency notices remain under the runtime root's
`usr/share/doc`; those dependencies are separate from the application `.deb`.
This task published source and compact receipts in the fork PR, not a binary
release or a public runtime image.

## Scripted evidence

| Check | Actual result |
| --- | --- |
| Full offline suite from archived main | 340 passed |
| Full WSL offline suite at qualification commit | 340 passed |
| Windows Python 3.12 offline suite at qualification commit | 293 passed, 47 platform skips |
| Frozen stdio/skill/provenance | Passed, with no source checkout/system Python for the child |
| New installed runtime over real MCP stdio | Placement, wiring, undo/redo, explicit save/restart/reopen and connectivity passed |
| Installed payload | All 897 native files and all 518 frozen files/symlink entries match |
| Current `block_example` native probe | Passed on clean merged main and final qualification commit |
| Hierarchical capture | 3 sheets, 7 physical parts, 28 physical pins; declared intent and all PCB pads match |
| ERC | 0 violations, 0 exclusions, 4 disabled check categories retained |
| Negative ESD bridges | Removing the bridges splits DM/DP despite ERC remaining zero; exact undo restores items/pins/nets |
| PCB transfer/DRC | 7 footprints, 0 transfer errors/warnings; 18 unrouted connection findings, 0 exclusions, 5 ignored categories |
| Persistence | Explicit save, owned stop/restart/reopen; normalized full state, native pin/net identities, file hashes and PCB parity equal |
| Pages | Root, MCU, Interface and PCB fit; all four framed PNGs inspected |

The frozen checker starts the child from an empty directory with no Python on
PATH and deliberately unusable PYTHONHOME/PYTHONPATH, then uses real MCP stdio.
It checks initialization, tool/resource discovery, all advertised skill files,
source-versus-frozen versions/hashes, traversal/missing-file and argument errors,
helper inclusion in the PYZ archive, and clean EOF. It does not execute the
source-only helper through MCP. The installed workflow separately starts the
packaged desktop and frozen MCP in Bubblewrap with no source mounts, network or
system Python executable. The client SDK runs outside that runtime.

The additional `--helper-restrictions` probe runs serially before capture. Its 20
checks use real native readbacks for safe route replacement and exact one-step
undo/redo, manual wire interior crossing preservation, symbol/sheet-pin branches,
manual wiring/label attachment guards, sheet deletion/retargeting refusal,
duplicate no-connect refusal, owned no-connect replacement and `add()` boundaries.
Unsafe plans are refused before apply. No native mid-commit cleanup guard was
relaxed and no new transaction protocol or high-level block MCP layer was added.
Failed-batch rollback and normal undo/redo also restore exact items/pins in the
hierarchical fixture. Every manager/display belongs to its run and was stopped.

Failed setup attempts are retained separately. The first capture selected a
nonexistent stock-library path and failed before helper mutation; the next run
uses the retained runtime's stock libraries. Packaging first lacked the Poppler
overlay, then the namespace's device bind; its third attempt succeeded. The
initial Windows venv lacked PyYAML and failed collection; the successful Windows
suite uses a new task-owned venv. These attempts are not counted as passes.

## Independent installed trial

The artifact-only trial descriptor is
`C:/Users/KarlVonRichter/.t3/scratch/main-installed-trial-b630c7b/TRIAL.md`.
It supplies executable/SDK-client invocation, stock libraries, an explicit private
socket/environment, a new disposable project with an existing manual annotation,
raw evidence paths and owned restart controls. The evaluator receives a realistic
two-sheet creation/revision/validation/save/reopen request and discovers the
installed tools/resources and skill itself. It receives no source/test files,
implementation findings, expected solution or earlier conversation. Source-only
helpers remain outside the installed product boundary; generic/typed MCP tools
are permitted.

T3 rejects local `delegate_task` calls targeting another project, even when given
this machine's environment ID. An authorized artifact-project orchestration thread
therefore launches the sole evaluator with a new `delegate_task` and no parent
history. Its task ID and invocation are retained in `evaluator-task.json` in the
trial directory. This orchestration thread performs no design evaluation itself.

The [original independent evaluator](t3-thread://v2/41952155-64b5-4278-aa7a-2d21fe7c12c9/thread:delegated-task:command%3Amcp%3A7c9a46c9-9196-4cf7-bed1-58804a66b07c%3Adelegate-task%3A36b5d5c9-b19b-4d9a-a777-3f90a275be77)
reported **partial pass**. It discovered 112 installed tools and skill 1.1.1,
created two sheets and titled functional blocks, retained the manual annotation,
revised C1's connection, inspected framed pages, and verified all ten physical
pins as four exact nets. Both pages fit. Live ERC and cold installed CLI ERC each
reported zero violations with four ignored categories; saved files and all 45
top-level UUIDs survived the restart. GUI reopening repeatedly returned
`AS_NOT_READY`, so the evaluator correctly withheld GUI persistence acceptance.
The original REPORT.md, requests, errors, snapshots and renders are retained
unchanged in the trial directory.

Its other findings remain visible: full returned-symbol updates of instance
values/footprints produced four library-mismatch warnings, cleared by selective
`update_from_library(reset_field_text=false)` while retaining intended fields and
connectivity. The underlying full-definition round-trip issue is unresolved; this
PR does not claim to fix it. Root typed netlist pin coverage is sheet-scoped;
whole-hierarchy export was needed for complete connectivity. Page containment did
not detect local text overlaps; framed visual inspection did. CLI commands need
the installed launcher library/configuration environment. Source-only Python
block helpers were not made available to either installed evaluator.

## Demonstrated save/reopen fix and new installed runtime

Read-only inspection after the original evaluator found a native **Recover
Unsaved Changes** modal and three autosave commits with no `Last_Save_*` tags.
The GUI API context wrote all sheets successfully but omitted the manual-save
history/ZIP cleanup already performed by interactive save. This explains both
the valid saved CLI content and blocked GUI API readiness. The fix records the
same manual history marker after a successful API save, then removes only ZIP
autosaves belonging to saved schematic screens. Failed saves and null projects
skip that bookkeeping. It does not disable recovery, delete history or change
native commit/cleanup guards.

The installed `--recovery-history` regression first observes an actual incremental
autosave. The retained baseline fails at the missing `Last_Save_sch` marker after
API save; its logs, history and project remain in `save-fix-9af0ff1/baseline-home`.
The new installed runtime passes: saved HEAD equals the manual marker, unchanged
repeated save preserves it, owned stop/restart and GUI reopen preserve exact
items/nets/file hashes, and a subsequent unsaved R3 edit creates newer history
without moving the saved marker. This tests incremental recovery; ZIP sibling
preservation is scoped by the existing source-specific cleanup API and was not
separately exercised in this slice.

Only `sch_context.cpp` was recompiled and `_eeschema.kiface` relinked in the
retained build. Normal CMake component installation removes build RPATHs. A new
native stage contains that installed module and 896 unchanged retained files.
All 4,852 files in the six native/API source trees match fix revision `9af0ff1`
after CRLF/LF normalization. This is an incremental native rebuild of the fix on
verified retained inputs, **not a full rebuild of main**. The measured version
string remains the retained d880-loader dirty version; file hashes and archive
identity distinguish the new binary. The packaged manifest's historical
`source_changes`/`source_description` describe its retained loader base;
`build.claim` and `build.production_correspondence` describe the incremental fix.

| New artifact | Identity |
| --- | --- |
| Native complete source archive | `native-source-d880-loader-save-9af0ff1.tar.gz` |
| Archive SHA-256 | `4faadc0727be8443a6af30a5819bed54d3e514b1b539c6789d93d7bb2cd5054c` |
| Installed `_eeschema.kiface` SHA-256 | `93496eed094247228f636b530735f1098b7187dff9b3b28c6908d14d07e4b3ce` |
| Native provenance SHA-256 | `daeb9dc5108b6fed0182fb469a06f98aebbdfd709cafcc0e4536d3e43843aea0` |
| Package version | `10.99.0~dev+d880.loader1.save.9af0ff1.mcp.b630c7b` |
| Package SHA-256 | `fffe8574f3680024966436336beb22905b99fbcc5efb6d82f6698f85785891f8` |
| Runtime | `/home/karlvonrichter/embeint-qualification/main-installed-b630c7b/save-fix-9af0ff1/runtime-root` |

The package retains the exact frozen main MCP/skill/schema identities above.
All 897 installed native files and 518 frozen entries were verified against
inputs, with empty dpkg audit and no system Python executable. Its new native
corresponding-source archive, the frozen main source archive, GPL text and
bundled dependency notices remain beside the package/runtime. A complete Git
archive of qualification source `318dad0` is also retained.

The full helper/restriction/hierarchy acceptance passed again from clean
`318dad0`, on the new native module: 28-pin intent and PCB parity, ERC zero,
18 actual unrouted DRC findings, framed pages, manual-item preservation,
targeted edits, failed batch/undo/redo, explicit save and owned restart/reopen
with normalized state/net/file equality. Three framed PNG hashes equal the
previously inspected main pages; the changed Interface PNG was inspected anew.
All 340 affected WSL offline tests pass on `318dad0`. Final Windows Python offline checks also pass on `318dad0`: 293 passed and 47
platform skips. No native Windows result is inferred.

A **new** artifact-only evaluator task uses a new project/home/display/socket
namespace and the new runtime. Its descriptor is
`C:/Users/KarlVonRichter/.t3/scratch/main-installed-recheck-9af0ff1/TRIAL.md`.
It receives the same realistic request and discovers installed tools and skill
without source access, findings, proposed fixes or expected solution. The new
task ID is in `evaluator-task.json`; the original evaluator was not reused.

The [new independent evaluator](t3-thread://v2/41952155-64b5-4278-aa7a-2d21fe7c12c9/thread:delegated-task:command%3Amcp%3Ac5b19e4a-da16-4ca2-81f1-f1ab7c361227%3Adelegate-task%3Afa76bf51-2ba0-435b-9621-a5212e592c5c)
reports **passed with limits**. It independently discovered skill 1.1.1, 112 tools
and 145 schemas. Its two-sheet input/load design has real SIGNAL/RETURN hierarchy,
titled dashed blocks and the retained manual annotation UUID/text/formatting
(anchor moved to fit). It revised the input from J1.2 to J1.1, marking J1.2 unused.
Whole-design exports before and after GUI restart/reopen prove the final exact
sets: J1.1/R1.1, R1.2/R2.1, J1.3/R2.2 and unused J1.2. Both framed pages fit and
were visually inspected. Final and reopened ERC each report zero violations,
zero exclusions and four ignored checks. Explicit child/root saves return success
and unmodified state; the owned manager/display were stopped and restarted,
then installed tools reopened both sheets. Saved schematic hashes match across
restart. This establishes installed GUI persistence for the intended connectivity.

The recheck does **not** establish arbitrary field equality: all item IDs survived,
but several serialized fields changed, including a no-connect marker from 1.27
to 1.2192 mm. Two resistor/library mismatch warnings after item updates again
needed selective stock-library refresh. The root typed netlist remains sheet-scoped.
Its rejected revision used the wrong DeleteItems argument and was confirmed
unchanged before a corrected batch; it does not independently prove rollback
after partial apply (the scripted capture does exercise that case). The evaluator
also disclosed an initial enumeration of global tool descriptions; no global
KiCad endpoint was invoked, no source/test/history was read, and all design
operations used the supplied installed bridge. These limits and unsuccessful
attempts remain in the original REPORT.md and raw evidence; the trial is not
represented as a broader electrical or field-round-trip qualification.

The evaluator's final report is
`C:/Users/KarlVonRichter/.t3/scratch/main-installed-recheck-9af0ff1/REPORT.md`.
The final owner stop left no owned application/display processes. The retained
project can be reopened with the lifecycle in
`C:/Users/KarlVonRichter/.t3/scratch/main-installed-recheck-9af0ff1/HANDOFF.md`;
TRIAL.md remains the exact original evaluation descriptor. The original partial
trial and new recheck have separate immutable full snapshots and hashed indices.
Scripted checks are separate evidence from both independent evaluations.

## Retained evidence and remaining gates

Raw build/package/check logs, full native correspondence and installed-content
manifests, before/after snapshots, framed renders, projects/settings and receipts:
`/home/karlvonrichter/embeint-qualification/main-installed-b630c7b`.
The Windows raw-evidence copy is
`C:/Users/KarlVonRichter/.t3/scratch/main-installed-b630c7b`.
The initial installed root is `runtime-root` under the WSL evidence root; the
new save-fix root and final handoff are identified above.
Independent trial state is
`/home/karlvonrichter/embeint-qualification/main-installed-trial-b630c7b`;
its raw MCP evidence is in `evidence`, and its native project is in
`home/synthetic-trial`. Keep the descriptor and installed root together for handoff.

The compact receipt includes both native/package identities, both independent
outcomes, actual findings and all exact tested revisions. It is
[main-installed-windows-wsl-2026-11-19.json](qualification/evidence/main-installed-windows-wsl-2026-11-19.json).
The raw logs and historical receipts retain their original identities. Clean
merged PR #7, #8 and #9 worktrees were removed with Git after verifying merged
heads, no active task/process, full source archives and copies/hashes of all 334
ignored files. Backups remain in the Windows evidence root's
`merged-worktree-preservation`. The older qualification worktree with untracked
evidence and the historical dirty skill checkout were preserved.

Native Windows build/IPC/installer, WSLg, Docker, other native platforms and a
full rebuild of main remain separate gates. Full-symbol update warnings and
arbitrary item-field persistence remain open findings. One writer is required;
fingerprints do
not provide atomic editor revision checks or multi-document commits. These are
synthetic designs and an unrouted board, not production component qualification
or manufacturing sign-off.

Disposable archive extraction and duplicate native component/package stages were
removed only after checking resolved ownership paths and process use. Their
complete source archives, packages, native manifests, build logs, installed roots
and trial projects remain. Completed raw evidence is indexed and owned files are
read-only; 1,089 original/main files and 135 recheck files are covered separately.
The current topic checkout and retained native build remain available for review.
