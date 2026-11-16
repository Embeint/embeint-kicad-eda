# Agent guide: live KiCad over IPC (this fork)

This fork (`Karl-VonRichter/kicad-source-mirror`, branch **`live-schematic-ipc`**, the default
branch) extends KiCad's IPC API so an MCP server can drive a running KiCad live: schematics,
symbol libraries and the Symbol Editor, footprint libraries and the Footprint Editor, embedded
files and 3D models.  Nothing here goes upstream; KiCad does not accept AI-generated merge
requests (see the root `AGENTS.md`).

## Branches

- `master` mirrors upstream KiCad (`KiCad/kicad-source-mirror`).  Never commit to it.
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
| MCP server (Python, 75 tools) | `tools/live_schematic_mcp/server.py` |
| Client: schematics, symbols, libraries | `tools/live_schematic_mcp/client.py` |
| Client: footprints, embedded files, 3D | `tools/live_schematic_mcp/footprint_client.py` |
| Symbol pin layout / footprint generator / SolidCraft plans | `symbol_layout.py`, `footprint_gen.py`, `model_handoff.py` |
| Generic command registry (execute/batch) | `tools/live_schematic_mcp/commands.py` |
| Protocol additions | `api/proto/schematic/schematic_commands.proto`, `api/proto/board/board_commands.proto`, `api/proto/common/commands/{editor,library}_commands.proto` |
| Schematic editor commands | `eeschema/api/api_handler_sch.cpp` |
| Symbol Editor commands | `eeschema/api/api_handler_symbol.cpp` |
| Footprint Editor commands | `pcbnew/api/api_handler_footprint.cpp` |
| Library tables and search | `common/api/api_handler_libraries.cpp`, `eeschema/api/api_handler_sch_libraries.cpp` |

`git log --oneline master..live-schematic-ipc` lists every change with a full explanation.

## Build, run, test (WSL Ubuntu)

```bash
tools/live_schematic_mcp/scripts/setup_wsl.sh      # once, and after pulls that change C++ (idempotent)
tools/live_schematic_mcp/run_kicad_wsl.sh          # start the dev eeschema (background it)
tools/live_schematic_mcp/scripts/wait_ready.sh     # wait for the API
tools/live_schematic_mcp/scripts/run_tests.sh      # unit tests, no KiCad needed
tools/live_schematic_mcp/scripts/run_tests.sh --live   # plus end-to-end tests against the editor
tools/live_schematic_mcp/scripts/rebuild.sh        # after C++/.proto edits (stops eeschema first)
```

Builds happen in `$KICAD_LIVE_ROOT` (default `~/kicad-live`) from an rsync copy of the
repository; building from `/mnt/c` is far slower.  Register the MCP server with
`run_mcp_wsl.sh` (see `README.md`); it regenerates the Python protocol code when a `.proto` changed.

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

## The user's library conventions

- Symbols: hidden `Manufacturer` and `MPN` fields; designator above value at the body's bottom
  right, left-aligned; datasheet-style pin groups (supplies first, 7.62 mm before signals: VIN
  over EN, VOUT over ADJ); NC pins hidden no-connects; stock body styling (device layer, 0.254 mm
  outline, body-colour fill); roomier than stock symbols.
- Footprints: KiCad Library Conventions (pads from the datasheet land pattern, 0.12 mm silk 0.2 mm
  clear of copper, 0.10 mm fab outline with chamfered pin 1 and `${REFERENCE}`, 0.05 mm courtyard
  0.25 mm out on a 0.01 mm grid).
- Datasheets are **linked** in the Datasheet field; STEP models are **embedded** in the footprint
  (`kicad-embed://`).  SolidCraft builds missing models (`live_footprint_model_plan`).
