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

`embeint-kicad-mcp --diagnose` inventories the bundled runtime and local endpoints
without dialing them. Add `--probe` only when opt-in version/document reads are
appropriate; the existing Linux/WSL limitations and token redaction still apply.

## Linux package and clean installation

After native and MCP validation, run in the Linux build environment:

```sh
/qualification/venv/bin/python \
    /qualification/src/tools/live_schematic_mcp/qualification/package_linux.py \
    /opt/embeint-kicad /new/output/mcp /new/output/embeint-kicad.deb \
    --version '10.99.0~dev+REVISION'
```

The `.deb` installs the native prefix in `/opt/embeint-kicad` and provides
`embeint-kicad` and `embeint-kicad-mcp` launchers. Native runtime dependencies are
derived from the actual ELF payload. Symbol/footprint libraries are distribution
dependencies. The launcher initializes missing settings and library tables under
`${XDG_CONFIG_HOME:-$HOME/.config}/embeint-kicad`; existing preferences, including a
disabled IPC server, are preserved. Both launchers use the same user-owned IPC
namespace. A native Windows transport or macOS application installer is not
provided by this Linux package.

Put only `embeint-kicad.deb` in a new build context, then build
`Dockerfile.runtime` with that context. Start the resulting image with
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

After collecting final evidence and artifacts, stop/remove the disposable build
and runtime containers and remove their source/build/dependency volumes. Keep
unfinished builds only while they are still needed and record their exact names.
