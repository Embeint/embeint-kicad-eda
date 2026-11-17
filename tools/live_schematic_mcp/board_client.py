# SPDX-License-Identifier: GPL-3.0-or-later
"""PCB editor operations over the live IPC API, mixed into LiveSchematic.

Everything works on the board open in the PCB editor, including unsaved changes; nothing reads or
writes .kicad_pcb files except board_save and the exports.  Units are millimetres and degrees
(counter-clockwise, KiCad's convention); the board's Y axis points down.
"""

from pathlib import Path
import math
import sys
import tempfile
import time
import uuid

from google.protobuf import empty_pb2, json_format

sys.path.insert(0, str(Path(__file__).resolve().parent / "_generated"))
from board import board_commands_pb2 as bc            # noqa: E402
from board import board_jobs_pb2 as board_jobs        # noqa: E402
from board import board_pb2 as board_pb               # noqa: E402
from board import board_rules_pb2 as board_rules      # noqa: E402
from board import board_tasks_pb2 as board_tasks      # noqa: E402
from board import board_types_pb2 as bt               # noqa: E402
from common import tasks_pb2 as tasks                 # noqa: E402
from common.commands import editor_commands_pb2 as editor  # noqa: E402
from common.commands import project_commands_pb2 as project  # noqa: E402
from common.types import base_types_pb2 as types      # noqa: E402
from common.types import enums_pb2 as enums           # noqa: E402
from common.types import jobs_pb2 as jobs             # noqa: E402

_ = board_pb, board_rules  # registered for GetBoardStackup / DRC replies

LIMIT_MM = 10000


def _nm(value):
    if not math.isfinite(value) or abs(value) > LIMIT_MM:
        raise ValueError("Coordinates and sizes must be finite millimetres within +/-10000")
    return round(value * 1_000_000)


def _vec(x, y):
    return types.Vector2(x_nm=_nm(x), y_nm=_nm(y))


def _mm(vector):
    return [round(vector.x_nm / 1e6, 6), round(vector.y_nm / 1e6, 6)]


def _dist(value):
    return types.Distance(value_nm=_nm(value))


def _id():
    return types.KIID(value=str(uuid.uuid4()))


def _dict(message):
    return json_format.MessageToDict(message, preserving_proto_field_name=True)


def board_document(value):
    """A DOCTYPE_PCB document as returned by live_pcb_open or live_pcb_documents."""
    result = json_format.ParseDict(value, types.DocumentSpecifier())
    if result.type != types.DOCTYPE_PCB or not result.board_filename:
        raise ValueError("Use a board document returned by live_pcb_open or live_pcb_documents")
    return result


# ---- Layers -----------------------------------------------------------------------------------

LAYER_ALIASES = {"F.Silkscreen": "F.SilkS", "B.Silkscreen": "B.SilkS", "F.Courtyard": "F.CrtYd",
                 "B.Courtyard": "B.CrtYd", "User.Drawings": "Dwgs.User", "User.Comments": "Cmts.User",
                 "User.Eco1": "Eco1.User", "User.Eco2": "Eco2.User", "Edge": "Edge.Cuts", "Top": "F.Cu",
                 "Bottom": "B.Cu"}


def layer(name):
    """BoardLayer for a KiCad layer name such as F.Cu, In1.Cu, Edge.Cuts, F.SilkS or User.1."""
    name = LAYER_ALIASES.get(name, name)
    key = "BL_" + name.replace(".", "_", 1)
    try:
        return bt.BoardLayer.Value(key)
    except ValueError:
        raise ValueError(f"Unknown layer {name!r}; use KiCad names like F.Cu, In1.Cu, B.SilkS, Edge.Cuts") from None


def layer_name(value):
    name = bt.BoardLayer.Name(value).removeprefix("BL_")
    return name.replace("_", ".", 1) if "_" in name else name


def is_copper(value):
    return bt.BoardLayer.Name(value).endswith("_Cu")


# ---- Item construction ------------------------------------------------------------------------

def make_track(start, end, width, layer_value, net):
    return bt.Track(id=_id(), start=_vec(*start), end=_vec(*end), width=_dist(width), layer=layer_value,
                    net=bt.Net(name=net or ""))


def make_via(position, net, size, drill, start="F.Cu", end="B.Cu"):
    via = bt.Via(id=_id(), position=_vec(*position), net=bt.Net(name=net or ""))
    through = (start, end) == ("F.Cu", "B.Cu")
    via.type = bt.VT_THROUGH if through else bt.VT_BLIND_BURIED
    stack = via.pad_stack
    stack.type = bt.PST_NORMAL
    stack.drill.diameter.CopyFrom(_vec(drill, drill))
    stack.drill.shape = bt.DS_CIRCLE
    stack.drill.start_layer = layer(start)
    stack.drill.end_layer = layer(end)
    stack.copper_layers.add(layer=bt.BL_F_Cu, shape=bt.PSS_CIRCLE, size=_vec(size, size))
    return via


def make_segment(start, end, layer_value, width):
    shape = bt.BoardGraphicShape(id=_id(), layer=layer_value)
    shape.shape.segment.start.CopyFrom(_vec(*start))
    shape.shape.segment.end.CopyFrom(_vec(*end))
    shape.shape.attributes.stroke.width.value_nm = _nm(width)
    shape.shape.attributes.stroke.style = enums.SLS_SOLID
    return shape


def make_arc(start, mid, end, layer_value, width):
    shape = bt.BoardGraphicShape(id=_id(), layer=layer_value)
    shape.shape.arc.start.CopyFrom(_vec(*start))
    shape.shape.arc.mid.CopyFrom(_vec(*mid))
    shape.shape.arc.end.CopyFrom(_vec(*end))
    shape.shape.attributes.stroke.width.value_nm = _nm(width)
    shape.shape.attributes.stroke.style = enums.SLS_SOLID
    return shape


def make_rectangle(x1, y1, x2, y2, layer_value, width, filled=False):
    shape = bt.BoardGraphicShape(id=_id(), layer=layer_value)
    shape.shape.rectangle.top_left.CopyFrom(_vec(x1, y1))
    shape.shape.rectangle.bottom_right.CopyFrom(_vec(x2, y2))
    shape.shape.attributes.stroke.width.value_nm = _nm(width)
    shape.shape.attributes.stroke.style = enums.SLS_SOLID
    shape.shape.attributes.fill.fill_type = types.GFT_FILLED if filled else types.GFT_UNFILLED
    return shape


def make_circle(center, radius, layer_value, width, filled=False):
    shape = bt.BoardGraphicShape(id=_id(), layer=layer_value)
    shape.shape.circle.center.CopyFrom(_vec(*center))
    shape.shape.circle.radius_point.CopyFrom(_vec(center[0] + radius, center[1]))
    shape.shape.attributes.stroke.width.value_nm = _nm(width)
    shape.shape.attributes.stroke.style = enums.SLS_SOLID
    shape.shape.attributes.fill.fill_type = types.GFT_FILLED if filled else types.GFT_UNFILLED
    return shape


def make_polygon(points, layer_value, width, filled=True):
    shape = bt.BoardGraphicShape(id=_id(), layer=layer_value)
    outline = shape.shape.polygon.polygons.add().outline
    outline.closed = True
    for p in points:
        outline.nodes.add().point.CopyFrom(_vec(*p))
    shape.shape.attributes.stroke.width.value_nm = _nm(width)
    shape.shape.attributes.stroke.style = enums.SLS_SOLID
    shape.shape.attributes.fill.fill_type = types.GFT_FILLED if filled else types.GFT_UNFILLED
    return shape


def make_text(content, position, layer_value, size, rotation=0.0, thickness=None, mirrored=None):
    text = bt.BoardText(id=_id(), layer=layer_value)
    text.text.text = content
    text.text.position.CopyFrom(_vec(*position))
    attributes = text.text.attributes
    attributes.size.CopyFrom(_vec(size, size))
    attributes.stroke_width.value_nm = _nm(thickness if thickness is not None else round(size * 0.15, 3))
    attributes.horizontal_alignment = enums.HA_CENTER
    attributes.vertical_alignment = enums.VA_CENTER
    attributes.angle.value_degrees = rotation
    attributes.visible = True
    # Text on the back is drawn mirrored, as KiCad does when it is placed there
    attributes.mirrored = layer_name(layer_value).startswith("B.") if mirrored is None else mirrored
    return text


def rounded_rectangle(x, y, width, height, radius, layer_value, line_width):
    """Edge.Cuts style outline: four lines and four quarter arcs."""
    if radius <= 0:
        return [make_rectangle(x, y, x + width, y + height, layer_value, line_width)]
    if radius * 2 > min(width, height):
        raise ValueError("corner_radius is larger than half the board's smaller side")
    r, x2, y2 = radius, x + width, y + height
    s = r * (1 - math.sqrt(0.5))
    return [
        make_segment((x + r, y), (x2 - r, y), layer_value, line_width),
        make_segment((x2, y + r), (x2, y2 - r), layer_value, line_width),
        make_segment((x2 - r, y2), (x + r, y2), layer_value, line_width),
        make_segment((x, y2 - r), (x, y + r), layer_value, line_width),
        make_arc((x2 - r, y), (x2 - s, y + s), (x2, y + r), layer_value, line_width),
        make_arc((x2, y2 - r), (x2 - s, y2 - s), (x2 - r, y2), layer_value, line_width),
        make_arc((x + r, y2), (x + s, y2 - s), (x, y2 - r), layer_value, line_width),
        make_arc((x, y + r), (x + s, y + s), (x + r, y), layer_value, line_width),
    ]


def make_zone(points, layers, net=None, name="", priority=0, clearance=0.2, min_width=0.25,
              thermal_gap=0.5, thermal_spoke=0.5, connection="thermal", rule_area=None):
    zone = bt.Zone(id=_id(), name=name, priority=priority)
    zone.layers.extend(layer(n) for n in layers)
    outline = zone.outline.polygons.add().outline
    outline.closed = True
    for p in points:
        outline.nodes.add().point.CopyFrom(_vec(*p))
    if rule_area is not None:
        zone.type = bt.ZT_RULE_AREA
        settings = zone.rule_area_settings
        for key in ("keepout_copper", "keepout_vias", "keepout_tracks", "keepout_pads", "keepout_footprints"):
            setattr(settings, key, bool(rule_area.get(key, key != "keepout_footprints")))
    else:
        zone.type = bt.ZT_COPPER
        settings = zone.copper_settings
        settings.net.name = net or ""
        settings.clearance.value_nm = _nm(clearance)
        settings.min_thickness.value_nm = _nm(min_width)
        settings.fill_mode = bt.ZFM_SOLID
        settings.island_mode = bt.IRM_ALWAYS
        modes = {"thermal": bt.ZCS_THERMAL, "solid": bt.ZCS_FULL, "none": bt.ZCS_NONE,
                 "pth_thermal": bt.ZCS_PTH_THERMAL}
        if connection not in modes:
            raise ValueError(f"connection must be one of {list(modes)}")
        settings.connection.zone_connection = modes[connection]
        settings.connection.thermal_spokes.width.value_nm = _nm(thermal_spoke)
        settings.connection.thermal_spokes.gap.value_nm = _nm(thermal_gap)
    zone.border.style = bt.ZBS_DIAGONAL_EDGE
    zone.border.pitch.value_nm = _nm(0.5)
    return zone


def route_points(start, end, posture="horizontal_first"):
    """A 45-degree dog-leg from start to end: straight, then diagonal (or the reverse)."""
    (x1, y1), (x2, y2) = start, end
    dx, dy = x2 - x1, y2 - y1
    if abs(dx) < 1e-9 or abs(dy) < 1e-9 or abs(abs(dx) - abs(dy)) < 1e-9:
        return [start, end]
    diagonal = min(abs(dx), abs(dy))
    sx, sy = math.copysign(1, dx), math.copysign(1, dy)
    if posture == "diagonal_first":
        corner = (x1 + sx * diagonal, y1 + sy * diagonal)
    elif posture == "orthogonal":
        corner = (x2, y1)
    else:
        straight = (abs(dx) - diagonal, 0) if abs(dx) > abs(dy) else (0, abs(dy) - diagonal)
        corner = (x1 + sx * straight[0], y1 + sy * straight[1])
    return [start, corner, end]


ITEM_TYPES = {"footprints": enums.KOT_PCB_FOOTPRINT, "pads": enums.KOT_PCB_PAD, "tracks": enums.KOT_PCB_TRACE,
              "arcs": enums.KOT_PCB_ARC, "vias": enums.KOT_PCB_VIA, "zones": enums.KOT_PCB_ZONE,
              "shapes": enums.KOT_PCB_SHAPE, "texts": enums.KOT_PCB_TEXT, "textboxes": enums.KOT_PCB_TEXTBOX,
              "dimensions": enums.KOT_PCB_DIMENSION, "groups": enums.KOT_PCB_GROUP,
              "tables": enums.KOT_PCB_TABLE, "images": enums.KOT_PCB_REFERENCE_IMAGE}

ITEM_MESSAGES = {"footprints": bt.FootprintInstance, "pads": bt.Pad, "tracks": bt.Track, "arcs": bt.Arc,
                 "vias": bt.Via, "zones": bt.Zone, "shapes": bt.BoardGraphicShape, "texts": bt.BoardText}

GRID_STYLES = {"dots": editor.GST_DOTS, "lines": editor.GST_LINES, "small_cross": editor.GST_SMALL_CROSS}
GRID_SNAPPING = {"always": editor.GSN_ALWAYS, "with_grid": editor.GSN_WITH_GRID, "never": editor.GSN_NEVER}


def summarize_footprint_instance(fp, pads=None):
    entry = {"id": fp.id.value, "reference": fp.reference_field.text.text.text,
             "value": fp.value_field.text.text.text,
             "lib_id": f"{fp.definition.id.library_nickname}:{fp.definition.id.entry_name}",
             "position_mm": _mm(fp.position), "rotation": round(fp.orientation.value_degrees, 4),
             "side": "back" if fp.layer == bt.BL_B_Cu else "front"}
    if fp.locked == types.LS_LOCKED:
        entry["locked"] = True
    if fp.attributes.do_not_populate:
        entry["dnp"] = True
    if pads is not None:
        entry["pads"] = pads
    return entry


def summarize_pad(pad):
    copper = pad.pad_stack.copper_layers[0] if pad.pad_stack.copper_layers else None
    entry = {"number": pad.number, "net": pad.net.name, "position_mm": _mm(pad.position),
             "type": bt.PadType.Name(pad.type).removeprefix("PT_")}
    if copper is not None:
        entry["size_mm"] = _mm(copper.size)
        entry["shape"] = bt.PadStackShape.Name(copper.shape).removeprefix("PSS_").lower()
    if pad.pad_stack.drill.diameter.x_nm:
        entry["drill_mm"] = round(pad.pad_stack.drill.diameter.x_nm / 1e6, 4)
    return entry


class BoardTools:
    """PCB editor methods; LiveSchematic provides send/transaction."""

    # ---- Documents ---------------------------------------------------------------------------

    def open_board(self, path=None):
        """Open (or create) the open project's board in the PCB editor of the running KiCad."""
        request = project.OpenDocument(type=types.DOCTYPE_PCB, path=str(path or ""))
        try:
            response = self.send(request, project.OpenDocumentResponse)
        except RuntimeError as error:
            if "AS_UNIMPLEMENTED" in str(error) or "AS_UNHANDLED" in str(error):
                raise RuntimeError("This KiCad cannot open the PCB editor over the API. Start KiCad's project "
                                   "manager with the project (run_kicad_wsl.sh project.kicad_pro), not the "
                                   "schematic editor on its own.") from error
            raise
        return _dict(response.document)

    def boards(self):
        try:
            opened = self.send(editor.GetOpenDocuments(type=types.DOCTYPE_PCB), editor.GetOpenDocumentsResponse)
        except RuntimeError as error:
            if "AS_UNHANDLED" not in str(error):  # no PCB editor open
                raise
            return []
        return [_dict(d) for d in opened.documents]

    def board_save(self, doc):
        self.send(project.SaveDocument(document=doc), empty_pb2.Empty)
        return {"saved": doc.board_filename}

    # ---- Reading -----------------------------------------------------------------------------

    def board_items(self, doc, kinds):
        request = editor.GetItems(header=types.ItemHeader(document=doc))
        for kind in kinds:
            if kind not in ITEM_TYPES:
                raise ValueError(f"item kind must be one of {sorted(ITEM_TYPES)}")
            request.types.append(ITEM_TYPES[kind])
        response = self.send(request, editor.GetItemsResponse)
        if response.status != types.IRS_OK:
            raise RuntimeError(f"GetItems failed: {types.ItemRequestStatus.Name(response.status)}")
        return response.items

    def board_unpacked(self, doc, kind):
        message_type = ITEM_MESSAGES[kind]
        result = []
        for packed in self.board_items(doc, [kind]):
            message = message_type()
            if packed.Unpack(message):
                result.append(message)
        return result

    def board_footprints(self, doc, references=None, with_pads=False):
        footprints = self.board_unpacked(doc, "footprints")
        if references:
            wanted = set(references)
            footprints = [fp for fp in footprints if fp.reference_field.text.text.text in wanted]
        pads_by_parent = {}
        if with_pads:
            for pad in self.board_unpacked(doc, "pads"):
                pads_by_parent.setdefault(pad.parent.value, []).append(summarize_pad(pad))
        result = []
        for fp in footprints:
            pads = None
            if with_pads:
                pads = sorted(pads_by_parent.get(fp.id.value, []), key=lambda p: (len(p["number"]), p["number"]))
            result.append(summarize_footprint_instance(fp, pads))
        return sorted(result, key=lambda f: f["reference"])

    def board_footprint(self, doc, reference):
        for fp in self.board_unpacked(doc, "footprints"):
            if fp.reference_field.text.text.text == reference:
                return fp
        raise ValueError(f"No footprint {reference} on the board")

    def find_pad(self, doc, reference, number):
        fp = self.board_footprint(doc, reference)
        for pad in self.board_unpacked(doc, "pads"):
            if pad.parent.value == fp.id.value and pad.number == str(number):
                return pad
        raise ValueError(f"No pad {reference}.{number}")

    def board_nets(self, doc):
        nets = self.send(bc.GetNets(board=doc), bc.NetsResponse).nets
        classes = {}
        if nets:
            request = bc.GetNetClassForNets()
            request.net.extend(nets)
            classes = self.send(request, bc.NetClassForNetsResponse).classes
        result = []
        for net in nets:
            if not net.name:
                continue
            entry = {"name": net.name, "code": net.code.value}
            if net.name in classes:
                entry["netclass"] = classes[net.name].name
            result.append(entry)
        return result

    def netclass_widths(self, doc, net):
        """Track width and via size/drill (mm) of the net's class, or None."""
        request = bc.GetNetClassForNets()
        request.net.add(name=net)
        classes = self.send(request, bc.NetClassForNetsResponse).classes
        if net not in classes or not classes[net].HasField("board"):
            return None
        board = classes[net].board
        via = board.via_stack if board.HasField("via_stack") else None
        copper = via.copper_layers[0] if via is not None and via.copper_layers else None
        return {"track_width": board.track_width.value_nm / 1e6 if board.HasField("track_width") else None,
                "via_diameter": copper.size.x_nm / 1e6 if copper is not None and copper.size.x_nm else None,
                "via_drill": via.drill.diameter.x_nm / 1e6 if via is not None and via.drill.diameter.x_nm else None,
                "clearance": board.clearance.value_nm / 1e6 if board.HasField("clearance") else None}

    def board_box(self, doc, mode="edges"):
        modes = {"edges": bc.BBBM_BOARD_EDGES_ONLY, "physical": bc.BBBM_PHYSICAL_LAYERS_ONLY,
                 "all": bc.BBBM_ALL_ITEMS}
        box = self.send(bc.GetBoardBoundingBox(board=doc, mode=modes[mode]), bc.BoardBoundingBoxResponse).box
        if not box.size.x_nm and not box.size.y_nm:
            return None
        x, y = box.position.x_nm / 1e6, box.position.y_nm / 1e6
        return [round(x, 4), round(y, 4), round(x + box.size.x_nm / 1e6, 4), round(y + box.size.y_nm / 1e6, 4)]

    def board_summary(self, doc):
        counts = {}
        for kind in ("footprints", "tracks", "arcs", "vias", "zones", "shapes", "texts"):
            counts[kind] = len(self.board_items(doc, [kind]))
        enabled = self.send(bc.GetBoardEnabledLayers(board=doc), bc.BoardEnabledLayersResponse)
        nets = self.board_nets(doc)
        outline = self.board_box(doc, "edges")
        modified = self.send(editor.GetDocumentModifiedState(document=doc),
                             editor.GetDocumentModifiedStateResponse)
        return {"board": doc.board_filename, "copper_layers": enabled.copper_layer_count,
                "outline_mm": outline, **({"outline_size_mm": [round(outline[2] - outline[0], 4),
                                                              round(outline[3] - outline[1], 4)]}
                                         if outline else {}),
                "counts": counts, "nets": len(nets), "unsaved": modified.state == editor.DMS_MODIFIED,
                "footprints": self.board_footprints(doc)}

    # ---- Editing -----------------------------------------------------------------------------

    def board_create(self, doc, items, description="MCP: add board items"):
        return self.mutate(doc, items, description=description)

    def board_place_footprint(self, doc, lib_id, x, y, rotation=0.0, side="front", reference=None, value=None,
                              locked=False):
        library, _, entry = lib_id.partition(":")
        if not library or not entry:
            raise ValueError("lib_id must be Library:Footprint")
        if side not in ("front", "back"):
            raise ValueError("side must be front or back")
        request = bc.PlaceFootprintFromLibrary(
            header=types.ItemHeader(document=doc),
            lib_id=types.LibraryIdentifier(library_nickname=library, entry_name=entry),
            position=_vec(x, y), orientation=types.Angle(value_degrees=rotation),
            layer=bt.BL_B_Cu if side == "back" else bt.BL_F_Cu)
        from common.commands import library_commands_pb2 as library_cmds
        with self.transaction(doc, f"MCP: place {lib_id}"):
            response = self.send(request, library_cmds.PlaceFromLibraryResponse)
        placed = bt.FootprintInstance()
        if not response.item.Unpack(placed):
            raise RuntimeError("KiCad did not return the placed footprint")
        # The new footprint joins the board only when its commit is pushed, so an update inside that
        # commit is rejected; name it in a second step
        if reference or value or locked:
            if reference:
                placed.reference_field.text.text.text = reference
            if value:
                placed.value_field.text.text.text = value
            if locked:
                placed.locked = types.LS_LOCKED
            self.mutate(doc, [placed], update=True, description=f"MCP: name {reference or lib_id}")
        return summarize_footprint_instance(placed)

    def board_move_footprints(self, doc, moves):
        """moves: [{reference, x_mm, y_mm, rotation, side, locked, relative}] as one undo step.

        Uses TransformBoardItems, which moves footprints in place (pads, graphics and fields follow);
        UpdateItems would rebuild them from the message and leave the children behind.
        """
        if not moves or len(moves) > 500:
            raise ValueError("Supply between 1 and 500 moves")
        footprints = {fp.reference_field.text.text.text: fp for fp in self.board_unpacked(doc, "footprints")}
        requests, report, flips = [], [], 0
        for move in moves:
            ref = move["reference"]
            if ref not in footprints:
                raise ValueError(f"No footprint {ref} on the board")
            fp = footprints[ref]
            relative = move.get("relative", False)
            request = bc.TransformBoardItems(header=types.ItemHeader(document=doc))
            request.items.add(value=fp.id.value)
            side = move.get("side")
            if side not in (None, "front", "back"):
                raise ValueError("side must be front or back")
            if side and side != ("back" if fp.layer == bt.BL_B_Cu else "front"):
                request.flip = bc.BFD_LEFT_RIGHT
                flips += 1
            if "x_mm" in move or "y_mm" in move:
                if relative:
                    request.translation.CopyFrom(_vec(move.get("x_mm", 0), move.get("y_mm", 0)))
                else:
                    x, y = _mm(fp.position)
                    request.position.CopyFrom(_vec(move.get("x_mm", x), move.get("y_mm", y)))
            if "rotation" in move:
                if relative:
                    request.rotation.value_degrees = move["rotation"]
                else:
                    request.orientation.value_degrees = ((move["rotation"] + 180) % 360) - 180
            if "locked" in move:
                request.locked = bool(move["locked"])
            requests.append(request)
            report.append(ref)
        with self.transaction(doc, "MCP: move footprints"):
            for request in requests:
                response = self.send(request, bc.TransformBoardItemsResponse)
                errors = [r.status.error_message or editor.ItemStatusCode.Name(r.status.code)
                          for r in response.items if r.status.code != editor.ISC_OK]
                if errors:
                    raise RuntimeError("; ".join(errors))
        return {"moved": report, "flipped": flips, "footprints": self.board_footprints(doc, references=report)}

    def _width_for(self, doc, net, width):
        if width is not None:
            return width
        widths = self.netclass_widths(doc, net) if net else None
        return (widths or {}).get("track_width") or 0.25

    def board_add_tracks(self, doc, points, layer_name_value="F.Cu", width=None, net=None):
        """A connected chain of track segments through the given points (mm)."""
        if len(points) < 2 or len(points) > 200:
            raise ValueError("Supply between 2 and 200 points")
        layer_value = layer(layer_name_value)
        if not is_copper(layer_value):
            raise ValueError("Tracks go on copper layers (F.Cu, B.Cu, In1.Cu, ...)")
        width = self._width_for(doc, net, width)
        tracks = [make_track(a, b, width, layer_value, net) for a, b in zip(points, points[1:]) if a != b]
        self.mutate(doc, tracks, description="MCP: add tracks")
        return {"tracks": [t.id.value for t in tracks], "width_mm": width, "net": net or ""}

    def board_route(self, doc, from_ref, from_pad, to_ref, to_pad, layer_name_value=None, width=None,
                    posture="horizontal_first", via_at=None, second_layer=None, avoid=True, layers=None,
                    vias=True, grid=0.1, verify=True):
        """Track between two pads.  avoid (default): around other nets' copper, keepouts and the board edge
        (board_design.board_route_avoiding).  avoid=False: a plain 45-degree dog-leg, optionally through a
        via at via_at, that does not look at anything else on the board."""
        if avoid and via_at is None:
            allowed = layers or ([layer_name_value] if layer_name_value and not vias else None)
            return self.board_route_avoiding(doc, from_ref, from_pad, to_ref, to_pad, allowed, width, vias,
                                             grid=grid, verify=verify)
        a = self.find_pad(doc, from_ref, from_pad)
        b = self.find_pad(doc, to_ref, to_pad)
        net = a.net.name or b.net.name
        if a.net.name and b.net.name and a.net.name != b.net.name:
            raise ValueError(f"{from_ref}.{from_pad} is on {a.net.name} but {to_ref}.{to_pad} is on {b.net.name}")
        start, end = tuple(_mm(a.position)), tuple(_mm(b.position))
        fp_a = self.board_footprint(doc, from_ref)
        default_layer = "B.Cu" if fp_a.layer == bt.BL_B_Cu else "F.Cu"
        first = layer(layer_name_value or default_layer)
        width = self._width_for(doc, net, width)
        items, path = [], []
        if via_at:
            second = layer(second_layer or ("B.Cu" if layer_name(first) != "B.Cu" else "F.Cu"))
            via_point = tuple(via_at)
            leg1 = route_points(start, via_point, posture)
            leg2 = route_points(via_point, end, posture)
            items += [make_track(p, q, width, first, net) for p, q in zip(leg1, leg1[1:])]
            sizes = self.netclass_widths(doc, net) or {}
            items.append(make_via(via_point, net, sizes.get("via_diameter") or 0.6, sizes.get("via_drill") or 0.3))
            items += [make_track(p, q, width, second, net) for p, q in zip(leg2, leg2[1:])]
            path = leg1 + leg2[1:]
        else:
            path = route_points(start, end, posture)
            items += [make_track(p, q, width, first, net) for p, q in zip(path, path[1:])]
        self.mutate(doc, items, description=f"MCP: route {from_ref}.{from_pad} to {to_ref}.{to_pad}")
        return {"net": net, "width_mm": width, "layer": layer_name(first), "path_mm": [list(p) for p in path],
                "items": [i.id.value for i in items]}

    def board_add_via(self, doc, x, y, net=None, size=None, drill=None, start="F.Cu", end="B.Cu"):
        sizes = (self.netclass_widths(doc, net) or {}) if net else {}
        size = size or sizes.get("via_diameter") or 0.6
        drill = drill or sizes.get("via_drill") or 0.3
        if drill >= size:
            raise ValueError("drill must be smaller than the via size")
        via = make_via((x, y), net, size, drill, start, end)
        self.mutate(doc, [via], description="MCP: add via")
        return {"via": via.id.value, "size_mm": size, "drill_mm": drill, "net": net or ""}

    def board_outline(self, doc, width, height, x=0.0, y=0.0, corner_radius=0.0, line_width=0.05,
                      replace=True, points=None):
        """Draw the board outline on Edge.Cuts (rectangle, rounded rectangle or polygon)."""
        edge = bt.BL_Edge_Cuts
        if points:
            if len(points) < 3:
                raise ValueError("A polygon outline needs at least 3 points")
            closed = list(points) + [points[0]]
            items = [make_segment(a, b, edge, line_width) for a, b in zip(closed, closed[1:])]
        else:
            if width <= 0 or height <= 0:
                raise ValueError("width and height must be positive")
            items = rounded_rectangle(x, y, width, height, corner_radius, edge, line_width)
        with self.transaction(doc, "MCP: board outline"):
            removed = []
            if replace:
                # Board-level shapes carry the board as parent; a footprint's carry the footprint
                footprint_ids = {fp.id.value for fp in self.board_unpacked(doc, "footprints")}
                for shape in self.board_unpacked(doc, "shapes"):
                    if shape.layer == edge and shape.parent.value not in footprint_ids:
                        removed.append(shape.id.value)
                if removed:
                    request = editor.DeleteItems(header=types.ItemHeader(document=doc))
                    request.item_ids.extend(types.KIID(value=v) for v in removed)
                    self.send(request, editor.DeleteItemsResponse)
            request = editor.CreateItems(header=types.ItemHeader(document=doc))
            for item in items:
                request.items.add().Pack(item)
            response = self.send(request, editor.CreateItemsResponse)
            errors = [r.status.error_message or editor.ItemStatusCode.Name(r.status.code)
                      for r in response.created_items if r.status.code != editor.ISC_OK]
            if errors:
                raise RuntimeError("; ".join(errors))
        return {"replaced": len(removed), "items": [i.id.value for i in items], "outline_mm": self.board_box(doc)}

    def board_add_zone(self, doc, net, layers=("F.Cu",), points=None, name="", priority=0, clearance=0.2,
                       min_width=0.25, connection="thermal", inset=0.0, fill=True):
        """Copper pour for a net; follows the board outline (inset by `inset` mm) unless points are given."""
        if points is None:
            box = self.board_box(doc, "edges")
            if not box:
                raise ValueError("The board has no outline yet; draw one (live_pcb_outline) or give points")
            x1, y1, x2, y2 = box
            points = [(x1 + inset, y1 + inset), (x2 - inset, y1 + inset), (x2 - inset, y2 - inset),
                      (x1 + inset, y2 - inset)]
        zone = make_zone(points, layers, net, name, priority, clearance, min_width, connection=connection)
        self.mutate(doc, [zone], description=f"MCP: add {net} zone")
        if fill:
            self.board_refill(doc)
        return {"zone": zone.id.value, "net": net, "layers": list(layers), "filled": fill}

    def board_add_keepout(self, doc, points, layers=("F.Cu", "B.Cu"), name="", tracks=True, vias=True,
                          pads=True, copper=True, footprints=False):
        rule = {"keepout_tracks": tracks, "keepout_vias": vias, "keepout_pads": pads, "keepout_copper": copper,
                "keepout_footprints": footprints}
        zone = make_zone(points, layers, name=name, rule_area=rule)
        self.mutate(doc, [zone], description="MCP: add keepout")
        return {"rule_area": zone.id.value}

    def board_add_graphics(self, doc, shapes):
        """shapes: [{kind: line|arc|rect|circle|polygon|text, layer, ...}] in mm, one undo step."""
        items = []
        for spec in shapes:
            kind = spec.get("kind")
            layer_value = layer(spec.get("layer", "F.SilkS"))
            width = spec.get("width", 0.12 if "Silk" in layer_name(layer_value) else 0.1)
            if kind == "line":
                items.append(make_segment(spec["start"], spec["end"], layer_value, width))
            elif kind == "arc":
                items.append(make_arc(spec["start"], spec["mid"], spec["end"], layer_value, width))
            elif kind == "rect":
                x1, y1, x2, y2 = spec["box"]
                items.append(make_rectangle(x1, y1, x2, y2, layer_value, width, spec.get("filled", False)))
            elif kind == "circle":
                items.append(make_circle(spec["center"], spec["radius"], layer_value, width, spec.get("filled", False)))
            elif kind == "polygon":
                items.append(make_polygon(spec["points"], layer_value, width, spec.get("filled", True)))
            elif kind == "text":
                items.append(make_text(spec["text"], spec["position"], layer_value, spec.get("size", 1.0),
                                       spec.get("rotation", 0.0), spec.get("thickness")))
            else:
                raise ValueError("kind must be line, arc, rect, circle, polygon or text")
        if not items or len(items) > 200:
            raise ValueError("Supply between 1 and 200 shapes")
        self.mutate(doc, items, description="MCP: add graphics")
        return {"items": [i.id.value for i in items]}

    def board_delete_where(self, doc, kinds, net=None, layer_filter=None):
        """Delete tracks/vias/arcs/zones by net and/or layer (e.g. to rip up a net)."""
        ids = []
        for kind in kinds:
            for item in self.board_unpacked(doc, kind):
                item_net = item.copper_settings.net.name if kind == "zones" else getattr(item, "net", bt.Net()).name
                if net is not None and item_net != net:
                    continue
                if layer_filter is not None:
                    wanted = layer(layer_filter)
                    if kind == "zones":
                        if wanted not in item.layers:
                            continue
                    elif kind != "vias" and item.layer != wanted:
                        continue
                ids.append(item.id.value)
        if not ids:
            return {"deleted": 0}
        for chunk in range(0, len(ids), 200):
            self.delete(doc, ids[chunk:chunk + 200])
        return {"deleted": len(ids)}

    def board_refill(self, doc, zones=None):
        request = bc.RefillZones(board=doc)
        request.zones.extend(types.KIID(value=v) for v in (zones or []))
        self.send(request, empty_pb2.Empty)
        return {"refilled": True}

    def board_drc(self, doc, timeout_s=300.0, parity=True, refill=True):
        import pynng
        started = self.send(board_tasks.StartDrcTask(board=doc, test_schematic_parity=parity,
                                                     require_zones_filled=refill),
                            tasks.StartTaskResponse)
        deadline = time.monotonic() + timeout_s
        while True:
            try:
                # Read-only, so a reply held up while KiCad (re)loads footprint libraries for the
                # schematic parity check is simply waited for again
                status = self.send(tasks.GetTaskStatus(token=started.token), tasks.TaskStatusResponse)
            except pynng.Timeout:
                if time.monotonic() > deadline:
                    raise TimeoutError("DRC did not finish in time; it may still be running in KiCad") from None
                continue
            if status.state == tasks.TS_COMPLETED:
                break
            if status.state in (tasks.TS_FAILED, tasks.TS_CANCELLED):
                raise RuntimeError(f"DRC {tasks.TaskState.Name(status.state)}: {status.error_message}")
            if time.monotonic() > deadline:
                raise TimeoutError("DRC did not finish in time; it may still be running in KiCad")
            time.sleep(0.25)
        result = self.send(board_tasks.GetDrcTaskResult(token=started.token), board_tasks.DrcTaskResult)
        by_type, violations = {}, []
        for marker in result.violations:
            kind = board_rules.DrcErrorType.Name(marker.error_type).removeprefix("DRCET_").removeprefix("DRCE_").lower()
            by_type[kind] = by_type.get(kind, 0) + 1
            violations.append({"type": kind, "severity": types.RuleSeverity.Name(marker.severity).removeprefix("RS_"),
                               "description": marker.description, "position_mm": _mm(marker.position),
                               "layer": layer_name(marker.layer) if marker.layer else None,
                               "items": [i.value for i in marker.items],
                               **({"rule": marker.violating_rule} if marker.violating_rule else {})})
        return {"violation_count": len(violations), "by_type": by_type, "ignored": result.ignored_count,
                "excluded": result.excluded_count, "violations": violations[:200]}

    def board_update_from_schematic(self, doc, schematic_doc, dry_run=False, delete_extra=False,
                                    update_footprints=True, match="uuid"):
        """Update PCB from Schematic: the schematic's live netlist, applied to the live board."""
        from schematic import schematic_jobs_pb2 as sch_jobs
        with tempfile.TemporaryDirectory(prefix="kicad-live-netlist-") as directory:
            path = Path(directory) / "update.net"
            request = sch_jobs.RunSchematicJobExportNetlist(format=sch_jobs.SNF_KICAD_SEXPR)
            request.job_settings.document.CopyFrom(schematic_doc)
            request.job_settings.output_path = str(path)
            response = self.send(request, jobs.RunJobResponse)
            if response.status != jobs.JS_SUCCESS or not path.is_file():
                raise RuntimeError(f"Could not export the schematic netlist: {response.message}")
            imported = self.send(bc.ImportNetlist(
                board=doc, netlist_path=str(path), dry_run=dry_run,
                match_mode=bc.NMM_REFERENCE if match == "reference" else bc.NMM_UUID,
                delete_extra_footprints=delete_extra, update_footprints=update_footprints),
                bc.ImportNetlistResponse)
        return {"dry_run": dry_run, "errors": imported.error_count, "warnings": imported.warning_count,
                "new_footprints": imported.new_footprint_count, "report": imported.report[-6000:]}

    # ---- Settings: origins, grid, layers, rules ------------------------------------------------

    def board_origins(self, doc, grid=None, drill=None):
        for kind, value in ((bc.BOT_GRID, grid), (bc.BOT_DRILL, drill)):
            if value is not None:
                self.send(bc.SetBoardOrigin(board=doc, type=kind, origin=_vec(*value)), empty_pb2.Empty)
        result = {}
        for name, kind in (("grid_origin_mm", bc.BOT_GRID), ("drill_origin_mm", bc.BOT_DRILL)):
            result[name] = _mm(self.send(bc.GetBoardOrigin(board=doc, type=kind), types.Vector2))
        return result

    def editor_grid(self, doc, size_mm=None, preset_index=None, visible=None, style=None, snapping=None):
        """The editor's drawing grid; works in every editor.  Returns the grid after any change."""
        changes = {"size": size_mm, "preset_index": preset_index, "visible": visible, "style": style,
                   "snapping": snapping}
        if any(v is not None for v in changes.values()):
            request = editor.SetGridSettings(document=doc)
            if size_mm is not None:
                sx, sy = (size_mm, size_mm) if isinstance(size_mm, (int, float)) else size_mm
                if sx <= 0 or sy <= 0:
                    raise ValueError("grid size must be positive")
                request.size.CopyFrom(_vec(sx, sy))
            if preset_index is not None:
                request.preset_index = preset_index
            if visible is not None:
                request.visible = visible
            if style is not None:
                if style not in GRID_STYLES:
                    raise ValueError(f"style must be one of {list(GRID_STYLES)}")
                request.style = GRID_STYLES[style]
            if snapping is not None:
                if snapping not in GRID_SNAPPING:
                    raise ValueError(f"snapping must be one of {list(GRID_SNAPPING)}")
                request.snapping = GRID_SNAPPING[snapping]
            settings = self.send(request, editor.GridSettings)
        else:
            settings = self.send(editor.GetGridSettings(document=doc), editor.GridSettings)
        return grid_dict(settings)

    def board_layers(self, doc, copper_layers=None, enable=None, visible=None, active=None):
        if copper_layers is not None:
            # Through the stackup, which refuses to drop copper layers that still carry items
            # (SetBoardEnabledLayers would delete them)
            self.board_set_stackup(doc, copper_layers=copper_layers)
        if enable is not None:
            current = self.send(bc.GetBoardEnabledLayers(board=doc), bc.BoardEnabledLayersResponse)
            request = bc.SetBoardEnabledLayers(board=doc, copper_layer_count=current.copper_layer_count)
            names = set(enable) if enable is not None else None
            layers = [v for v in current.layers if not is_copper(v)] if names is None \
                else [layer(n) for n in names if not is_copper(layer(n))]
            request.layers.extend(layers)
            self.send(request, bc.BoardEnabledLayersResponse)
        if visible is not None:
            request = bc.SetVisibleLayers(board=doc)
            request.layers.extend(layer(n) for n in visible)
            self.send(request, empty_pb2.Empty)
        if active is not None:
            self.send(bc.SetActiveLayer(board=doc, layer=layer(active)), empty_pb2.Empty)
        enabled = self.send(bc.GetBoardEnabledLayers(board=doc), bc.BoardEnabledLayersResponse)
        shown = self.send(bc.GetVisibleLayers(board=doc), bc.BoardLayers)
        current_layer = self.send(bc.GetActiveLayer(board=doc), bc.BoardLayerResponse)
        return {"copper_layers": enabled.copper_layer_count,
                "enabled": [layer_name(v) for v in enabled.layers],
                "visible": [layer_name(v) for v in shown.layers],
                "active": layer_name(current_layer.layer)}

    def board_design_rules(self, doc, changes=None):
        """Read the board's design rules; with changes (protobuf JSON, merged), set them first."""
        current = self.send(bc.GetBoardDesignRules(board=doc), bc.BoardDesignRulesResponse)
        if changes:
            rules = current.rules
            json_format.ParseDict(changes, rules)
            current = self.send(bc.SetBoardDesignRules(board=doc, rules=rules), bc.BoardDesignRulesResponse)
        return _dict(current)

    def board_stackup(self, doc):
        return _dict(self.send(bc.GetBoardStackup(board=doc), bc.BoardStackupResponse).stackup)

    # ---- Seeing the board ----------------------------------------------------------------------

    def board_render(self, doc, layers=None, dpi=100, frame=True):
        """PNG of the live board: the given layers (default copper, silk, outline), on its page."""
        if not 50 <= dpi <= 600:
            raise ValueError("dpi must be between 50 and 600")
        names = layers or ["F.Cu", "B.Cu", "F.SilkS", "B.SilkS", "Edge.Cuts", "F.Fab"]
        with self.lock, tempfile.TemporaryDirectory(prefix="kicad-live-board-") as directory:
            target = Path(directory) / "board.png"
            request = board_jobs.RunBoardJobExportPng(dpi=dpi, plot_background_color=True)
            request.job_settings.document.CopyFrom(doc)
            request.job_settings.output_path = str(target)
            request.page_mode = board_jobs.BJPM_ALL_LAYERS_ONE_PAGE
            settings = request.plot_settings
            settings.layers.extend(layer(n) for n in names)
            settings.plot_drawing_sheet = frame
            settings.plot_reference_designators = True
            settings.plot_footprint_values = True
            settings.scale = 1.0
            response = self.send(request, jobs.RunJobResponse)
            outputs = [Path(p) for p in response.output_path] or [target]
            produced = next((p for p in outputs if p.is_file()), None)
            if response.status != jobs.JS_SUCCESS or produced is None:
                raise RuntimeError(f"Board render failed: {_dict(response)}")
            return produced.read_bytes()

    def board_render_3d(self, doc, side="top", width=1200, height=900, zoom=1.0, quality="basic",
                        rotation=None):
        sides = {"top": board_jobs.RS_TOP, "bottom": board_jobs.RS_BOTTOM, "front": board_jobs.RS_FRONT,
                 "back": board_jobs.RS_BACK, "left": board_jobs.RS_LEFT, "right": board_jobs.RS_RIGHT}
        qualities = {"basic": board_jobs.RQ_BASIC, "high": board_jobs.RQ_HIGH}
        if side not in sides or quality not in qualities:
            raise ValueError(f"side must be one of {list(sides)}; quality basic or high")
        with self.lock, tempfile.TemporaryDirectory(prefix="kicad-live-3d-") as directory:
            target = Path(directory) / "board3d.png"
            request = board_jobs.RunBoardJobExportRender(
                format=board_jobs.RF_PNG, quality=qualities[quality], side=sides[side], width=width,
                height=height, zoom=zoom, background_style=board_jobs.RBS_OPAQUE, floor=False,
                use_board_stackup_colors=False, anti_alias=True)
            if rotation:
                request.rotation.CopyFrom(types.Vector3D(x_nm=rotation[0], y_nm=rotation[1], z_nm=rotation[2]))
            request.job_settings.document.CopyFrom(doc)
            request.job_settings.output_path = str(target)
            response = self.send(request, jobs.RunJobResponse)
            if response.status != jobs.JS_SUCCESS or not target.is_file():
                raise RuntimeError(f"3D render failed: {_dict(response)}")
            return target.read_bytes()

    def board_export(self, doc, kind, output_path):
        """Fabrication and other outputs from the live board (unsaved edits included)."""
        classes = {"gerbers": board_jobs.RunBoardJobExportGerbers, "drill": board_jobs.RunBoardJobExportDrill,
                   "position": board_jobs.RunBoardJobExportPosition, "step": board_jobs.RunBoardJobExport3D,
                   "pdf": board_jobs.RunBoardJobExportPdf, "svg": board_jobs.RunBoardJobExportSvg,
                   "dxf": board_jobs.RunBoardJobExportDxf, "ipc2581": board_jobs.RunBoardJobExportIpc2581,
                   "ipcd356": board_jobs.RunBoardJobExportIpcD356, "odb": board_jobs.RunBoardJobExportODB,
                   "stats": board_jobs.RunBoardJobExportStats, "dsn": board_jobs.RunBoardJobExportSpecctraDsn,
                   "gencad": board_jobs.RunBoardJobExportGencad, "idf": board_jobs.RunBoardJobExportIdf}
        if kind not in classes:
            raise ValueError(f"kind must be one of {sorted(classes)}")
        request = classes[kind]()
        request.job_settings.document.CopyFrom(doc)
        request.job_settings.output_path = str(output_path)
        if kind == "gerbers":
            request.use_board_plot_params = True
            request.create_gerber_job_file = True
        elif kind == "step":
            request.format = board_jobs.B3D_STEP
            request.overwrite = True
            request.export_board_body = True
            request.export_components = True
            request.include_unspecified = True
            request.substitute_models = True
        elif kind in ("pdf", "svg", "dxf"):
            request.plot_settings.layers.extend([bt.BL_F_Cu, bt.BL_B_Cu, bt.BL_F_SilkS, bt.BL_Edge_Cuts])
            request.plot_settings.plot_drawing_sheet = True
        response = self.send(request, jobs.RunJobResponse)
        if response.status != jobs.JS_SUCCESS:
            raise RuntimeError(f"Export failed: {response.message}")
        return {"outputs": list(response.output_path), "message": response.message}

    def board_select(self, doc, ids=None, references=None, add=False):
        ids = list(ids or [])
        if references:
            ids += [self.board_footprint(doc, r).id.value for r in references]
        if not add:
            self.send(editor.ClearSelection(header=types.ItemHeader(document=doc)), empty_pb2.Empty)
        if not ids:
            return {"selected": 0}
        request = editor.AddToSelection(header=types.ItemHeader(document=doc))
        request.items.extend(types.KIID(value=v) for v in ids)
        response = self.send(request, editor.SelectionResponse)
        return {"selected": len(response.items)}

    def board_focus(self, doc, ids=None, references=None):
        ids = list(ids or [])
        if references:
            ids += [self.board_footprint(doc, r).id.value for r in references]
        request = editor.FocusOnItems(document=doc)
        request.items.extend(types.KIID(value=v) for v in ids)
        self.send(request, empty_pb2.Empty)
        return {"focused": len(ids)}


def grid_dict(settings):
    return {"size_mm": _mm(settings.size), "preset_index": settings.preset_index,
            "presets_mm": [_mm(p.size) for p in settings.presets], "visible": settings.visible,
            "style": editor.GridStyle.Name(settings.style).removeprefix("GST_").lower(),
            "snapping": editor.GridSnapping.Name(settings.snapping).removeprefix("GSN_").lower(),
            "origin_mm": _mm(settings.origin)}


def parse_board_items(values):
    """Accept protobuf-JSON Any objects of board item types."""
    from google.protobuf import any_pb2
    result = []
    for value in values:
        packed = json_format.ParseDict(value, any_pb2.Any())
        name = packed.TypeName().removeprefix("kiapi.board.types.")
        descriptor = bt.DESCRIPTOR.message_types_by_name.get(name)
        if descriptor is None or packed.TypeName() != descriptor.full_name:
            raise ValueError("Only kiapi.board.types item types are accepted")
        message = getattr(bt, name)()
        if not packed.Unpack(message):
            raise ValueError("Invalid board item")
        result.append(message)
    return result
