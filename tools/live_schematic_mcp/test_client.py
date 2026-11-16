# SPDX-License-Identifier: GPL-3.0-or-later
"""Protocol, rollback and rendering contracts for the live schematic companion."""

from pathlib import Path
import uuid

import pytest

from client import (LiveSchematic, as_dict, base, document, editor, jobs,
                    parse_items, point, sch, sch_jobs, types)
from client import commands as sch_cmds
from client import enums


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
        super().__init__("ipc:///unused-test-socket")
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
    client = FakeClient(lambda request, _: sch_cmds.UndoRedoResponse(performed=True, undo_count=2))
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


def test_model_plan_follows_kicad_3d_conventions():
    from model_handoff import model_plan
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
        from compression import zstd
    except ImportError:
        pytest.skip("needs Python 3.14 compression.zstd")
    payload = b"ISO-10303-21;\nHEADER;\n" * 50
    encoded = base64.b64encode(zstd.compress(payload)).decode()
    text = f'(embedded_files (file (name "m.step") (type model) (data |{encoded[:40]}|\n |{encoded[40:]}|) (checksum "x")))'
    assert embedded_file_bytes(text, "m.step") == payload
