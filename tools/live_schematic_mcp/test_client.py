# SPDX-License-Identifier: GPL-3.0-or-later
"""Protocol, rollback and rendering contracts for the live schematic companion."""

from pathlib import Path
import uuid

import pytest

from client import (LiveSchematic, as_dict, base, document, editor, jobs,
                    parse_items, point, sch, sch_jobs, types)


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
