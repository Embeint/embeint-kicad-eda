# SPDX-License-Identifier: GPL-3.0-or-later
"""Item round-trip probe: does an unchanged API edit leave the saved schematic unchanged?

The probe saves the document to a string (SaveDocumentToString: KiCad's own file writer, no disk
write), sends an item back exactly as GetItems returned it, saves again and compares the two
files item by item.  It does the same for delete + create-from-returned-item.  Differences are
reported per top-level file block (keyed by the block's UUID), so each one names the item and the
serialized field that changed.  The live driver is e2e_item_roundtrip.py; the helpers here need no
KiCad and are unit tested.
"""

import difflib
import math
import re
import struct
import zlib

from google.protobuf import json_format

from client import editor, enums, kiid, point, sch, types

#: Child block heads whose UUIDs KiCad mints afresh when an item is created (pins of a new
#: symbol or sheet), so a created copy cannot reproduce them.
FRESH_CHILD_IDS = ("pin", "table_cell")

_UUID = re.compile(r'\(uuid "([0-9a-fA-F-]{36})"\)')


def _children(text, start, end):
    """Spans (begin, end) of the s-expressions directly inside text[start:end]."""
    spans, depth, begin, quoted, escaped = [], 0, None, False, False
    for index in range(start, end):
        char = text[index]
        if quoted:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                quoted = False
        elif char == '"':
            quoted = True
        elif char == "(":
            if depth == 0:
                begin = index
            depth += 1
        elif char == ")":
            depth -= 1
            if depth == 0:
                spans.append((begin, index + 1))
            if depth < 0:
                raise ValueError("unbalanced s-expression")
    if depth or quoted:
        raise ValueError("unbalanced s-expression")
    return spans


def _head(block):
    match = re.match(r"\(\s*([^\s()]+)", block)
    return match.group(1) if match else "?"


def _own_uuid(block):
    """The UUID that is a direct child of block, or None."""
    for begin, end in _children(block, 1, len(block) - 1):
        match = _UUID.fullmatch(block[begin:end].strip())
        if match:
            return match.group(1).lower()
    return None


def sexpr_blocks(text):
    """{key: block text} for each top-level block of a saved schematic or board.

    Items are keyed by their own UUID; blocks without one (lib_symbols, sheet_instances, paper...)
    by head and occurrence, e.g. "lib_symbols#0"."""
    text = text.strip()
    outer = _children(text, 0, len(text))
    if len(outer) != 1:
        raise ValueError("expected one top-level s-expression")
    begin, end = outer[0]
    blocks, counts = {}, {}
    for child_begin, child_end in _children(text, begin + 1, end - 1):
        block = text[child_begin:child_end]
        key = _own_uuid(block)
        if key is None:
            head = _head(block)
            key = f"{head}#{counts.get(head, 0)}"
            counts[head] = counts.get(head, 0) + 1
        blocks[key] = block
    return blocks


def mask_fresh_ids(block):
    """block with the UUIDs of its pins blanked (a created symbol or sheet gets new pin UUIDs)."""
    spans = []

    def visit(begin, end):
        for child_begin, child_end in _children(block, begin + 1, end - 1):
            if _head(block[child_begin:child_end]) in FRESH_CHILD_IDS:
                for uuid_begin, uuid_end in _children(block, child_begin + 1, child_end - 1):
                    if _UUID.fullmatch(block[uuid_begin:uuid_end]):
                        spans.append((uuid_begin, uuid_end))
            visit(child_begin, child_end)

    visit(0, len(block))
    for begin, end in sorted(spans, reverse=True):
        block = block[:begin] + '(uuid "<fresh>")' + block[end:]
    # A created symbol lists its pins in the order of the request (by UUID), not the saved order
    pins = [span for span in _children(block, 1, len(block) - 1)
            if _head(block[span[0]:span[1]]) in FRESH_CHILD_IDS]
    if pins and _head(block) == "symbol":
        ordered = sorted(block[begin:end] for begin, end in pins)
        for (begin, end), text in sorted(zip(pins, ordered), reverse=True):
            block = block[:begin] + text + block[end:]
    return block


def block_differences(before, after, mask=False, keys=None):
    """Changed, added and removed blocks between two saved files, with a unified diff each.

    mask blanks the pin UUIDs that creation renews; keys limits the comparison to those blocks."""
    old, new = sexpr_blocks(before), sexpr_blocks(after)
    if keys is not None:
        old = {k: v for k, v in old.items() if k in keys}
        new = {k: v for k, v in new.items() if k in keys}
    if mask:
        old = {k: mask_fresh_ids(v) for k, v in old.items()}
        new = {k: mask_fresh_ids(v) for k, v in new.items()}
    found = []
    for key in sorted(set(old) | set(new)):
        if old.get(key) == new.get(key):
            continue
        kind = "changed" if key in old and key in new else ("removed" if key in old else "added")
        diff = difflib.unified_diff(old.get(key, "").splitlines(), new.get(key, "").splitlines(),
                                    "before", "after", lineterm="", n=1)
        block = old.get(key) or new.get(key)
        found.append({"key": key, "head": _head(block), "kind": kind, "diff": "\n".join(diff)})
    return found


def unpack(packed):
    """(message, uuid) for a packed item returned by GetItems."""
    from commands import message_class
    message = message_class(packed.TypeName())()
    if not packed.Unpack(message):
        raise ValueError(f"cannot unpack {packed.TypeName()}")
    return message, item_id(message)


def item_id(message):
    """The item's UUID; generated board tables keep theirs in the wrapped table."""
    holder = message
    while "id" not in holder.DESCRIPTOR.fields_by_name and "table" in holder.DESCRIPTOR.fields_by_name:
        holder = holder.table
    return holder.id.value


#: Fields KiCad cannot keep as sent, with the reason, keyed by message type and field path without
#: list indices.  The probe reports them as known, not as failures; README.md lists them too.
UNPRESERVABLE = {
    ("NoConnectMarker", "size.value_nm"):
        "the schematic file does not store a marker size; every marker has the default size",
    ("SheetSymbol", "border_stroke.style"):
        "sheet borders are always solid; the file stores only their width and colour",
    ("DirectiveLabel", "symbol_size.value_nm"):
        "the schematic file does not store a directive label's flag size; every flag has the default",
    ("SchematicGraphicShape", "shape.polygon.polygons.outline.closed"):
        "a schematic polyline is saved as its points and loads open (a filled one is still drawn closed)",
}

#: Values that legitimately differ after saving and reopening, with the reason.  Paths match
#: field paths with list indices removed.
NOT_SAVED = {
    "parent.value": "a board has no saved UUID, so the parent UUID of board items is new on every load",
    "table.parent.value": "a board has no saved UUID, so the parent UUID of board items is new on every load",
    "cells.text_box.parent.value": "a board has no saved UUID, so the parent UUID of board items is new on "
                                   "every load",
    "table.cells.text_box.parent.value": "a board has no saved UUID, so the parent UUID of board items is new "
                                         "on every load",
    "definition.items.pad_stack.copper_layers.custom_shapes.id.value":
        "pad primitives have no saved UUID",
    "intersheet_refs_field.visible":
        "a global label's intersheet references follow the schematic's setting when it is loaded",
}


def unindexed(path):
    """A field path without list indices, e.g. "cells.text_box.id.value"."""
    return re.sub(r"\[\d+\]", "", path)

#: The size every no-connect marker has (48 mils)
NO_CONNECT_SIZE_NM = 1_219_200


def is_type(message, message_type):
    """isinstance by protobuf type: messages unpacked through the descriptor pool may be another
    Python class than the generated module's."""
    return message.DESCRIPTOR.full_name == message_type.DESCRIPTOR.full_name


def same_colour_channel(path, sent, returned):
    """True for a colour channel that KiCad rounded to the 8 bits a document stores."""
    return (re.search(r"(^|\.)color\.[rgb]$", path) is not None and isinstance(sent, (int, float))
            and isinstance(returned, (int, float)) and _channel(sent) == _channel(returned))


def _channel(value):
    """The 8-bit channel KiCad stores, rounded as KiROUND does (half away from zero)."""
    return math.floor(min(max(value, 0.0), 1.0) * 255 + 0.5)


def normalize_created(message):
    """A copy without what creation or reloading renews: the UUIDs of sheet pins, table cells and a
    symbol definition's pins and graphics (and so the UUID order they are listed in)."""
    copy = type(message)()
    copy.CopyFrom(message)
    if "cells" in copy.DESCRIPTOR.fields_by_name:
        # Created tables get new cell UUIDs
        for cell in copy.cells:
            cell.text_box.ClearField("id")
    if is_type(copy, sch.SheetSymbol):
        for pin in copy.pins:
            pin.ClearField("id")
    elif is_type(copy, sch.SchematicSymbolInstance):
        from commands import message_class
        children = []
        for child in copy.definition.items:
            # Pins, and the library graphics a definition also lists, have no saved UUID of their own
            body = message_class(child.item.TypeName())()
            number = ""
            if child.item.Unpack(body):
                if "id" in body.DESCRIPTOR.fields_by_name:
                    body.ClearField("id")
                number = getattr(body, "number", "") if is_type(body, sch.SchematicPin) else ""
                child.item.Pack(body)
            children.append((number, child.SerializeToString(deterministic=True), child))
        children.sort(key=lambda entry: entry[:2])
        ordered = [type(entry[2])() for entry in children]
        for target, entry in zip(ordered, children):
            target.CopyFrom(entry[2])
        del copy.definition.items[:]
        copy.definition.items.extend(ordered)
    return copy


def message_differences(sent, returned):
    """Field paths whose values differ between two messages of one type (JSON view)."""
    def flat(value, prefix=""):
        if isinstance(value, dict):
            out = {}
            for key, child in value.items():
                out.update(flat(child, f"{prefix}.{key}" if prefix else key))
            return out
        if isinstance(value, list):
            out = {}
            for index, child in enumerate(value):
                out.update(flat(child, f"{prefix}[{index}]"))
            return out or {prefix: []}
        return {prefix: value}

    def view(message):
        return flat(json_format.MessageToDict(message, preserving_proto_field_name=True))

    old, new = view(sent), view(returned)
    return sorted(k for k in set(old) | set(new) if old.get(k) != new.get(k))


def _png(width=2, height=2):
    """A small valid RGBA PNG (checker pattern) for the image item."""
    def chunk(kind, data):
        return (struct.pack(">I", len(data)) + kind + data
                + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF))
    red, blue = bytes((255, 0, 0, 255)), bytes((0, 0, 255, 255))
    rows = b"".join(bytes(1) + b"".join(red if (x + y) % 2 else blue for x in range(width))
                    for y in range(height))
    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (bytes((137, 80, 78, 71, 13, 10, 26, 10)) + chunk(b"IHDR", header)
            + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))


PNG = _png()


def _mm(value):
    return types.Distance(value_nm=round(value * 1_000_000))


def _color(r, g, b, a=1.0):
    return types.Color(r=r, g=g, b=b, a=a)


def _text(value, x=None, y=None, **attrs):
    """Text with attributes.  Without x, y the position and alignment are left to KiCad: a label's
    or sheet pin's text sits at the item and is aligned by its spin style or side."""
    text = types.Text(text=value)
    text.attributes.size.CopyFrom(point(1.5, 1.5))
    if x is not None:
        text.position.CopyFrom(point(x, y))
        text.attributes.horizontal_alignment = enums.HA_LEFT
        text.attributes.vertical_alignment = enums.VA_BOTTOM
    for key, value in attrs.items():
        if key == "color":
            text.attributes.color.CopyFrom(value)
        elif key == "angle":
            text.attributes.angle.value_degrees = value
        elif key == "stroke_width":
            text.attributes.stroke_width.CopyFrom(_mm(value))
        else:
            setattr(text.attributes, key, value)
    return text


def _stroke(width, style, color=None):
    stroke = types.StrokeAttributes(width=_mm(width), style=style)
    if color:
        stroke.color.CopyFrom(color)
    return stroke


def _field(name, value, x, y, visible=True):
    field = sch.SchematicField(name=name, visible=visible, show_name=True, allow_auto_place=False)
    field.text.CopyFrom(_text(value, x, y, italic=True))
    return field


def _props(tag):
    return [types.CustomProperty(key="probe", value=tag)]


def api_fixture(x0=500.0, y0=50.0):
    """One item of every schematic type the API creates, with non-default values in its fields.

    Placed far from the page (x0, y0 in mm) so it neither touches nor connects to anything else
    in the test schematic.  Symbols come from PlaceSymbolFromLibrary in the driver instead: the
    API needs a library definition for them."""
    def at(dx, dy):
        return point(x0 + dx, y0 + dy)

    items = []
    wire = sch.SchematicLine(id=kiid(), start=at(0, 0), end=at(10, 0), type=sch.SLT_WIRE,
                             locked=types.LS_LOCKED,
                             stroke=_stroke(0.3, enums.SLS_DASH, _color(0.2, 0.4, 0.6)))
    wire.custom_properties.extend(_props("wire"))
    # Two more wires end at (10, 0): a T needs its junction, so KiCad's cleanup keeps it
    for end in ((20, 0), (10, 10)):
        items.append(sch.SchematicLine(id=kiid(), start=at(10, 0), end=at(*end), type=sch.SLT_WIRE))
    bus = sch.SchematicLine(id=kiid(), start=at(0, 20), end=at(30, 20), type=sch.SLT_BUS,
                            stroke=_stroke(0.5, enums.SLS_DOT))
    note = sch.SchematicLine(id=kiid(), start=at(40, 0), end=at(60, 5), type=sch.SLT_GRAPHIC,
                             stroke=_stroke(0.2, enums.SLS_DASHDOT, _color(1, 0, 0)))
    note.start_ending.style = types.LES_ARROW
    note.end_ending.style = types.LES_CIRCLE
    items += [wire, bus, note]
    items.append(sch.Junction(id=kiid(), position=at(10, 0), diameter=_mm(1.5),
                              color=_color(0, 0.5, 0)))
    # One marker leaves the size out, one asks for a size the file cannot store
    items.append(sch.NoConnectMarker(id=kiid(), position=at(20, 10)))
    items.append(sch.NoConnectMarker(id=kiid(), position=at(25, 10), size=_mm(1.27)))
    items.append(sch.BusEntry(id=kiid(), position=at(10, 17.46), size=point(2.54, 2.54),
                              type=sch.BET_WIRE_TO_BUS, stroke=_stroke(0.25, enums.SLS_SOLID)))
    items.append(sch.BusEntry(id=kiid(), position=at(20, 17.46), size=point(2.54, 2.54),
                              type=sch.BET_BUS_TO_BUS, stroke=_stroke(0.4, enums.SLS_DASH)))

    text = sch.SchematicText(id=kiid(), exclude_from_sim=True, locked=types.LS_LOCKED,
                             text=_text("Probe text", 0, 40, bold=True, italic=True, angle=90,
                                        color=_color(0.1, 0.2, 0.3), stroke_width=0.3))
    text.text.hyperlink = "https://example.com/probe"
    text.custom_properties.extend(_props("text"))
    items.append(text)

    box = sch.SchematicTextBox(id=kiid(), exclude_from_sim=True, margin_left=_mm(0.5),
                               margin_top=_mm(0.6), margin_right=_mm(0.7), margin_bottom=_mm(0.8))
    box.textbox.top_left.CopyFrom(at(10, 35))
    box.textbox.bottom_right.CopyFrom(at(40, 45))
    box.textbox.text = "Probe text box"
    box.hyperlink = "https://example.com/box"
    box.textbox.border_enabled = True
    box.textbox.attributes.CopyFrom(_text("", 0, 0, italic=True, color=_color(0.5, 0, 0.5)).attributes)
    box.graphic_attributes.stroke.CopyFrom(_stroke(0.3, enums.SLS_DASH, _color(0, 0, 1)))
    box.graphic_attributes.fill.fill_type = types.GFT_FILLED_WITH_COLOR
    box.graphic_attributes.fill.color.CopyFrom(_color(1, 1, 0.8))
    items.append(box)

    def shape(**geometry):
        item = sch.SchematicGraphicShape(id=kiid())
        item.shape.attributes.stroke.CopyFrom(_stroke(0.35, enums.SLS_DOT, _color(0.3, 0.3, 0.3)))
        item.shape.attributes.fill.fill_type = types.GFT_FILLED_WITH_COLOR
        item.shape.attributes.fill.color.CopyFrom(_color(0.9, 0.8, 0.7))
        for name, value in geometry.items():
            getattr(item.shape, name).CopyFrom(value)
        return item

    items.append(shape(rectangle=types.GraphicRectangleAttributes(top_left=at(50, 30), bottom_right=at(60, 40),
                                                                  corner_radius=_mm(1))))
    items.append(shape(circle=types.GraphicCircleAttributes(center=at(70, 35), radius_point=at(74, 35))))
    items.append(shape(arc=types.GraphicArcAttributes(start=at(80, 35), mid=at(85, 30), end=at(90, 35))))
    items.append(shape(bezier=types.GraphicBezierAttributes(start=at(50, 50), control1=at(55, 45),
                                                            control2=at(60, 55), end=at(65, 50))))
    poly = types.PolySet()
    outline = poly.polygons.add().outline
    for dx, dy in ((70, 45), (80, 45), (75, 52)):
        outline.nodes.add().point.CopyFrom(at(dx, dy))
    outline.closed = True
    items.append(shape(polygon=poly))

    local = sch.LocalLabel(id=kiid(), position=at(0, 60), spin_style=sch.SLSS_UP,
                           text=_text("PROBE_LOCAL", bold=True), locked=types.LS_LOCKED)
    local.fields.append(_field("Probe", "local field", x0 + 2, y0 + 64))
    glob = sch.GlobalLabel(id=kiid(), position=at(20, 60), spin_style=sch.SLSS_RIGHT, shape=sch.SLSH_OUTPUT,
                           text=_text("PROBE_GLOBAL", italic=True))
    # Hidden: on loading, KiCad shows intersheet references only if the schematic's setting asks
    glob.intersheet_refs_field.CopyFrom(_field("Intersheetrefs", "${INTERSHEET_REFS}", x0 + 30, y0 + 60,
                                               visible=False))
    hier = sch.HierarchicalLabel(id=kiid(), position=at(40, 60), spin_style=sch.SLSS_BOTTOM,
                                 shape=sch.SLSH_BIDI, text=_text("PROBE_HIER"))
    directive = sch.DirectiveLabel(id=kiid(), position=at(60, 60), spin_style=sch.SLSS_LEFT,
                                   shape=sch.SLSH_DIAMOND, pin_length=_mm(3.81), symbol_size=_mm(0.762),
                                   text=_text(""))
    directive.fields.append(_field("Netclass", "Probe", x0 + 60, y0 + 66))
    items += [local, glob, hier, directive]

    area = sch.SchematicRuleArea(id=kiid(), exclude_from_sim=True, exclude_from_bom=True, dnp=True)
    area.shape.attributes.stroke.CopyFrom(_stroke(0.2, enums.SLS_DASH))
    area_outline = area.shape.polygon.polygons.add().outline
    for dx, dy in ((55, 55), (70, 55), (70, 70), (55, 70)):
        area_outline.nodes.add().point.CopyFrom(at(dx, dy))
    area_outline.closed = True
    items.append(area)

    image = sch.SchematicImage(id=kiid(), position=at(100, 10), image_data=PNG, locked=types.LS_LOCKED,
                               opacity=0.5)
    image.image_scale.value = 2.0
    items.append(image)

    sheet = sch.SheetSymbol(id=kiid(), position=at(100, 40), size=point(30, 20), exclude_from_bom=True,
                            dnp=True, fields_autoplaced=False)
    sheet.border_stroke.CopyFrom(_stroke(0.3, enums.SLS_DASH, _color(0.5, 0.2, 0.1)))
    sheet.fill.fill_type = types.GFT_FILLED_WITH_COLOR
    sheet.fill.color.CopyFrom(_color(1, 1, 0.9))
    sheet.name_field.CopyFrom(_field("Sheetname", "Round-trip probe", x0 + 100, y0 + 39))
    sheet.filename_field.CopyFrom(_field("Sheetfile", "roundtrip-probe.kicad_sch", x0 + 100, y0 + 61))
    sheet.user_fields.append(_field("Probe", "sheet field", x0 + 101, y0 + 63))
    pin = sheet.pins.add(id=kiid(), position=at(100, 45), side=sch.SHS_LEFT, shape=sch.SLSH_INPUT)
    pin.text.CopyFrom(_text("PROBE_PIN"))
    items.append(sheet)

    table = sch.SchematicTable(id=kiid(), column_count=2, external_border=sch.TSM_ENABLED,
                               header_separator=sch.TSM_ENABLED, row_separators=sch.TSM_ENABLED,
                               column_separators=sch.TSM_ENABLED)
    table.border_stroke.CopyFrom(_stroke(0.3, enums.SLS_SOLID))
    table.separators_stroke.CopyFrom(_stroke(0.15, enums.SLS_DASH))
    table.column_widths.extend([_mm(15), _mm(20)])
    table.row_heights.extend([_mm(5), _mm(6)])
    for row in range(2):
        for column in range(2):
            cell = table.cells.add(column_span=1, row_span=1)
            cell.text_box.textbox.text = f"r{row}c{column}"
            cell.text_box.textbox.top_left.CopyFrom(at(140 + column * 15, 10 + row * 5))
            cell.text_box.textbox.bottom_right.CopyFrom(at(140 + column * 15 + (15 if not column else 20),
                                                           10 + row * 5 + (5 if not row else 6)))
            cell.text_box.textbox.attributes.size.CopyFrom(point(1.27, 1.27))
            cell.text_box.textbox.attributes.bold = row == 0
    items.append(table)
    return items


def save_string(client, doc):
    """The document as KiCad would save it, without writing to disk."""
    return client.send(editor.SaveDocumentToString(document=doc), editor.SavedDocumentResponse).contents
