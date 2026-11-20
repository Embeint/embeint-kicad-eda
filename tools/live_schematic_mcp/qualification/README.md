# Native desktop qualification and development packaging

These helpers build a Linux development distribution, run guarded workflows in
Xvfb, freeze the existing Python MCP server, and check installation in a separate
container. A recipe or an offline test pass is not native qualification. Preserve
the reports for the exact source revision, application version and schema hash.
Windows/WSL results must be recorded separately from Linux ARM64 container results.

The build uses Ubuntu 24.04, LLVM's linker and `-O1 -DNDEBUG` by default. This is a
development configuration, not a performance-qualified release. Override
`KICAD_BUILD_OPTIMIZATION` to choose another optimization level, and
`KICAD_BUILD_JOBS` to select concurrency (default one). Record each configuration.
Build from a source snapshot on the Linux filesystem; large C++ builds reading a
host-mounted macOS/Windows checkout can be substantially slower.

## Build and live checks

Build the image locally. Publishing it is a separate action; internal images use
the development registry namespace.

```sh
docker build -t registry.taileef599.ts.net/embeint-kicad/qualification:local \
    tools/live_schematic_mcp/qualification
```

Mount a source snapshot at `/qualification/src`, a persistent build directory at
`/qualification/build`, an install directory at `/opt/embeint-kicad`, and an
evidence directory at `/qualification/evidence`. Set `KICAD_SOURCE_REVISION` to the
snapshot's commit, then run inside that image:

```sh
bash /qualification/src/tools/live_schematic_mcp/qualification/build.sh
xvfb-run -a /qualification/venv/bin/python \
    /qualification/src/tools/live_schematic_mcp/qualification/open_cases.py
xvfb-run -a /qualification/venv/bin/python \
    /qualification/src/tools/live_schematic_mcp/qualification/run.py
```

The Python environment lives in the build container. Keep the container or persist
`/qualification/venv` until the checks finish. `build.sh` records installed native
and Python packages and configure/build/install/offline-test logs. A source
archive has no Git metadata; its application version can contain an unknown commit.
Keep the exact source archive, its SHA-256 and the revision record beside the
binary in that case. Do not label an installed upstream binary as a fork build.

Use a container without host configuration, project or socket mounts for live
checks. Each run creates its own temporary config, project and IPC directory, and
stops only the application it started. The scripts refuse an ordinary host run.
They do not connect to the user's running desktop application. Results directories
must be new; rename earlier evidence before rerunning rather than overwriting it.

`open_cases.py` tries malformed and future-version schematic/PCB files, plus a
missing hierarchical sheet. Every rejection must return an IPC error and the
manager must answer a subsequent read. A timeout alone is not a successful check.
The modern future-schematic fixture includes `generator_version`, which triggers
the parser's version check. Each case records its input hash, a bounded read after
the open attempt, and whether the schematic/PCB files remained unchanged, including
when the open timed out. A compatible future-header-only PCB is expected to open.

`run.py` runs the existing symbol, footprint, hierarchy, edit/rollback, PCB and
design-rule checks. Its STEP embedding input is the repository's TO-252 test model;
that checks storage, not the mechanical suitability of a SOIC model. SolidCraft
handoff is outside this suite. The runner then starts a separate project, creates
root/child sheets and three resistors, asserts hierarchical connectivity, transfers
the actual schematic to the board, and checks page containment. It records ERC/DRC
findings; their presence is not silently converted into electrical/manufacturing
sign-off. It explicitly saves, restarts the application, and compares hierarchy,
item contents, net membership and saved-file hashes. Set `KICAD_MCP_EXECUTABLE` to
the frozen server to run its stdio edit/undo check through the packaged runtime.
The direct native launcher adds the installed prefix's private library directory.
Persistence comparison treats footprint definition items as an unordered
collection while retaining their IDs, geometry, fields and duplicate counts.

## Freeze and check MCP

Use a dedicated virtual environment with `requirements.txt` and
`pyinstaller>=6.16,<7` installed. Run with Python isolation enabled so user site
packages and `PYTHONPATH` cannot affect the bundle:

```sh
/path/to/build-venv/bin/python -I \
    tools/live_schematic_mcp/qualification/package_mcp.py /new/output/mcp
/path/to/build-venv/bin/python -I \
    tools/live_schematic_mcp/qualification/check_package.py \
    /new/output/mcp/embeint-kicad-mcp
```

Build each platform's executable on that platform. The output includes the Python
runtime, protocol modules, dependencies, license notices and `build-info.json`.
`--application-version` records the KiCad version the package ships with; with the
adapter version and source revision (`mcp_version`) it is reported by
`embeint-kicad-mcp --version`, `live_mcp_capabilities` and the MCP `serverInfo`.
Licence files are copied only for the distributions that ship in the package. Notices
for native code without its own licence file in that metadata (nng and Mbed TLS in
`pynng`, the runtime's OpenSSL) come from `notices/`, whose `manifest.json` pins the
versions they cover; `notices/README.md` lists what is not covered (crates statically
linked into Rust extension modules). On Windows,
`scripts/build_windows.ps1` runs both steps and installs the result as
`install\mcp\embeint-kicad-mcp.exe` (`-McpOnly` rebundles an existing install).
For an unpacked source archive, pass `--source-revision FULL_COMMIT_HASH`. The
manifest labels that revision as caller supplied, records an unknown dirty state,
and includes adapter/protocol source checksums; it does not invent Git provenance.
The check launches it from an empty directory with a restricted environment,
checks offline diagnostics, performs a real stdio initialization and inventory,
reads the capability resource, verifies argument/unknown-tool errors, and checks
clean shutdown on input EOF. The packaged entry point gives the SDK duplicate
stdio handles so its wrapper cleanup cannot close the bootloader's original
output stream. It makes
no editor requests and does not qualify native editing.

The frozen MCP also includes every advertised skill Markdown file beside its
bundled `skills.py`. `build-info.json` records `skills.skill_versions`,
`skills.skill_file_sha256` (relative POSIX paths), and `skills.skill_content_sha256`.
The aggregate is SHA-256 of UTF-8 canonical JSON of the path-to-file-hash map
(`sort_keys=True`, separators `,` and `:`); version labels are in the hashed
frontmatter. `schema_sha256` continues to cover only tool/protobuf schemas.
The packager rejects missing/unsafe skill data and checks delivered bytes before
publishing its output. Local non-Markdown configuration is not bundled.

`check_package.py` reads every skill through both real stdio tools and resources,
checks discovery, initialization guidance, versions, hashes, missing-file and
traversal errors. It launches the frozen process from an empty working directory
with no Python on PATH and deliberately absent PYTHONHOME/PYTHONPATH locations.
For source-versus-frozen evidence, capture `skills.skill_manifest()` as JSON from
the source environment before freezing and pass its path as
`--expected-skill-manifest /path/to/source-skills.json`. The check can be copied
outside the checkout; neither the frozen executable nor its data depends on that
checkout or a system Python. The checker itself uses an MCP client environment.
This validates content delivery, not independent fresh-session design behavior,
native editing, or electrical acceptance; those require separate evaluation.

`embeint-kicad-mcp --diagnose` inventories the bundled runtime and local endpoints
without dialing them. Add `--probe` only when opt-in version/document reads are
appropriate; the existing Linux/WSL limitations and token redaction still apply.

### Native Rust server in the package

Add `--rust` to `package_mcp.py` (it needs cargo) to bundle the native Rust MCP server
`embeint-mcp` and the pin layout runner `embeint-layout` beside `embeint-kicad-mcp`. `rust_bundle.py`
builds them from this checkout with `cargo build --locked`. It copies the licence files of every
statically linked crate, and the Rust standard library's notices, into `licenses/rust/`, and records
the binaries' hashes, the toolchain, `Cargo.lock` and the native server's schema hash under `rust`
in `build-info.json`.

When that section is present, `check_package.py` also checks the native server from the same
restricted environment: its tools must be the recorded ones, with schemas identical to the bundled
Python server's. The layout runner must answer, and the frozen server must find it next to its
executable. `scripts/build_windows.ps1` adds `--rust` when cargo is available. The native server
covers a subset of the tools and is used only when a client registers it (rust/MCP.md).

### Release workflow

`.github/workflows/release-mcp.yml` builds and checks the package with `--rust` on Windows x86_64,
Linux x86_64 and Linux arm64 hosted runners. The Python/Rust parity suite must pass on each platform
first. The workflow uploads each archive with its `check_package.py` report, parity report and
`build-info.json`. Pushing a tag `mcp-v*` (or a manual run with `release_tag`) also creates a draft
GitHub release with the archives and `SHA256SUMS`. Review and publish it by hand. The workflow does
not build KiCad: record the KiCad build a package is meant for with the manual run's
`application_version`, and qualify the pair natively as described above.

## Linux package and clean installation

After native and MCP validation, run in the Linux build environment:

```sh
/qualification/venv/bin/python \
    /qualification/src/tools/live_schematic_mcp/qualification/package_linux.py \
    /opt/embeint-kicad /new/output/mcp /new/output/embeint-kicad.deb \
    --version '10.99.0~dev+REVISION' --native-provenance /new/output/native.json
```

The `.deb` installs the native prefix in `/opt/embeint-kicad` and provides
`embeint-kicad` and `embeint-kicad-mcp` launchers. Native runtime dependencies are
derived from the actual ELF payload. Symbol/footprint libraries are distribution
dependencies. The launcher initializes missing settings and library tables under
`${XDG_CONFIG_HOME:-$HOME/.config}/embeint-kicad`; existing preferences, including a
disabled IPC server, are preserved. Both launchers use the same user-owned IPC
namespace. A native Windows transport or macOS application installer is not
provided by this Linux package.

The required native provenance JSON has `source_revision`, `application_version`,
`source_archive` (`filename` and `sha256`), `build`, `platform`, and `files_sha256`
(paths relative to the native install). Include the exact patched-source blobs,
configure identity, compiler and libc/distro in the corresponding fields. The
packager checks at least the manager, CLI and both editor hashes against the
actual payload and retains this identity separately from the frozen MCP metadata
in `distribution.json`. Preserve the referenced source archive with the package.
Desktop and frozen-interpreter dependencies are scanned separately: bundled C++
runtime libraries in MCP do not satisfy the desktop launcher's search path.

Put only `embeint-kicad.deb` in a new build context, then build
`Dockerfile.runtime` with that context. Its default Ubuntu version is 24.04;
set `--build-arg UBUNTU_VERSION=26.04` for a build using Ubuntu 26.04. Match the
builder's distro/libc; these packages do not promise cross-distro portability.
Start the resulting image with
`--network none` and no mounts. From outside that container, run:

```sh
/path/to/test-venv/bin/python -I \
    tools/live_schematic_mcp/qualification/check_install.py \
    CONTAINER /new/output/installed-workflow.json
```

That driver requires the runtime image's unprivileged user, rejects host mounts
and network access, and uses the installed executable through real MCP stdio. It
checks first launch, symbol placement, wiring, undo/redo and explicit save/restart/
reopen. The application container has no development checkout or MCP virtual
environment. Keep the corresponding source archive and all required component
notices with distributed builds; these scripts do not publish a release.

The packager stages on the Linux temporary filesystem so frozen runtime symlinks
are preserved even when the output directory is a host-shared mount. The runtime
image allocates an unused UID for `qualification`; base images may already own
UID 1000. The ARM64 installed workflow was verified with 2400 MiB RAM and 1024 MiB
permitted swap. A 1400 MiB run hit memory pressure and timed out; this is not a
general minimum-memory specification.

When no Docker daemon is available, `check_install.py ROOT OUTPUT --runtime bwrap`
accepts a separately prepared Ubuntu root with the `.deb` and its dependencies
installed. Preparation may use network access; the driver forces a new network
and PID namespace for every runtime process. It exposes only that root read-only
and its disposable home/tmp/run directories writable, uses UID 1000, and rejects a
system Python executable. No development checkout or outside socket is mounted.
The test driver and its Python interpreter remain outside the runtime. This is
Bubblewrap evidence and does not qualify Docker. Prepare a fresh root matching
the build distro; do not supply the host root or reuse a customer application root.

After collecting final evidence and artifacts, stop/remove the disposable build
and runtime containers and remove their source/build/dependency volumes. Keep
unfinished builds only while they are still needed and record their exact names.
