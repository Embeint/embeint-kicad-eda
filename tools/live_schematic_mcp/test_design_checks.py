# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline contracts for whole-design verification: the hierarchical netlist.  No running KiCad is
required."""

import pytest

from client import types
from client import commands as sch_cmds
from test_client import FakeClient, doc


ROOT, CHILD, REUSED_A, REUSED_B = "root-0000", "child-1111", "reuse-aaaa", "reuse-bbbb"


def sheet_path(*ids, human="/"):
    return types.SheetPath(path=[types.KIID(value=i) for i in ids], path_human_readable=human)


def hierarchy_reply():
    """Root R1; child Filter with R2 and C1; one sheet file used twice (U1 and U2 share pin UUIDs)."""
    paths = {
        (ROOT,): ("/", [("R1", [("r1-p1", "1"), ("r1-p2", "2")])]),
        (ROOT, CHILD): ("/Filter/", [("R2", [("r2-p1", "1"), ("r2-p2", "2")]), ("C1", [("c1-p1", "1")])]),
        (ROOT, REUSED_A): ("/Chan_A/", [("U1", [("u-p1", "1")])]),
        (ROOT, REUSED_B): ("/Chan_B/", [("U2", [("u-p1", "1")])]),
    }

    def reply(request, response_type):
        name = type(request).__name__
        if name == "GetSchematicHierarchy":
            response = sch_cmds.SchematicHierarchyResponse()
            root = response.top_level_sheets.add(name="Root", path=sheet_path(ROOT, human="/"))
            for sheet, human in ((CHILD, "/Filter/"), (REUSED_A, "/Chan_A/"), (REUSED_B, "/Chan_B/")):
                root.children.add(name=human.strip("/"), path=sheet_path(ROOT, sheet, human=human))
            return response
        if name == "GetSymbolPins":
            key = tuple(k.value for k in request.document.sheet_path.path)
            response = sch_cmds.SymbolPinsResponse()
            for reference, pins in paths[key][1]:
                symbol = response.symbols.add(reference=reference)
                for pin_id, number in pins:
                    symbol.pins.add(id=types.KIID(value=pin_id), number=number)
            return response
        if name == "GetSchematicNetlist":
            response = sch_cmds.SchematicNetlistResponse()
            net = response.nets.add(name="/Filter/SIGNAL")
            net.sheets.add(path=sheet_path(ROOT, human="/"),
                           items=[types.KIID(value="r1-p2"), types.KIID(value="root-wire")])
            net.sheets.add(path=sheet_path(ROOT, CHILD, human="/Filter/"),
                           items=[types.KIID(value="r2-p1"), types.KIID(value="c1-p1"), types.KIID(value="hlabel")])
            out = response.nets.add(name="/Filter/OUT")
            out.sheets.add(path=sheet_path(ROOT, CHILD, human="/Filter/"), items=[types.KIID(value="r2-p2")])
            for sheet, net_name in ((REUSED_A, "/Chan_A/IN"), (REUSED_B, "/Chan_B/IN")):
                reused = response.nets.add(name=net_name)
                reused.sheets.add(path=sheet_path(ROOT, sheet), items=[types.KIID(value="u-p1")])
            return response
        raise AssertionError(f"unexpected request {name}")
    return reply


def root_doc():
    spec = doc()
    spec.sheet_path.CopyFrom(sheet_path(ROOT))
    return spec


def test_hierarchy_netlist_lists_pins_on_every_sheet_with_their_sheet_path():
    client = FakeClient(hierarchy_reply())
    nets = client.netlist_pins(root_doc())
    assert nets["/Filter/SIGNAL"] == {
        "pins": ["C1.1", "R1.2", "R2.1"], "items": 5,
        "sheets": [{"path": "/", "pins": ["R1.2"]}, {"path": "/Filter/", "pins": ["C1.1", "R2.1"]}]}
    assert nets["/Filter/OUT"]["pins"] == ["R2.2"]
    # One sheet file placed twice: the shared pin UUID resolves to each instance's own reference
    assert nets["/Chan_A/IN"]["sheets"] == [{"path": "/Chan_A/", "pins": ["U1.1"]}]
    assert nets["/Chan_B/IN"]["sheets"] == [{"path": "/Chan_B/", "pins": ["U2.1"]}]
    pin_requests = [r for r in client.requests if type(r).__name__ == "GetSymbolPins"]
    assert len(pin_requests) == 4, "one pin read per sheet instance"


def test_hierarchy_netlist_does_not_depend_on_the_requested_sheet():
    child = root_doc()
    child.sheet_path.CopyFrom(sheet_path(ROOT, CHILD, human="/Filter/"))
    assert FakeClient(hierarchy_reply()).netlist_pins(child) == FakeClient(hierarchy_reply()).netlist_pins(root_doc())


def test_sheet_scope_keeps_the_original_sheet_only_reply():
    client = FakeClient(hierarchy_reply())
    nets = client.netlist_pins(root_doc(), scope="sheet")
    assert nets["/Filter/SIGNAL"] == {"pins": ["R1.2"], "items": 5}
    assert nets["/Filter/OUT"] == {"pins": [], "items": 1}
    assert not any(type(r).__name__ == "GetSchematicHierarchy" for r in client.requests)
    with pytest.raises(ValueError, match="scope"):
        client.netlist_pins(root_doc(), scope="project")


def test_netlist_tool_defaults_to_the_whole_hierarchy(monkeypatch):
    import server
    from google.protobuf import json_format

    fake = FakeClient(hierarchy_reply())
    monkeypatch.setattr(server, "client", fake)
    spec = json_format.MessageToDict(root_doc())
    whole = server.live_schematic_netlist(spec)
    assert whole["/Filter/SIGNAL"]["pins"] == ["C1.1", "R1.2", "R2.1"]
    assert server.live_schematic_netlist(spec, scope="sheet")["/Filter/SIGNAL"]["pins"] == ["R1.2"]

