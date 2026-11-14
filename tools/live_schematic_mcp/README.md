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

Hierarchy: create a `SheetSymbol` (with `pins`) on the parent, read its path from
`live_schematic_hierarchy`, then target the child sheet with that `sheet_path` to add matching
`HierarchicalLabel`s.

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
```
