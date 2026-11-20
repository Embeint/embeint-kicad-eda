# Agent guide: live KiCad over IPC (this fork)

Embeint KiCad builds on the work from `Karl-VonRichter/kicad-source-mirror`. The product
branch is **`main`**; **`live-schematic-ipc`** is the development integration branch.
This fork extends KiCad's IPC API so an MCP server can drive a running KiCad live: schematics,
symbol libraries and the Symbol Editor, footprint libraries and the Footprint Editor, the PCB
editor, embedded files and 3D models.  Nothing here goes upstream; KiCad does not accept AI-generated merge
requests (see the root `AGENTS.md`).

## Branches

- `master` mirrors upstream KiCad (`KiCad/kicad-source-mirror`).  Never commit to it.
- `main` is the Embeint product branch. Promote changes after validation; the
  upstream-sync workflow does not update it. See the root README and CONTRIBUTING guide.
- `live-schematic-ipc` is upstream master plus this work.  A scheduled workflow
  (`.github/workflows/sync-upstream.yml`) fast-forwards `master` daily and merges it into this
  branch; on conflicts it opens an issue labelled `upstream-sync` and leaves the branch alone.
- Automatic merges are not built or tested (a KiCad build does not fit a hosted runner): after
  pulling, run `scripts/rebuild.sh` and `scripts/run_tests.sh --live` before relying on them.
- Work on a topic branch off `live-schematic-ipc` and merge back.  Prefer merges to rebases:
  other agents share this branch, so never force-push it.

## Where things are

| What | Where |
| --- | --- |
| MCP server (Python; counts/schemas from `live_mcp_capabilities`) | `tools/live_schematic_mcp/server.py` |
| Client: schematics, symbols, libraries | `tools/live_schematic_mcp/client.py` |
| Client: footprints, embedded files, 3D | `tools/live_schematic_mcp/footprint_client.py` |
| Client: PCB editor | `tools/live_schematic_mcp/board_client.py` |
| Client: rules, net classes, stackup, RF, checks, routing | `tools/live_schematic_mcp/board_design.py` |
| Router (shapely grid A*, no KiCad calls) / impedance maths | `board_router.py`, `board_rf.py` |
| Symbol pin layout / footprint generator / SolidCraft plans | `symbol_layout.py`, `footprint_gen.py`, `model_handoff.py` |
| Native Rust MCP (gate 4: discovery, reads, create_items) and its parity harness | `rust/embeint-ipc`, `rust/embeint-mcp`, `rust/parity/mcp_parity.py` ([rust/MCP.md](../../rust/MCP.md)) |
| Generic command registry (execute/batch) | `tools/live_schematic_mcp/commands.py` |
| Protocol additions | `api/proto/schematic/schematic_commands.proto`, `api/proto/board/board_commands.proto`, `api/proto/common/commands/{editor,library}_commands.proto` |
| Schematic editor commands | `eeschema/api/api_handler_sch.cpp` |
| Symbol Editor commands | `eeschema/api/api_handler_symbol.cpp` |
| Footprint Editor commands | `pcbnew/api/api_handler_footprint.cpp` |
| PCB editor commands | `pcbnew/api/api_handler_pcb.cpp`, `pcbnew/api/api_handler_board.cpp` |
| Commands every editor shares (undo/redo, page check, grid, title block) | `common/api/api_handler_editor.cpp` |
| Opening the schematic/board from the project manager | `openDocumentInManager` in `kicad/kicad.cpp` |
| Library tables and search | `common/api/api_handler_libraries.cpp`, `eeschema/api/api_handler_sch_libraries.cpp` |

`git log --oneline master..live-schematic-ipc` lists every change with a full explanation.

## Build, run, test (WSL Ubuntu)

```bash
tools/live_schematic_mcp/scripts/setup_wsl.sh      # once, and after pulls that change C++ (idempotent)
tools/live_schematic_mcp/run_kicad_wsl.sh          # start the dev KiCad with live-demo (background it)
tools/live_schematic_mcp/scripts/wait_ready.sh     # wait for the API
tools/live_schematic_mcp/scripts/run_tests.sh      # unit tests, no KiCad needed
tools/live_schematic_mcp/scripts/run_tests.sh --live   # plus end-to-end tests against the editor
tools/live_schematic_mcp/scripts/rebuild.sh        # after C++/.proto edits (stops eeschema first)
```

Builds happen in `$KICAD_LIVE_ROOT` (default `~/kicad-live`) from an rsync copy of the
repository; building from `/mnt/c` is far slower.  Register the MCP server with
`run_mcp_wsl.sh` (see `README.md`); it regenerates the Python protocol code when a `.proto` changed.

## Build, run, test (native Windows)

```powershell
tools\live_schematic_mcp\scripts\build_windows.ps1 -NoConfigure   # incremental build + install to C:\kb\install
tools\live_schematic_mcp\scripts\setup_windows.ps1                # venv in C:\kb\venv, protocol code (idempotent)
tools\live_schematic_mcp\run_kicad_windows.ps1 -Wait              # isolated config, live-demo, project manager
tools\live_schematic_mcp\scripts\run_tests.ps1 [-Live]            # unit tests [plus end-to-end tests]
tools\live_schematic_mcp\run_kicad_windows.ps1 -Stop              # stop only the KiCad the launcher started
```

The MCP server is `run_mcp_windows.ps1` from the checkout, or the bundled
`C:\kb\install\mcp\embeint-kicad-mcp.exe` (`build_windows.ps1 -McpOnly` rebundles it after an MCP
change; check it with `qualification\check_package.py`).  The endpoint is a local named pipe; only
one KiCad per Windows account can hold it.  Installing over `C:\kb\install` fails while any KiCad
started from it is running (Windows locks the DLLs): never stop a KiCad you did not start; ask its
user.  `-McpOnly` works while KiCad runs, but not while an MCP client runs the bundled server: it
then refuses before touching `install\mcp` (the old package stays complete) and names a file in use.

## Rules that save time

- **Adding a command** touches four places: the `.proto` message, the C++ handler and its
  `registerHandler` line, a client method, and a server tool (plus `commands.py`).  Add a unit
  test in `test_client.py` and an end-to-end check in the matching `e2e_*.py`.
- **Never block the UI thread or open a dialog from a handler.**  An API client cannot answer a
  modal dialog and the editor freezes.  Check up front and return `AS_BUSY`/`AS_BAD_REQUEST`
  instead (see `openSymbolEditor`, `CanOpenFootprintWithoutPrompt`).  Do not make all libraries
  load synchronously (search only covers loaded libraries for that reason).
- **Edits are undoable**: use `getCurrentCommit(aCtx.ClientName)` and push only when the client
  has no open commit, so batches stay one undo step.
- **Nothing is saved implicitly.**  Save tools are explicit; tests check files are unchanged.
- Shell scripts must keep LF line endings (`.gitattributes` enforces it; Windows editors do not).
- After a table edit KiCad drops all loaded libraries; code that loads a symbol must cope
  (`loadLibrarySymbol`).
- **End-to-end tests edit the open schematic.**  `e2e_guard.test_document` limits them to the
  `live-demo` test project (or `KICAD_LIVE_TEST_PROJECT`); never point them at a real design.
- **Check the page, not just the drawing.**  `live_schematic_view` and `live_pcb_view` render the
  whole page with the drawing sheet (border and title block) by default (`frame=False` hides it);
  `live_schematic_page` and `live_pcb_page` report the same as numbers: the drawing area, the title
  block and every item outside the border or in the title block (`fits`).  Content placed only
  against the view without the frame can run into the title block unnoticed.
- **One KiCad process for schematic and PCB.**  The PCB editor only joins the API when KiCad runs
  as the project manager (`run_kicad_wsl.sh project.kicad_pro`, the default with no argument);
  the MCP then opens the editors with `live_schematic_open` / `live_pcb_open`.  Starting
  `eeschema` on a `.kicad_sch` gives the schematic, symbol and footprint tools only.
- **Commands every editor shares go in `API_HANDLER_EDITOR`** (undo/redo, `GetPageLayout`, grid,
  title block, page settings): one implementation, dispatched by document type.  An editor adds
  what is specific to it through a virtual hook (e.g. `getPageLayoutInput`).

## The user's library conventions

The maintained design guidance is the bundled
[kicad-house-style skill](skills/kicad-house-style/SKILL.md). Load its entrypoint
and relevant reference for design tasks; the summary below is for orientation.
`live_skill_reference` and `kicad-skill://` resources serve it offline. Keep local
workstation configuration and private reference designs outside the published skill.
Skill folders contain a flat `SKILL.md` plus linked Markdown references, with
name/description frontmatter and a quoted `metadata.version` (major.minor.patch).
Bump that content version for maintained guidance changes. Per-file and aggregate
content hashes identify exact bytes independently of tool/protobuf schema hashes.
The packager includes only advertised Markdown files and validates their delivery.

- Symbols: hidden `Manufacturer` and `MPN` fields; designator above value at the body's bottom
  right, left-aligned; datasheet-style pin groups (supplies first, 7.62 mm before signals: VIN
  over EN, VOUT over ADJ); NC pins hidden no-connects; stock body styling (device layer, 0.254 mm
  outline, body-colour fill); roomier than stock symbols.
- Footprints: KiCad Library Conventions (pads from the datasheet land pattern, 0.12 mm silk 0.2 mm
  clear of copper, 0.10 mm fab outline with chamfered pin 1 and `${REFERENCE}`, 0.05 mm courtyard
  0.25 mm out on a 0.01 mm grid).
- Datasheets are **linked** in the Datasheet field; STEP models are **embedded** in the footprint
  (`kicad-embed://`).  SolidCraft builds missing models (`live_footprint_model_plan`).
- Schematics: organise every sheet into logical blocks (dashed box + title per function).  Wire
  parts pin-to-pin inside a block (Manhattan, junctions at T-joins, never wires on top of each
  other); join blocks with short net labels instead of long wires.  No stub + label on every pin.
  Cross-sheet signals use hierarchical labels and sheet pins, never global labels; rails and GND
  are power symbols at the part.  Drive enables straight from the controlling MCU signal.  Keep
  each block's notes inside its box and check the rendered view for overlaps before saving.
- Pages: choose A4 or A3 (landscape) to fit the sheet with margin; keep everything inside the
  drawing area and clear of the title block (`live_schematic_page` must report `fits: true`).
  Title block (Embeint style, `live_schematic_set_page`): title `<project> - <sheet name>`, the
  date, the revision, company `Embeint Pty Ltd`, and a one-line description in Comment1.
- Boards: the same page rules (`live_pcb_page` must report `fits: true`) and Embeint title block
  (`live_pcb_set_page`); draw the outline inside the drawing area, clear of the title block.
  Footprints come from the schematic (`live_pcb_update_from_schematic`), not placed by hand.
  Check `live_pcb_drc` and the rendered view before saving.  (Origin and grid conventions for
  boards are not settled yet: ask before choosing them.)
- Board setup before routing: stackup (`live_pcb_set_stackup`), design rules
  (`live_pcb_design_rules`, custom rules in `live_pcb_custom_rules`), net classes with the default
  track width, clearance and via size (`live_pcb_netclasses`), RF/antenna nets (`live_pcb_rf_net`)
  and keepouts (`live_pcb_add_keepout`, e.g. around an antenna).
- Routing: `live_pcb_route` / `live_pcb_route_net` route around other copper, keepouts and the
  edge and are checked by KiCad's DRC (undone if they break a rule).  Never draw tracks that cross
  or crowd other nets; `live_pcb_add_tracks` refuses such a path.  Route RF feeds first, on one
  layer without vias, then critical nets, then the rest; leave pour-connected nets (GND) to zones.
  Finish with `live_pcb_check` (clearance, silkscreen, courtyard, connectivity) clean.
