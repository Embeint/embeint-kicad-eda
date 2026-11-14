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
