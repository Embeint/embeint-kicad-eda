# SPDX-License-Identifier: GPL-3.0-or-later
"""MCP companion for the schematic IPC API in this KiCad development checkout."""

import json

from google.protobuf import empty_pb2
from mcp.server.fastmcp import FastMCP, Image

from board_client import parse_board_items
from client import LiveSchematic, as_dict, document, editor, parse_items, project
from client import board_document as board_document_spec
from client import footprint_document as footprint_document_spec
from client import symbol_document as symbol_document_spec
from skills import (DEFAULT_SKILL, INDEX_URI, MANIFEST_URI, read_skill, skill_index,
                    skill_manifest)
from support import CAPABILITIES_URI, local_capabilities

INSTRUCTIONS = (
    "These tools drive Embeint KiCad over IPC. For schematic capture/review, part selection, "
    "symbols, footprints or boards, load the bundled Embeint house style before editing: "
    f'live_skill_reference {{"skill":"{DEFAULT_SKILL}"}} or '
    f"kicad-skill://{DEFAULT_SKILL}/SKILL.md. Read the linked reference for the current task. "
    "Skill discovery, file reads, content manifest and local capabilities work offline. "
    "Follow the user's current intent and existing authorization; coordinate shared editor use. "
    "Preserve manual edits, verify mutation outcomes, and save explicitly. "
    "Local schemas and historical version notes do not prove current live editor support."
)

mcp = FastMCP("KiCad Live", instructions=INSTRUCTIONS)
client = LiveSchematic()


@mcp.tool()
def live_skill_reference(skill: str = "", file: str = "SKILL.md") -> dict:
    """Read offline Embeint design guidance; no arguments lists bundled skills/files.

    Start design work with skill="kicad-house-style", then select a linked file:
    schematic-style.md, parts-and-libraries.md, design-review.md, pcb.md,
    live-workflow.md or workstation.md. Returns text, URI and raw-byte SHA-256.
    """
    return read_skill(skill, file)


@mcp.resource(INDEX_URI, name="KiCad skills", mime_type="application/json")
def skill_index_resource() -> str:
    """Offline index of bundled skills, versions and available Markdown files."""
    return json.dumps(skill_index(), sort_keys=True)


@mcp.resource(MANIFEST_URI, name="KiCad skill content manifest", mime_type="application/json")
def skill_manifest_resource() -> str:
    """Skill versions and content hashes, independent of the capability schema hash."""
    return json.dumps(skill_manifest(), sort_keys=True)


@mcp.resource("kicad-skill://{skill}/{file}", name="KiCad skill file", mime_type="text/markdown")
def skill_file_resource(skill: str, file: str) -> str:
    """One allowlisted bundled Markdown file; no editor connection is needed."""
    return read_skill(skill, file)["text"]


@mcp.tool()
async def live_mcp_capabilities() -> dict:
    """Inspect local MCP tools and command schemas without connecting to KiCad.

    This inventory describes the loaded adapter, not negotiated support in a live
    editor. Document availability, command support and undo behavior still depend
    on the connected KiCad version and editor.
    """
    return local_capabilities(await mcp.list_tools())


@mcp.resource(CAPABILITIES_URI, name="Embeint MCP capabilities", mime_type="application/json")
async def local_capability_resource() -> str:
    """Offline tool/command schema inventory, also available as live_mcp_capabilities."""
    return json.dumps(await live_mcp_capabilities(), sort_keys=True)


@mcp.tool()
def live_schematic_documents() -> dict:
    """List open live schematic documents and KiCad version. Requires development KiCad 10.99/11+."""
    return client.documents()


@mcp.tool()
def live_schematic_open(path: str | None = None) -> dict:
    """Open the project's schematic in the running KiCad when it runs as the project manager (path defaults
    to <project>.kicad_sch). Returns the open schematic documents, like live_schematic_documents."""
    return client.open_schematic(path)


@mcp.tool()
def live_schematic_hierarchy(document_spec: dict) -> dict:
    """Read the live hierarchy, including unsaved child sheets. Use UUID sheet paths to target instances."""
    return as_dict(client.hierarchy(document(document_spec)))


@mcp.tool()
def live_schematic_items(document_spec: dict) -> dict:
    """Read unsaved items on one sheet as protobuf JSON. Positions are nanometers (1 mm = 1000000 nm).

    Preserve the returned @type and all fields when updating an item. SheetSymbol includes its pins.
    """
    return as_dict(client.items(document(document_spec)))


@mcp.tool()
def live_schematic_create_items(document_spec: dict, items: list[dict]) -> dict:
    """Create up to 200 items in the live editor as one undo step, without saving.

    Each item is protobuf JSON with @type=type.googleapis.com/kiapi.schematic.types.TYPE.
    Types include SchematicLine, HierarchicalLabel, Junction, SheetSymbol and SchematicText.
    Use live_schematic_item_schema for field definitions. Coordinates are nanometers.
    New sheets need a unique filename_field, name_field, size, position and page_number;
    set visible=true on name_field and filename_field (otherwise the sheet is drawn unlabeled)
    and fields_autoplaced=true so KiCad positions them at the sheet border.
    Pins belong in SheetSymbol.pins, positioned on the sheet edge given by side.
    Use live_schematic_place_symbol for library symbols.
    """
    return client.mutate(document(document_spec), parse_items(items))


@mcp.tool()
def live_schematic_update_items(document_spec: dict, items: list[dict]) -> dict:
    """Replace live items in one undo step without saving. Supply complete items from live_schematic_items.

    Preserve IDs and unrelated fields; omitted fields are not a partial update. To add sheet pins,
    modify the parent SheetSymbol.pins and return its complete item. Hierarchical labels must match.
    """
    return client.mutate(document(document_spec), parse_items(items), update=True)


@mcp.tool()
def live_schematic_delete_items(document_spec: dict, ids: list[str]) -> dict:
    """Delete the specified UUIDs on one live sheet as one undo step, without saving."""
    return client.delete(document(document_spec), ids)


@mcp.tool()
def live_schematic_place_symbol(document_spec: dict, lib_id: str, x_mm: float, y_mm: float,
                                reference: str | None = None, rotation: int = 0) -> dict:
    """Place Library:Symbol directly in an open sheet, using millimeters and 0/90/180/270 degrees.

    Returns the new symbol's id, reference (auto-annotated if omitted), value and position.
    """
    return client.place_symbol(document(document_spec), lib_id, x_mm, y_mm, reference, rotation)


@mcp.tool()
def live_schematic_view(document_spec: dict, dpi: int = 100, frame: bool = True) -> Image:
    """See an open sheet's current unsaved contents rendered by KiCad. Does not save/reload the design.

    Returns the whole page as a PNG. frame=True (default) draws the drawing sheet: border and title
    block, so content running past the border or into the title block shows. frame=False leaves them
    out. Use live_schematic_page for the same check as numbers.
    """
    return Image(data=client.render(document(document_spec), dpi, frame), format="png")


@mcp.tool()
def live_schematic_page(document_spec: dict) -> dict:
    """The sheet's page: paper (A4, A3, ...) and orientation, page size, the drawing area inside the
    border, the title block box and its fields, the extent of the content, all in mm, and every item
    that crosses the border or overlaps the title block (fits=false). Check this before saving.
    """
    return client.page_layout(document(document_spec))


@mcp.tool()
def live_schematic_set_page(document_spec: dict, size: str | None = None, orientation: str | None = None,
                            title: str | None = None, date: str | None = None, revision: str | None = None,
                            company: str | None = None, comments: list[str] | None = None) -> dict:
    """Set the sheet's paper and title block; omitted settings are kept. Not an undo step.

    size: A5, A4, A3, A2, A1, A0, ANSI_A..ANSI_E, LETTER, LEGAL, LEDGER. orientation: landscape |
    portrait. date may be "today". comments fill Comment1..9 in order. Follow the project's
    title-block convention (see AGENTS.md). Returns the new page layout, so check `fits`: a smaller
    page can leave content outside the border.
    """
    return client.set_page(document(document_spec), size, orientation, None, title, date, revision, company,
                           comments)


@mcp.tool()
def live_schematic_focus(document_spec: dict, ids: list[str]) -> dict:
    """Switch the editor to the specified sheet and zoom to its items. Does not save."""
    request = editor.FocusOnItems(document=document(document_spec))
    for value in ids:
        request.items.add(value=value)
    client.send(request, empty_pb2.Empty)
    return {"success": True}


@mcp.tool()
def live_schematic_save(document_spec: dict) -> dict:
    """Explicitly SAVE the live schematic to disk. Use only when the user wants to persist edits."""
    client.send(project.SaveDocument(document=document(document_spec)), empty_pb2.Empty)
    return {"success": True}


@mcp.tool()
def live_schematic_annotate(document_spec: dict, scope: str = "all", symbols: list[str] | None = None,
                            order: str = "x", numbering: str = "first_free", reset_existing: bool = False,
                            recursive: bool = True, start_number: int = 0) -> dict:
    """Annotate reference designators as one undo step, without saving.

    scope: all | sheet (the document's sheet, plus subsheets if recursive) | symbols (UUID list).
    order: x | y | unsorted. numbering: first_free | sheet_x100 | sheet_x1000.
    reset_existing renumbers already-annotated symbols too. Returns old -> new references.
    """
    return client.annotate(document(document_spec), scope, symbols or [], order, numbering,
                           reset_existing, recursive, start_number)


@mcp.tool()
def live_schematic_update_from_library(document_spec: dict, symbols: list[str] | None = None,
                                       new_lib_id: str | None = None, reset_field_text: bool = False,
                                       reset_field_visibility: bool = False,
                                       reset_field_positions: bool = False,
                                       remove_extra_fields: bool = False) -> dict:
    """Reload symbols from their library, or change them to new_lib_id (Library:Symbol), as one undo step.

    symbols are UUIDs; omit to update every symbol on the document's sheet. References are kept.
    reset_field_text resets value/footprint/datasheet/description to the library's (not power values).
    """
    return client.update_from_library(document(document_spec), symbols or [], new_lib_id, reset_field_text,
                                      reset_field_visibility, reset_field_positions, remove_extra_fields)


@mcp.tool()
def live_schematic_autoplace_fields(document_spec: dict, ids: list[str] | None = None) -> dict:
    """Re-run KiCad field autoplacement on symbols, sheets and labels (all on the sheet if ids omitted)."""
    return client.autoplace(document(document_spec), ids or [])


@mcp.tool()
def live_schematic_move(document_spec: dict, ids: list[str], dx_mm: float = 0, dy_mm: float = 0,
                        rotation: int = 0, mirror_horizontally: bool = False, mirror_vertically: bool = False,
                        drag_wires: bool = True) -> dict:
    """Move, rotate (degrees counter-clockwise: 0/90/180/270) and/or mirror items on one sheet in one undo step.

    One item turns about its own position; several turn about their shared centre. With drag_wires, wire
    and bus ends, labels, junctions and no-connects attached to the items' pins follow them; a horizontal or
    vertical wire stays square by gaining a bend. Positive dy is down.
    """
    return client.transform(document(document_spec), ids, dx_mm, dy_mm, rotation, mirror_horizontally,
                            mirror_vertically, drag_wires)


@mcp.tool()
def live_schematic_undo(document_spec: dict, redo: bool = False) -> dict:
    """Undo (or redo) the editor's most recent change, including the user's own edits. Not while a batch is open."""
    return client.undo(document(document_spec), redo)


@mcp.tool()
def live_schematic_symbol_pins(document_spec: dict, symbols: list[str] | None = None) -> list[dict]:
    """List placed pins with absolute connection points (mm) and current net, for symbols on one sheet.

    orientation is the direction from the connection point toward the body; wires leave the other way.
    """
    return client.symbol_pins(document(document_spec), symbols or [])


@mcp.tool()
def live_schematic_connect_pins(document_spec: dict, from_reference: str, from_pin: str,
                                to_reference: str, to_pin: str) -> dict:
    """Wire two pins on one sheet with an orthogonal one- or two-segment wire, as one undo step.

    Pins are given by reference designator and pin number or name (e.g. R1, 2). Check the view afterwards:
    the route does not avoid other symbols.
    """
    return client.connect_pins(document(document_spec), from_reference, from_pin, to_reference, to_pin)


@mcp.tool()
def live_schematic_connect_pin_to_net(document_spec: dict, reference: str, pin: str, net: str,
                                      label: str = "local", stub_mm: float = 2.54) -> dict:
    """Attach a pin to a named net: a short wire leaving the pin plus a local/global/hierarchical label."""
    return client.connect_pin_to_net(document(document_spec), reference, pin, net, label, stub_mm)


@mcp.tool()
def live_schematic_netlist(document_spec: dict) -> dict:
    """Read live connectivity: each net with its pins as REF.PIN, plus item counts. Includes unsaved edits."""
    doc = document(document_spec)
    nets, pin_nets = client.netlist(doc)
    labels = {}
    for symbol in client.symbol_pins(doc, with_nets=False):
        for pin in symbol["pins"]:
            labels[pin["id"]] = f'{symbol["reference"]}.{pin["number"]}'
    return {name: {"pins": sorted({labels[i] for _, i in members if i in labels}), "items": len(members)}
            for name, members in sorted(nets.items())}


@mcp.tool()
def live_schematic_erc(document_spec: dict) -> dict:
    """Run KiCad's electrical rules check on the live (unsaved) schematic and return the violations."""
    return client.erc(document(document_spec))


@mcp.tool()
def live_schematic_select(document_spec: dict, ids: list[str], add: bool = False) -> dict:
    """Select items in the editor so the user can see them (replaces the selection unless add=true)."""
    return client.select(document(document_spec), ids, add)


@mcp.tool()
def live_schematic_highlight_nets(document_spec: dict, nets: list[str]) -> dict:
    """Highlight nets in the editor by name (an empty list clears the highlight)."""
    return client.highlight_nets(document(document_spec), nets)


@mcp.tool()
def live_schematic_export(document_spec: dict, kind: str, output_path: str) -> dict:
    """Export the unsaved schematic as svg, pdf, dxf, ps, netlist or bom to a path KiCad can write."""
    return client.export(document(document_spec), kind, output_path)


@mcp.tool()
def live_schematic_list_commands() -> list[dict]:
    """List every KiCad IPC command these tools know (schematic, PCB, libraries), its kind
    (read/edit/view/file/job) and reply type."""
    from commands import COMMANDS
    return [{"command": name.rsplit(".", 1)[-1], "full_name": name, "kind": kind, "returns": reply}
            for name, (reply, kind) in COMMANDS.items()]


@mcp.tool()
def live_schematic_describe_command(command: str) -> dict:
    """Show the fields of a command from live_schematic_list_commands, for use with execute or batch."""
    from commands import describe
    return describe(command)


@mcp.tool()
def live_schematic_execute(command: str, payload: dict | None = None) -> dict:
    """Run any KiCad IPC command (schematic or PCB) by name with a protobuf-JSON payload.

    Prefer the typed tools. Most commands need a document (or header.document) from
    live_schematic_documents. File-kind commands (SaveDocument, RevertDocument, ...) change or discard
    files: run them only when the user asks. An edit command is not retried if the reply is lost.
    """
    return client.execute(command, payload)


@mcp.tool()
def live_schematic_batch(document_spec: dict, steps: list[dict], description: str = "MCP: batch") -> dict:
    """Run supported item edits as ONE undo step; failed steps roll back those item edits.

    document_spec is a schematic, board (DOCTYPE_PCB), symbol or footprint document.

    steps: [{"command": "CreateItems", "payload": {...}}, ...] using names from live_schematic_list_commands.
    Only edits marked batchable participate in rollback; settings, editor replacement,
    asynchronous work, undo/redo and file/job commands are refused. Omitted document
    targets inherit this batch's document; explicit targets must match it exactly.
    Selection and zoom changes are not rolled back.
    """
    kind = document_spec.get("type")
    if kind == "DOCTYPE_PCB":
        spec = board_document_spec(document_spec)
    elif kind in ("DOCTYPE_SYMBOL", "DOCTYPE_FOOTPRINT"):
        spec = any_editor_document(document_spec)
    else:
        spec = document(document_spec)
    return client.batch(spec, steps, description)


# ---- Symbol libraries and the Symbol Editor ---------------------------------------------------

@mcp.tool()
def live_symbol_search(query: str, limit: int = 25) -> dict:
    """Search every symbol library KiCad has loaded (names, then keywords and descriptions).

    Words must all match; * and ? are wildcards in names. Returns lib_ids with description, keywords,
    pin count and footprint. Use live_symbol_get for pins. If libraries_still_loading is reported,
    those libraries were skipped while KiCad loads them in the background; search again shortly.
    """
    return client.search_symbols(query, limit)


@mcp.tool()
def live_symbol_get(lib_id: str) -> dict:
    """Read one library symbol (Library:Symbol): fields, keywords, footprint filters, units and pins."""
    return client.library_symbol(lib_id)


@mcp.tool()
def live_symbol_libraries(scope: str = "both") -> list[dict]:
    """List symbol libraries in the global and/or project library table (scope: global | project | both)."""
    return client.library_table(scope)


@mcp.tool()
def live_symbol_create_library(nickname: str, path: str, scope: str = "global", description: str = "") -> dict:
    """Create an empty .kicad_sym library at path and add it to the global (all projects) or project table.

    path is as KiCad sees it (a WSL path for the WSL build, a Windows path for native Windows); environment variables like ${KIPRJMOD} work.
    """
    return client.add_library(nickname, path, scope, description, create=True)


@mcp.tool()
def live_symbol_register_library(nickname: str, path: str, scope: str = "global", description: str = "") -> dict:
    """Add an existing symbol library file to the global or project library table."""
    return client.add_library(nickname, path, scope, description, create=False)


@mcp.tool()
def live_symbol_unregister_library(nickname: str, scope: str = "global") -> dict:
    """Remove a library from the global or project table. The library file is NOT deleted."""
    return client.remove_library(nickname, scope)


@mcp.tool()
def live_symbol_new(library: str, name: str, reference_prefix: str = "U", unit_count: int = 1,
                    inherit_from: str = "", power_symbol: bool = False, pin_names_outside: bool = False,
                    exclude_from_bom: bool = False, exclude_from_board: bool = False,
                    replace_existing: bool = False, manufacturer: str = "", mpn: str = "") -> dict:
    """Start a new symbol in a writable library and show it in KiCad's Symbol Editor (unsaved).

    Returns the symbol document the other live_symbol_* tools take. The symbol always gets hidden
    Manufacturer and MPN fields (filled from these arguments when given). inherit_from derives from
    another symbol in the same library. Nothing is written until live_symbol_save.
    """
    return client.new_symbol(library, name, reference_prefix, unit_count, inherit_from, power_symbol,
                             pin_names_outside, False, False, exclude_from_bom, exclude_from_board,
                             replace_existing, manufacturer, mpn)


@mcp.tool()
def live_symbol_edit(lib_id: str) -> dict:
    """Open an existing library symbol in the Symbol Editor and return its symbol document."""
    return client.edit_symbol(lib_id)


@mcp.tool()
def live_symbol_add_pins(symbol_document: dict, pins: list[dict], draw_body: bool = True,
                         replace: bool = False, arrange: str = "auto", group_gap: int = 2) -> dict:
    """Lay out pins around a rectangular body on the 2.54 mm grid, as one undo step, datasheet-style.

    pins: [{"number": "1", "name": "VIN", "type": "power_in"}, ...]. type is one of input, output,
    bidirectional, tri_state, passive, free, unspecified, power_in, power_out, open_collector,
    open_emitter, no_connect. side (left/right/top/bottom) is optional; by default supplies and
    control inputs (EN, SHDN, ...) go left, outputs and feedback (VOUT, ADJ, FB, PG) go right,
    grounds bottom. arrange="auto" lists supplies first on each side, then signals, with group_gap
    empty rows between (2 rows = 7.62 mm, room for a pull-up or feedback resistor): an LDO gets VIN
    then EN on the left, VOUT then ADJ on the right. Follow the datasheet pinout when it differs:
    give sides, and arrange="as_given" with {"gap": true, "side": "left"} rows for exact spacing.
    NC pins (name NC/DNC or type no_connect) become hidden no-connect pins inside the body, as stock
    symbols do, and take no row; pass "hidden": false to show one. Keep our spacing even where a
    stock symbol is more cramped. Optional per pin: "shape" (SPS_INVERTED, SPS_CLOCK, ...), "hidden".
    replace=true first removes existing pins and shapes. The designator sits above the value at the
    body's bottom right, their first letters aligned.
    """
    return client.symbol_add_pins(symbol_document_spec(symbol_document), pins, draw_body, replace, arrange,
                                  group_gap)


@mcp.tool()
def live_symbol_set_properties(symbol_document: dict, value: str | None = None, footprint: str | None = None,
                               datasheet: str | None = None, description: str | None = None,
                               keywords: str | None = None, reference_prefix: str | None = None,
                               footprint_filters: list[str] | None = None,
                               user_fields: list[dict] | None = None,
                               remove_user_fields: list[str] | None = None,
                               show_pin_numbers: bool | None = None, show_pin_names: bool | None = None,
                               exclude_from_bom: bool | None = None, exclude_from_board: bool | None = None,
                               exclude_from_sim: bool | None = None, manufacturer: str | None = None,
                               mpn: str | None = None) -> dict:
    """Fill in the open symbol's details as one undo step; omitted settings are unchanged.

    footprint is Library:Footprint. footprint_filters are globs like "SOT?223*". manufacturer and mpn
    set the hidden Manufacturer and MPN fields. user_fields: [{"name": "Distributor PN", "value": "...",
    "visible": false}] adds or updates any other field.
    """
    return client.symbol_set_properties(
        symbol_document_spec(symbol_document), value, footprint, datasheet, description, keywords,
        reference_prefix, footprint_filters, user_fields, remove_user_fields, show_pin_numbers,
        show_pin_names, exclude_from_bom, exclude_from_board, exclude_from_sim, None, False,
        manufacturer, mpn)


@mcp.tool()
def live_symbol_view(symbol_document: dict, dpi: int = 600) -> Image:
    """See the symbol open in the Symbol Editor, including unsaved changes, as a PNG."""
    return Image(data=client.symbol_render(symbol_document_spec(symbol_document), dpi), format="png")


@mcp.tool()
def live_symbol_summary(symbol_document: dict) -> dict:
    """List the open symbol's pins (number, name, type, position, id) and graphic shape ids."""
    return client.symbol_summary(symbol_document_spec(symbol_document))


@mcp.tool()
def live_symbol_delete_items(symbol_document: dict, ids: list[str]) -> dict:
    """Delete pins or graphics from the open symbol by UUID (ids from live_symbol_summary), as one undo step."""
    return client.symbol_delete(symbol_document_spec(symbol_document), ids)


@mcp.tool()
def live_symbol_save(symbol_document: dict) -> dict:
    """SAVE the open symbol into its library file, then reload the library so schematics can place it."""
    return client.symbol_save(symbol_document_spec(symbol_document))


# ---- Footprint libraries, the Footprint Editor and embedded files -----------------------------

@mcp.tool()
def live_footprint_search(query: str, limit: int = 25) -> dict:
    """Search footprint libraries by name (names encode the package, e.g. SOIC-8_3.9x4.9mm_P1.27mm).

    Words must all match; * and ? are wildcards. Opens the Footprint Editor if needed.
    """
    return client.footprint_search(query, limit)


@mcp.tool()
def live_footprint_get(lib_id: str) -> dict:
    """Read a library footprint (Library:Footprint): pads, description, keywords, mounting, datasheet, models."""
    return client.library_footprint(lib_id)


@mcp.tool()
def live_footprint_libraries(scope: str = "both") -> list[dict]:
    """List footprint libraries in the global and/or project table (scope: global | project | both)."""
    return client.library_table(scope, kind="footprint")


@mcp.tool()
def live_footprint_create_library(nickname: str, path: str, scope: str = "global", description: str = "") -> dict:
    """Create an empty .pretty footprint library at path and add it to the global or project table."""
    return client.add_library(nickname, path, scope, description, create=True, kind="footprint")


@mcp.tool()
def live_footprint_register_library(nickname: str, path: str, scope: str = "global", description: str = "") -> dict:
    """Add an existing .pretty footprint library to the global or project table."""
    return client.add_library(nickname, path, scope, description, create=False, kind="footprint")


@mcp.tool()
def live_footprint_unregister_library(nickname: str, scope: str = "global") -> dict:
    """Remove a footprint library from a table. The library folder is NOT deleted."""
    return client.remove_library(nickname, scope, kind="footprint")


@mcp.tool()
def live_footprint_new(library: str, name: str, mounting: str = "smd", replace_existing: bool = False) -> dict:
    """Start a new footprint in a writable library and show it in KiCad's Footprint Editor (unsaved).

    mounting: smd | tht | unspecified. Returns the footprint document the other live_footprint_* tools
    take. Nothing is written until live_footprint_save.
    """
    return client.new_footprint(library, name, mounting, replace_existing)


@mcp.tool()
def live_footprint_edit(lib_id: str) -> dict:
    """Open an existing library footprint in the Footprint Editor and return its footprint document."""
    return client.edit_footprint(lib_id)


@mcp.tool()
def live_footprint_generate(footprint_document: dict, package: str, params: dict, replace: bool = False) -> dict:
    """Build pads, silkscreen, fabrication, courtyard and text from package dimensions (KLC style).

    Prefer the datasheet's recommended land pattern. package and params (mm):
    - soic/tssop/msop/ssop/gullwing: pins, pitch, body_width (across rows), body_length, then either
      pad_length + pad_width + row_pitch (land pattern) or lead_span + lead_length + lead_width;
      optional exposed_pad [w, h]
    - qfn: pins_x (per top/bottom edge), pins_y (per side), pitch, body_x, body_y, terminal_length,
      terminal_width (or pad_length/pad_width), optional exposed_pad [w, h]; dfn: same without pins_x
    - sot: variant "23", "23-5", "23-6" or "223"
    - chip: size "0402" | "0603" | "0805" | "1206" | "1210" | "2512", prefix "R"/"C"/"L"
    - pin_header: pins_per_row, rows (1 or 2), pitch 2.54, drill 1.0, pad 1.7
    Pin 1 is top-left, numbering counter-clockwise. replace=true clears existing items first.
    """
    return client.footprint_generate(footprint_document_spec(footprint_document), package, params, replace)


@mcp.tool()
def live_footprint_set_properties(footprint_document: dict, description: str | None = None,
                                  keywords: str | None = None, mounting: str | None = None,
                                  value: str | None = None, datasheet: str | None = None,
                                  exclude_from_bom: bool | None = None,
                                  exclude_from_position_files: bool | None = None) -> dict:
    """Fill in the open footprint's details as one undo step. datasheet is a link (URL), not embedded."""
    return client.footprint_set_properties(footprint_document_spec(footprint_document), description, keywords,
                                           mounting, value, datasheet, exclude_from_bom,
                                           exclude_from_position_files)


@mcp.tool()
def live_footprint_attach_step(footprint_document: dict, step_path: str, offset_mm: list[float] | None = None,
                               rotation_deg: list[float] | None = None) -> dict:
    """EMBED a STEP/WRL model in the footprint and make it the 3D model (kicad-embed:// link).

    step_path must be absolute and readable by KiCad (Windows and WSL /mnt/x paths are both accepted and
    translated for the KiCad in use). The model travels
    inside the footprint, so the library needs no separate 3D folder.
    """
    return client.footprint_attach_step(footprint_document_spec(footprint_document), step_path,
                                        tuple(offset_mm or (0, 0, 0)), tuple(rotation_deg or (0, 0, 0)))


@mcp.tool()
def live_embedded_files(document_spec: dict) -> list[dict]:
    """List files embedded in an open footprint or library symbol (name, type, kicad-embed:// uri, size)."""
    return client.embedded_files(any_editor_document(document_spec))


@mcp.tool()
def live_embed_file(document_spec: dict, path: str, overwrite: bool = False) -> dict:
    """Embed any file KiCad can read into an open footprint or library symbol; returns its kicad-embed:// uri.

    Datasheets are normally linked in the Datasheet field instead of embedded.
    """
    return client.embed_file(any_editor_document(document_spec), path, overwrite)


@mcp.tool()
def live_remove_embedded_file(document_spec: dict, name: str) -> dict:
    """Remove an embedded file from an open footprint or library symbol."""
    return client.remove_embedded_file(any_editor_document(document_spec), name)


@mcp.tool()
def live_footprint_view(footprint_document: dict, dpi: int = 1500) -> Image:
    """See the open footprint (copper, paste, fab, courtyard, silkscreen), including unsaved changes."""
    return Image(data=client.footprint_render(footprint_document_spec(footprint_document), dpi), format="png")


@mcp.tool()
def live_footprint_summary(footprint_document: dict) -> dict:
    """List the open footprint's pads, graphics per layer and texts."""
    return client.footprint_summary(footprint_document_spec(footprint_document))


@mcp.tool()
def live_footprint_save(footprint_document: dict) -> dict:
    """SAVE the open footprint into its library and reload the library so symbols and boards can use it."""
    return client.footprint_save(footprint_document_spec(footprint_document))


@mcp.tool()
def live_footprint_model_plan(package: str, params: dict, body_height: float, standoff: float = 0.1,
                              lead_thickness: float = 0.2, lead_width: float | None = None,
                              lead_span: float | None = None, foot_length: float | None = None,
                              export_path: str | None = None) -> dict:
    """Plan a STEP model for the package in SolidCraft, from the datasheet, to hand off and come back.

    package/params are those given to live_footprint_generate. From the datasheet's package drawing:
    body_height (A, seated height), standoff (A1), lead_thickness (c), lead_width (b), lead_span (E,
    tip to tip), foot_length (L). Returns:
      solidcraft: a `batch` call for the SolidCraft MCP (body, leads, pin-1 dimple, colours, built in
                  KiCad 3D conventions: mm, origin on the footprint origin, +Z up, model +Y = footprint -Y)
      export:     the SolidCraft `export` call writing the STEP (a Windows path SolidCraft may write;
                  start SolidCraft's MCP with --allow-path for that folder)
      then:       live_footprint_attach_step with the same file as KiCad sees it
    Check the SolidCraft model (its screenshot/measure tools) before exporting, then attach, save,
    and confirm with live_footprint_view_3d and live_footprint_check_model.
    """
    return client.footprint_model_plan(package, params, body_height, standoff, lead_thickness, lead_width,
                                       lead_span, foot_length, export_path)


@mcp.tool()
def live_footprint_view_3d(lib_id: str, rotate: str = "-45,0,45", zoom: float = 1.0) -> Image:
    """KiCad's own 3D render (kicad-cli) of a SAVED footprint and its models on a scratch board.

    rotate is 'X,Y,Z' degrees ('0,0,0' looks straight down). Use after live_footprint_save.
    """
    return Image(data=client.footprint_view_3d(lib_id, rotate, zoom), format="png")


@mcp.tool()
def live_footprint_check_model(lib_id: str, source_path: str | None = None) -> dict:
    """Check a SAVED footprint's 3D model: each embedded file decoded (size, sha256, is it STEP, is it the
    footprint's model, does it match source_path byte for byte) and the solid count KiCad exports when it
    loads the footprint in a separate process (0 means the model did not load)."""
    return client.footprint_check_model(lib_id, source_path)


# ---- PCB editor (live board, unsaved until live_pcb_save) ---------------------------------------

def board(spec):
    return board_document_spec(spec)


@mcp.tool()
def live_pcb_open(path: str | None = None) -> dict:
    """Open the project's board in the PCB editor of the running KiCad (created if it does not exist yet)
    and return its board document for the other live_pcb_* tools.

    path defaults to <project>.kicad_pcb. Needs KiCad started as the project manager with the project
    (run_kicad_wsl.sh project.kicad_pro), so schematic and PCB editors share one live session.
    """
    return client.open_board(path)


@mcp.tool()
def live_pcb_documents() -> list[dict]:
    """List boards open in the PCB editor (board documents for the live_pcb_* tools)."""
    return client.boards()


@mcp.tool()
def live_pcb_summary(board_document: dict) -> dict:
    """Overview of the live board: outline size, copper layer count, item counts, nets, unsaved state, and
    every footprint (reference, value, lib_id, position mm, rotation, side)."""
    return client.board_summary(board(board_document))


@mcp.tool()
def live_pcb_footprints(board_document: dict, references: list[str] | None = None,
                        pads: bool = False) -> list[dict]:
    """Footprints on the board (all, or the given references); pads=true adds each pad's number, net,
    absolute position, size, shape and drill (mm)."""
    return client.board_footprints(board(board_document), references, pads)


@mcp.tool()
def live_pcb_nets(board_document: dict) -> list[dict]:
    """Nets on the board with their net class."""
    return client.board_nets(board(board_document))


@mcp.tool()
def live_pcb_items(board_document: dict, kinds: list[str]) -> list[dict]:
    """Board items as protobuf JSON (positions in nanometres) for live_pcb_update_items.

    kinds: footprints, pads, tracks, arcs, vias, zones, shapes, texts, textboxes, dimensions, groups,
    tables, images. Footprints include their full definition; prefer live_pcb_footprints to read them.
    """
    from client import as_dict
    return [as_dict(item) for item in client.board_items(board(board_document), kinds)]


@mcp.tool()
def live_pcb_create_items(board_document: dict, items: list[dict]) -> dict:
    """Create up to 200 board items (protobuf JSON, @type kiapi.board.types.*, nanometres) as one undo step.

    Prefer the typed tools (tracks, vias, zones, outline, graphics). Use live_pcb_item_schema for fields.
    """
    return client.mutate(board(board_document), parse_board_items(items), description="MCP: create board items")


@mcp.tool()
def live_pcb_update_items(board_document: dict, items: list[dict]) -> dict:
    """Replace board items with complete items from live_pcb_items (same ids), as one undo step."""
    return client.mutate(board(board_document), parse_board_items(items), update=True,
                         description="MCP: update board items")


@mcp.tool()
def live_pcb_delete_items(board_document: dict, ids: list[str]) -> dict:
    """Delete board items by UUID as one undo step."""
    return client.delete(board(board_document), ids)


@mcp.tool()
def live_pcb_delete_routing(board_document: dict, net: str | None = None, layer: str | None = None,
                            kinds: list[str] | None = None) -> dict:
    """Rip up tracks, arcs and vias (and zones if listed in kinds) by net and/or layer, one undo step per
    200 items. kinds defaults to tracks, arcs and vias. With neither net nor layer, removes ALL of them."""
    return client.board_delete_where(board(board_document), kinds or ["tracks", "arcs", "vias"], net, layer)


@mcp.tool()
def live_pcb_item_schema(type_name: str) -> dict:
    """Describe a board protobuf type and its fields (Track, Via, Arc, Zone, BoardGraphicShape, BoardText,
    FootprintInstance, Pad, ...), including enum choices."""
    from google.protobuf import descriptor_pool
    if not type_name.startswith("kiapi."):
        type_name = "kiapi.board.types." + type_name
    if not type_name.startswith(("kiapi.board.", "kiapi.common.types.")):
        raise ValueError("Only KiCad board/common types are available")
    descriptor = descriptor_pool.Default().FindMessageTypeByName(type_name)
    return {"type": descriptor.full_name, "fields": [
        {"name": field.name, "repeated": field.is_repeated,
         "message": field.message_type.full_name if field.message_type else None,
         "enum": [v.name for v in field.enum_type.values] if field.enum_type else None,
         "scalar_type": field.type}
        for field in descriptor.fields
    ]}


@mcp.tool()
def live_pcb_update_from_schematic(board_document: dict, schematic_document: dict, dry_run: bool = False,
                                   delete_extra_footprints: bool = False, update_footprints: bool = True,
                                   match: str = "uuid") -> dict:
    """Update PCB from Schematic: apply the open schematic's live netlist (unsaved edits included) to the
    board: adds new footprints, updates footprints, values and nets. dry_run reports without changing.

    match: uuid (normal) | reference. New footprints arrive stacked; place them with live_pcb_move.
    """
    return client.board_update_from_schematic(board(board_document), document(schematic_document), dry_run,
                                              delete_extra_footprints, update_footprints, match)


@mcp.tool()
def live_pcb_place_footprint(board_document: dict, lib_id: str, x_mm: float, y_mm: float, rotation: float = 0,
                             side: str = "front", reference: str | None = None, value: str | None = None,
                             locked: bool = False) -> dict:
    """Place a library footprint (Library:Footprint) on the board, one undo step. Normally footprints come
    from the schematic (live_pcb_update_from_schematic); use this for mounting holes, fiducials, logos."""
    return client.board_place_footprint(board(board_document), lib_id, x_mm, y_mm, rotation, side, reference,
                                        value, locked)


@mcp.tool()
def live_pcb_move(board_document: dict, moves: list[dict]) -> dict:
    """Move, rotate, flip and lock footprints by reference, all as one undo step.

    moves: [{"reference": "U1", "x_mm": 20, "y_mm": 15, "rotation": 90, "side": "back", "locked": true}]
    Positions are the footprint origin, absolute in mm (Y down), rotation in degrees counter-clockwise.
    "relative": true makes x_mm/y_mm/rotation offsets. Omitted values are kept.
    """
    return client.board_move_footprints(board(board_document), moves)


@mcp.tool()
def live_pcb_add_tracks(board_document: dict, points: list[list[float]], layer: str = "F.Cu",
                        width_mm: float | None = None, net: str | None = None, check: bool = True) -> dict:
    """Draw a chain of track segments through points [[x, y], ...] (mm) on one copper layer, one undo step.

    width_mm defaults to the net class track width. Give the net so DRC and zones see the connection.
    check (default) first tests the path against other nets' copper (with their clearances), keepouts
    and the board edge, and refuses a path that would overlap or crowd them; prefer live_pcb_route.
    """
    spec = board(board_document)
    if check:
        width = client._width_for(spec, net, width_mm)
        found = client.board_clear_path(spec, net or "", points, layer, width)
        if not found["clear"]:
            raise ValueError(f"the path conflicts with other copper: {found['conflicts'][:5]} "
                             "(use live_pcb_route to find a clear way, or check=false to draw it anyway)")
    return client.board_add_tracks(spec, points, layer, width_mm, net)


@mcp.tool()
def live_pcb_route(board_document: dict, from_reference: str, from_pad: str, to_reference: str, to_pad: str,
                   layers: list[str] | None = None, width_mm: float | None = None, vias: bool = True,
                   grid_mm: float = 0.1, verify: bool = True, avoid: bool = True) -> dict:
    """Route a track between two pads (e.g. U1 pad 3 to R2 pad 1), as one undo step, without overlapping
    anything: it goes around other nets' pads, tracks, vias and copper shapes (keeping each net class
    clearance and the board minimum), keepout areas (including those inside footprints, e.g. antenna
    keepouts) and the board edge clearance, with 45-degree corners.  vias=true lets it change layer
    (net class via size); layers limits the layers it may use.  Width defaults to the net class.
    verify (default) checks clearance, keepout and manufacturing DRC findings for the new copper.
    Rejected route UUIDs are removed in a separate undoable cleanup, preserving unrelated user edits.
    grid_mm is the search step (finer for dense parts).  avoid=false draws a plain dog-leg instead.
    """
    spec = board(board_document)
    if not avoid:
        return client.board_route(spec, from_reference, from_pad, to_reference, to_pad,
                                  layers[0] if layers else None, width_mm, avoid=False)
    return client.board_route_avoiding(spec, from_reference, from_pad, to_reference, to_pad, layers, width_mm,
                                       vias, grid=grid_mm, verify=verify)


@mcp.tool()
def live_pcb_route_net(board_document: dict, net: str, layers: list[str] | None = None,
                       width_mm: float | None = None, vias: bool = True, grid_mm: float = 0.1,
                       verify: bool = True) -> dict:
    """Route a whole net: join every group of its pads that is not yet connected (shortest joins first),
    each leg around other copper, keepouts and the board edge like live_pcb_route, all as one undo step,
    then check clearance, keepout and manufacturing DRC findings. Rejected route UUIDs are removed
    in a separate undoable cleanup, preserving unrelated user edits. Route critical nets (RF, power, clocks)
    first, then the rest; leave nets that a pour connects (GND) to the zone."""
    return client.board_route_net(board(board_document), net, layers, width_mm, vias, grid=grid_mm, verify=verify)


@mcp.tool()
def live_pcb_check_path(board_document: dict, points: list[list[float]], layer: str = "F.Cu",
                        width_mm: float = 0.25, net: str = "") -> dict:
    """Would a track along points [[x, y], ...] (mm) on this layer keep clear of other nets' copper (with
    clearances), keepouts and the board edge? Lists each conflict and the gap it leaves."""
    return client.board_clear_path(board(board_document), net, points, layer, width_mm)


@mcp.tool()
def live_pcb_add_via(board_document: dict, x_mm: float, y_mm: float, net: str | None = None,
                     size_mm: float | None = None, drill_mm: float | None = None) -> dict:
    """Add a through via (size and drill default to the net class), one undo step."""
    return client.board_add_via(board(board_document), x_mm, y_mm, net, size_mm, drill_mm)


@mcp.tool()
def live_pcb_outline(board_document: dict, width_mm: float = 0, height_mm: float = 0, x_mm: float = 0,
                     y_mm: float = 0, corner_radius_mm: float = 0, points: list[list[float]] | None = None,
                     replace: bool = True) -> dict:
    """Draw the board outline on Edge.Cuts: a rectangle from (x, y) of width x height with optional
    rounded corners, or a polygon through points. replace=true removes the old outline. One undo step.

    Place the board inside the page's drawing area, clear of the title block (see live_pcb_page).
    """
    return client.board_outline(board(board_document), width_mm, height_mm, x_mm, y_mm, corner_radius_mm,
                                replace=replace, points=points)


@mcp.tool()
def live_pcb_add_zone(board_document: dict, net: str, layers: list[str] | None = None,
                      points: list[list[float]] | None = None, name: str = "", priority: int = 0,
                      clearance_mm: float = 0.2, min_width_mm: float = 0.25, connection: str = "thermal",
                      inset_mm: float = 0.0, fill: bool = True) -> dict:
    """Add a copper pour for a net (e.g. GND on F.Cu and B.Cu), following the board outline unless points
    are given, then fill zones. connection: thermal | solid | pth_thermal | none. One undo step."""
    return client.board_add_zone(board(board_document), net, layers or ["F.Cu"], points, name, priority,
                                 clearance_mm, min_width_mm, connection, inset_mm, fill)


@mcp.tool()
def live_pcb_add_keepout(board_document: dict, points: list[list[float]] | None = None,
                         around_reference: str | None = None, margin_mm: float = 1.0,
                         layers: list[str] | None = None, name: str = "", tracks: bool = True, vias: bool = True,
                         pads: bool = False, copper: bool = True, footprints: bool = False) -> dict:
    """Add a keepout (rule area), one undo step: through points (mm), or around a footprint
    (around_reference, its courtyard plus margin_mm), e.g. the clear area under and around a chip
    antenna or module antenna.  layers default to every copper layer.  tracks/vias/pads/copper (pours)/
    footprints choose what is kept out.  live_pcb_route and live_pcb_route_net stay out of it."""
    spec = board(board_document)
    if around_reference:
        return client.board_footprint_keepout(spec, around_reference, margin_mm, layers, tracks, vias, pads, copper,
                                              footprints)
    if not points:
        raise ValueError("give points or around_reference")
    names = layers or client.board_copper_layers(spec)
    return client.board_add_keepout(spec, points, names, name, tracks, vias, pads, copper, footprints)


@mcp.tool()
def live_pcb_add_graphics(board_document: dict, shapes: list[dict]) -> dict:
    """Add lines, arcs, rectangles, circles, polygons and text on any layer (mm), one undo step.

    shapes: [{"kind": "line", "start": [x, y], "end": [x, y], "layer": "F.SilkS", "width": 0.12},
             {"kind": "arc", "start", "mid", "end"}, {"kind": "rect", "box": [x1, y1, x2, y2], "filled"},
             {"kind": "circle", "center", "radius"}, {"kind": "polygon", "points": [[x, y], ...]},
             {"kind": "text", "text": "REV A", "position": [x, y], "size": 1.0, "rotation": 0}]
    Text on B.* layers is mirrored automatically.
    """
    return client.board_add_graphics(board(board_document), shapes)


@mcp.tool()
def live_pcb_refill_zones(board_document: dict) -> dict:
    """Refill every zone on the board (after moving footprints or tracks)."""
    return client.board_refill(board(board_document))


@mcp.tool()
def live_pcb_drc(board_document: dict, schematic_parity: bool = True) -> dict:
    """Run KiCad's design rules check on the live board: violations by type (clearance, unconnected items,
    courtyard overlap, silk, ...) with positions in mm. schematic_parity also checks against the schematic."""
    return client.board_drc(board(board_document), parity=schematic_parity)


@mcp.tool()
def live_pcb_undo(board_document: dict, redo: bool = False) -> dict:
    """Undo (or redo) the PCB editor's most recent change, including the user's own edits."""
    return client.undo(board(board_document), redo)


@mcp.tool()
def live_pcb_save(board_document: dict) -> dict:
    """SAVE the board to disk. Use only when the user wants to keep the edits."""
    return client.board_save(board(board_document))


@mcp.tool()
def live_pcb_view(board_document: dict, layers: list[str] | None = None, dpi: int = 100,
                  frame: bool = True) -> Image:
    """See the live board (unsaved edits included) rendered by KiCad on its page.

    layers default to F.Cu, B.Cu, F.SilkS, B.SilkS, Edge.Cuts, F.Fab; pass e.g. ["F.Cu", "Edge.Cuts"] to
    look at one side. frame=true (default) draws the drawing sheet: border and title block, so a board
    or text past the border or in the title block shows. Use live_pcb_page for the same check as numbers.
    """
    return Image(data=client.board_render(board(board_document), layers, dpi, frame), format="png")


@mcp.tool()
def live_pcb_view_3d(board_document: dict, side: str = "top", zoom: float = 1.0, quality: str = "basic",
                     width: int = 1200, height: int = 900) -> Image:
    """KiCad's 3D render of the live board with its component models. side: top | bottom | front | back |
    left | right. quality: basic (fast) | high (ray traced, slow)."""
    return Image(data=client.board_render_3d(board(board_document), side, width, height, zoom, quality),
                 format="png")


@mcp.tool()
def live_pcb_page(board_document: dict) -> dict:
    """The board's page: paper and orientation, the drawing area inside the border, the title block box and
    its fields, the board outline and the extent of everything, all in mm, and every item that crosses
    the border or overlaps the title block (fits=false). Check this before saving."""
    return client.page_layout(board(board_document))


@mcp.tool()
def live_pcb_set_page(board_document: dict, size: str | None = None, orientation: str | None = None,
                      title: str | None = None, date: str | None = None, revision: str | None = None,
                      company: str | None = None, comments: list[str] | None = None) -> dict:
    """Set the board's paper and title block (the drawing block at the bottom right); omitted settings are
    kept. size: A5..A0, ANSI_A..ANSI_E, LETTER, LEGAL, LEDGER. orientation: landscape | portrait.
    date may be "today". comments fill Comment1..9. Follow the title-block convention in AGENTS.md.
    Returns the page layout, so check `fits`."""
    return client.set_page(board(board_document), size, orientation, None, title, date, revision, company,
                           comments)


@mcp.tool()
def live_pcb_origins(board_document: dict, grid_origin_mm: list[float] | None = None,
                     drill_origin_mm: list[float] | None = None) -> dict:
    """Read, and optionally set, the board's two origins (mm): the grid origin (where the grid starts and
    user coordinates count from) and the drill/place origin (used by drill and position files)."""
    return client.board_origins(board(board_document), grid_origin_mm, drill_origin_mm)


@mcp.tool()
def live_editor_grid(document_spec: dict, size_mm: float | list[float] | None = None,
                     preset_index: int | None = None, visible: bool | None = None, style: str | None = None,
                     snapping: str | None = None) -> dict:
    """Read, and optionally change, an editor's drawing grid: board, schematic, symbol or footprint document.

    size_mm (one value, or [x, y]) picks the grid preset of that size, adding one if there is none, e.g.
    0.5 or 0.1 on boards, 1.27 (50 mil) on schematics. style: dots | lines | small_cross. snapping:
    always | with_grid | never. Returns the grid, its presets and (boards) the grid origin.
    """
    kind = document_spec.get("type")
    if kind == "DOCTYPE_PCB":
        spec = board(document_spec)
    elif kind in ("DOCTYPE_SYMBOL", "DOCTYPE_FOOTPRINT"):
        spec = any_editor_document(document_spec)
    else:
        spec = document(document_spec)
    return client.editor_grid(spec, size_mm, preset_index, visible, style, snapping)


@mcp.tool()
def live_pcb_layers(board_document: dict, copper_layers: int | None = None, visible: list[str] | None = None,
                    active: str | None = None) -> dict:
    """Read, and optionally set, the copper layer count (2, 4, 6, ...), the layers shown in the editor and the
    active layer. Returns enabled, visible and active layers."""
    return client.board_layers(board(board_document), copper_layers, None, visible, active)


@mcp.tool()
def live_pcb_design_rules(board_document: dict, min_clearance: float | None = None,
                          min_track_width: float | None = None, min_via_diameter: float | None = None,
                          min_drill: float | None = None, min_annular_ring: float | None = None,
                          copper_edge_clearance: float | None = None, hole_clearance: float | None = None,
                          hole_to_hole: float | None = None, silk_clearance: float | None = None,
                          min_silk_text_height: float | None = None, min_silk_text_thickness: float | None = None,
                          min_connection_width: float | None = None, min_microvia_diameter: float | None = None,
                          min_microvia_drill: float | None = None, track_widths_mm: list[float] | None = None,
                          via_sizes_mm: list[list[float]] | None = None,
                          severities: dict | None = None) -> dict:
    """Read, and optionally set, the board's built-in design rules (Board Setup > Constraints), in mm:
    minimum clearance, track width, via diameter, drill, annular ring, copper-to-edge, hole clearance,
    hole-to-hole, silkscreen clearance and text size.  track_widths_mm and via_sizes_mm ([[diameter,
    drill], ...]) are the predefined sizes offered while routing.  severities sets DRC checks to error,
    warning or ignore, e.g. {"silk_clearance": "error", "silk_mask_clearance": "error"}.  Omitted
    values are kept.  Defaults per net come from live_pcb_netclasses; special cases from
    live_pcb_custom_rules."""
    return client.board_rules(board(board_document), track_widths_mm, via_sizes_mm, severities,
                              min_clearance=min_clearance, min_track_width=min_track_width,
                              min_via_diameter=min_via_diameter, min_drill=min_drill,
                              min_annular_ring=min_annular_ring, copper_edge_clearance=copper_edge_clearance,
                              hole_clearance=hole_clearance, hole_to_hole=hole_to_hole,
                              silk_clearance=silk_clearance, min_silk_text_height=min_silk_text_height,
                              min_silk_text_thickness=min_silk_text_thickness,
                              min_connection_width=min_connection_width,
                              min_microvia_diameter=min_microvia_diameter, min_microvia_drill=min_microvia_drill)


@mcp.tool()
def live_pcb_custom_rules(board_document: dict, action: str = "get", rules: list[dict] | None = None,
                          names: list[str] | None = None) -> dict:
    """Custom design rules (Board Setup > Custom Rules, the .kicad_dru file).  action: get | add (replaces
    rules of the same name) | remove (names) | set (replaces all).  Each rule:
      {"name": "RF clearance", "condition": "A.NetClass == 'RF'",
       "constraints": [{"type": "clearance", "min": 0.5}], "layer": "outer", "severity": "error"}
    Constraint types: clearance, track_width, via_diameter, hole_size, hole_clearance, edge_clearance,
    silk_clearance, courtyard_clearance, annular_width, length, skew, diff_pair_gap, via_count,
    connection_width, physical_clearance, ... with min/opt/max in mm; disallow with items (through_vias,
    micro_vias, blind_vias, buried_vias, tracks, pads, zones, texts, graphics, holes, footprints);
    assertion with expression.  Conditions use KiCad's rule language (A.NetName, A.NetClass,
    A.insideArea('name'), A.Reference, ...).  KiCad checks the rules and reports errors."""
    return client.board_custom_rules(board(board_document), action, rules, names)


@mcp.tool()
def live_pcb_netclasses(board_document: dict, classes: list[dict] | None = None, assign: dict | None = None,
                        patterns: dict | None = None) -> dict:
    """Net classes: the default track width, clearance (spacing) and via size per group of nets.
    classes: [{"name": "Default", "track_width": 0.2, "clearance": 0.15, "via_diameter": 0.6,
    "via_drill": 0.3}, {"name": "Power", "track_width": 0.5}] in mm (created if new; "Default" is the
    board default).  assign {net: class} and patterns {"/USB_D*": "USB90"} put nets in classes.  The
    routing tools use these widths, clearances and via sizes.  Returns every class and assignment."""
    return client.board_netclasses(board(board_document), classes, assign, patterns)


@mcp.tool()
def live_pcb_stackup(board_document: dict) -> dict:
    """The board's physical stackup top to bottom (mm): copper, dielectrics (core/prepreg, epsilon_r, loss
    tangent, material), soldermask and silkscreen, board thickness, finish, impedance control."""
    return client.board_stackup_layers(board(board_document))


@mcp.tool()
def live_pcb_set_stackup(board_document: dict, copper_layers: int | None = None,
                         copper_thickness_mm: float | dict | None = None, dielectrics: list[dict] | None = None,
                         board_thickness_mm: float | None = None, soldermask_color: str | None = None,
                         silkscreen_color: str | None = None, finish: str | None = None,
                         impedance_controlled: bool | None = None) -> dict:
    """Change the stackup (Board Setup > Physical Stackup).  copper_layers: 2, 4, 6, ... (refused if a
    removed layer still has items).  copper_thickness_mm: one value (0.035 = 1 oz) or {"F.Cu": 0.035}.
    dielectrics top to bottom, one fewer than the copper layers: [{"type": "prepreg", "thickness": 0.2,
    "epsilon_r": 4.4, "loss_tangent": 0.02, "material": "FR4"}, {"type": "core", ...}, ...].
    board_thickness_mm scales the dielectrics to reach it.  Colours as "#RRGGBB" (soldermask, silk).
    finish e.g. "ENIG", "HAL lead-free".  Not an undo step; check with live_pcb_stackup."""
    return client.board_set_stackup(board(board_document), copper_layers, copper_thickness_mm, dielectrics,
                                    board_thickness_mm, soldermask_color, silkscreen_color, finish,
                                    impedance_controlled)


@mcp.tool()
def live_pcb_impedance_width(board_document: dict, impedance_ohm: float = 50.0, layer: str = "F.Cu",
                             structure: str = "microstrip", gap_mm: float | None = None,
                             reference_layer: str | None = None) -> dict:
    """Track width for a controlled impedance (e.g. 50 ohm antenna feed, 90 ohm USB pair half) on this
    board's stackup: microstrip (outer layer over the next copper plane) or cpwg (grounded coplanar
    waveguide, with gap_mm to the side ground pour).  Reports the dielectric height and epsilon_r used."""
    return client.board_impedance_width(board(board_document), impedance_ohm, layer, structure, gap_mm,
                                        reference_layer)


@mcp.tool()
def live_pcb_rf_net(board_document: dict, nets: list[str], netclass: str = "RF", impedance_ohm: float = 50.0,
                    width_mm: float | None = None, structure: str = "microstrip", gap_mm: float | None = None,
                    clearance_mm: float | None = None, layer: str = "F.Cu",
                    keep_others_away_mm: float | None = None) -> dict:
    """Set up RF / antenna nets for routing: a net class with the impedance-controlled width (computed from
    the stackup unless width_mm is given) and clearance (the CPWG gap, else 3x the width), assigned to
    the nets, and optionally a custom rule keeping every other net keep_others_away_mm away.  Then add
    a keepout around the antenna (live_pcb_add_keepout around_reference), route the feed first
    (live_pcb_route or live_pcb_route_net, layers=[the RF layer], vias=false), and pour ground."""
    return client.board_rf_net(board(board_document), nets, netclass, impedance_ohm, width_mm, structure, gap_mm,
                               clearance_mm, layer, keep_others_away=keep_others_away_mm)


@mcp.tool()
def live_pcb_check(board_document: dict, categories: list[str] | None = None, schematic_parity: bool = False) -> dict:
    """Design rules check grouped by what it is about, with positions (mm) and the items involved:
    clearance (overlapping or too-close copper, shorts, crossing tracks, holes, board edge), keepout,
    silkscreen (silk over pads or mask openings, silk overlapping silk, silk off the board edge, text
    too small, mirrored text), courtyard (overlapping footprints, missing courtyards), connectivity
    (unrouted, dangling, isolated copper), manufacturing (track width, annular ring, drill, via size,
    soldermask bridges) and schematic (with schematic_parity).  categories limits the report."""
    return client.board_check(board(board_document), categories, schematic_parity)


@mcp.tool()
def live_pcb_select(board_document: dict, references: list[str] | None = None, ids: list[str] | None = None,
                    add: bool = False) -> dict:
    """Select footprints (by reference) or items (by UUID) in the PCB editor so the user sees them."""
    return client.board_select(board(board_document), ids, references, add)


@mcp.tool()
def live_pcb_focus(board_document: dict, references: list[str] | None = None,
                   ids: list[str] | None = None) -> dict:
    """Zoom the PCB editor to footprints (by reference) or items (by UUID)."""
    return client.board_focus(board(board_document), ids, references)


@mcp.tool()
def live_pcb_export(board_document: dict, kind: str, output_path: str) -> dict:
    """Export from the live board (unsaved edits included) to a folder or file KiCad can write.

    kind: gerbers | drill | position | step | pdf | svg | dxf | ipc2581 | ipcd356 | odb | stats | dsn |
    gencad | idf. Gerbers and drill take an output folder.
    """
    return client.board_export(board(board_document), kind, output_path)


def any_editor_document(spec):
    if spec.get("type") == "DOCTYPE_FOOTPRINT":
        return footprint_document_spec(spec)
    return symbol_document_spec(spec)


@mcp.tool()
def live_schematic_item_schema(type_name: str) -> dict:
    """Describe a schematic protobuf item and nested fields, including enum choices.

    Start with SheetSymbol, SheetPin, HierarchicalLabel or SchematicLine. Use returned message names
    in further calls. Protobuf JSON uses snake_case fields here and UUIDs as {value: UUID}.
    """
    from google.protobuf import descriptor_pool
    if not type_name.startswith("kiapi."):
        type_name = "kiapi.schematic.types." + type_name
    if not type_name.startswith(("kiapi.schematic.types.", "kiapi.common.types.")):
        raise ValueError("Only KiCad schematic/common types are available")
    descriptor = descriptor_pool.Default().FindMessageTypeByName(type_name)
    return {"type": descriptor.full_name, "fields": [
        {"name": field.name, "repeated": field.is_repeated,
         "message": field.message_type.full_name if field.message_type else None,
         "enum": [v.name for v in field.enum_type.values] if field.enum_type else None,
         "scalar_type": field.type}
        for field in descriptor.fields
    ]}


if __name__ == "__main__":
    mcp.run(transport="stdio")
