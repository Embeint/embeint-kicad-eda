# Embeint KiCad

Embeint KiCad is an Embeint-maintained fork of KiCad for AI-assisted electronic
design. The project aims to make schematic generation and PCB workflows native,
live and inspectable, with first-class Model Context Protocol (MCP) support and a
long-term path to shared Rust engines and WebAssembly applications.

Source and contributions: [Embeint/embeint-kicad-eda](https://github.com/Embeint/embeint-kicad-eda).

This is an independent project built on KiCad. It is not an official KiCad release
and is not endorsed by the KiCad project. KiCad and its contributors provide the
underlying editors, electrical engines and file formats; their attribution and
licenses remain in this repository.

## Project goals

- **AI schematic generation:** create and refine hierarchical schematics through
  the live editor, using real symbols, pins, wires and connectivity. Generated
  designs must be readable, reviewable and checked with KiCad's electrical rules.
- **First-class MCP support:** provide a supported, bundled interface for assistants
  to discover capabilities, inspect the open design and perform explicit, undoable
  edits. Keep the design engine usable across model providers and assistant clients.
- **A dependable desktop application:** package reproducible builds, isolate
  development configuration, test save/reopen behavior and publish clear
  compatibility information before calling a build a supported release.
- **Shared Rust systems:** replace bounded components incrementally behind stable
  interfaces, starting with deterministic layout and design operations. Validate
  each replacement against the existing implementation and native KiCad checks.
- **WebAssembly and browser workflows:** reuse portable Rust components for browser
  viewing and layout previews, then develop interactive editing. Desktop KiCad
  initially remains responsible for native electrical checks and manufacturing
  exports.
- **KiCad compatibility and maintenance:** preserve familiar project and library
  workflows, track upstream changes and qualify updates before promoting releases.

## Current status

This is a development fork. It already includes C++ IPC extensions and a Python
MCP server for live schematic, PCB, symbol and footprint editing; hierarchy and
connectivity inspection; rendering; ERC/DRC; and explicit saves and exports.
Supported item-edit batches use KiCad commits and undo. Some settings operations
have separate limitations, documented by their tools.

The current source identifies as the **10.99 development line**. Compatibility with
KiCad 10.0.6, particularly saving files that 10.0.6 can reopen, has not been qualified.
Use copies of production designs when evaluating the fork.

Bundled installers, a Rust MCP runtime, shared Rust engines and a browser editor are
**planned work**. The current MCP server runs in Python; this repository does not
yet provide an entirely Rust application or a browser-only KiCad replacement.

## Roadmap

1. Establish the Embeint distribution, project identity and release process.
2. Deliver a repeatable desktop schematic-generation workflow with bundled MCP,
   hierarchical fixtures, connectivity checks, undo and save/reopen validation.
3. Port a bounded schematic-layout component to Rust and validate native/WASM parity.
4. Migrate MCP tools to a bundled native executable while preserving their contracts.
5. Deliver browser previews and then interactive editing using the shared components.

See the [engineering roadmap](tools/live_schematic_mcp/EMBEINT_FORK_ROADMAP.md) for
architecture, delivery gates, compatibility requirements and planning assumptions.
The immediate priority is [complete, qualified MCP support](tools/live_schematic_mcp/MCP_SUPPORT_PLAN.md)
for the desktop workflows before Rust or WebAssembly migration.

## Build and use the development version

The documented development environment is Windows with WSL Ubuntu. Building KiCad
requires substantial native dependencies; the setup script reports missing system
packages. See the [live MCP setup guide](tools/live_schematic_mcp/README.md) for
prerequisites, client registration and tool reference.

From the repository root in WSL:

```bash
tools/live_schematic_mcp/scripts/setup_wsl.sh
tools/live_schematic_mcp/run_kicad_wsl.sh
```

For a new checkout, clone the Embeint product branch first:

```bash
git clone --branch main https://github.com/Embeint/embeint-kicad-eda.git
cd embeint-kicad-eda
```

Run the launcher in a separate terminal while testing. Use the disposable
`live-demo` project, which is the launcher's default, for live tests:

```bash
tools/live_schematic_mcp/scripts/run_tests.sh
tools/live_schematic_mcp/scripts/run_tests.sh --live
```

The live tests edit the open test document. Do not point them at a production
project. The [agent and developer guide](tools/live_schematic_mcp/AGENTS.md) explains
the code structure, test guard and editor integration requirements.

### Native Windows build

To open and edit projects with the fork as a normal Windows application, build it
natively with Visual Studio ("Desktop development with C++"; tested with Visual
Studio 2026). From the repository root in PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File tools\live_schematic_mcp\scripts\build_windows.ps1
```

The script clones vcpkg into `C:\kb\vcpkg` and builds into `C:\kb\build`. Use `-Root`
for another location, but keep the path short. It then installs to `C:\kb\install`,
copies the runtime DLLs and links the stock symbol, footprint and 3D libraries from
the newest installed upstream KiCad. It also adds an "Embeint KiCad" Start Menu
shortcut. The first run builds every dependency from source and takes hours; later
runs rebuild only what changed. Pass `-NoConfigure` to skip reconfiguring.

The native build keeps its settings separate from an upstream KiCad install.

The MCP adapter drives the native editor too, over KiCad's local named pipe. Set it up
and start an isolated test instance with:

```powershell
tools\live_schematic_mcp\scripts\setup_windows.ps1      # Python venv in C:\kb\venv and protocol code
tools\live_schematic_mcp\run_kicad_windows.ps1 -Wait    # isolated config, API enabled, disposable live-demo project
tools\live_schematic_mcp\scripts\run_tests.ps1 -Live    # unit and end-to-end tests against it
```

Register `run_mcp_windows.ps1` as the MCP server, or, without Python or this checkout,
the bundled `C:\kb\install\mcp\embeint-kicad-mcp.exe` that `build_windows.ps1` installs
(Python 3.11+ is needed only to build it; `-McpOnly` rebundles it). See
[Setup (native Windows)](tools/live_schematic_mcp/README.md#setup-native-windows) for
details and limits; the WSL build and its scripts work as before.

## Contributing and licensing

Contributions to this fork belong in this repository. Read
[CONTRIBUTING.md](CONTRIBUTING.md) for Embeint's development and AI-assistance
guidelines. Upstream KiCad has its own contribution policies, which apply to work
submitted to upstream KiCad.

The combined KiCad distribution is licensed under **GPLv3 or later**, with component
licenses detailed in [LICENSE.README](LICENSE.README). See [LICENSE](LICENSE),
[AUTHORS.txt](AUTHORS.txt) and the existing source notices for terms and attribution.
Keep corresponding source available for distributed builds.

## Upstream KiCad reference

For specific documentation about [building KiCad](https://dev-docs.kicad.org/en/build/), policies
and guidelines, and source code documentation see the
[Developer Documentation](https://dev-docs.kicad.org) website.

You may also take a look into the [Wiki](https://gitlab.com/kicad/code/kicad/-/wikis/home),
the [contribution guide](https://dev-docs.kicad.org/en/contribute/).

For general information about KiCad and information about contributing to the documentation and
libraries, see our [Website](https://kicad.org/) and our [Forum](https://forum.kicad.info/).

## Build state

KiCad uses a host of CI resources.

GitLab CI pipeline status can be viewed for Linux and Windows builds of the latest commits.

## Release status
[![latest released version(s)](https://repology.org/badge/latest-versions/kicad.svg)](https://repology.org/project/kicad/versions)
[![Release status](https://repology.org/badge/tiny-repos/kicad.svg)](https://repology.org/metapackage/kicad/versions)

## Files
* [AUTHORS.txt](AUTHORS.txt) - The authors, contributors, document writers and translators list
* [CMakeLists.txt](CMakeLists.txt) - Main CMAKE build tool script
* [copyright.h](copyright.h) - A very short copy of the GNU General Public License to be included in new source files
* [Doxyfile](Doxyfile) - Doxygen config file for KiCad
* [INSTALL.txt](INSTALL.txt) - The release (binary) installation instructions
* [uncrustify.cfg](uncrustify.cfg) - Uncrustify config file for uncrustify sources formatting tool
* [_clang-format](_clang-format) - clang config file for clang-format sources formatting tool

## Subdirectories

* [3d-viewer](3d-viewer)         - Sourcecode of the 3D viewer
* [cmake](cmake)      - Modules for the CMAKE build tool
* [common](common)            - Sourcecode of the common library
* [cvpcb](cvpcb)             - Sourcecode of the CvPCB tool
* [demos](demos)             - Some demo examples
* [doxygen](doxygen)     - Configuration for generating pretty doxygen manual of the codebase
* [eeschema](eeschema)          - Sourcecode of the schematic editor
* [gerbview](gerbview)          - Sourcecode of the gerber viewer
* [include](include)           - Interfaces to the common library
* [kicad](kicad)             - Sourcecode of the project manager
* [libs](libs)           - Sourcecode of KiCad utilities (geometry and others)
* [pagelayout_editor](pagelayout_editor) - Sourcecode of the pagelayout editor
* [patches](patches)           - Collection of patches for external dependencies
* [pcbnew](pcbnew)           - Sourcecode of the printed circuit board editor
* [plugins](plugins)           - Sourcecode for the 3D viewer plugins
* [qa](qa)                - Unit testing framework for KiCad
* [resources](resources)         - Packaging resources such as bitmaps and operating system specific files
    - [bitmaps_png](resources/bitmaps_png)       - Menu and program icons
    - [project_template](resources/project_template)          - Project template
* [thirdparty](thirdparty)           - Sourcecode of external libraries used in KiCad but not written by the KiCad team
* [tools](tools)             - Helpers for developing, testing and building
* [translation](translation) - Translation data files (managed through [Weblate](https://hosted.weblate.org/projects/kicad/master-source/) for most languages)
* [utils](utils)             - Small utils for KiCad, e.g. IDF, STEP, and OGL tools and converters
