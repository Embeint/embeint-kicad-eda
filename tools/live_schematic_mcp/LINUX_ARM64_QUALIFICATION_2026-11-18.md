# Linux ARM64 native and installed-distribution qualification, 2026-11-18 UTC

The d880 baseline and the exact four-file noninteractive loader patch passed the
bounded ARM64 live workflows. The patched Ubuntu 24.04 `.deb` also passed a fresh,
unprivileged Docker installed-runtime check with no network, mounts, checkout or
system Python executable. This is development-build evidence, not a release.

## Source and build identity

Baseline source was `d880b3be5dbf30003e4b0069262417d0278bf528`. Its native source
snapshot excluded the host's then-uncommitted loader changes. Baseline build,
install, payload inventory, prefix backup and guarded live reports were archived
before the source and host Git metadata advanced to clean
`b47c4fd2ce7cdc9dd071ac7b79da3875e36f6fc6`. This ancestry preserves the completed
Windows/WSL thread's commits and authorship through `973e8fa`.

The patched binary's CLI commit is b47c4fd. Its actual version is
`10.99.0-unknown`; CLI about appends `release build` to describe the CMake profile.
Neither string establishes product release qualification. The four native blobs
are the same ones tested independently in WSL:

| File | Git blob |
| --- | --- |
| `eeschema/files-io.cpp` | `7c78ed6ea49723ec268d6e7a2c6768285adb033b` |
| `include/kiway_player.h` | `652b8a568a8cc698a449b32c24926595e962609a` |
| `kicad/kicad.cpp` | `fd8108323cd676ac84ab6505bd375eb13c5000df` |
| `pcbnew/files.cpp` | `77fc7922307a9797b5a4eb705b140b36dbc5157c` |

Build environment: Ubuntu 24.04 ARM64, GCC 13.3, lld, one compilation job,
`Release -O1 -DNDEBUG`, install prefix `/opt/embeint-kicad`. C++ QA tests, Sentry,
demos and i18n were disabled. The 235-passed/1-skipped baseline build-pipeline
suite is a **Python adapter suite**, not a native C++ QA suite.

An initial baseline compiler OOM was archived and the same cache resumed with
1600 MiB RAM and 1024 MiB permitted container swap, supported by two dedicated
1 GiB VM swap files. A later incremental patch attempt incorrectly reused source
copy timestamps and rebuilt only metadata/links. Failed probes exposed the stale
objects; that attempt is explicitly invalid. After correcting four input mtimes
without changing bytes, the actual 1407-target rebuild completed. Provenance
checks verified that all three changed CPP objects were newer than their source
and shared header and had different SHA-256 values from the stale objects. All
192 installed native file hashes and the actual CLI commit were captured.

## Loader and live results

| Guarded input | d880 baseline | b47 patch | Patch API read / files |
| --- | --- | --- | --- |
| Malformed schematic | Open and subsequent read timed out | Refused in 0.339 s | Responsive / unchanged |
| Future schematic with modern generator metadata | Both timed out | Refused in 0.202 s | Responsive / unchanged |
| Missing hierarchical child | Both timed out | Refused in 0.229 s | Responsive / unchanged |
| Malformed PCB | Both timed out | Refused in 0.317 s | Responsive / unchanged |
| Unsupported future PCB | Both timed out | Refused in 0.312 s | Responsive / unchanged |
| Compatible PCB with only a future header | Opened | Opened in 0.281 s | Responsive / unchanged |

Invalid requests are checked for `AS_BAD_REQUEST`. Baseline open timeout was about
2 s, followed by a separately bounded 750 ms read. These checks use fresh owned
managers and disposable projects, and compare every fixture file's hash.

Both native phases passed `e2e_smoke`, `e2e_features`, `e2e_symbols`,
`e2e_footprints`, `e2e_pcb` and `e2e_design`, plus the frozen MCP edit/undo check.
The hierarchical fixture transferred three real schematic footprints to the PCB,
verified connectivity and hierarchy, explicitly saved, stopped/restarted and
reopened, and compared normalized complete item/net snapshots and saved-file
hashes. The fixture reported 10 ERC and 3 DRC findings; successful execution is
not a clean-design signoff.

Five helper corrections precede this native run in b47: modern future-schematic
generator metadata; recording post-open reads and file invariants even after
failure; private native library lookup; and precise unordered footprint-definition
item comparison with a negative content/duplicate regression. Latest associated
adapter suites passed 242/2 skips on macOS Python 3.14 and 243/1 skip on ARM64
Python 3.12. The ARM64 skip needs Python 3.14 `compression.zstd`.

## Frozen MCP and installed package

The existing ARM64 frozen MCP passed diagnostics, clean EOF and real MCP stdio
contracts in a clean unprivileged Ubuntu 24.04 container without network, mounts,
system Python or a checkout. Its unchanged manifest has the caller-supplied d880
archive label and null dirty state. Checksums establish that all runtime Python
and all 27 protocol inputs match b47; one recorded pytest-only `conftest.py`
differs, and its exact original input is retained. The manifest is not relabeled.
Native and MCP provenance are separate in installed `distribution.json`.

The adapter exposes 111 loaded tools and 145 commands, schema SHA-256
`0dfe9020f6c9377221971c6a7194761d18ee05398165570fcd25eb4f22b2c8cd`.
Those counts describe adapter registrations, not negotiated editor capabilities.

The ARM64 package is `embeint-kicad_10.99.0~dev+b47c4fd.loader1_arm64.deb`,
89,867,048 bytes, SHA-256
`b782755ab2c48cba91d67686ef471d79cc0a33ce5fa1dda25e2249f4e06645b5`.
Its package helper had a small subsequent staging fix: use Linux temporary
storage instead of the host-shared output directory, where copying a valid
frozen GEOS symlink failed. The actual helper SHA-256 is
`78f5b1560a8d641b229e4061ce357cf1c27c4735fb67bfeff09441d590425904`.
The exact patch is retained beside the artifact; it does not change native b47
source or relabel its binaries.

`Dockerfile.runtime` used only that `.deb` as build context on Ubuntu 24.04;
installation fetched declared Ubuntu dependencies and stock symbol/footprint
libraries. An existing base-image UID 1000 required allocating an unused UID for
`qualification` (1001 in this image). No host service or account was modified.
The final image ID is
`sha256:f717a3c364bbb5ccafb8c24f002d10359d77eaff81ff16e1b2809381b091cc89`.

The external `check_install.py` driver passed first launch, real installed frozen
MCP stdio, two symbol placements, pin wiring, connectivity, undo/redo, explicit
save, restart/reopen, complete normalized item/net comparison and unchanged saved
file hashes. All 192 installed native hashes and both installed provenance
manifests matched their inputs; `dpkg --audit` was empty. Runtime was
`qualification`, network `none`, no mounts, Xvfb/software rendering, 2400 MiB RAM
and 1024 MiB permitted swap; no memory-limit or OOM events occurred.

A prior fresh runtime with a 1400 MiB cap timed out on pin connection and recorded
629491 memory-limit events without OOM. That attempt's output was retained and
its manager stopped. No mutation was replayed there: the successful run created a
new container and empty fixture. The result does not establish a general minimum
memory requirement.

## Follow-up after the Windows/WSL PR merged

PR #5 subsequently merged as `66b2d28870d9f9eb0748debbbfc42bdfbe51815d`,
including an additional packaging review fix after the original ARM64 artifacts
were produced. The two ARM64 follow-up commits were rebased onto that `main`.
The only conflict was adjacent test insertion; both the new provenance fixture
and the footprint-order/content regression were retained. The native loader
source bytes remain identical to the original b47 build.

The combined rebased source passed **247 tests with 2 skips** on macOS/Python
3.14. The stricter merged provenance guard also accepted the retained ARM64
prefix: all 192 recorded payload entries matched, including four internal library
aliases whose targets are hashed. These are new source/validator checks, not a
new native build or installed-package run. The original frozen MCP and `.deb`
remain unchanged; in particular, they predate the later malformed-preferences
launcher hardening. The artifact hashes, original source identities and earlier
qualification results above are historical receipts for those exact binaries.

## Artifacts, corresponding sources and limits

Raw evidence and binaries are retained at
`/Users/karl/.t3/scratch/embeint-kicad-native-qualification-2026-11-19/linux/`.
The committed [machine receipt](qualification/evidence/linux-arm64-2026-11-18.json)
contains exact native payload/object hashes, manifests, package/source/notices
hashes, loader results and source-download receipts. Main raw records are
`evidence/baseline-live`, `baseline-open-cases`, `baseline-installed`,
`live`, `open-cases`, `patch-installed`, `installed-workflow-arm64.json`,
`installed-runtime-payload-verification.json` and the phase logs.

Keep the package with `evidence/embeint-kicad-b47c4fd-source.tar.gz` (318,226,288
bytes; SHA-256
`d70b7b20389a1d85b7afecd391741a3e351bd14363e14ae9ddbbb6388ed2e17c`),
`packaging-followup.patch`, `mcp-bomfix-input-conftest.py`, and
`embeint-kicad-arm64-notices.tar.gz`. The notices companion retains 156 native,
Python/component and bundled Ubuntu-library copyright/license files; the full
native archive retains vendored source. `corresponding-sources/` contains exact
Ubuntu GCC-runtime/Python/readline source packages, GEOS 3.14.1 source, and all 46
frozen-build-environment Python distribution source archives, including build/test
packages. Download URLs and SHA-256 receipts are retained. This is not a separate
license audit or a reproducible-build claim.

The [Windows/WSL record](WINDOWS_WSL_QUALIFICATION_2026-11-18.md) separately bounds
its WSL baseline/patch live checks, Ubuntu 26.04.1 x86_64 Bubblewrap installed
runtime and DBus namespace setup caveat. Its final suites passed 243 on WSL and
200 with 43 platform skips on native Windows Python. Those checks are for their
recorded source phases. Native Windows fork application/build/installer, WSLg,
physical Linux desktop behavior and macOS native application packaging remain
unqualified. macOS adapter/frozen MCP checks do not establish a native app build.
No release, upstream submission or fork 10.0.6 qualification is claimed.
