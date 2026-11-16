# Live schematic MCP

An MCP server that edits the schematic **open in KiCad** through the IPC API: no file edits,
no save/reload. Changes appear in the editor immediately, each tool call is one undo step, and
`live_schematic_view` returns a PNG KiCad renders from the in-memory (unsaved) design.

Requires this development build (10.99 / KiCad 11). Released KiCad 10.x has no schematic IPC.

## Tools

| Tool | Purpose |
| --- | --- |
| `live_schematic_documents` | Open schematic + KiCad version; returns the document spec other tools take |
| `live_schematic_hierarchy` | Live sheet tree, including unsaved sheets, with UUID sheet paths |
| `live_schematic_items` | Items on one sheet (protobuf JSON, nanometers) |
| `live_schematic_create_items` / `_update_items` / `_delete_items` | Batched edits, one undo step, rolled back on any rejection |
| `live_schematic_place_symbol` | Place `Library:Symbol` at mm coordinates |
| `live_schematic_view` | PNG snapshot of a sheet's current unsaved state |
| `live_schematic_focus` | Switch the editor to a sheet and zoom to items |
| `live_schematic_item_schema` | Field reference for item types (SheetSymbol, SheetPin, ...) |
| `live_schematic_save` | Explicit save, only when asked |
| `live_schematic_annotate` | Number references (all / one sheet / listed symbols; by X or Y; first free or sheet x100/x1000) |
| `live_schematic_update_from_library` | Reload symbols from their library, or swap them to another `Library:Symbol` |
| `live_schematic_autoplace_fields` | Re-run field autoplacement on symbols, sheets and labels |
| `live_schematic_move` | Move / rotate / mirror items; attached wires follow and stay square |
| `live_schematic_undo` | Undo or redo the editor's last change |
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

The Footprint Editor lives in pcbnew; the footprint tools ask the schematic editor to open it
(`OpenFootprintEditor`), which loads pcbnew's code into the running process.

Hierarchy: create a `SheetSymbol` (with `pins`) on the parent, read its path from
`live_schematic_hierarchy`, then target the child sheet with that `sheet_path` to add matching
`HierarchicalLabel`s.

KiCad commands this fork adds for the tools above (`api/proto/schematic/schematic_commands.proto`):
`AnnotateSchematic`, `UpdateSymbolsFromLibrary`, `AutoplaceFields`, `TransformItems`,
`UndoLastChange` / `RedoLastChange`, `GetSymbolPins`, `NewLibrarySymbol`, `EditLibrarySymbol`,
`SetLibrarySymbolProperties`, `RenderLibrarySymbol`, `OpenFootprintEditor`, `NewLibraryFootprint`,
`EditLibraryFootprint`, `SetLibraryFootprintProperties`, `RenderLibraryFootprint` and
`EmbedFileFromPath` / `GetEmbeddedFileList` / `RemoveEmbeddedFile`; it also implements the library table and
search commands KiCad declared but left unimplemented (`GetLibraryTable`, `Add`/`Update`/
`DeleteLibraryTableEntry`, `SearchLibraries`). Every editing command joins an open
`BeginCommit`, so a batch is one undo step. Undo/redo need the GUI and refuse an open commit.

## Setup (Windows + WSL Ubuntu)

```bash
# in WSL, once
python3 -m venv ~/kicad-live/venv
~/kicad-live/venv/bin/pip install -r requirements.txt
~/kicad-live/venv/bin/python generate_proto.py

cmake -S <source> -B ~/kicad-live/build -G Ninja -DCMAKE_BUILD_TYPE=Release \
      -DKICAD_BUILD_QA_TESTS=OFF -DCMAKE_INSTALL_PREFIX=$HOME/kicad-live/install
ninja -C ~/kicad-live/build && ninja -C ~/kicad-live/build install

./run_kicad_wsl.sh            # isolated config, IPC server enabled, opens a demo project
```

`run_kicad_wsl.sh` keeps its settings in `~/kicad-live/config` and reuses the symbol and
footprint libraries from the Windows KiCad 10.0 install.

Register the MCP server (it must run inside WSL, where KiCad's Unix socket lives):

```bash
claude mcp add kicad-live -- wsl.exe -d Ubuntu -- bash /mnt/c/<fork>/tools/live_schematic_mcp/run_mcp_wsl.sh
```

## Tests

```bash
~/kicad-live/venv/bin/python -m pytest test_client.py      # protocol contracts, no KiCad needed
~/kicad-live/venv/bin/python e2e_smoke.py out/             # against a running editor; never saves
~/kicad-live/venv/bin/python e2e_features.py out/          # annotate, wiring, move, undo, ERC, batch, export
~/kicad-live/venv/bin/python e2e_symbols.py out/           # search, build, save, register and place a symbol
~/kicad-live/venv/bin/python e2e_footprints.py out/        # generate a footprint, embed its STEP, link it from a symbol
```
