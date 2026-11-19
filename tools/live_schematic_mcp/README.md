# Live KiCad MCP

An MCP server that edits the schematic and board **open in KiCad** through the IPC API: no file
edits, no save/reload. Changes appear in the editor immediately, each tool call is one undo step,
and `live_schematic_view` / `live_pcb_view` return PNGs KiCad renders from the in-memory (unsaved)
design.

Requires this development build (10.99 / KiCad 11). Released KiCad 10.x has no schematic IPC.

The proposed path to an Embeint distribution with bundled MCP, shared Rust components
and a browser client is in [Embeint fork roadmap](EMBEINT_FORK_ROADMAP.md).
Desktop MCP reliability and workflow coverage take priority; see the
[MCP support delivery plan](MCP_SUPPORT_PLAN.md) and the
[IPC/MCP review record](IPC_MCP_REVIEW.md).

`live_mcp_capabilities` and the JSON resource `embeint://mcp/capabilities` expose the
loaded MCP tool schemas, curated IPC command schemas and a deterministic schema
fingerprint without connecting to KiCad. This describes the local adapter; it does
not assert that every operation is supported by the running editor or document.
The unit suite also exercises the real MCP stdio initialize/discover/call/resource
protocol, and the `MCP contracts` GitHub workflow runs these checks on pull requests.

Design work uses the bundled [Embeint house style](skills/kicad-house-style/SKILL.md).
MCP initialization instructions direct agents to `live_skill_reference` with
`skill="kicad-house-style"`; the entrypoint links the reference for each task.
With no arguments the tool lists skills, versions and files. Offline resources
provide the same data: `kicad-skill://index`, `kicad-skill://{skill}/{file}` and
`kicad-skill://manifest`. The manifest records raw UTF-8 file SHA-256 hashes and
an aggregate content hash separately from the capability schema fingerprint.
Workstation paths and optional selector configuration stay outside the portable
guidance. Start a new server process/session to receive updated initialization
instructions and bundled content; discovery does not establish live design acceptance.

The editor transport is KiCad's local IPC only: a Unix socket for the WSL/Linux build,
and a local named pipe (`\\.\pipe\<temp>\kicad\api.sock`, URL
`ipc://C:\Users\...\Temp\kicad\api.sock`) for the native Windows build. Run the server
next to the KiCad it drives: inside WSL for the WSL build (through `wsl.exe`), natively
for the native build (see [Setup (native Windows)](#setup-native-windows)). Network
transports, UNC pipes and dot-segment pipe paths are rejected before any request.
Offline tests that create Unix socket peers are skipped on Windows and must also run
inside WSL; the named-pipe tests run only on Windows.

Atomic batches accept only item edits that participate in the current commit.
Settings, file operations, editor replacement and asynchronous work are excluded.
Omitted command document targets inherit the batch document; explicit targets must
match it exactly. Selection and zoom changes are not rolled back. Inspect a command
with `live_schematic_describe_command` to see its `batchable` status.

Routing verification checks all clearance, keepout and manufacturing findings for
the new route UUIDs, without truncating the internal DRC report. A rejected route
is removed by its UUIDs in a separate undoable cleanup, preserving unrelated user
edits. A timeout or unconfirmed cleanup reports that the editor needs inspection.
This is scoped route validation, not a claim that the whole board is DRC-clean.

## Tools

| Tool | Purpose |
| --- | --- |
| `live_skill_reference` | Offline bundled Embeint design guidance; no arguments lists skills, `skill=` reads SKILL.md, `file=` selects a linked reference |
| `live_schematic_documents` | Open schematic + KiCad version; returns the document spec other tools take |
| `live_schematic_open` | Open the project's schematic when KiCad runs as the project manager |
| `live_schematic_hierarchy` | Live sheet tree, including unsaved sheets, with UUID sheet paths |
| `live_schematic_items` | Items on one sheet (protobuf JSON, nanometers) |
| `live_schematic_create_items` / `_update_items` / `_delete_items` | Batched edits, one undo step, rolled back on any rejection |
| `live_schematic_place_symbol` | Place `Library:Symbol` at mm coordinates |
| `live_schematic_view` | PNG of the whole page in its current unsaved state, with the drawing sheet (border, title block) unless `frame=false` |
| `live_schematic_page` | Paper and orientation, drawing area, title block box and fields (mm), and items outside the border or in the title block |
| `live_schematic_set_page` | Set the paper (A4, A3, ...), orientation and title block fields |
| `live_schematic_focus` | Switch the editor to a sheet and zoom to items |
| `live_schematic_item_schema` | Field reference for item types (SheetSymbol, SheetPin, ...) |
| `live_schematic_save` | Explicit save, only when asked |
| `live_schematic_annotate` | Number references (all / one sheet / listed symbols; by X or Y; first free or sheet x100/x1000) |
| `live_schematic_update_from_library` | Reload symbols from their library, or swap them to another `Library:Symbol` |
| `live_schematic_autoplace_fields` | Re-run field autoplacement on symbols, sheets and labels |
| `live_schematic_move` | Move / rotate / mirror items; attached wires follow and stay square |
| `live_schematic_undo` | Undo or redo the editor's last change |
| `live_editor_grid` | Read or set any editor's grid (size, presets, visibility, style, snapping) |
| `live_schematic_symbol_pins` | Placed pins with absolute positions, drawn direction and current net |
| `live_schematic_connect_pins` | Orthogonal wire between two pins (`R1`,`2` to `U1`,`VCC`) |
| `live_schematic_connect_pin_to_net` | Stub wire plus local/global/hierarchical label on a pin |
| `live_schematic_netlist` | Live nets with their `REF.PIN` members |
| `live_schematic_erc` | Electrical rules check of the unsaved design |
| `live_schematic_select` / `_highlight_nets` | Show items or nets to the user in the editor |
| `live_schematic_export` | SVG, PDF, DXF, PS, netlist or BOM from the unsaved design |
| `live_schematic_list_commands` / `_describe_command` / `_execute` | Every IPC command KiCad registers, by name |
| `live_schematic_batch` | Several commands as one undo step, all rolled back if any fails |

### Symbol libraries and the Symbol Editor

| Tool | Purpose |
| --- | --- |
| `live_symbol_search` | Search all loaded symbol libraries by name, keywords and description |
| `live_symbol_get` | One library symbol's fields, filters, units and pins |
| `live_symbol_libraries` | Rows of the global and/or project symbol library table |
| `live_symbol_create_library` | New empty `.kicad_sym`, added to the global or project table |
| `live_symbol_register_library` / `_unregister_library` | Add or remove an existing library file in a table |
| `live_symbol_new` / `live_symbol_edit` | Start a new symbol, or open one, in the Symbol Editor (shown live, unsaved) |
| `live_symbol_add_pins` | Pins laid out around a body rectangle on the 2.54 mm grid, from a pin list |
| `live_symbol_set_properties` | Value, footprint, datasheet, description, keywords, filters, user fields (MPN ...) |
| `live_symbol_view` | PNG of the open symbol, including unsaved changes |
| `live_symbol_summary` / `_delete_items` | Pins and shapes of the open symbol; remove items |
| `live_symbol_save` | Write the symbol to its library and reload it, ready for `live_schematic_place_symbol` |

Typical flow: search first; if nothing fits, `live_symbol_create_library` (once), `live_symbol_new`,
`live_symbol_add_pins`, `live_symbol_set_properties`, check with `live_symbol_view`, `live_symbol_save`,
then place it. Pins of multi-unit symbols cannot be assigned to units yet.

Defaults: new symbols get hidden `Manufacturer` and `MPN` fields. Pins follow datasheet convention:
supplies and control inputs on the left, outputs and feedback (ADJ, FB, PG) on the right, grounds
at the bottom. Supplies come first on each side, with two empty rows (7.62 mm, a resistor's pin
spacing) before the signals, so an LDO gets VIN over EN and VOUT over ADJ. The designator sits above
the value at the body's bottom right. Pass sides with `arrange="as_given"` to follow a specific
datasheet pinout exactly.

### Footprints, 3D models and embedded files

| Tool | Purpose |
| --- | --- |
| `live_footprint_search` / `live_footprint_get` | Find footprints by name; read pads, attributes, datasheet and 3D models |
| `live_footprint_libraries` / `_create_library` / `_register_library` / `_unregister_library` | Footprint library tables (global or project); new `.pretty` libraries |
| `live_footprint_new` / `live_footprint_edit` | Start or open a footprint in the Footprint Editor (shown live, unsaved) |
| `live_footprint_generate` | KLC-style footprint from datasheet dimensions: SOIC/TSSOP/MSOP, QFN/DFN (with exposed pad), SOT-23/223, chip 0402-2512, pin headers |
| `live_footprint_set_properties` | Description, keywords, SMD/THT, value, datasheet link, BOM/position exclusions |
| `live_footprint_attach_step` | Embed a STEP/WRL model in the footprint and use it as the 3D model (`kicad-embed://`) |
| `live_embedded_files` / `live_embed_file` / `live_remove_embedded_file` | Files embedded in an open footprint or library symbol |
| `live_footprint_view` / `live_footprint_summary` | PNG of the unsaved footprint; its pads, graphics and texts |
| `live_footprint_save` | Write the footprint to its library and reload it, ready to link from a symbol's Footprint field |

Generated footprints follow the KiCad Library Conventions: pads from the datasheet land pattern
(or IPC-7351 nominal fillets from the lead dimensions), roundrect pads at 25 % (radius at most
0.25 mm), 0.12 mm silkscreen kept 0.2 mm from copper with a pin-1 marker, a 0.10 mm fabrication
outline with a chamfered pin-1 corner and `${REFERENCE}`, and a 0.05 mm courtyard 0.25 mm outside
(0.5 mm for connectors) on a 0.01 mm grid. Datasheets are linked in the Datasheet field, not
embedded; STEP models are embedded so a library carries its own 3D models.

### 3D models from SolidCraft

| Tool | Purpose |
| --- | --- |
| `live_footprint_model_plan` | From the datasheet package drawing: a SolidCraft `batch` (body, gull-wing leads or terminals, pin-1 dimple, colours) and `export` call, in KiCad 3D conventions |
| `live_footprint_view_3d` | KiCad's own 3D render (kicad-cli) of a saved footprint and its models |
| `live_footprint_check_model` | Decode each embedded file, compare it byte for byte with its source, confirm it is the footprint's model, and count the solids KiCad exports when it loads the footprint |

The agent holds both MCP servers and carries the plan across:

1. `live_footprint_generate` the footprint, then `live_footprint_model_plan` with the drawing's A, A1, b, c, E, L
2. SolidCraft MCP: run `solidcraft.arguments` with `batch`, check it with `screenshot`/`measure`, run `export`
3. `live_footprint_attach_step` with the exported file (a Windows path is translated for KiCad in WSL),
   `live_footprint_save`, then `live_footprint_view_3d` and `live_footprint_check_model`

Model conventions: millimetres, origin on the footprint origin at the board's top surface, +Z up,
+X as the footprint, +Y as the footprint's -Y (pin 1 top-left in KiCad sits at model x<0, y>0).
Plans cover gull-wing packages (SOIC/TSSOP/MSOP/SSOP, SOT-23/223), QFN/DFN and chip parts.

Give SolidCraft's MCP a folder on the Windows drive both programs can reach (the plans default to
`%USERPROFILE%\kicad-models`, or set `KICAD_LIVE_MODEL_DIR`):

```bash
claude mcp add solidcraft -- solidcraft-cli mcp --allow-path C:\Users\<you>\kicad-models
```

The Footprint Editor lives in pcbnew; the footprint tools ask the schematic editor to open it
(`OpenFootprintEditor`), which loads pcbnew's code into the running process.

### PCB editor

KiCad must run as the project manager (`run_kicad_wsl.sh project.kicad_pro`, the default), which
hosts the schematic and PCB editors in one process; `live_pcb_open` opens the project's board
(creating it if needed) and returns the board document the other tools take.

| Tool | Purpose |
| --- | --- |
| `live_pcb_open` / `live_pcb_documents` | Open the project's board in the PCB editor; list open boards |
| `live_pcb_summary` / `live_pcb_footprints` / `live_pcb_nets` | Outline, layer count, item counts, footprints (with pads and nets), nets and net classes |
| `live_pcb_update_from_schematic` | Update PCB from Schematic using the schematic's live netlist (dry run available) |
| `live_pcb_place_footprint` / `live_pcb_move` | Place a library footprint; move, rotate, flip and lock footprints by reference |
| `live_pcb_route` / `live_pcb_route_net` | Routes around other nets' copper (with net class clearances), keepouts (also inside footprints) and the board edge, changing layer through vias if allowed; checked by KiCad's DRC and undone if a rule breaks |
| `live_pcb_check_path` / `live_pcb_add_tracks` / `live_pcb_add_via` | Test a proposed track for conflicts; draw tracks (refused if they cross or crowd other nets); vias (net class sizes by default) |
| `live_pcb_outline` | Board outline on Edge.Cuts: rectangle, rounded rectangle or polygon |
| `live_pcb_add_zone` / `live_pcb_add_keepout` / `live_pcb_refill_zones` | Copper pours that follow the outline; keepouts by points or around a footprint (antennas); refill |
| `live_pcb_add_graphics` | Lines, arcs, rectangles, circles, polygons and text on any layer |
| `live_pcb_items` / `_create_items` / `_update_items` / `_delete_items` / `_item_schema` | Raw board items (protobuf JSON), one undo step per call |
| `live_pcb_delete_routing` | Rip up tracks and vias by net and/or layer |
| `live_pcb_view` / `live_pcb_view_3d` | PNG of chosen layers on the page with the drawing sheet; KiCad's 3D render |
| `live_pcb_page` / `live_pcb_set_page` | Page check (drawing area, title block, board outline, items outside the border); paper and title block |
| `live_pcb_origins` | Read or set the grid origin and the drill/place origin |
| `live_pcb_layers` | Copper layer count (through the stackup, refused if a removed layer has items), visible and active layers |
| `live_pcb_stackup` / `live_pcb_set_stackup` | Read or change the stackup: layer count, copper and dielectric thickness, core/prepreg, epsilon_r, loss tangent, board thickness, colours, finish |
| `live_pcb_design_rules` / `live_pcb_custom_rules` | Built-in constraints, predefined track widths and via sizes, DRC severities; custom `.kicad_dru` rules |
| `live_pcb_netclasses` | Default track width, clearance and via size per net class, and net assignments |
| `live_pcb_impedance_width` / `live_pcb_rf_net` | Controlled-impedance width (microstrip, CPWG) from the stackup; RF/antenna net class, assignment and isolation rule |
| `live_pcb_drc` / `live_pcb_check` | Design rules check; the same grouped as clearance, keepout, silkscreen, courtyard, connectivity, manufacturing, schematic |
| `live_pcb_select` / `live_pcb_focus` / `live_pcb_undo` | Show footprints or items in the editor; undo and redo |
| `live_pcb_export` | Gerbers, drill, position, STEP, PDF, SVG, DXF, IPC-2581, IPC-D-356, ODB++, DSN and more from the live board |
| `live_pcb_save` | Explicit save, only when asked |

`live_schematic_execute` and `live_schematic_batch` also take every board command (see
`live_schematic_list_commands`); supported transactional edits in a batch on one
board document form one undo step.

Hierarchy: create a `SheetSymbol` (with `pins`) on the parent, read its path from
`live_schematic_hierarchy`, then target the child sheet with that `sheet_path` to add matching
`HierarchicalLabel`s.

KiCad commands this fork adds for the tools above (`api/proto/schematic/schematic_commands.proto`):
`AnnotateSchematic`, `UpdateSymbolsFromLibrary`, `AutoplaceFields`, `TransformItems`,
`GetSymbolPins`, `NewLibrarySymbol`, `EditLibrarySymbol`,
`SetLibrarySymbolProperties`, `RenderLibrarySymbol`, `OpenFootprintEditor`, `NewLibraryFootprint`,
`EditLibraryFootprint`, `SetLibraryFootprintProperties`, `RenderLibraryFootprint` and
`EmbedFileFromPath` / `GetEmbeddedFileList` / `RemoveEmbeddedFile`; it also implements the library table and
search commands KiCad declared but left unimplemented (`GetLibraryTable`, `Add`/`Update`/
`DeleteLibraryTableEntry`, `SearchLibraries`). Every editor (schematic, symbol, PCB, footprint)
shares `UndoLastChange` / `RedoLastChange`, `GetPageLayout` and `GetGridSettings` /
`SetGridSettings` (`api/proto/common/commands/editor_commands.proto`), and the project manager
answers `OpenDocument` for the project's schematic and board. For boards it adds `TransformBoardItems` (move, rotate, flip and lock in place) and implements
`UpdateBoardStackup`, which KiCad declares but leaves unimplemented. Transactional item-edit
commands join an open `BeginCommit`, so a supported batch is one undo step. Settings
operations have separate effects and are excluded from batches. Undo/redo need the
GUI and refuse an open commit.

## For agents

Start with [AGENTS.md](AGENTS.md): branch policy, where each command lives, build and test
commands, and the library conventions the tools follow.

## Setup (Windows + WSL Ubuntu)

For the native Windows build see [Setup (native Windows)](#setup-native-windows).

`scripts/setup_wsl.sh` does everything below in one go (and is safe to rerun after a pull);
`scripts/rebuild.sh` rebuilds after C++ edits; `scripts/run_tests.sh [--live]` runs the tests.


```bash
# in WSL, once
python3 -m venv ~/kicad-live/venv
~/kicad-live/venv/bin/pip install -r requirements.txt
~/kicad-live/venv/bin/python generate_proto.py

cmake -S <source> -B ~/kicad-live/build -G Ninja -DCMAKE_BUILD_TYPE=Release \
      -DKICAD_BUILD_QA_TESTS=OFF -DCMAKE_INSTALL_PREFIX=$HOME/kicad-live/install
ninja -j 8 -C ~/kicad-live/build && ninja -j 8 -C ~/kicad-live/build install

./run_kicad_wsl.sh            # isolated config, IPC server enabled, project manager with a demo project
```

Keep the build to 8 jobs: each KiCad compile job can take about 1 GB, and one job per core can
exhaust WSL's memory and kill other work on the machine. `scripts/setup_wsl.sh` and
`scripts/rebuild.sh` read the limit from `KICAD_BUILD_JOBS` (default 8).

`run_kicad_wsl.sh` keeps its settings in `~/kicad-live/config` and reuses the symbol and
footprint libraries from the Windows KiCad 10.0 install.

Register the MCP server (it must run inside WSL, where KiCad's Unix socket lives):

```bash
claude mcp add kicad-live -- wsl.exe -d Ubuntu -- bash /mnt/c/<fork>/tools/live_schematic_mcp/run_mcp_wsl.sh
```

## Setup (native Windows)

For the native Windows build (`scripts/build_windows.ps1`, installed in `C:\kb\install`), the
MCP runs in native Python next to KiCad and talks to its named pipe. From the repository root
in PowerShell (`powershell -ExecutionPolicy Bypass -File <script>` if scripts are blocked):

```powershell
tools\live_schematic_mcp\scripts\setup_windows.ps1       # once, and after pulls: venv in C:\kb\venv, protocol code
tools\live_schematic_mcp\run_kicad_windows.ps1 -Wait     # isolated config, API enabled, project manager with live-demo
tools\live_schematic_mcp\scripts\run_tests.ps1 -Live     # unit tests, then the end-to-end tests against that KiCad
tools\live_schematic_mcp\run_kicad_windows.ps1 -Stop     # stops only the KiCad the launcher started
```

`run_kicad_windows.ps1` keeps its settings in `C:\kb\live\config` (never the Start Menu
build's or upstream KiCad's), creates the disposable test project in
`C:\kb\live\projects\live-demo` (`-Fresh` recreates it) and uses the stock libraries that
`build_windows.ps1` linked from upstream KiCad. `-Project <file>` opens another project or a
`.kicad_sch` (schematic editor only). `KICAD_WINDOWS_ROOT` moves `C:\kb`; `KICAD_LIVE_PYTHON`
selects another interpreter. Python 3.11+ (64-bit) is needed; the launcher uses it too.

Only one KiCad on a Windows account can serve the API: the pipe name is fixed and a second
instance runs without an API. The launcher refuses to start while another KiCad with the API
enabled holds the pipe (in either spelling of the temporary directory), so the MCP cannot
reach the wrong instance. Pipes are found by name; their owner is not verified, so use the
MCP where you trust the machine's other local accounts. For your own KiCad, enable
the API in Preferences > Plugins (Enable KiCad API). The MCP finds the pipe in your temporary
directory; to name an instance explicitly, set `KICAD_API_SOCKET` to the exact endpoint
`scripts\wait_ready.ps1` prints (pipe names are literal: `C:\Users\LONGNA~1\...` and
`C:\Users\LongName\...` are different pipes). `python diagnose.py --probe` lists and probes
the candidates.

Register the MCP server (stdout carries only MCP messages):

```powershell
claude mcp add kicad-live -- powershell -NoProfile -ExecutionPolicy Bypass -File C:\<fork>\tools\live_schematic_mcp\run_mcp_windows.ps1
```

### Bundled MCP (no Python, no checkout)

`build_windows.ps1` also installs the MCP as `C:\kb\install\mcp\embeint-kicad-mcp.exe`: the
server frozen with PyInstaller (`qualification\package_mcp.py`), with its own Python runtime,
protocol code, skills and licence notices (`licenses\`). Users of the install need neither
Python nor this repository; 64-bit Python 3.11+ is needed only to build it (`-McpPython` picks the
interpreter and recreates the build environment in `C:\kb\build\mcp\venv`; the script checks it
before compiling). `-McpOnly` rebundles an existing install after an MCP change and `-NoMcp` skips
it. The new package replaces `install\mcp` as a whole: while an MCP client runs the bundled server,
the script refuses and leaves the installed package untouched; close that client and run it again.
Register it directly:

```powershell
claude mcp add kicad-live -- C:\kb\install\mcp\embeint-kicad-mcp.exe
```

```powershell
C:\kb\install\mcp\embeint-kicad-mcp.exe --version            # server and KiCad versions, schema fingerprint
C:\kb\install\mcp\embeint-kicad-mcp.exe --diagnose [--probe]  # runtime, discovered editors, read-only probes
```

The same identity is in `live_mcp_capabilities` (`server_version`, `application_version`,
`source_revision`, `schema_sha256`) and the MCP `serverInfo`. With no editor running, every
editor tool answers at once that no KiCad is listening and what to start; nothing is sent, and
the next call looks again, so the MCP may start before KiCad.

Paths in tool arguments are as the native KiCad reads them (`C:\...`); WSL `/mnt/c/...` paths
are translated. SolidCraft exports default to `%USERPROFILE%\kicad-models`
(`KICAD_LIVE_MODEL_DIR` overrides it).

## Tests

```bash
~/kicad-live/venv/bin/python -m pytest test_client.py      # protocol contracts, no KiCad needed
~/kicad-live/venv/bin/python e2e_smoke.py out/             # against a running editor; never saves
~/kicad-live/venv/bin/python e2e_features.py out/          # annotate, wiring, move, undo, ERC, batch, export
~/kicad-live/venv/bin/python e2e_symbols.py out/           # search, build, save, register and place a symbol
~/kicad-live/venv/bin/python e2e_footprints.py out/        # generate a footprint, embed its STEP, link it from a symbol
~/kicad-live/venv/bin/python e2e_pcb.py out/               # board page, grid, origins, outline, footprints, routing, zones, DRC, exports
~/kicad-live/venv/bin/python e2e_design.py out/            # stackup, impedance, rules, custom rules, net classes, keepouts, routing around copper, checks
SOLIDCRAFT_CLI=/path/to/solidcraft-cli ~/kicad-live/venv/bin/python e2e_model_handoff.py out/     # SolidCraft builds the STEP; KiCad embeds and renders it
```

On native Windows, `scripts\run_tests.ps1 [-Live]` runs the same set with `C:\kb\venv`
(output in `C:\kb\live\test-output`); run single scripts with `C:\kb\venv\Scripts\python.exe`.

## Supported schematic block scripting

The source distribution provides `block_helpers.py` for small generated blocks and
UUID-scoped revisions through the existing Python client. Start with
`python tools/live_schematic_mcp/block_example.py` for an offline synthetic plan,
or read the bundled [helper API reference](skills/kicad-house-style/block-helpers.md).
Stable ownership manifests, stale-state fingerprints and single-use plans preserve
manual items and avoid blind mutation replay. Because KiCad's connectivity cleanup
could merge wires or drop junctions, planning refuses block wires touching manual
wiring, removing a block wire or moving/removing a block pin or label with manual
wiring attached, and moving a pin at a wire branch in one apply; wires are replaced
rather than updated. Build wires, junctions, symbols and sheets with `wire()`,
`symbol()` and `sheet()`, not `add()`. Manifests are version 2; version 1 manifests
lack pin points and are refused. This is a scripting API; no new MCP
helper execution tool is registered. Fingerprints do not provide atomic editor
revision checks, and live use requires a single writer plus pin/net/page validation.
