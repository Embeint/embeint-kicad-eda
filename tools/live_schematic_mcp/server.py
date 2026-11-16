# SPDX-License-Identifier: GPL-3.0-or-later
"""MCP companion for the schematic IPC API in this KiCad development checkout."""

from google.protobuf import empty_pb2
from mcp.server.fastmcp import FastMCP, Image

from client import LiveSchematic, as_dict, document, editor, parse_items, project

mcp = FastMCP("KiCad Live Schematic")
client = LiveSchematic()


@mcp.tool()
def live_schematic_documents() -> dict:
    """List open live schematic documents and KiCad version. Requires development KiCad 10.99/11+."""
    return client.documents()


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
def live_schematic_view(document_spec: dict, dpi: int = 100) -> Image:
    """See an open sheet's current unsaved contents rendered by KiCad. Does not save/reload the design.

    Returns a PNG image for visual inspection after edits. This is an on-demand snapshot, not video.
    """
    return Image(data=client.render(document(document_spec), dpi), format="png")


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
    """List every KiCad schematic IPC command, its kind (read/edit/view/file/job) and reply type."""
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
    """Run any KiCad schematic IPC command by name with a protobuf-JSON payload.

    Prefer the typed tools. Most commands need a document (or header.document) from
    live_schematic_documents. File-kind commands (SaveDocument, RevertDocument, ...) change or discard
    files: run them only when the user asks. An edit command is not retried if the reply is lost.
    """
    return client.execute(command, payload)


@mcp.tool()
def live_schematic_batch(document_spec: dict, steps: list[dict], description: str = "MCP: batch") -> dict:
    """Run several read/edit/view commands as ONE undo step; if any step fails, all are rolled back.

    steps: [{"command": "CreateItems", "payload": {...}}, ...] using names from live_schematic_list_commands.
    Undo/redo and file/job commands are not allowed in a batch.
    """
    return client.batch(document(document_spec), steps, description)


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
