# SPDX-License-Identifier: GPL-3.0-or-later
"""Protocol, rollback and rendering contracts for the live schematic companion."""

from pathlib import Path
import math
import sys
import uuid

import pytest

from client import (LiveSchematic, as_dict, base, document, editor, jobs,
                    parse_items, point, sch, sch_jobs, types)
from client import commands as sch_cmds
from client import enums
from fake_endpoints import unused_socket
from board import board_rules_pb2 as rules_pb2
from common.types import project_settings_pb2 as ps_pb2


def doc():
    return types.DocumentSpecifier(
        type=types.DOCTYPE_SCHEMATIC,
        project=types.ProjectSpecifier(name="test", path="/tmp/test/"),
        sheet_path=types.SheetPath(path=[types.KIID(value=str(uuid.uuid4()))]),
    )


def wire():
    return sch.SchematicLine(id=types.KIID(value=str(uuid.uuid4())),
                             start=point(10, 20), end=point(30, 20), type=sch.SLT_WIRE)


class FakeClient(LiveSchematic):
    def __init__(self, reply=None):
        super().__init__(unused_socket("unused-test-socket"))
        self.requests = []
        self.reply = reply

    def send(self, request, response_type):
        self.requests.append(request)
        if isinstance(request, editor.BeginCommit):
            return editor.BeginCommitResponse(id=types.KIID(value="commit"))
        if isinstance(request, editor.EndCommit):
            return editor.EndCommitResponse()
        return self.reply(request, response_type)


def test_item_roundtrip_preserves_sheet_pins_and_fields():
    sheet = sch.SheetSymbol(id=types.KIID(value=str(uuid.uuid4())), page_number="7")
    sheet.name_field.text.text = "Power"
    sheet.filename_field.text.text = "power.kicad_sch"
    sheet.pins.add(text=types.Text(text="VIN"), side=sch.SHS_LEFT, shape=sch.SLSH_INPUT)
    from google.protobuf.any_pb2 import Any
    packed = Any()
    packed.Pack(sheet)
    assert parse_items([as_dict(packed)])[0] == sheet


def test_rejects_non_schematic_payload():
    from google.protobuf.any_pb2 import Any
    packed = Any()
    packed.Pack(types.Vector2(x_nm=10))
    with pytest.raises(ValueError, match="schematic"):
        parse_items([as_dict(packed)])


@pytest.mark.parametrize("partial", [{}, {"type": "DOCTYPE_PCB"},
                                     {"type": "DOCTYPE_SCHEMATIC", "project": {"name": "test"}}])
def test_document_requires_explicit_sheet_and_project(partial):
    with pytest.raises(ValueError):
        document(partial)


def test_coordinates_use_nanometers():
    assert point(2.54, -1.27) == types.Vector2(x_nm=2540000, y_nm=-1270000)


@pytest.mark.parametrize("value", [float("nan"), float("inf"), 10001])
def test_invalid_coordinates_fail_before_contacting_editor(value):
    with pytest.raises(ValueError):
        point(value, 0)


def test_failed_batch_drops_all_changes():
    def reply(request, response_type):
        response = editor.CreateItemsResponse(status=types.IRS_OK)
        response.created_items.add(status=editor.ItemStatus(code=editor.ISC_OK))
        response.created_items.add(status=editor.ItemStatus(code=editor.ISC_INVALID_DATA,
                                                            error_message="invalid sheet"))
        return response
    client = FakeClient(reply)
    with pytest.raises(RuntimeError, match="invalid sheet"):
        client.mutate(doc(), [wire(), wire()])
    assert client.requests[-1].action == editor.CMA_DROP
    assert not any(isinstance(r, editor.EndCommit) and r.action == editor.CMA_COMMIT
                   for r in client.requests)


def test_success_commits_without_save():
    def reply(request, response_type):
        response = editor.CreateItemsResponse(status=types.IRS_OK)
        for item in request.items:
            response.created_items.add(status=editor.ItemStatus(code=editor.ISC_OK), item=item)
        return response
    client = FakeClient(reply)
    result = client.mutate(doc(), [wire()])
    assert result["status"] == "IRS_OK"
    assert [type(r).__name__ for r in client.requests] == ["BeginCommit", "CreateItems", "EndCommit"]
    assert client.requests[-1].action == editor.CMA_COMMIT


def returned_symbol():
    """A placed resistor as GetItems returns it, with its embedded library definition and pins."""
    def field(target, name, text, angle=0.0, visible=True):
        target.name, target.visible, target.allow_auto_place = name, visible, True
        target.text.text = text
        target.text.attributes.angle.value_degrees = angle
        target.text.attributes.stroke_width.SetInParent()
        target.text.attributes.size.x_nm = target.text.attributes.size.y_nm = 1270000
        target.text.position.SetInParent()

    symbol = sch.SchematicSymbolInstance(id=types.KIID(value=str(uuid.uuid4())), position=point(86.36, 76.2),
                                         path=doc().sheet_path)
    symbol.transform.orientation = sch.SSO_90
    symbol.unit.unit = 1
    definition = symbol.definition
    definition.id.library_nickname, definition.id.entry_name = "Device", "R"
    definition.unit_count = 1
    definition.body_style.add(name="?")
    definition.keywords = "R res resistor"
    definition.footprint_filters.append("R_*")
    definition.pin_maps.SetInParent()
    for target, name, text in ((definition.reference_field, "Reference", "R"),
                               (definition.value_field, "Value", "R"),
                               (definition.footprint_field, "Footprint", ""),
                               (definition.datasheet_field, "Datasheet", "~"),
                               (definition.description_field, "Description", "Resistor")):
        field(target, name, text, 90.0, name in ("Reference", "Value"))
    body = definition.items.add()
    body.unit.unit, body.body_style.style = 1, 1
    body.item.Pack(sch.SchematicGraphicShape(id=types.KIID(value=str(uuid.uuid4()))))
    for number, y in (("1", 3.81), ("2", -3.81)):
        child = definition.items.add()
        child.unit.unit, child.body_style.style = 1, 1
        child.item.Pack(sch.SchematicPin(id=types.KIID(value=str(uuid.uuid4())), name="~", number=number,
                                         position=point(0, y), length=types.Distance(value_nm=1270000)))
    field(symbol.reference_field, "Reference", "R1", 90.0)
    field(symbol.value_field, "Value", "R", 90.0)
    field(symbol.footprint_field, "Footprint", "", visible=False)
    return symbol


def test_full_symbol_update_returns_the_definition_unchanged():
    """KiCad keeps the embedded library symbol only when the definition comes back as it was read.

    Editing instance fields through the MCP's JSON form must therefore leave the definition
    identical, or the editor rebuilds it and it can stop matching its library.
    """
    import json
    from google.protobuf.any_pb2 import Any
    original = returned_symbol()
    packed = Any()
    packed.Pack(original)
    edited = json.loads(json.dumps(as_dict(packed)))   # what an MCP client holds and sends back
    edited["value_field"]["text"]["text"] = "4k7"
    edited["footprint_field"]["text"]["text"] = "Resistor_SMD:R_0603_1608Metric"
    edited.setdefault("user_fields", []).append(
        dict(edited["value_field"], name="MPN", visible=False, text=dict(edited["value_field"]["text"], text="X1")))

    def reply(request, response_type):
        response = editor.UpdateItemsResponse(status=types.IRS_OK)
        for item in request.items:
            response.updated_items.add(status=editor.ItemStatus(code=editor.ISC_OK), item=item)
        return response
    client = FakeClient(reply)
    client.mutate(doc(), parse_items([edited]), update=True)
    sent = sch.SchematicSymbolInstance()
    assert next(r for r in client.requests if isinstance(r, editor.UpdateItems)).items[0].Unpack(sent)
    assert sent.definition == original.definition
    assert sent.definition.SerializeToString(deterministic=True) == \
        original.definition.SerializeToString(deterministic=True)
    assert sent.value_field.text.text == "4k7" and [f.name for f in sent.user_fields] == ["MPN"]
    assert sent.footprint_field.text.text == "Resistor_SMD:R_0603_1608Metric"


def test_connection_failure_rolls_back_without_replaying_edit():
    def reply(request, response_type):
        raise TimeoutError("reply lost")
    client = FakeClient(reply)
    with pytest.raises(TimeoutError):
        client.mutate(doc(), [wire()])
    assert sum(isinstance(r, editor.CreateItems) for r in client.requests) == 1
    assert client.requests[-1].action == editor.CMA_DROP


def test_render_targets_live_sheet_and_only_writes_temporary_image():
    output = []
    target_doc = doc()
    def reply(request, response_type):
        assert isinstance(request, sch_jobs.RunSchematicJobExportPng)
        assert request.job_settings.document == target_doc
        assert request.plot_settings.sheet_mode == sch_jobs.SJSM_SINGLE_SHEET
        path = Path(request.job_settings.output_path)
        output.append(path)
        path.write_bytes(b"PNG fixture")
        return jobs.RunJobResponse(status=jobs.JS_SUCCESS, output_path=[str(path)])
    client = FakeClient(reply)
    assert client.render(target_doc) == b"PNG fixture"
    assert len(client.requests) == 1
    assert not output[0].exists()


def test_place_symbol_commits_and_returns_compact_summary():
    from common.commands import library_commands_pb2 as lib
    def reply(request, response_type):
        assert request.lib_id.library_nickname == "Device" and request.lib_id.entry_name == "R"
        assert request.position == point(60, 60) and request.orientation == sch.SSO_90
        placed = sch.SchematicSymbolInstance(id=types.KIID(value="sym"), position=request.position)
        placed.reference_field.text.text = "R1"
        placed.value_field.text.text = "10k"
        placed.unit.unit = 1
        response = lib.PlaceFromLibraryResponse()
        response.item.Pack(placed)
        return response
    client = FakeClient(reply)
    result = client.place_symbol(doc(), "Device:R", 60, 60, rotation=90)
    assert result == {"id": "sym", "lib_id": "Device:R", "reference": "R1", "value": "10k",
                      "position_mm": [60.0, 60.0], "unit": 1}
    assert [type(r).__name__ for r in client.requests] == ["BeginCommit", "PlaceSymbolFromLibrary",
                                                           "EndCommit"]
    assert client.requests[-1].action == editor.CMA_COMMIT


def test_every_registered_command_resolves_to_real_messages():
    import commands
    for name, (reply, kind) in commands.COMMANDS.items():
        assert commands.message_class(name)().DESCRIPTOR.full_name == name
        assert commands.message_class(reply)().DESCRIPTOR.full_name == reply
        assert kind in {"read", "edit", "view", "file", "job"}
    assert commands.resolve("TransformItems").endswith("schematic.commands.TransformItems")
    with pytest.raises(ValueError):
        commands.resolve("NoSuchCommand")


def pins_reply(*pins):
    response = sch_cmds.SymbolPinsResponse()
    for reference, number, x, y, orientation in pins:
        symbol = response.symbols.add(reference=reference, unit=1)
        symbol.symbol.value = str(uuid.uuid4())
        symbol.pins.add(id=types.KIID(value=str(uuid.uuid4())), number=number, position=point(x, y),
                        orientation=orientation)
    return response


def test_connect_pins_routes_orthogonally_from_the_first_pin_axis():
    created = []
    def reply(request, response_type):
        if isinstance(request, sch_cmds.GetSymbolPins):
            return pins_reply(("R1", "2", 10, 10, sch.SPO_LEFT), ("R2", "1", 30, 25, sch.SPO_RIGHT))
        created.extend(request.items)
        response = editor.CreateItemsResponse(status=types.IRS_OK)
        for item in request.items:
            response.created_items.add(status=editor.ItemStatus(code=editor.ISC_OK), item=item)
        return response
    result = FakeClient(reply).connect_pins(doc(), "R1", "2", "R2", "1")
    assert result["path_mm"] == [[10.0, 10.0], [30.0, 10.0], [30.0, 25.0]]
    assert len(created) == 2


def test_connect_pin_to_net_leaves_away_from_the_body():
    created = []
    def reply(request, response_type):
        if isinstance(request, sch_cmds.GetSymbolPins):
            # SPO_RIGHT: the pin runs right from its end toward the body, so the wire leaves leftward
            return pins_reply(("U1", "3", 50, 40, sch.SPO_RIGHT))
        created.extend(request.items)
        response = editor.CreateItemsResponse(status=types.IRS_OK)
        for item in request.items:
            response.created_items.add(status=editor.ItemStatus(code=editor.ISC_OK), item=item)
        return response
    result = FakeClient(reply).connect_pin_to_net(doc(), "U1", "3", "VCC", stub_mm=5)
    assert result["label_position_mm"] == [45.0, 40.0]
    label = sch.LocalLabel()
    assert created[1].Unpack(label) and label.text.text == "VCC" and label.spin_style == sch.SLSS_LEFT


def test_batch_drops_everything_when_a_step_is_rejected():
    def reply(request, response_type):
        response = editor.CreateItemsResponse(status=types.IRS_OK)
        response.created_items.add(status=editor.ItemStatus(code=editor.ISC_INVALID_DATA,
                                                            error_message="bad wire"))
        return response
    client = FakeClient(reply)
    packed = {"@type": "type.googleapis.com/kiapi.schematic.types.SchematicLine"}
    steps = [{"command": "CreateItems", "payload": {"items": [packed]}}]
    with pytest.raises(RuntimeError, match="bad wire"):
        client.batch(doc(), steps)
    assert client.requests[-1].action == editor.CMA_DROP


def test_batch_refuses_undo_and_file_commands_before_starting():
    client = FakeClient(lambda *_: pytest.fail("nothing should be sent"))
    for command in ("UndoLastChange", "SaveDocument", "RunSchematicJobExportPdf"):
        with pytest.raises(ValueError, match="cannot run inside a batch"):
            client.batch(doc(), [{"command": command, "payload": {}}])
    assert client.requests == []


def test_undo_is_sent_outside_any_commit():
    client = FakeClient(lambda request, _: editor.UndoRedoResponse(performed=True, undo_count=2))
    assert client.undo(doc())["performed"] is True
    assert [type(r).__name__ for r in client.requests] == ["UndoLastChange"]


def test_transform_maps_rotation_and_reports_dragged_items():
    def reply(request, response_type):
        assert request.rotation == sch_cmds.IR_CCW_90 and request.drag_wires
        assert request.translation == point(2.54, 0)
        response = sch_cmds.TransformItemsResponse()
        response.dragged_items.add(value="w1")
        return response
    client = FakeClient(reply)
    result = client.transform(doc(), [str(uuid.uuid4())], dx=2.54, rotation=90)
    assert result == {"moved": 1, "dragged": ["w1"]}
    assert client.requests[-1].action == editor.CMA_COMMIT
    with pytest.raises(ValueError):
        client.transform(doc(), [str(uuid.uuid4())], rotation=45)


def test_box_layout_keeps_pins_on_grid_and_outside_the_body():
    from symbol_layout import layout_box
    pins = [{"number": "1", "name": "GND", "type": "power_in"},
            {"number": "2", "name": "VOUT", "type": "power_out"},
            {"number": "3", "name": "VIN", "type": "power_in"},
            {"number": "4", "name": "EN", "type": "input"},
            {"number": "5", "name": "ADJ", "type": "passive"}]
    layout = layout_box(pins)
    x1, y1, x2, y2 = layout["body"]
    pin = {p["name"]: p for p in layout["pins"]}
    for item in layout["pins"]:
        x, y = item["position"]
        assert abs(x / 2.54 - round(x / 2.54)) < 1e-6 and abs(y / 2.54 - round(y / 2.54)) < 1e-6
        assert not (x1 < x < x2 and y1 < y < y2), f"pin {item['number']} sits inside the body"
    assert pin["GND"]["side"] == "bottom" and pin["GND"]["orientation"] == "SPO_UP"
    assert pin["VIN"]["orientation"] == "SPO_RIGHT"


def test_ldo_layout_follows_datasheet_convention():
    """VIN then EN on the left, VOUT then ADJ on the right, a resistor's length apart."""
    from symbol_layout import layout_box
    pins = [{"number": "1", "name": "GND", "type": "power_in"}, {"number": "2", "name": "VOUT", "type": "power_out"},
            {"number": "3", "name": "VIN", "type": "power_in"}, {"number": "4", "name": "EN", "type": "input"},
            {"number": "5", "name": "ADJ", "type": "passive"}]
    pin = {p["name"]: p for p in layout_box(pins)["pins"]}
    assert (pin["VIN"]["side"], pin["EN"]["side"]) == ("left", "left")
    assert (pin["VOUT"]["side"], pin["ADJ"]["side"]) == ("right", "right")
    assert round(pin["EN"]["position"][1] - pin["VIN"]["position"][1], 4) == 7.62
    assert round(pin["ADJ"]["position"][1] - pin["VOUT"]["position"][1], 4) == 7.62
    assert pin["VIN"]["position"][1] == pin["VOUT"]["position"][1], "input and output share the top row"
    tight = {p["name"]: p for p in layout_box(pins, group_gap=0)["pins"]}
    assert round(tight["EN"]["position"][1] - tight["VIN"]["position"][1], 4) == 2.54
    exact = {p["name"]: p for p in layout_box(
        [{"number": "4", "name": "EN", "type": "input", "side": "left"},
         {"number": "3", "name": "VIN", "type": "power_in", "side": "left"}], arrange="as_given")["pins"]}
    assert exact["EN"]["position"][1] < exact["VIN"]["position"][1], "as_given keeps the caller's order"


def test_nc_pins_are_hidden_no_connects_that_take_no_row():
    from symbol_layout import layout_box
    base = [{"number": "1", "name": "GND", "type": "power_in"}, {"number": "2", "name": "VOUT", "type": "power_out"},
            {"number": "3", "name": "VIN", "type": "power_in"}, {"number": "4", "name": "EN", "type": "input"},
            {"number": "5", "name": "ADJ", "type": "passive"}]
    plain = layout_box(base)
    with_nc = layout_box(base + [{"number": "6", "name": "NC", "type": "passive"}])
    assert with_nc["body"] == plain["body"], "a hidden NC pin must not grow the body"
    nc = next(p for p in with_nc["pins"] if p["number"] == "6")
    assert nc["type"] == "no_connect" and nc["electrical_type"] == "EPT_NO_CONNECT" and nc["hidden"]
    x1, y1, x2, y2 = with_nc["body"]
    x, y = nc["position"]
    assert x1 < x < x2 and y1 < y < y2, "hidden pins sit inside the body"
    taken = {p["position"][1] for p in with_nc["pins"] if p["side"] == "right"}
    assert y not in taken, "a hidden pin uses a row no visible right-side pin uses"
    shown = layout_box(base + [{"number": "6", "name": "NC", "hidden": False}])
    assert next(p for p in shown["pins"] if p["number"] == "6")["side"] in ("left", "right")


@pytest.mark.parametrize("name,side", [("~{SHDN}", "left"), ("FB", "right"), ("PGND", "bottom"),
                                       ("PG", "right"), ("VCC", "left")])
def test_default_sides_by_pin_role(name, side):
    from symbol_layout import default_side
    kind = "power_in" if name in ("PGND", "VCC") else "input"
    assert default_side({"name": name, "type": kind}) == side


def test_box_layout_centres_even_pin_counts_on_grid():
    from symbol_layout import layout_box
    layout = layout_box([{"number": str(n), "name": f"P{n}", "side": "top"} for n in range(1, 5)])
    xs = sorted(p["position"][0] for p in layout["pins"])
    assert xs == [-5.08, -2.54, 2.54, 5.08]


@pytest.mark.parametrize("bad", [[], [{"number": "1"}, {"number": "1"}], [{"number": "1", "type": "magic"}],
                                 [{"number": "1", "side": "middle"}]])
def test_box_layout_rejects_bad_pin_lists(bad):
    from symbol_layout import layout_box
    with pytest.raises(ValueError):
        layout_box(bad)


def symbol_doc():
    return types.DocumentSpecifier(type=types.DOCTYPE_SYMBOL,
                                   lib_id=types.LibraryIdentifier(library_nickname="mylib", entry_name="REG"))


def test_symbol_add_pins_sends_body_and_pins_in_one_commit():
    sent = []
    def reply(request, response_type):
        if not isinstance(request, editor.CreateItems):
            return response_type()
        sent.append(request)
        response = editor.CreateItemsResponse(status=types.IRS_OK)
        for item in request.items:
            response.created_items.add(status=editor.ItemStatus(code=editor.ISC_OK), item=item)
        return response
    client = FakeClient(reply)
    result = client.symbol_add_pins(symbol_doc(), [{"number": "1", "name": "A"}, {"number": "2", "name": "B",
                                                                                  "type": "output"}])
    assert [type(r).__name__ for r in client.requests] == ["BeginCommit", "CreateItems",
                                                           "SetLibrarySymbolProperties", "EndCommit"]
    assert client.requests[2].place_fields_around_body
    kinds = [i.TypeName().rsplit(".", 1)[-1] for i in sent[0].items]
    assert kinds == ["SchematicGraphicShape", "SchematicPin", "SchematicPin"]
    pin = sch.SchematicPin()
    sent[0].items[2].Unpack(pin)
    assert pin.number == "2" and pin.electrical_type == types.EPT_OUTPUT and pin.length.value_nm == 2540000
    assert pin.name_text_size.value_nm == pin.number_text_size.value_nm == 1270000
    body = sch.SchematicGraphicShape()
    sent[0].items[0].Unpack(body)
    stroke = body.shape.attributes.stroke
    assert stroke.width.value_nm == 254000 and stroke.style == enums.SLS_DEFAULT and not stroke.HasField("color")
    assert body.shape.attributes.fill.fill_type == types.GFT_FILLED_WITH_BACKGROUND_BODY_COLOR
    assert [p["side"] for p in result["pins"]] == ["left", "right"]


def test_library_errors_are_raised_not_returned():
    from client import lib_types
    status = lib_types.LibraryCommandStatus(code=lib_types.LibraryCommandStatus.LCS_ALREADY_EXISTS,
                                            error_message="Library 'mylib' is already in the table")
    client = FakeClient(lambda *_: status)
    with pytest.raises(RuntimeError, match="ALREADY_EXISTS"):
        client.add_library("mylib", "/tmp/mylib.kicad_sym")
    with pytest.raises(ValueError):
        client.add_library("mylib", "/tmp/mylib.txt")


def test_symbol_documents_are_validated():
    from client import symbol_document
    assert symbol_document(as_dict(symbol_doc())).lib_id.entry_name == "REG"
    with pytest.raises(ValueError):
        symbol_document(as_dict(doc()))


def test_installed_kicad10_is_rejected_clearly():
    client = FakeClient(lambda *_: base.GetVersionResponse(version=types.KiCadVersion(major=10, minor=0, patch=6)))
    with pytest.raises(RuntimeError, match="development build"):
        client.documents()


def test_server_exposes_valid_tool_schemas():
    import asyncio
    from server import mcp
    tools = asyncio.run(mcp.list_tools())
    names = {tool.name for tool in tools}
    assert {"live_schematic_view", "live_schematic_create_items", "live_schematic_hierarchy"} <= names
    assert all(tool.inputSchema.get("type") == "object" for tool in tools)


# ---- Footprints -------------------------------------------------------------------------------

FOOTPRINT_CASES = [
    ("soic", dict(pins=8, pitch=1.27, body_width=3.9, body_length=4.9, pad_length=1.95, pad_width=0.6,
                  row_pitch=4.95)),
    ("soic", dict(pins=14, pitch=1.27, body_width=3.9, body_length=8.65, lead_span=6.0, lead_length=0.835,
                  lead_width=0.41)),
    ("qfn", dict(pins_x=4, pins_y=4, pitch=0.5, body_x=3.0, body_y=3.0, terminal_length=0.4,
                 terminal_width=0.25, exposed_pad=(1.7, 1.7))),
    ("dfn", dict(pins_y=4, pitch=0.65, body_x=3.0, body_y=3.0, terminal_length=0.4, terminal_width=0.3)),
    ("sot", dict(variant="23")), ("sot", dict(variant="23-5")), ("sot", dict(variant="223")),
    ("chip", dict(size="0603")), ("pin_header", dict(pins_per_row=4, rows=2)),
]


def _box(pad):
    (x, y), (w, h) = pad["position"], pad["size"]
    return x - w / 2, y - h / 2, x + w / 2, y + h / 2


def _segment_gap(segment, box):
    """Shortest distance from an axis-aligned segment to a rectangle."""
    (x1, y1), (x2, y2) = segment
    bx1, by1, bx2, by2 = box
    dx = max(bx1 - max(x1, x2), min(x1, x2) - bx2, 0)
    dy = max(by1 - max(y1, y2), min(y1, y2) - by2, 0)
    return (dx * dx + dy * dy) ** 0.5


@pytest.mark.parametrize("package,params", FOOTPRINT_CASES)
def test_generated_footprints_follow_klc(package, params):
    from footprint_gen import SILK_PAD_CLEARANCE, SILK_WIDTH, generate
    shape = generate(package, **params)
    pads = shape["pads"]
    numbers = [p["number"] for p in pads]
    assert numbers == [str(n) for n in range(1, len(pads) + 1)], "pads are numbered 1..n"
    cx1, cy1, cx2, cy2 = shape["courtyard"]
    for value in shape["courtyard"]:
        assert abs(value * 100 - round(value * 100)) < 1e-6, "courtyard on the 0.01 mm grid"
    clearance = 0.5 if package == "pin_header" else 0.25
    bx1, by1, bx2, by2 = shape["body"]
    for x1, y1, x2, y2 in [_box(p) for p in pads] + [(bx1, by1, bx2, by2)]:
        assert cx1 <= x1 - clearance + 1e-6 and cy1 <= y1 - clearance + 1e-6
        assert cx2 >= x2 + clearance - 1e-6 and cy2 >= y2 + clearance - 1e-6
    for segment in shape["silk_lines"]:
        for pad in pads:
            assert _segment_gap(segment, _box(pad)) >= SILK_PAD_CLEARANCE + SILK_WIDTH / 2 - 1e-6, \
                f"silk {segment} too close to pad {pad['number']}"
    if shape["pin1_marker"]:
        for pad in pads:
            x1, y1, x2, y2 = _box(pad)
            assert all(not (x1 < x < x2 and y1 < y < y2) for x, y in shape["pin1_marker"]), "marker off copper"
    for a, b in zip(pads, pads[1:]):
        if a["type"] == b["type"] == "smd":
            ax1, ay1, ax2, ay2 = _box(a)
            bx1_, by1_, bx2_, by2_ = _box(b)
            assert ax2 <= bx1_ or bx2_ <= ax1 or ay2 <= by1_ or by2_ <= ay1, "pads do not overlap"


def test_soic8_land_pattern_matches_stock_footprint():
    """Package_SO:SOIC-8_3.9x4.9mm_P1.27mm: pads 1.95 x 0.6 at x = +/-2.475, pin 1 top left, CCW."""
    from footprint_gen import generate
    shape = generate("soic", pins=8, pitch=1.27, body_width=3.9, body_length=4.9, pad_length=1.95,
                     pad_width=0.6, row_pitch=4.95)
    at = {p["number"]: p["position"] for p in shape["pads"]}
    assert at["1"] == (-2.475, -1.905) and at["4"] == (-2.475, 1.905)
    assert at["5"] == (2.475, 1.905) and at["8"] == (2.475, -1.905)
    assert all(p["size"] == (1.95, 0.6) for p in shape["pads"])


def test_pad_messages_use_klc_layers_and_corners():
    from footprint_client import ALL_COPPER, bt, make_pad
    smd = make_pad({"number": "1", "type": "smd", "shape": "roundrect", "position": (1, 2), "size": (1.95, 0.6)})
    assert list(smd.pad_stack.layers) == [bt.BL_F_Cu, bt.BL_F_Paste, bt.BL_F_Mask] and smd.type == bt.PT_SMD
    assert abs(smd.pad_stack.copper_layers[0].corner_rounding_ratio - 0.25) < 1e-9
    big = make_pad({"number": "9", "type": "smd", "shape": "roundrect", "position": (0, 0), "size": (3, 3)})
    assert abs(big.pad_stack.copper_layers[0].corner_rounding_ratio - 0.25 / 3) < 1e-9, "radius capped at 0.25 mm"
    tht = make_pad({"number": "1", "type": "pth", "shape": "rect", "position": (0, 0), "size": (1.7, 1.7),
                    "drill": 1.0})
    assert tht.type == bt.PT_PTH and set(ALL_COPPER) <= set(tht.pad_stack.layers)
    assert tht.pad_stack.drill.diameter.x_nm == 1_000_000


def test_attach_step_embeds_then_links_the_model():
    sent = []
    def reply(request, response_type):
        sent.append(request)
        if type(request).__name__ == "EmbedFileFromPath":
            response = response_type()
            response.file.name, response.file.uri = "part.step", "kicad-embed://part.step"
            return response
        return response_type()
    client = FakeClient(reply)
    doc_ = types.DocumentSpecifier(type=types.DOCTYPE_FOOTPRINT,
                                   lib_id=types.LibraryIdentifier(library_nickname="fp", entry_name="X"))
    result = client.footprint_attach_step(doc_, "/tmp/part.step", rotation=(0, 0, 90))
    assert result == {"embedded": "part.step", "model": "kicad-embed://part.step"}
    props = next(r for r in sent if type(r).__name__ == "SetLibraryFootprintProperties")
    assert props.set_models and props.models[0].filename == "kicad-embed://part.step"
    assert props.models[0].scale.x_nm == 1 and props.models[0].rotation.z_nm == 90
    with pytest.raises(ValueError):
        client.footprint_attach_step(doc_, "/tmp/datasheet.pdf")


# ---- SolidCraft model handoff -----------------------------------------------------------------

def test_paths_translate_between_windows_and_wsl():
    from model_handoff import to_windows, to_wsl
    assert to_wsl(r"C:\Users\me\kicad-models\a.step") == "/mnt/c/Users/me/kicad-models/a.step"
    assert to_windows("/mnt/d/models/a.step") == r"D:\models\a.step"
    assert to_wsl("/home/me/a.step") == "/home/me/a.step" and to_windows("relative.step") == "relative.step"


def test_model_plan_follows_kicad_3d_conventions(monkeypatch):
    from model_handoff import model_plan
    monkeypatch.setattr(sys, "platform", "linux")  # the WSL KiCad reads /mnt/c
    plan = model_plan("soic", dict(pins=8, pitch=1.27, body_width=3.9, body_length=4.9, pad_length=1.95,
                                   pad_width=0.6, row_pitch=4.95),
                      body_height=1.75, standoff=0.1, lead_width=0.41, lead_span=6.0, foot_length=0.835,
                      export_path="/mnt/c/Users/me/kicad-models/SOIC-8.step")
    steps = plan["solidcraft"]["arguments"]["commands"]
    assert plan["bodies"] == ["Body"] + [f"Lead{n}" for n in range(1, 9)]
    assert plan["export"]["arguments"] == {"path": r"C:\Users\me\kicad-models\SOIC-8.step", "format": "step"}
    assert plan["then"]["step_path"] == "/mnt/c/Users/me/kicad-models/SOIC-8.step"
    # The body rises from the standoff to the seated height
    body = next(s for s in steps if s["params"].get("body_name") == "Body")
    first_sketch = steps[0]["params"]
    assert first_sketch["offset"] == 0.1 and abs(body["params"]["distance"] - 1.65) < 1e-9
    # Pin 1 is at footprint y = -1.905, so its lead is sketched at model y = +1.905 (Y flips)
    lead_planes = [s["params"]["plane"] for s in steps if s["command"] == "sketch.create"
                   and isinstance(s["params"]["plane"], dict)]
    assert lead_planes[0]["origin"] == [0, 1.905, 0]
    # Every lead toe reaches the datasheet span, and touches the board
    lines = [s["params"]["points"] for s in steps if s["command"] == "sketch.line"]
    assert all(max(abs(x) for x, _ in pts) == 3.0 and min(z for _, z in pts) == 0.0 for pts in lines)
    dimple = [s for s in steps if s["command"] == "sketch.circle.center"][0]["params"]["center"]
    assert dimple[0] < 0 < dimple[1], "pin-1 dimple at model x<0, y>0"


def test_footprint_board_unwraps_kicad11_library_files():
    from model_handoff import footprint_board
    library_file = ('(kicad_footprint_lib (version 20261031) (generator "kicad_footprint_editor") '
                    '(generator_version "10.99")\n(footprint "X" (version 20261031) (generator "pcbnew")\n'
                    '  (layer "F.Cu")\n  (fp_text user "a(b)" (at 1 2))\n  (pad "1" smd rect (at -1 0) (size 1 1)))\n)')
    board = footprint_board(library_file, "lib:X")
    assert board.startswith("(kicad_pcb\n  (version 20261031)")
    assert '(footprint "lib:X"' in board and "kicad_footprint_lib" not in board and "(at 0 0)" in board
    assert 'Edge.Cuts' in board and board.count("(") == board.count(")")


def test_embedded_file_bytes_decodes_kicad_storage():
    import base64
    from model_handoff import embedded_file_bytes
    try:
        from compression import zstd  # Python 3.14+
        compress = zstd.compress
    except ImportError:
        zstandard = pytest.importorskip("zstandard", reason="needs compression.zstd or zstandard")
        compress = zstandard.ZstdCompressor().compress
    payload = b"ISO-10303-21;\nHEADER;\n" * 50
    encoded = base64.b64encode(compress(payload)).decode()
    text = f'(embedded_files (file (name "m.step") (type model) (data |{encoded[:40]}|\n |{encoded[40:]}|) (checksum "x")))'
    assert embedded_file_bytes(text, "m.step") == payload


# ---- Page, drawing sheet and title block ------------------------------------------------------

def test_view_draws_the_drawing_sheet_unless_asked_not_to():
    seen = []
    def reply(request, response_type):
        seen.append(request.plot_settings.plot_drawing_sheet)
        Path(request.job_settings.output_path).write_bytes(b"PNG")
        return jobs.RunJobResponse(status=jobs.JS_SUCCESS)
    client = FakeClient(reply)
    client.render(doc())
    client.render(doc(), frame=False)
    assert seen == [True, False]


def _page_box(x1, y1, x2, y2):
    return types.Box2(position=point(x1, y1), size=point(x2 - x1, y2 - y1))


def page_reply(problems=()):
    def reply(request, response_type):
        name = type(request).__name__
        if name == "GetPageLayout":
            response = editor.PageLayoutResponse(page_size=point(420, 297), drawing_area=_page_box(12, 12, 408, 285),
                                                 title_block=_page_box(298, 251, 408, 285),
                                                 content=_page_box(20, 20, 400, 270), item_count=42)
            for p in problems:
                response.problems.add(**p)
            return response
        if name == "GetPageSettings":
            return types.PageSettings(page_size=types.PS_A3, orientation=types.PO_LANDSCAPE)
        if name == "GetTitleBlockInfo":
            return types.TitleBlockInfo(title="EPL - Power", company="Embeint Pty Ltd", comment1="PoC")
        return response_type()
    return reply


def test_page_layout_reports_mm_and_flags_items_outside_the_frame():
    client = FakeClient(page_reply())
    page = client.page_layout(doc())
    assert page["paper"] == "A3" and page["orientation"] == "landscape" and page["page_size_mm"] == [420.0, 297.0]
    assert page["drawing_area_mm"] == [12.0, 12.0, 408.0, 285.0] and page["title_block_mm"] == [298.0, 251.0, 408.0, 285.0]
    assert page["fits"] and page["items"] == 42
    assert page["title_block"] == {"title": "EPL - Power", "date": "", "revision": "", "company": "Embeint Pty Ltd",
                                   "comments": ["PoC"]}
    bad = FakeClient(page_reply([dict(id=types.KIID(value="u1"), type="Symbol", name="U1",
                                      box=_page_box(380, 260, 420, 290), outside_drawing_area=True,
                                      overlaps_title_block=True)]))
    page = bad.page_layout(doc())
    assert not page["fits"]
    assert page["problems"] == [{"id": "u1", "type": "Symbol", "name": "U1", "box_mm": [380.0, 260.0, 420.0, 290.0],
                                 "outside_drawing_area": True, "overlaps_title_block": True}]


def test_set_page_changes_only_what_is_given():
    sent = []
    base = page_reply()
    def reply(request, response_type):
        sent.append(request)
        return base(request, response_type)
    client = FakeClient(reply)
    client.set_page(doc(), size="a4", orientation="portrait", revision="B", date="today")
    page = next(r for r in sent if type(r).__name__ == "SetPageSettings").page_settings
    assert page.page_size == types.PS_A4 and page.orientation == types.PO_PORTRAIT
    block = next(r for r in sent if type(r).__name__ == "SetTitleBlockInfo").title_block
    assert block.revision == "B" and block.title == "EPL - Power" and block.company == "Embeint Pty Ltd"
    assert block.comment1 == "PoC" and len(block.date) == 10
    sent.clear()
    client.set_page(doc(), title="EPL - MCU")
    assert not any(type(r).__name__ == "SetPageSettings" for r in sent), "paper untouched when not given"
    with pytest.raises(ValueError):
        client.set_page(doc(), size="B7")


# ---- PCB editor -------------------------------------------------------------------------------

from board_client import (bc, board_document, board_jobs, bt, layer, layer_name, rounded_rectangle,  # noqa: E402
                          route_points)


def board_doc():
    return types.DocumentSpecifier(type=types.DOCTYPE_PCB, board_filename="test.kicad_pcb",
                                   project=types.ProjectSpecifier(name="test", path="/tmp/test/"))


def footprint(reference, x, y, side="front", fid=None):
    fp = bt.FootprintInstance(id=types.KIID(value=fid or str(uuid.uuid4())), position=point(x, y),
                              layer=bt.BL_B_Cu if side == "back" else bt.BL_F_Cu)
    fp.reference_field.text.text.text = reference
    fp.definition.id.library_nickname = "Resistor_SMD"
    fp.definition.id.entry_name = "R_0603_1608Metric"
    return fp


def pad(parent, number, x, y, net):
    return bt.Pad(id=types.KIID(value=str(uuid.uuid4())), parent=types.KIID(value=parent.id.value),
                  number=number, position=point(x, y), net=bt.Net(name=net), type=bt.PT_SMD)


def items_reply(by_kind, extra=None):
    """Answer GetItems by requested type, everything else from extra (name -> response) or empty."""
    from google.protobuf.any_pb2 import Any
    kinds = {enums.KOT_PCB_FOOTPRINT: "footprints", enums.KOT_PCB_PAD: "pads", enums.KOT_PCB_SHAPE: "shapes",
             enums.KOT_PCB_TRACE: "tracks"}

    def reply(request, response_type):
        name = type(request).__name__
        if name == "GetItems":
            response = editor.GetItemsResponse(status=types.IRS_OK)
            for kind in request.types:
                for item in by_kind.get(kinds.get(kind), []):
                    packed = Any()
                    packed.Pack(item)
                    response.items.append(packed)
            return response
        if extra and name in extra:
            value = extra[name]
            return value(request) if callable(value) else value
        if name in ("CreateItems", "UpdateItems"):
            field = "created_items" if name == "CreateItems" else "updated_items"
            response = response_type(status=types.IRS_OK)
            for _ in request.items:
                getattr(response, field).add().status.code = editor.ISC_OK
            return response
        return response_type()
    return reply


def test_board_layer_names_follow_kicad():
    assert layer("F.Cu") == bt.BL_F_Cu and layer("In2.Cu") == bt.BL_In2_Cu and layer("Edge.Cuts") == bt.BL_Edge_Cuts
    assert layer("F.Silkscreen") == bt.BL_F_SilkS and layer("User.1") == bt.BL_User_1
    for value in (bt.BL_F_Cu, bt.BL_In1_Cu, bt.BL_Edge_Cuts, bt.BL_F_SilkS, bt.BL_Dwgs_User, bt.BL_User_3):
        assert layer(layer_name(value)) == value
    with pytest.raises(ValueError, match="Unknown layer"):
        layer("Top.Copper")


def test_board_documents_are_validated():
    assert board_document({"type": "DOCTYPE_PCB", "board_filename": "a.kicad_pcb"}).board_filename == "a.kicad_pcb"
    with pytest.raises(ValueError):
        board_document({"type": "DOCTYPE_SCHEMATIC"})


def test_route_points_use_45_degree_dog_legs():
    assert route_points((0, 0), (10, 0)) == [(0, 0), (10, 0)]
    assert route_points((0, 0), (5, 5)) == [(0, 0), (5, 5)]
    assert route_points((0, 0), (10, 4)) == [(0, 0), (6, 0), (10, 4)]
    assert route_points((0, 0), (10, 4), "diagonal_first") == [(0, 0), (4, 4), (10, 4)]
    assert route_points((0, 0), (-3, 10)) == [(0, 0), (0, 7), (-3, 10)]
    assert route_points((0, 0), (10, 4), "orthogonal") == [(0, 0), (10, 0), (10, 4)]


def test_rounded_outline_is_closed():
    items = rounded_rectangle(10, 20, 50, 30, 3, bt.BL_Edge_Cuts, 0.05)
    ends = []
    for item in items:
        shape = item.shape
        geom = shape.segment if shape.HasField("segment") else shape.arc
        ends += [(geom.start.x_nm, geom.start.y_nm), (geom.end.x_nm, geom.end.y_nm)]
    assert len(items) == 8 and all(ends.count(e) == 2 for e in ends), "every corner joins a line and an arc"
    with pytest.raises(ValueError):
        rounded_rectangle(0, 0, 10, 10, 6, bt.BL_Edge_Cuts, 0.05)


def test_move_footprints_transforms_in_place_in_one_commit():
    r1, r2 = footprint("R1", 10, 10), footprint("R2", 20, 10)
    client = FakeClient(items_reply({"footprints": [r1, r2]}, {
        "TransformBoardItems": lambda request: bc.TransformBoardItemsResponse(
            status=types.IRS_OK, items=[bc.ItemFlipResult(status=editor.ItemStatus(code=editor.ISC_OK))])}))
    result = client.board_move_footprints(board_doc(), [
        {"reference": "R1", "x_mm": 30, "y_mm": 40, "rotation": 90, "locked": True},
        {"reference": "R2", "x_mm": 5, "rotation": 45, "relative": True, "side": "back"}])
    names = [type(r).__name__ for r in client.requests]
    assert names[names.index("BeginCommit"):names.index("EndCommit") + 1] ==         ["BeginCommit", "TransformBoardItems", "TransformBoardItems", "EndCommit"]
    a, b = [r for r in client.requests if type(r).__name__ == "TransformBoardItems"]
    assert [i.value for i in a.items] == [r1.id.value] and a.position == point(30, 40)
    assert a.orientation.value_degrees == 90 and a.locked and not a.HasField("flip")
    assert not a.HasField("rotation") and not a.HasField("translation")
    assert b.translation == point(5, 0) and b.rotation.value_degrees == 45 and not b.HasField("orientation")
    assert b.flip == bc.BFD_LEFT_RIGHT and not b.HasField("position") and result["flipped"] == 1
    with pytest.raises(ValueError, match="No footprint R9"):
        client.board_move_footprints(board_doc(), [{"reference": "R9", "x_mm": 0}])


def test_route_follows_pads_net_and_netclass_width():
    u1, r1 = footprint("U1", 0, 0), footprint("R1", 10, 4)
    pads = [pad(u1, "3", 0, 0, "SDA"), pad(r1, "1", 10, 4, "SDA"), pad(r1, "2", 11, 4, "VCC")]
    from common.types import project_settings_pb2 as ps
    netclass = ps.NetClass(name="I2C", board=ps.NetClassBoardSettings(track_width=types.Distance(value_nm=200000)))
    client = FakeClient(items_reply({"footprints": [u1, r1], "pads": pads},
                                    {"GetNetClassForNets": bc.NetClassForNetsResponse(classes={"SDA": netclass})}))
    result = client.board_route(board_doc(), "U1", "3", "R1", "1", avoid=False)
    assert result["net"] == "SDA" and result["width_mm"] == 0.2 and result["layer"] == "F.Cu"
    assert result["path_mm"] == [[0.0, 0.0], [6.0, 0.0], [10.0, 4.0]]
    create = next(r for r in client.requests if type(r).__name__ == "CreateItems")
    tracks = [bt.Track() for _ in create.items]
    for message, packed in zip(tracks, create.items):
        packed.Unpack(message)
    assert all(t.net.name == "SDA" and t.width.value_nm == 200000 and t.layer == bt.BL_F_Cu for t in tracks)
    with pytest.raises(ValueError, match="SDA but R1.2 is on VCC"):
        client.board_route(board_doc(), "U1", "3", "R1", "2", avoid=False)


def test_outline_replaces_old_edge_cuts_in_one_commit():
    old = bt.BoardGraphicShape(id=types.KIID(value=str(uuid.uuid4())), layer=bt.BL_Edge_Cuts)
    silk = bt.BoardGraphicShape(id=types.KIID(value=str(uuid.uuid4())), layer=bt.BL_F_SilkS)
    client = FakeClient(items_reply({"shapes": [old, silk]}, {
        "GetBoardBoundingBox": bc.BoardBoundingBoxResponse(box=_page_box(10, 10, 60, 40))}))
    result = client.board_outline(board_doc(), 50, 30, 10, 10, corner_radius=2)
    delete = next(r for r in client.requests if type(r).__name__ == "DeleteItems")
    assert [i.value for i in delete.item_ids] == [old.id.value], "only the board-level Edge.Cuts shapes go"
    assert result["replaced"] == 1 and len(result["items"]) == 8 and result["outline_mm"] == [10, 10, 60, 40]


def test_grid_and_origins_requests():
    sent = []

    def reply(request, response_type):
        sent.append(request)
        if type(request).__name__ in ("SetGridSettings", "GetGridSettings"):
            return editor.GridSettings(size=point(0.5, 0.5), preset_index=3, visible=True, style=editor.GST_LINES,
                                       snapping=editor.GSN_ALWAYS, presets=[editor.GridPreset(size=point(0.5, 0.5))])
        if type(request).__name__ == "GetBoardOrigin":
            return point(100, 50) if request.type == bc.BOT_GRID else point(20, 30)
        return response_type()
    client = FakeClient(reply)
    grid = client.editor_grid(board_doc(), size_mm=0.5, style="lines")
    request = sent[0]
    assert request.HasField("size") and request.size == point(0.5, 0.5) and request.style == editor.GST_LINES
    assert not request.HasField("visible") and not request.HasField("preset_index"), "unset fields keep settings"
    assert grid["size_mm"] == [0.5, 0.5] and grid["style"] == "lines" and grid["snapping"] == "always"
    sent.clear()
    client.editor_grid(board_doc())
    assert [type(r).__name__ for r in sent] == ["GetGridSettings"]
    sent.clear()
    origins = client.board_origins(board_doc(), grid=(100, 50), drill=(20, 30))
    sets = [r for r in sent if type(r).__name__ == "SetBoardOrigin"]
    assert [(r.type, r.origin) for r in sets] == [(bc.BOT_GRID, point(100, 50)), (bc.BOT_DRILL, point(20, 30))]
    assert origins == {"grid_origin_mm": [100.0, 50.0], "drill_origin_mm": [20.0, 30.0]}
    with pytest.raises(ValueError):
        client.editor_grid(board_doc(), style="hatched")


def test_board_view_plots_layers_on_the_page():
    seen = []

    def reply(request, response_type):
        seen.append(request)
        Path(request.job_settings.output_path).write_bytes(b"PNG")
        return jobs.RunJobResponse(status=jobs.JS_SUCCESS)
    client = FakeClient(reply)
    assert client.board_render(board_doc(), ["F.Cu", "Edge.Cuts"]) == b"PNG"
    client.board_render(board_doc(), frame=False)
    assert isinstance(seen[0], board_jobs.RunBoardJobExportPng)
    assert list(seen[0].plot_settings.layers) == [bt.BL_F_Cu, bt.BL_Edge_Cuts]
    assert [r.plot_settings.plot_drawing_sheet for r in seen] == [True, False]


def test_board_page_reports_the_outline():
    base = page_reply()

    def reply(request, response_type):
        response = base(request, response_type)
        if type(request).__name__ == "GetPageLayout":
            response.board_outline.CopyFrom(_page_box(50, 50, 150, 120))
        return response
    page = FakeClient(reply).page_layout(board_doc())
    assert page["board_outline_mm"] == [50.0, 50.0, 150.0, 120.0] and page["fits"]


def test_bom_export_supplies_required_fields_and_csv_format(tmp_path):
    def reply(request, response_type):
        assert isinstance(request, sch_jobs.RunSchematicJobExportBOM)
        assert request.fields.fields and request.format.preset_name == "CSV"
        assert request.fields.sort_field == "Reference"
        fields = [(field.name, field.label) for field in request.fields.fields]
        assert fields == [(name, name) for name in ("Reference", "Value", "Footprint", "Datasheet")]
        return jobs.RunJobResponse(status=jobs.JS_SUCCESS, output_path=[request.job_settings.output_path])
    output = str(tmp_path / "bom.csv")
    client = FakeClient(reply)
    assert client.export(doc(), "bom", output) == {"outputs": [output]}
    assert len(client.requests) == 1


def test_update_from_schematic_exports_then_imports_the_netlist():
    seen = []

    def reply(request, response_type):
        seen.append(request)
        if type(request).__name__ == "RunSchematicJobExportNetlist":
            Path(request.job_settings.output_path).write_text("(export)")
            return jobs.RunJobResponse(status=jobs.JS_SUCCESS)
        return bc.ImportNetlistResponse(new_footprint_count=3, report="added R1")
    result = FakeClient(reply).board_update_from_schematic(board_doc(), doc(), dry_run=True)
    export, imported = seen
    assert imported.netlist_path == export.job_settings.output_path and imported.dry_run
    assert imported.match_mode == bc.NMM_UUID and result["new_footprints"] == 3


def test_every_registered_command_has_known_messages():
    from commands import COMMANDS, message_class, resolve
    for name, (reply, kind) in COMMANDS.items():
        message_class(name)
        message_class(reply)
        assert kind in ("read", "edit", "view", "file", "job")
    assert resolve("UndoLastChange") == "kiapi.common.commands.UndoLastChange"
    assert resolve("RunBoardJobExportGerbers").startswith("kiapi.board.jobs.")


def test_server_exposes_pcb_tools():
    import asyncio
    from server import mcp
    names = {tool.name for tool in asyncio.run(mcp.list_tools())}
    assert {"live_pcb_open", "live_pcb_view", "live_pcb_page", "live_pcb_set_page", "live_pcb_origins",
            "live_editor_grid", "live_pcb_route", "live_pcb_move", "live_pcb_drc", "live_pcb_outline"} <= names


def test_every_board_item_builder_makes_valid_messages():
    from board_client import (make_circle, make_polygon, make_text, make_track, make_via, make_zone)
    via = make_via((1, 2), "GND", 0.6, 0.3)
    assert via.type == bt.VT_THROUGH and via.pad_stack.drill.diameter == point(0.3, 0.3)
    assert via.pad_stack.copper_layers[0].size == point(0.6, 0.6) and via.net.name == "GND"
    assert make_via((0, 0), "", 0.45, 0.2, "F.Cu", "In1.Cu").type == bt.VT_BLIND_BURIED
    track = make_track((0, 0), (5, 0), 0.25, bt.BL_B_Cu, "SDA")
    assert track.width.value_nm == 250000 and track.layer == bt.BL_B_Cu
    zone = make_zone([(0, 0), (10, 0), (10, 10)], ["F.Cu", "B.Cu"], "GND", clearance=0.3, connection="solid")
    assert zone.type == bt.ZT_COPPER and list(zone.layers) == [bt.BL_F_Cu, bt.BL_B_Cu]
    assert zone.copper_settings.net.name == "GND" and zone.copper_settings.clearance.value_nm == 300000
    assert zone.copper_settings.connection.zone_connection == bt.ZCS_FULL
    assert len(zone.outline.polygons[0].outline.nodes) == 3
    keepout = make_zone([(0, 0), (1, 0), (1, 1)], ["F.Cu"], rule_area={"keepout_vias": True,
                                                                       "keepout_tracks": False})
    assert keepout.type == bt.ZT_RULE_AREA and keepout.rule_area_settings.keepout_vias
    assert not keepout.rule_area_settings.keepout_tracks and not keepout.rule_area_settings.keepout_footprints
    with pytest.raises(ValueError):
        make_zone([(0, 0), (1, 0), (1, 1)], ["F.Cu"], "GND", connection="spokes")
    back = make_text("REV A", (5, 5), bt.BL_B_SilkS, 1.0)
    assert back.text.attributes.mirrored and back.text.attributes.stroke_width.value_nm == 150000
    assert not make_text("X", (0, 0), bt.BL_F_SilkS, 1.0).text.attributes.mirrored
    assert make_circle((0, 0), 2, bt.BL_F_SilkS, 0.12).shape.circle.radius_point == point(2, 0)
    assert make_polygon([(0, 0), (1, 0), (1, 1)], bt.BL_F_Cu, 0).shape.polygon.polygons[0].outline.closed


def test_board_exports_and_3d_render_build_their_jobs():
    seen = []

    def reply(request, response_type):
        seen.append(request)
        Path(request.job_settings.output_path).write_bytes(b"PNG") if request.job_settings.output_path.endswith(
            ".png") else None
        return jobs.RunJobResponse(status=jobs.JS_SUCCESS, output_path=[request.job_settings.output_path])
    client = FakeClient(reply)
    for kind in ("gerbers", "drill", "position", "step", "pdf", "svg", "dxf", "ipc2581", "ipcd356", "odb",
                 "stats", "dsn", "gencad", "idf"):
        assert client.board_export(board_doc(), kind, "/tmp/out")["outputs"] == ["/tmp/out"]
    step = next(r for r in seen if type(r).__name__ == "RunBoardJobExport3D")
    assert step.format == board_jobs.B3D_STEP and step.export_components
    assert client.board_render_3d(board_doc(), side="bottom") == b"PNG"
    assert seen[-1].side == board_jobs.RS_BOTTOM and seen[-1].format == board_jobs.RF_PNG
    with pytest.raises(ValueError):
        client.board_export(board_doc(), "brd", "/tmp/out")


def test_board_summary_layers_and_rules():
    def reply(request, response_type):
        name = type(request).__name__
        if name == "GetItems":
            return editor.GetItemsResponse(status=types.IRS_OK)
        if name == "GetBoardEnabledLayers":
            return bc.BoardEnabledLayersResponse(copper_layer_count=4, layers=[bt.BL_F_Cu, bt.BL_In1_Cu,
                                                                               bt.BL_Edge_Cuts])
        if name == "GetDocumentModifiedState":
            return editor.GetDocumentModifiedStateResponse(state=editor.DMS_MODIFIED)
        if name == "GetVisibleLayers":
            return bc.BoardLayers(layers=[bt.BL_F_Cu])
        if name == "GetActiveLayer":
            return bc.BoardLayerResponse(layer=bt.BL_F_Cu)
        return response_type()
    client = FakeClient(reply)
    summary = client.board_summary(board_doc())
    assert summary["copper_layers"] == 4 and summary["unsaved"] and summary["outline_mm"] is None
    layers = client.board_layers(board_doc(), copper_layers=6, visible=["F.Cu"], active="F.Cu")
    stackup = next(r for r in client.requests if type(r).__name__ == "UpdateBoardStackup").stackup
    assert sum(1 for e in stackup.layers if e.type == 1) == 6, "layer count goes through the stackup"
    assert not any(type(r).__name__ == "SetBoardEnabledLayers" for r in client.requests), "never deletes items"
    assert layers["enabled"] == ["F.Cu", "In1.Cu", "Edge.Cuts"] and layers["active"] == "F.Cu"
    rules = client.board_design_rules(board_doc(), {"constraints": {"min_clearance": {"value_nm": 150000}}})
    sent = next(r for r in client.requests if type(r).__name__ == "SetBoardDesignRules")
    assert sent.rules.constraints.min_clearance.value_nm == 150000 and isinstance(rules, dict)


# ---- Router, RF, rules, net classes, stackup, checks -----------------------------------------------

import board_rf  # noqa: E402
import board_router as rt  # noqa: E402
from shapely.geometry import LineString as _Line, Point as _Point, box as _sbox  # noqa: E402


def _problem(**kw):
    problem = rt.RouteProblem(layers=["F.Cu", "B.Cu"], outline=_sbox(0, 0, 40, 30), edge_clearance=0.5,
                              min_clearance=0.2, **kw)
    return problem


def _track_clearance(route_result, width, obstacle):
    gaps = []
    for layer_name_value, pts in route_result.segments:
        gaps.append(_Line(pts).buffer(width / 2).distance(obstacle))
    return min(gaps)


def test_router_goes_around_other_nets_with_clearance():
    wall = _sbox(19, 2, 21, 28)                       # another net's copper between the pads
    problem = _problem(obstacles={"F.Cu": [rt.Obstacle(wall, "VCC")]}, net_clearance={"SIG": 0.3})
    found = rt.route(problem, "SIG", (5, 15), (35, 15), ["F.Cu"], ["F.Cu"], 0.25, layers=["F.Cu"], grid=0.25)
    assert found.checked and found.segments[0][1][0] == (5, 15) and found.segments[0][1][-1] == (35, 15)
    assert _track_clearance(found, 0.25, wall) >= 0.3 - 1e-6, "keeps the larger net class clearance"
    for _, pts in found.segments:
        for (x1, y1), (x2, y2) in zip(pts, pts[1:]):
            assert x1 == x2 or y1 == y2 or abs(abs(x2 - x1) - abs(y2 - y1)) < 1e-6, "only 0/45/90 degree runs"
    for _, pts in found.segments:
        assert all(0.5 + 0.125 - 1e-6 <= x <= 40 - 0.625 + 1e-6 and 0.625 - 1e-6 <= y <= 30 - 0.625 + 1e-6
                   for x, y in pts[1:-1]), "stays the edge clearance inside the outline"


def test_router_ignores_its_own_net_and_respects_keepouts():
    same = _sbox(19, 0, 21, 30)
    problem = _problem(obstacles={"F.Cu": [rt.Obstacle(same, "SIG")]})
    found = rt.route(problem, "SIG", (5, 15), (35, 15), ["F.Cu"], ["F.Cu"], 0.25, layers=["F.Cu"], grid=0.5)
    assert len(found.segments[0][1]) == 2, "straight across its own net's copper"
    keepout = _sbox(15, 5, 25, 25)
    problem = _problem(track_keepouts={"F.Cu": [keepout]})
    found = rt.route(problem, "SIG", (5, 15), (35, 15), ["F.Cu"], ["F.Cu"], 0.25, layers=["F.Cu"], grid=0.5)
    assert all(not _Line(pts).buffer(0.125).intersects(keepout) for _, pts in found.segments)


def test_router_changes_layer_through_a_via_when_blocked():
    wall = _sbox(19, 0, 21, 30)                       # blocks F.Cu completely
    problem = _problem(obstacles={"F.Cu": [rt.Obstacle(wall, "GND")]})
    with pytest.raises(rt.NoRoute):
        rt.route(problem, "SIG", (5, 15), (35, 15), ["F.Cu"], ["F.Cu"], 0.25, layers=["F.Cu"], grid=0.5)
    found = rt.route(problem, "SIG", (5, 15), (35, 15), ["F.Cu"], ["F.Cu"], 0.25, layers=["F.Cu", "B.Cu"],
                     via=(0.6, 0.3), grid=0.5)
    assert len(found.vias) == 2 and {name for name, _ in found.segments} == {"F.Cu", "B.Cu"}
    assert all(_Point(v).distance(wall) >= 0.3 + 0.2 - 1e-6 for v in found.vias), "vias clear of the wall"
    problem.via_keepouts.append(_sbox(0, 0, 40, 30))
    with pytest.raises(rt.NoRoute):
        rt.route(problem, "SIG", (5, 15), (35, 15), ["F.Cu"], ["F.Cu"], 0.25, layers=["F.Cu", "B.Cu"],
                 via=(0.6, 0.3), grid=0.5)


def test_outline_and_spanning_order():
    rect = [_Line([(0, 0), (30, 0)]), _Line([(30, 0), (30, 20)]), _Line([(30, 20), (0, 20)]),
            _Line([(0, 20), (0, 0)]), _Point(15, 10).buffer(3).exterior]
    outline = rt.outline_from_lines(rect)
    assert abs(outline.area - (600 - 28.27)) < 0.5, "the cut-out hole is removed"
    arc = rt.arc_points((10, 0), (0, 10), (-10, 0))
    assert all(abs(math.hypot(x, y) - 10) < 1e-6 for x, y in arc) and arc[len(arc) // 2][1] > 9
    pairs = rt.spanning_order([[(0, 0, "a")], [(10, 0, "b")], [(1, 0, "c")]])
    assert [(p[2], q[2]) for p, q in pairs] == [("a", "c"), ("c", "b")]


def test_impedance_widths_match_references():
    assert abs(board_rf.width_for(50, 1.51, 4.5, 0.035) - 2.80) < 0.1        # 2-layer 1.6 mm FR-4
    assert abs(board_rf.width_for(50, 0.2, 4.4, 0.035) - 0.35) < 0.03        # 4-layer, 0.2 mm prepreg
    assert abs(board_rf.cpwg(1.0, 0.2, 1.6, 4.6)[0] - 53.4) < 1.5
    stack = [{"type": "copper", "layer": "F.Cu", "thickness": 0.035},
             {"type": "dielectric", "thickness": 0.2, "sublayers": [{"thickness": 0.2, "epsilon_r": 4.4}]},
             {"type": "copper", "layer": "In1.Cu", "thickness": 0.0175},
             {"type": "dielectric", "thickness": 1.0, "sublayers": [{"thickness": 1.0, "epsilon_r": 4.6}]},
             {"type": "copper", "layer": "B.Cu", "thickness": 0.035}]
    d = board_rf.dielectric_under(stack, "F.Cu")
    assert d["height"] == 0.2 and d["reference_layer"] == "In1.Cu" and d["copper_thickness"] == 0.035
    assert board_rf.dielectric_under(stack, "B.Cu")["reference_layer"] == "In1.Cu"
    with pytest.raises(ValueError):
        board_rf.width_for(500, 0.2, 4.4)


def test_custom_rules_convert_both_ways():
    from board_design import custom_rule, custom_rules_summary
    rule = custom_rule({"name": "RF", "condition": "A.NetClass == 'RF'", "layer": "outer", "severity": "error",
                        "constraints": [{"type": "clearance", "min": 0.5},
                                        {"type": "disallow", "items": ["through_vias", "tracks"]}]})
    assert rule.layer_mode == rules_pb2.CRLM_OUTER and rule.constraints[0].numeric.min == 500000
    assert list(rule.constraints[1].disallow.types) == [rules_pb2.CRDT_THROUGH_VIAS, rules_pb2.CRDT_TRACKS]
    summary = custom_rules_summary(bc.CustomRulesResponse(status=bc.CRS_VALID, rules=[rule]))
    assert summary["rules"][0]["constraints"][0] == {"type": "clearance", "min": 0.5}
    assert summary["rules"][0]["layer"] == "outer" and summary["status"] == "valid"
    with pytest.raises(ValueError):
        custom_rule({"name": "x", "constraints": [{"type": "nonsense"}]})


def test_custom_rules_add_replaces_by_name_and_reports_errors():
    from board_design import custom_rule
    old = custom_rule({"name": "A", "constraints": [{"type": "clearance", "min": 0.2}]})
    keep = custom_rule({"name": "B", "constraints": [{"type": "track_width", "min": 0.3}]})
    sent = []

    def reply(request, response_type):
        sent.append(request)
        if type(request).__name__ == "GetCustomDesignRules":
            return bc.CustomRulesResponse(status=bc.CRS_VALID, rules=[old, keep])
        return bc.CustomRulesResponse(status=bc.CRS_VALID, rules=request.rules)
    client = FakeClient(reply)
    out = client.board_custom_rules(board_doc(), "add", [{"name": "A", "constraints": [{"type": "clearance",
                                                                                       "min": 0.4}]}])
    assert [r["name"] for r in out["rules"]] == ["B", "A"] and out["rules"][1]["constraints"][0]["min"] == 0.4
    client = FakeClient(lambda request, _: bc.CustomRulesResponse(status=bc.CRS_INVALID, error_text="bad"))
    with pytest.raises(RuntimeError, match="bad"):
        client.board_custom_rules(board_doc(), "set", [])


def test_design_rules_and_netclasses_requests():
    sent = []

    def reply(request, response_type):
        sent.append(request)
        name = type(request).__name__
        if name in ("GetBoardDesignRules", "SetBoardDesignRules"):
            return bc.BoardDesignRulesResponse(rules=getattr(request, "rules", None))
        if name == "GetNetClasses":
            return response_type(net_classes=[ps_pb2.NetClass(name="Default")])
        return response_type()
    client = FakeClient(reply)
    out = client.board_rules(board_doc(), track_widths=[0.2, 0.5], via_sizes=[[0.6, 0.3]],
                             severities={"silk_clearance": "error"}, min_clearance=0.15, silk_clearance=0.1)
    rules = next(r for r in sent if type(r).__name__ == "SetBoardDesignRules").rules
    assert rules.constraints.min_clearance.value_nm == 150000 and rules.constraints.silk_clearance.value_nm == 100000
    assert out["track_widths_mm"] == [0.2, 0.5] and out["via_sizes_mm"] == [[0.6, 0.3]]
    assert out["severities"] == {"silk_clearance": "error"}
    with pytest.raises(ValueError):
        client.board_rules(board_doc(), via_sizes=[[0.3, 0.4]])
    sent.clear()
    client.board_netclasses(board_doc(), classes=[{"name": "Default", "track_width": 0.2, "clearance": 0.15,
                                                   "via_diameter": 0.6, "via_drill": 0.3}],
                            assign={"ANT": "RF"}, patterns={"/USB*": "USB"})
    classes = next(r for r in sent if type(r).__name__ == "SetNetClasses")
    nc = classes.net_classes[0]
    assert classes.merge_mode == types.MMM_MERGE and nc.board.track_width.value_nm == 200000
    assert nc.board.via_stack.copper_layers[0].size.x_nm == 600000 and nc.board.via_stack.drill.diameter.x_nm == 300000
    assignment = next(r for r in sent if type(r).__name__ == "SetNetClassAssignments")
    assert assignment.assignments[0].net == "ANT" and assignment.pattern_assignments[0].netclass == "USB"


@pytest.mark.parametrize("path, expected", [("/tmp/test", "/tmp/test/"), ("/tmp/test/", "/tmp/test/"),
                                            (r"C:\kb\live\projects\live-demo", "C:\\kb\\live\\projects\\live-demo\\"),
                                            ("C:\\kb\\demo\\", "C:\\kb\\demo\\"),
                                            (r"\\server\share\proj", "\\\\server\\share\\proj\\")])
def test_netclass_project_path_gets_the_native_trailing_separator(path, expected):
    """KiCad matches GetProjectPath(), which ends in the native separator; board listings omit it."""
    sent = []

    def reply(request, response_type):
        sent.append(request)
        return response_type()

    doc = board_doc()
    doc.project.path = path
    FakeClient(reply).board_netclasses(doc)
    assert {request.project.path for request in sent} == {expected}


def test_stackup_request_and_thickness_scaling():
    from board import board_pb2
    sent = []
    state = {"stackup": None}

    def reply(request, response_type):
        name = type(request).__name__
        if name == "UpdateBoardStackup":
            sent.append(request)
            stackup = board_pb2.BoardStackup()
            for entry in request.stackup.layers:
                if entry.type in (board_pb2.BSLT_COPPER, board_pb2.BSLT_DIELECTRIC):
                    e = stackup.layers.add()
                    e.CopyFrom(entry)
                    if entry.type == board_pb2.BSLT_COPPER and not entry.thickness.value_nm:
                        e.thickness.value_nm = 35000
                    if entry.type == board_pb2.BSLT_DIELECTRIC:
                        e.thickness.value_nm = sum(p.thickness.value_nm for p in entry.dielectric.layer) or 500000
            state["stackup"] = stackup
            return bc.BoardStackupResponse(stackup=stackup)
        if name == "GetBoardStackup":
            return bc.BoardStackupResponse(stackup=state["stackup"] or board_pb2.BoardStackup())
        return response_type()
    client = FakeClient(reply)
    with pytest.raises(ValueError):
        client.board_set_stackup(board_doc(), copper_layers=3)
    with pytest.raises(ValueError, match="dielectrics"):
        client.board_set_stackup(board_doc(), copper_layers=4, dielectrics=[{"thickness": 0.2}])
    out = client.board_set_stackup(board_doc(), copper_layers=4, soldermask_color="#103010",
                                   dielectrics=[{"type": "prepreg", "thickness": 0.2, "epsilon_r": 4.4},
                                                {"type": "core", "thickness": 1.0, "epsilon_r": 4.6},
                                                {"type": "prepreg", "thickness": 0.2, "epsilon_r": 4.4}],
                                   board_thickness=1.6)
    first = sent[0].stackup
    assert sum(1 for e in first.layers if e.type == board_pb2.BSLT_COPPER) == 4
    mask = next(e for e in first.layers if e.type == board_pb2.BSLT_SOLDERMASK)
    assert abs(mask.color.g - 0x30 / 255) < 1e-6
    assert len(sent) == 2 and abs(out["board_thickness_mm"] - 1.6) < 1e-3, "dielectrics scaled to 1.6 mm"


def test_check_groups_drc_by_category():
    client = FakeClient(lambda *_: None)
    client.board_drc = lambda doc, **kw: {"violations": [
        {"type": "silk_mask_clearance", "items": []}, {"type": "clearance", "items": []},
        {"type": "overlapping_footprints", "items": []}, {"type": "unconnected_items", "items": []}]}
    out = client.board_check(board_doc())
    assert out["counts"]["silkscreen"] == 1 and out["counts"]["clearance"] == 1 and out["counts"]["courtyard"] == 1
    assert not out["clean"]
    out = client.board_check(board_doc(), ["silkscreen"])
    assert list(out["counts"]) == ["silkscreen"]
    with pytest.raises(ValueError):
        client.board_check(board_doc(), ["colour"])


def test_router_leaves_its_own_pads_even_without_a_net():
    start_pad, end_pad = _sbox(4, 14, 6, 16), _sbox(34, 14, 36, 16)
    other = _sbox(19, 14, 21, 16)
    problem = _problem(obstacles={"F.Cu": [rt.Obstacle(start_pad, "", "pad", "a"), rt.Obstacle(end_pad, "", "pad", "b"),
                                           rt.Obstacle(other, "", "pad", "c")]})
    with pytest.raises(rt.NoRoute):
        rt.route(problem, "", (5, 15), (35, 15), ["F.Cu"], ["F.Cu"], 0.25, layers=["F.Cu"], grid=0.25)
    found = rt.route(problem, "", (5, 15), (35, 15), ["F.Cu"], ["F.Cu"], 0.25, layers=["F.Cu"], grid=0.25,
                     own_keys={"a", "b"}, own_pads={"F.Cu": [start_pad, end_pad]})
    assert _track_clearance(found, 0.25, other) >= 0.2 - 1e-6, "still avoids the third, net-less pad"


def test_rounded_outline_from_arcs_closes():
    from board_client import rounded_rectangle
    from board_design import edge_lines
    lines = []
    for shape in rounded_rectangle(50, 40, 60, 40, 2, bt.BL_Edge_Cuts, 0.05):
        lines += edge_lines(shape)
    outline = rt.outline_from_lines(lines)
    assert outline is not None and abs(outline.area - (2400 - (4 - math.pi) * 4)) < 0.05


# ---- Revisions and operation reconciliation (REVISIONS.md) ---------------------------------------

class HeaderClient(FakeClient):
    """Records the (expected_revision, operation_id) header each request was sent with."""

    def __init__(self, reply=None, end_commit=None):
        super().__init__(reply)
        self.headers = []
        self.end_commit = end_commit

    def send(self, request, response_type):
        self.headers.append(self._pending_header)
        if isinstance(request, editor.EndCommit) and self.end_commit:
            self.requests.append(request)
            return self.end_commit(request)
        if isinstance(request, editor.EndCommit) and request.action == editor.CMA_COMMIT:
            self.requests.append(request)
            self.last_revision = f"rev-{len(self.requests)}"
            return editor.EndCommitResponse()
        return super().send(request, response_type)


def created(request, response_type):
    if isinstance(request, editor.GetOperationStatus):
        raise AssertionError("no reconciliation expected")
    response = editor.CreateItemsResponse(status=types.IRS_OK)
    for item in request.items:
        response.created_items.add(status=editor.ItemStatus(code=editor.ISC_OK), item=item)
    return response


def test_document_revision_reports_the_live_token():
    client = FakeClient(lambda request, _: editor.DocumentRevisionResponse(revision="ab12cd34-7", open_commits=1,
                                                                           modified=True))
    assert client.document_revision(doc()) == {"revision": "ab12cd34-7", "open_commits": 1, "modified": True}
    assert isinstance(client.requests[0], editor.GetDocumentRevision)


def test_operation_status_names_every_state():
    client = FakeClient(lambda request, _: editor.OperationStatusResponse(operation_id=request.operation_id))
    assert client.operation_status("op-1")["state"] == "OPS_UNKNOWN"
    with pytest.raises(ValueError):
        client.operation_status("")


def test_every_transaction_carries_an_operation_id_and_options_chain_revisions():
    client = HeaderClient(created)
    client.mutate(doc(), [wire()])
    expected, operation = client.headers[0]
    assert expected == "" and operation.startswith("mcp-")
    assert client.last_operation["operation_id"] == operation
    assert client.last_operation["state"] == "committed"

    client = HeaderClient(created)
    with client.mutation_options("rev-0", "agent-op") as done:
        client.mutate(doc(), [wire()])
        client.mutate(doc(), [wire()])
    begins = [h for h, r in zip(client.headers, client.requests) if isinstance(r, editor.BeginCommit)]
    # The second commit must find the document exactly as the first left it
    assert begins == [("rev-0", "agent-op"), ("rev-3", "agent-op.2")]
    assert [o["operation_id"] for o in done["operations"]] == ["agent-op", "agent-op.2"]
    # Steps and EndCommit never repeat the commit's id or revision
    assert all(h == ("", "") or h is None for h, r in zip(client.headers, client.requests)
               if not isinstance(r, editor.BeginCommit))


def test_transactions_opt_into_a_commit_lease():
    from client import DEFAULT_LEASE_S

    # KiCad gives a commit no lease unless its client asks (upstream plugins keep theirs open), so
    # the MCP asks on every BeginCommit; a killed MCP's commit is then rolled back
    client = HeaderClient(created)
    client.mutate(doc(), [wire()])
    begin = next(r for r in client.requests if isinstance(r, editor.BeginCommit))
    assert begin.lease_seconds == DEFAULT_LEASE_S > 0

    client = HeaderClient(created)
    client.lease_seconds = None
    client.mutate(doc(), [wire()])
    begin = next(r for r in client.requests if isinstance(r, editor.BeginCommit))
    assert begin.lease_seconds == 0


def test_lost_commit_reply_is_reconciled_not_resent():
    from client import OutcomeUnknown, ReplyTimeout

    def lost(request):
        if request.action == editor.CMA_DROP:
            return editor.EndCommitResponse()
        raise ReplyTimeout("No reply from KiCad to EndCommit within 1 ms")

    settle = ["EndCommit", "GetOperationStatus"]
    # (states the status queries return, expected error, requests after the lost EndCommit)
    for states, outcome, extra in ((["OPS_COMMITTED"], None, []),
                                   (["OPS_ROLLED_BACK"], RuntimeError, []),
                                   (["OPS_UNKNOWN"], OutcomeUnknown, []),
                                   # Still open: a drop settles it either way, then KiCad is asked again
                                   (["OPS_IN_PROGRESS", "OPS_COMMITTED"], None, settle),
                                   (["OPS_IN_PROGRESS", "OPS_ROLLED_BACK"], RuntimeError, settle),
                                   (["OPS_IN_PROGRESS", "OPS_IN_PROGRESS"], OutcomeUnknown, settle)):
        answers = iter(states)

        def reply(request, response_type):
            if isinstance(request, editor.GetOperationStatus):
                return editor.OperationStatusResponse(operation_id=request.operation_id,
                                                      state=editor.OperationState.Value(next(answers)),
                                                      revision_after="rev-9", message="lease expired")
            return created(request, response_type)
        client = HeaderClient(reply, end_commit=lost)
        if outcome is None:
            client.mutate(doc(), [wire()])
            assert client.last_operation["reconciled"] and client.last_revision == "rev-9"
        else:
            with pytest.raises(outcome) as error:
                client.mutate(doc(), [wire()])
            if outcome is OutcomeUnknown:
                assert error.value.operation_id == client.headers[0][1]
                assert "Do not resend" in str(error.value)
        names = [type(r).__name__ for r in client.requests]
        # One edit and one commit attempt: the mutation is reconciled, never replayed
        assert names == ["BeginCommit", "CreateItems", "EndCommit", "GetOperationStatus", *extra]
        assert sum(isinstance(r, editor.EndCommit) and r.action == editor.CMA_COMMIT
                   for r in client.requests) == 1


def test_lost_begin_reply_drops_an_opened_commit_without_sending_edits():
    from client import ReplyTimeout

    class LostBegin(HeaderClient):
        def send(self, request, response_type):
            if isinstance(request, editor.BeginCommit):
                self.requests.append(request)
                raise ReplyTimeout("No reply from KiCad to BeginCommit within 1 ms")
            return super().send(request, response_type)

    def reply(request, response_type):
        if isinstance(request, editor.GetOperationStatus):
            return editor.OperationStatusResponse(state=editor.OPS_IN_PROGRESS)
        pytest.fail("no edit may be sent after a lost BeginCommit reply")
    client = LostBegin(reply)
    with pytest.raises(RuntimeError, match="nothing was changed"):
        client.mutate(doc(), [wire()])
    drop = client.requests[-1]
    assert isinstance(drop, editor.EndCommit) and drop.action == editor.CMA_DROP and not drop.id.value
    assert not any(isinstance(r, editor.CreateItems) for r in client.requests)


def test_conflicting_commit_is_reported_as_rolled_back():
    from client import RevisionMismatch

    def conflict(request):
        raise RevisionMismatch("AS_REVISION_MISMATCH: another client or the user changed the document")
    client = HeaderClient(created, end_commit=conflict)
    with pytest.raises(RevisionMismatch):
        client.mutate(doc(), [wire()])
    assert client.last_operation["state"] == "rolled_back"


def test_send_maps_revision_statuses_and_records_the_reply_revision(monkeypatch):
    import client as client_module
    from client import DuplicateOperation, RevisionMismatch, envelope
    sent = []

    def connection(status, revision="ab12cd34-4"):
        class Connection:
            def __init__(self, **kwargs):
                pass

            def __enter__(self):
                return self

            def __exit__(self, *_args):
                pass

            def dial(self, url, **kwargs):
                pass

            def send(self, raw):
                sent.append(envelope.ApiRequest.FromString(raw))

            def recv(self):
                reply = envelope.ApiResponse()
                reply.header.document_revision = revision
                reply.status.status = status
                reply.status.error_message = "the document is at revision ab12cd34-4, not ab12cd34-3"
                if status == envelope.AS_OK:
                    reply.message.Pack(editor.BeginCommitResponse(id=types.KIID(value="commit")))
                return reply.SerializeToString()
        return Connection

    live = LiveSchematic(unused_socket("unused-test-socket"))
    for status, error in ((envelope.AS_REVISION_MISMATCH, RevisionMismatch),
                          (envelope.AS_DUPLICATE_OPERATION, DuplicateOperation)):
        monkeypatch.setattr(client_module.pynng, "Req0", connection(status))
        with pytest.raises(error, match="AS_"):
            live._send_with(editor.BeginCommit(), editor.BeginCommitResponse, "ab12cd34-3", "op-7")
        assert live.last_revision == "ab12cd34-4"
    assert sent[-1].header.expected_revision == "ab12cd34-3" and sent[-1].header.operation_id == "op-7"

    monkeypatch.setattr(client_module.pynng, "Req0", connection(envelope.AS_OK, revision=""))
    live.send(editor.BeginCommit(), editor.BeginCommitResponse)
    # Plain sends carry no contract fields, and a reply without a document has no revision
    assert not sent[-1].header.expected_revision and not sent[-1].header.operation_id
    assert live.last_revision is None


def test_reply_timeout_is_still_a_pynng_timeout(monkeypatch):
    import client as client_module
    from client import ReplyTimeout

    class Silent:
        def __init__(self, **kwargs):
            pass

        def __enter__(self):
            return self

        def __exit__(self, *_args):
            pass

        def dial(self, url, **kwargs):
            pass

        def send(self, raw):
            pass

        def recv(self):
            raise client_module.pynng.Timeout("synthetic timeout", 5)

    monkeypatch.setattr(client_module.pynng, "Req0", Silent)
    with pytest.raises(client_module.pynng.Timeout) as error:
        LiveSchematic(unused_socket("unused-test-socket")).send(editor.BeginCommit(), editor.BeginCommitResponse)
    assert isinstance(error.value, ReplyTimeout) and isinstance(error.value, TimeoutError)
    assert "may still apply" in str(error.value)


def test_execute_tags_document_edits_but_not_project_settings():
    client = HeaderClient(lambda request, response_type: response_type())
    client.execute("SetNetClasses", {})
    client.execute("UndoLastChange", {"document": as_dict(doc())})
    assert client.headers[0] is None
    assert client.headers[1][1].startswith("mcp-")


def test_reply_timeout_comes_from_the_environment(monkeypatch):
    """KICAD_API_TIMEOUT_MS sets the reply timeout (the native Rust MCP reads the same variable)."""
    monkeypatch.delenv("KICAD_API_TIMEOUT_MS", raising=False)
    assert LiveSchematic(unused_socket("unused-test-socket")).timeout_ms == 15000
    monkeypatch.setenv("KICAD_API_TIMEOUT_MS", " 400 ")
    assert LiveSchematic(unused_socket("unused-test-socket")).timeout_ms == 400
    assert LiveSchematic(unused_socket("unused-test-socket"), timeout_ms=50).timeout_ms == 50
    for invalid in ("0", "600001", "1.5", "-5", "soon", "５００", "²"):
        monkeypatch.setenv("KICAD_API_TIMEOUT_MS", invalid)
        with pytest.raises(ValueError, match="KICAD_API_TIMEOUT_MS"):
            LiveSchematic(unused_socket("unused-test-socket"))


def test_connection_reset_after_sending_is_a_lost_reply(monkeypatch):
    """KiCad closing the connection before it replies is a lost reply: ReplyTimeout, so edits are
    reconciled instead of the reset escaping as an unknown error."""
    import client as client_module
    from client import ReplyTimeout

    class Reset:
        def __init__(self, **kwargs):
            pass

        def __enter__(self):
            return self

        def __exit__(self, *exc):
            return False

        def dial(self, url, block=False):
            pass

        def send(self, raw):
            pass

        def recv(self):
            raise client_module.pynng.ConnectionReset("Connection reset", 31)

    monkeypatch.setattr(client_module.pynng, "Req0", Reset)
    with pytest.raises(ReplyTimeout) as error:
        LiveSchematic(unused_socket("unused-test-socket")).send(editor.BeginCommit(), editor.BeginCommitResponse)
    assert str(error.value) == ("KiCad closed the connection before replying to BeginCommit; "
                                "KiCad may still apply it")
