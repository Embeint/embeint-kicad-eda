# Windows and WSL qualification record, 2026-11-18 UTC

This is bounded desktop qualification evidence, not a release qualification.

## Source and isolation

The initial clean HEAD was `d880b3be5dbf30003e4b0069262417d0278bf528`.
The canonical base is `https://github.com/Embeint/embeint-kicad-eda.git`,
`embeint/main`; `origin` remains the older personal mirror. The platform fix is
`cd4b155fda959c3f2861ce24225fd18af6f0b2f0`, pushed on
`embeint/mcp-wsl-windows-qualification`, initially without a PR. The branch is now
published as [Embeint PR #5](https://github.com/Embeint/embeint-kicad-eda/pull/5).
The separate setup dependency fix is
`da7cc8260d62fec2c86162e7ebe404c390a1c820` on the same branch.
The live-discovered BOM helper correction is
`8cc77eec3362c1b7b1752239fefef83e0fcd9fc5`.
The tested adapter/fixture HEAD is
`7b67d131e6b0ffd4c34f30093e47bb529db81de2`, adding the guarded persistence
fixture and assigning real nets in the PCB routing fixture. Native production
code remains d880 for the independent baseline.

Coordination thread:
[Qualify Embeint KiCad on WSL and Windows](t3-thread://v2/41952155-64b5-4278-aa7a-2d21fe7c12c9/mcp:launch:de13b73a98cd7cada7d0353c100c4c3bfbc8df2b72c4898f72fc038e35f6ccea).

Work ran in the existing T3 checkout at
`C:/Users/KarlVonRichter/.t3/worktrees/kicad-source-mirror/mcp-wsl-windows-qualifi-29f818af3fa02ed5`.
Raw Windows logs, command metadata, package inventories and temporary harnesses
are in its `.qualification/` directory. This directory also contains the isolated
Windows Python virtual environment and failed native build configuration.

The separate WSL root is
`/home/karlvonrichter/embeint-qualification/d880-windows-wsl`.
Its source snapshot, build, install, config, temporary IPC directories, scratch
libraries and disposable projects are separate from `~/kicad-live`.
Only the existing Python environment, Poppler dependency files and stock library
files were reused. Existing source/configuration and MCP processes were preserved.
No existing customer editor was used or stopped. The test harness stops only
processes/groups that it launches.

The native baseline contains d880 C++ and protocols. The copied source acquired
some in-progress Python adapter edits during rsync; these do not enter the native
build. The live adapter is the separately identified platform fix. Private Git
metadata in the snapshot resolves the native commit to d880. A private descriptive
tag records qualification identity and is not published or a release tag.

## Portable runtime and protocol evidence

| Environment | Result | Evidence in `.qualification/` |
| --- | --- | --- |
| Initial unmodified WSL adapter | 225 passed | Initial tool transcript; later runs reuse the offline log path |
| WSL adapter after all Python/setup fixes | 231 passed | `wsl-offline.log`, `wsl-offline.xml`, `wsl-python-packages.txt` |
| Native Windows Python after all Python/setup fixes | 188 passed, 43 skipped | `windows-unit-final.command.json`, `windows-unit-final.stdout.log`, `windows-offline-final.xml` |
| Native Windows protocol generation | Exit 0 | `windows-protocol-final.command.json` |
| Native Windows MCP stdio server | Passed | `windows-stdio-final.command.json`, `windows-stdio-final.stdout.log` |
| Windows MCP client launching WSL server through `wsl.exe` | Passed | `windows-to-wsl-stdio-final.command.json`, `windows-to-wsl-stdio-final.stdout.log` |

The actual stdio checks initialize a client session, enumerate tools and resources,
call the offline manifest, read the matching JSON resource, and reject invalid tool
arguments. Both Windows transports report 111 tools, 145 commands and schema SHA-256
`0dfe9020f6c9377221971c6a7194761d18ee05398165570fcd25eb4f22b2c8cd`.
These are loaded adapter registrations, not negotiated editor capabilities.

Windows is Windows 11 build 26320, AMD64, with Python 3.12.0. WSL is Ubuntu 26.04.1
x86_64 on kernel `6.18.58.1-microsoft-standard-WSL2`, with Python 3.14.4; exact Python packages are
recorded separately for each environment. Unix filesystem discovery/probe tests
use an explicitly skipped fixture on Windows; their full coverage runs inside WSL.
Some tests depending on that fixture are skipped even when their individual check
does not dial a real peer. Native Windows schema/runtime success does not establish
native Windows KiCad/IPC support. The diagnostic CLI reports that transport scope
without attempting discovery or a connection.

The original native Windows failure occurred during server import: the default
Windows temporary path failed Unix socket validation before offline tools could
start. The fix defers validation of the default until an editor request, keeps
explicit endpoint validation, normalizes missing protocol paths, and reports the
native Windows transport limitation. It adds no mutation retries or implicit saves.

## Native Windows build gate

The native CMake attempt used MSVC 19.50.35729, Ninja and Windows SDK 10.0.26100.0.
It stopped at the required `glslangValidator` shader compiler, before a fork binary
could be built or packaged. See `windows-configure-final.command.json` and its
stdout/stderr logs. No toolchain or system packages were installed. An earlier
Visual Studio generator attempt could not resolve its compiler; a subsequent
developer-environment/Ninja attempt identified MSVC and reached the shader gate.

The installed upstream Windows CLI reports 10.0.6. It was used only to inventory
that version, not to validate the fork or attach to any editor. Its version-only
command is recorded as `installed-upstream-windows.command.json` and associated
logs. Native Windows KiCad/IPC, build completion and installer/runtime packaging
remain unqualified independently of any WSL results.

The Ubuntu setup package list now includes the mandatory `glslang-tools`,
`spirv-tools` and `libnng-dev`. Its reporting regression passes with a synthetic
package inventory and sentinel build commands, proving the missing dependencies
are reported before setup touches its build root. This validation does not run
setup against the active build. See `wsl-setup-dependencies.log` and
`wsl-setup-dependencies.xml`. Vulkan headers remain optional.

## Native WSL baseline and supplied-loader phase

The independent baseline native build and install completed at
2026-11-18 15:57:59 UTC. The CLI reports commit
`d880b3be5dbf30003e4b0069262417d0278bf528` and version
`10.99.0-qualification-d880-dirty`. The build uses GCC 15.2.0, CMake 4.2.3,
Ninja 1.13.2 and Release configuration with native QA tests and PCH disabled.
KiCad's version output calls this a "release build" to describe its CMake profile;
that is not a product release qualification. It started with six
jobs and resumed with twelve after observing 32 logical CPUs and ample available
memory. The build log records both invocations and the private metadata setup.
A private tag initially lacked a numeric version and caused a reconfigure error;
correcting that local tag resolved the configuration issue without product edits.

Baseline output is under `logs/native-baseline/`. Tests run on a task-owned Xvfb
X11 display with Mesa software rendering inside WSL2; they do not qualify WSLg or
native Windows GUI behavior. Process maps identify `_eeschema.kiface`,
`_pcbnew.kiface`, `libkicommon` and `libkigal` from the isolated baseline install.

| Baseline workflow | Result |
| --- | --- |
| Windows client -> `wsl.exe` MCP server -> new native editor | Passed live document read |
| Symbol search, creation, save, library registration and placement | Passed |
| Footprint generation, saved embedded STEP, library readback and symbol linking | Passed |
| Live hierarchy creation and unsaved root/child rendering | Passed |
| Annotation, pin connectivity, failed batch rollback, undo/redo, page containment, ERC | Passed |
| PCB operations, routing verification, containment, undo/redo, 3D rendering, DRC and Gerbers | Passed after fixture correction |
| Board stackup/rules, obstacle routing, keepouts and net routing | Passed in a fresh disposable app |
| Real schematic-to-board transfer | Two new footprints, zero errors/warnings |
| Explicit save, app stop/restart, reopen | Hierarchy, UUIDs, cross-sheet nets, board pads and file hashes matched |
| SVG, PDF, netlist, CSV BOM and Gerber exports | Passed after BOM helper correction |

Original failures remain recorded. `e2e_pcb.log` routed manually placed pads with
no assigned net and DRC correctly rejected solder-mask bridges. The failed test
left footprints that obstructed the subsequent `e2e_design.log` run. The corrected
fixture assigns real test nets; fresh-app logs `e2e_pcb-rerun.log` and
`e2e_design-rerun.log` both pass. Route validation and DRC were not relaxed. The
PCB test deliberately creates manufacturing violations; its successful DRC run
reported seven findings and does not claim that fixture is DRC-clean.

The original typed BOM export failed because its request omitted required field
settings. Commit `8cc77eec3362c1b7b1752239fefef83e0fcd9fc5` supplies deterministic
CSV and field defaults. `checks-exports-rerun.log` records the successful checks
and exports; `exports-rerun/persistence.csv` contains R101 and R102 with Reference,
Value, Footprint and Datasheet columns. Supplemental checks verify exports do not
change the saved schematic or PCB hashes.

With short isolated socket paths, malformed and future schematic input, a missing
hierarchical child, malformed PCB input and an incompatible future PCB construct
all timed out at 1.5 seconds; a subsequent 750 ms version read also timed out.
Window-tree logs capture the loading/error dialogs. A minimal PCB containing only
a future version header was accepted and stayed responsive; the incompatible
future construct is a distinct case. All fixture files remained unchanged. An
earlier loader attempt used a 108-byte socket path and failed before reaching the
loader; this harness path-length error was corrected before the reported probes.

The subsequent phase applies only the four native loader files supplied by the
parent, after the independent baseline evidence above. The extracted diff SHA-256
is `a3cfb56976e56f8d19b1718aebd40098465376db369f8509df1e3e0a57f6a862`;
the parent's raw file SHA-256 is
`18a63d6ca88d040e42e07b8ceb7599d01e72fabfbd90de15eeb5938f75e9199b`.
Message extraction/newlines change the raw digest. All four authoritative Git
blob hashes match exactly:

| File | Applied Git blob |
| --- | --- |
| `eeschema/files-io.cpp` | `7c78ed6ea49723ec268d6e7a2c6768285adb033b` |
| `include/kiway_player.h` | `652b8a568a8cc698a449b32c24926595e962609a` |
| `kicad/kicad.cpp` | `fd8108323cd676ac84ab6505bd375eb13c5000df` |
| `pcbnew/files.cpp` | `77fc7922307a9797b5a4eb705b140b36dbc5157c` |

The patched build/install completed at 2026-11-18 16:28:15 UTC, using the separate
`install-patched` prefix. Only those four files were copied into the existing
native source snapshot. The incremental build resets CMake's cached absolute data,
documentation, plugin and template paths so installation preserves the baseline
prefix. The actual application reports
`10.99.0-qualification-d880-loader-patch-dirty`, native commit d880.
This is d880 plus the supplied patch, not pristine d880 or a released build.

All patched guarded workflows passed: symbols, footprints/embedded STEP, hierarchy,
annotation/connectivity, rollback/undo/redo, containment, PCB operations, advanced
routing, real schematic-to-board transfer, ERC/DRC, exports and the explicit
save/restart/reopen fixture. The Windows client also read the patched application's
documents through `wsl.exe` MCP stdio. PCB and advanced routing suites ran in
separate disposable applications. Process maps identify the patched install's
editor modules and libraries. DRC findings remain fixture findings, not a clean
electrical/manufacturing sign-off.

| Disposable load case | Independent baseline | Supplied patch | Patched API afterwards |
| --- | --- | --- | --- |
| Malformed schematic | Modal timeout | `AS_BAD_REQUEST`, 0.306 s | Responsive |
| Future schematic | Modal timeout | `AS_BAD_REQUEST`, 0.314 s | Responsive |
| Missing hierarchical child | Modal timeout | `AS_BAD_REQUEST`, 0.320 s | Responsive |
| Malformed PCB | Modal timeout | `AS_BAD_REQUEST`, 0.426 s | Responsive |
| Compatible PCB with only future version header | Accepted | Accepted, 0.436 s | Responsive |
| Incompatible future PCB construct | Modal timeout | `AS_BAD_REQUEST`, 0.425 s | Responsive |

All loader fixture files stayed unchanged. Window trees show no surviving load/error
dialog for the patched rejection cases. No native correction to the supplied patch
was needed. Baseline binary hashes were rechecked and remain unchanged. Results,
binary hashes, version output and compiler metadata are copied into
`.qualification/evidence/`, including `native-loader-patch/results.json`,
`patched-application-*`, `loader-patch-blobs.txt` and
`baseline-binaries-preserved.json`.

The persistence fixture is `e2e_persistence.py`. It requires explicit isolation and
socket selection, seeds an empty disposable project, creates a two-sheet circuit,
performs real schematic-to-board transfer, explicitly saves schematic and board,
then compares hierarchy, UUIDs, cross-sheet pin/board-pad nets and disk hashes
after a separate application restart. Other guarded workflows and bounded bad-load
probes run only in task-owned applications. SolidCraft handoff is not configured
for this run; footprint STEP embedding uses a stock model as a dependency fixture.

## Linux x86_64 development package and installed runtime

Parent tooling commit `ee4826b8f895c7849027d9b452dbf8f2e62dcf20` was applied here as
`c8d507086afbbf1d8b50e8405dfe8b5ffc81ed72`. Focused helper fixes were published as
`f4a064f5c455a733e02944365876c33f87890c94`. They add separate native provenance,
correct bundled OpenBLAS SONAME handling, separate desktop/frozen dependency scans,
and support a prepared Bubblewrap root when Docker is unavailable. The shared
persistence comparison retains placed IDs, geometry, fields and nets. The later
parent native commit `a7ac63022402eff3d7c23ab89e3041a02cbe2270` contains exactly the
four tested blobs and was incorporated here as
`e7a041e5bbd818adac618c34beda7c68f5a402de`. It identifies corresponding source;
these binaries retain their original d880 plus supplied-patch version stamp.

The MCP was frozen from an explicit Git source archive of c8d507, using a dedicated
Python 3.14.4 environment and PyInstaller 6.22.3. `build-info.json` correctly labels
the revision as caller-supplied archive provenance with unknown dirty state. Every
recorded adapter/protocol source checksum was checked against the retained archive.
The frozen executable passed `check_package.py`, including offline diagnostics,
clean EOF, initialization, tool/resource inventory and invalid/unknown requests.
Its schema remains `0dfe9020f6c9377221971c6a7194761d18ee05398165570fcd25eb4f22b2c8cd`.

Native source was rebuilt with final prefix `/opt/embeint-kicad`, clearing all six
cached resource/plugin paths, then installed through
`DESTDIR=/home/karlvonrichter/embeint-qualification/d880-windows-wsl/stage-package`.
This completed at 16:53:09 UTC. Neither earlier install was replaced. The package's
`distribution.json` records d880, the four patched source blobs/raw hashes,
compiler/platform/configure identity, source-archive hash and 897 native payload
hashes separately from the MCP identity. The packaging script hash is also retained.
Installed files matched all 897 staged native hashes. Baseline and patched binary
hashes were rechecked unchanged.

| Retained artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `embeint-kicad_10.99.0~dev+d880.loader1_amd64.deb` | 186623664 | `680c52652f9430a96c25b793322f7aa58f2d58e5f0ac2e6b65c7a1844cdb99be` |
| `mcp-frozen-c8d507-linux-x86_64.tar.gz` | 44644920 | `82b9562c8e72980e67b537bbc11aee86f9d1fe3d23b513a3050657e3f6468424` |
| `native-source-d880-supplied-loader.tar.gz` | 367491101 | `60c2e8b89fcd96b904ea3f7c20e586932e873ed1e3b2a29038f6541db4c742cb` |
| `mcp-source-c8d507086a.tar.gz` | 319595435 | `1533df36dff561db5fdccfe544f7a4fa8a0442e705b8d04f07e88f14ae4df50c` |

Docker clients were present in Windows and WSL, but the daemon/Rancher distribution
was stopped. No service was started. A separate Ubuntu 26.04.1 base root was
downloaded from Ubuntu, checked against its HTTPS SHA256SUMS, and prepared with
the package and distro dependencies. This matches the builder's Ubuntu/glibc
2.43 scope; the .deb does not claim Ubuntu 24.04 or cross-distro portability.

The initial single-user namespace install failed when DBus's maintainer script
tried to assign an unmapped group. Dependency configuration was completed with
`dpkg --configure -a` under WSL root inside a Bubblewrap filesystem/PID/network
namespace exposing only the disposable runtime root, private `/proc` and basic
`/dev`. Host packages and services were not changed. Final `dpkg --audit` was empty.
This ownership-mapping caveat and both logs are preserved; it is not a claim that
the first rootless apt invocation succeeded. Direct staged CLI inventory also
emitted missing-schema messages before the absolute final prefix existed; the
installed CLI and editor successfully use the real prefix inside the runtime root.

Runtime checks used UID 1000, a read-only prepared root, writable disposable
home/tmp/run directories, separate network/PID namespaces and Xvfb software
rendering. The network interface inventory contained only loopback. There was no
development checkout, outside API socket or system Python executable. The driver
and its interpreter remained outside. This establishes Bubblewrap evidence, not
a Docker runtime result.

The installed distribution passed first launch, bundled MCP stdio, symbol
placement, real pin wiring, exact undo/redo, explicit save/restart/reopen,
connectivity and saved-file hashes. The actual native version was
`10.99.0-qualification-d880-loader-patch-dirty`. A strict initial comparison exposed
regenerated embedded library-graphic UUIDs; a narrowly scoped shared helper now
handles those, with negative tests for changed placed IDs and geometry. A retry
also exposed an owned stale Xvfb lock; every subsequent runtime attempt uses a
separate scratch temporary directory. Earlier logs, projects and snapshots remain.

The parent's three-symbol hierarchical fixture and complete item snapshots were
also run against the patched native install. This exposed two fixture defaults:
no-connect size and hierarchical-label field placement. The fixture now sets them
explicitly. The symbol parser drops a library-ID override identical to the retained
definition ID; the shared helper collapses only that redundant encoding. BOARD's
container UUID is regenerated and absent from the board file; comparison asserts
all footprints share one root outside the item-ID set, then compares that root by
board filename. Placed footprint/pad IDs, pad parents, geometry, fields, effective
library IDs and nets remain exact. Negative tests cover changed geometry, parents,
placed IDs and distinct library overrides.

Follow-up `fd30573096730bb5264c98d9865db5826e437ac0` contains these helper corrections.
The fresh live hierarchical fixture passed: three footprints transferred with zero
errors/warnings, followed by exact normalized schematic/board snapshots, hierarchy,
net membership and disk hashes after explicit save/restart/reopen. ERC/DRC findings
are retained. The first temporary follow-up harness reused an obsolete instance
token after restart; that request was rejected, the owned app was stopped, and
verification was repeated with a fresh client and then a wholly fresh fixture.
The product helpers already construct a new client for every instance.

Final combined offline suites passed 243 tests on WSL and 200 with 43 platform
skips on Windows. A further fresh installed-runtime run passed after extracting
the shared comparison helper. Full raw receipts are in
`.qualification/evidence/packaging/`, including `runtime-install.log`,
`runtime-configure-root-namespace.log`, `check-installed-runtime.log`, source/build
metadata and before/after item snapshots. The durable
[receipt summary](qualification/evidence/windows-wsl-2026-11-18.json) contains exact
native binary, frozen executable and artifact hashes without binaries or customer
content.

Artifacts, source archives, third-party notices, provenance and `SHA256SUMS.json`
are retained in both the WSL root's `artifacts/` and the checkout's
`.qualification/artifacts/`; copy hashes were verified. The .deb includes the MCP
notices and the standalone frozen archive retains them. Source/helper patches and
the configure cache are retained alongside the binaries. The retained files can
be transferred through T3 file attachments or an existing desktop file-transfer
channel; no upload or new connection was started for this task.

## PR review follow-up

Claude Opus 5.5 reviewed PR #5 at `973e8fa808d052f18ac1493378934816d444e1ac`
and reported no blocking native-loader regression. Three packaging/launcher
findings were corrected in source with regression coverage:

- Frozen source identity accepts Git metadata only when its top-level directory
  is the actual source root. An archive nested inside another checkout requires
  its own explicit revision and cannot inherit the enclosing repository's HEAD.
- Native provenance rejects unrecorded regular files in a staged install. Library
  symlink aliases must resolve to hashed files within the install; their exact
  destinations are added to metadata for future packages. Revalidation of the
  preserved stage matched all 897 hashed regular files and four normal library
  aliases. The earlier manifest omitted these aliases; its original bytes and
  artifact hashes remain unchanged.
- Malformed JSON or invalid settings object shapes produce a warning and leave
  the settings unchanged while allowing the desktop launcher to continue to
  KiCad. Recovery of those preferences belongs to the application.

Follow-up offline suites passed 248 tests on WSL and 204 with 44 platform skips
on Windows (the added skip covers Linux install symlinks). Actual Git discovery
inside a nested archive and the `--configure-desktop` subprocess exit/file
preservation checks passed on both platforms. Logs and XML receipts are retained
as `.qualification/evidence/pr5-review-*`. No native binary, frozen executable,
source archive or .deb was rebuilt or relabeled for these helper corrections;
the retained c8d507 frozen artifact does not contain the new settings recovery.

The native evidence used bounded temporary drivers recorded in `.qualification/`,
including drivers importing the committed roundtrip fixture/snapshot functions.
The complete Docker-gated native entry points, Dockerfiles, `stdio_edit_check`
and the `KICAD_MCP_EXECUTABLE` edit/undo path were not exercised as complete
committed workflows. Failed-open preservation checks cover schematic/PCB files,
not project preferences or local history. Bus-migration, unreadable-file and
existing-lock refusal paths remain additional native test cases.

## Remaining gates

Native Windows fork build, editor IPC and installer/runtime packaging remain
unqualified. WSLg and native Windows GUI behavior were not exercised by Xvfb.
The parent's independent ARM64 baseline remains a separate qualification run.
These bounded results do not qualify a product release or all platforms.
