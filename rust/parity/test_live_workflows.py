# SPDX-License-Identifier: GPL-3.0-or-later
"""The live comparison must catch a wrong result: each stricter check is fed a deliberately wrong
server result here (no KiCad needed) and must fail, while the differences that are only names
(random ids, item order) must still pass."""

import base64
import copy
import json
from pathlib import Path
import struct
import sys
from types import SimpleNamespace
import uuid
import zlib

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools" / "live_schematic_mcp"))

import live_workflows as lw  # noqa: E402

BOARD = "type.googleapis.com/kiapi.board.types."
EXISTING = [str(uuid.UUID(int=n)) for n in (1, 2)]  # ids of items both servers start from


def text(value):
    return {"content": [{"type": "text", "text": json.dumps(value)}]}


def error(message):
    return {"content": [{"type": "text", "text": message}], "isError": True}


def call(tool, result, mode="exact", arguments=None, **extra):
    return {"tool": tool, "arguments": arguments or {}, "result": result, "mode": mode, "seconds": 0, **extra}


def compare(python_calls, rust_calls, seed=()):
    """compare_recordings on two recordings; the failed checks."""
    failed = []

    def check(label, ok, detail=""):
        if not ok:
            failed.append(f"{label}: {detail}")
    lw.compare_recordings("test", SimpleNamespace(calls=python_calls), SimpleNamespace(calls=rust_calls), check, [],
                          seed)
    return failed


def graphics(arc_mid, ids=None):
    """The board after live_pcb_add_graphics: a line and an arc with new (random) ids, and an
    existing footprint."""
    ids = ids or [str(uuid.uuid4()), str(uuid.uuid4())]
    return [{"@type": BOARD + "Footprint", "id": {"value": EXISTING[0]}, "reference": "U1"},
            {"@type": BOARD + "BoardGraphicShape", "id": {"value": ids[0]}, "layer": "BL_F_SilkS",
             "segment": {"start": {"x_nm": "102000000"}, "end": {"x_nm": "110000000"}}},
            {"@type": BOARD + "BoardGraphicShape", "id": {"value": ids[1]}, "layer": "BL_F_Fab",
             "arc": {"start": {"x_nm": "112000000"}, "mid": {"x_nm": arc_mid}, "end": {"x_nm": "116000000"}}}]


def snapshot_order(items):
    """As sorted_items orders a snapshot: by the raw JSON, so by the random ids."""
    return sorted(items, key=lambda item: json.dumps(item, sort_keys=True))


# ---- Finding 1: an edit's effect is compared between the servers -------------------------------

def test_same_effect_with_other_random_ids_agrees():
    python = snapshot_order(graphics("114000000"))
    rust = snapshot_order(graphics("114000000"))
    added = text({"added": 2})
    assert compare([call("live_pcb_add_graphics", added, after=python)],
                   [call("live_pcb_add_graphics", added, after=rust)], EXISTING) == []


def test_wrong_arc_mid_point_is_caught_although_the_results_agree():
    added = text({"added": 2})
    failed = compare([call("live_pcb_add_graphics", added, after=snapshot_order(graphics("114000000")))],
                     [call("live_pcb_add_graphics", added, after=snapshot_order(graphics("114500000")))], EXISTING)
    assert len(failed) == 1 and "document after the call" in failed[0] and "mid" in failed[0]


def test_wrong_layer_is_caught():
    added = text({"added": 2})
    wrong = graphics("114000000")
    wrong[2]["layer"] = "BL_B_Fab"
    failed = compare([call("live_pcb_add_graphics", added, after=snapshot_order(graphics("114000000")))],
                     [call("live_pcb_add_graphics", added, after=snapshot_order(wrong))], EXISTING)
    assert failed and "layer" in failed[0]


def test_wrong_keepout_constraint_is_caught():
    def keepout(tracks):
        return [{"@type": BOARD + "Zone", "id": {"value": str(uuid.uuid4())}, "type": "ZT_RULE_AREA",
                 "rule_area_settings": {"keepout_tracks": tracks, "keepout_vias": True}}]
    moved = text({"zone": "keepout"})
    assert compare([call("live_pcb_add_keepout", moved, after=keepout(True))],
                   [call("live_pcb_add_keepout", moved, after=keepout(True))]) == []
    assert compare([call("live_pcb_add_keepout", moved, after=keepout(True))],
                   [call("live_pcb_add_keepout", moved, after=keepout(False))])


def test_settings_effect_is_compared():
    result = text({"grid_origin_mm": [100, 60]})
    assert compare([call("live_pcb_origins", result, effect={"origins": {"drill_origin_mm": [100, 95]}})],
                   [call("live_pcb_origins", result, effect={"origins": {"drill_origin_mm": [100, 96]}})])


def test_effect_recorded_for_one_server_only_fails():
    assert compare([call("live_pcb_move", text({"moved": 1}), after=[])], [call("live_pcb_move", text({"moved": 1}))])


def test_footprint_children_in_another_id_order_agree():
    def footprint():
        pads = [{"id": {"value": str(uuid.uuid4())}, "number": n} for n in ("1", "2", "3")]
        return [{"@type": BOARD + "Footprint", "id": {"value": str(uuid.uuid4())},
                 "definition": {"items": sorted(pads, key=lambda p: p["id"]["value"])}}]
    assert compare([call("live_footprint_generate", text({"pads": 3}), after=footprint())],
                   [call("live_footprint_generate", text({"pads": 3}), after=footprint())]) == []


def test_net_codes_are_not_compared_but_net_names_are():
    def track(code, name):
        return [{"@type": BOARD + "Track", "id": {"value": EXISTING[1]}, "net": {"code": {"value": code}, "name": name}}]
    assert compare([call("live_pcb_add_via", text({}), after=track(3, "/GND"))],
                   [call("live_pcb_add_via", text({}), after=track(5, "/GND"))], EXISTING) == []
    assert compare([call("live_pcb_add_via", text({}), after=track(3, "/GND"))],
                   [call("live_pcb_add_via", text({}), after=track(3, "/VIN"))], EXISTING)


def test_route_effect_compares_the_rest_exactly_and_routes_by_net_and_layer():
    def routed(x_nm, net="/VOUT", label_x="1000"):
        return [{"@type": BOARD + "Track", "id": {"value": str(uuid.uuid4())}, "layer": "BL_F_Cu",
                 "net": {"name": net}, "start": {"x_nm": x_nm}},
                {"@type": BOARD + "BoardText", "id": {"value": EXISTING[0]}, "position": {"x_nm": label_x}}]
    result = text({"segments": [{"layer": "F.Cu", "path_mm": [[0, 0], [1, 0]]}], "vias": [],
                   "length_mm": 1.0})

    def route(after):
        return call("live_pcb_route", result, mode="route", after=after)
    # A route a grid step apart: the same by behaviour
    assert not [f for f in compare([route(routed("100"))], [route(routed("150"))], EXISTING) if "document" in f]
    assert [f for f in compare([route(routed("100"))], [route(routed("100", net="/VIN"))], EXISTING)
            if "routed tracks" in f]
    assert [f for f in compare([route(routed("100"))], [route(routed("100", label_x="2000"))], EXISTING)
            if "document" in f]


def test_images_need_identical_pixels_except_kicad_cli_renders():
    def png(colour):
        raw = b"\x00" + bytes([colour, 0, 0])
        chunk = zlib.compress(raw)
        body = (b"\x89PNG\r\n\x1a\n" + struct.pack(">I", 13) + b"IHDR" + struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0)
                + b"\0\0\0\0" + struct.pack(">I", len(chunk)) + b"IDAT" + chunk + b"\0\0\0\0")
        return {"content": [{"type": "image", "mimeType": "image/png", "data": base64.b64encode(body).decode()}]}
    assert compare([call("live_pcb_view", png(10), mode="png")], [call("live_pcb_view", png(10), mode="png")]) == []
    assert compare([call("live_pcb_view", png(10), mode="png")], [call("live_pcb_view", png(11), mode="png")])
    assert compare([call("live_footprint_view_3d", png(10), mode="png-size")],
                   [call("live_footprint_view_3d", png(11), mode="png-size")]) == []


# ---- Finding 2: the post-route DRC checks every clearance type ---------------------------------

def drc(*kinds, items=()):
    violations = [{"type": kind, "severity": "ERROR", "description": kind, "position_mm": [1, 2],
                   "items": list(items)} for kind in kinds]
    return {"violation_count": len(violations), "by_type": {k: kinds.count(k) for k in kinds}, "violations": violations}


@pytest.mark.parametrize("kind", sorted(__import__("board_design").CHECK_CATEGORIES["clearance"]))
def test_route_drc_fails_on_every_clearance_type(kind):
    failed = compare([call("live_pcb_drc", text(drc(kind)), mode="route-drc")],
                     [call("live_pcb_drc", text(drc(kind)), mode="route-drc")])
    assert failed and "clearance violations" in failed[0]


def test_edge_clearance_is_a_clearance_violation():
    assert lw.clearance_violations(drc("edge_clearance", "silk_overlap")) == drc("edge_clearance")["violations"]


def test_route_drc_checks_the_check_tool_by_category():
    check = {"clean": False, "counts": {"clearance": 1}, "violations": {"clearance": drc("copper_sliver")["violations"]}}
    failed = compare([call("live_pcb_check", text(check), mode="route-drc")],
                     [call("live_pcb_check", text(check), mode="route-drc")])
    assert failed and "copper_sliver" in failed[0]


def test_route_drc_compares_other_violations_exactly():
    python = drc("silk_overlap", items=[EXISTING[0]])
    rust = copy.deepcopy(python)
    rust["violations"][0]["position_mm"] = [5, 5]
    assert compare([call("live_pcb_drc", text(python), mode="route-drc")],
                   [call("live_pcb_drc", text(rust), mode="route-drc")], EXISTING)
    # A violation naming a routed item (an id never seen before) by type and severity only
    python = drc("dangling_track", items=[str(uuid.uuid4())])
    rust = drc("dangling_track", items=[str(uuid.uuid4())])
    rust["violations"][0]["position_mm"] = [5, 5]
    assert compare([call("live_pcb_drc", text(python), mode="route-drc")],
                   [call("live_pcb_drc", text(rust), mode="route-drc")], EXISTING) == []


# ---- Finding 5: a crossing track both servers accepted fails ------------------------------------

class FakeSession:
    def __init__(self, result):
        self.result = result

    def call(self, tool, arguments):
        return self.result


class FakeDocument:
    """A Tracked document whose snapshot an accepted call changes."""
    name, undoable = "board", True

    def __init__(self):
        self.items = []

    def snapshot(self):
        return list(self.items)

    def open_commits(self):
        return 0

    def undo(self, redo=False):
        return {"performed": True}


def test_accepted_crossing_track_raises():
    recording = lw.Recording("rust", FakeSession(text({"added": 1})))
    with pytest.raises(lw.WorkflowError, match="did not fail"):
        recording("live_pcb_add_tracks", {}, mode="route", expect_error=True)


def test_refusal_that_changed_the_document_breaks_the_contract():
    document = FakeDocument()

    class Changing(FakeSession):
        def call(self, tool, arguments):
            document.items.append({"@type": BOARD + "Track", "id": {"value": "x"}})
            return self.result
    recording = lw.Recording("rust", Changing(error("conflicts with other copper")))
    recording("live_pcb_add_tracks", {}, mode="route", expect_error=True, doc=document)
    outcome = recording.contract[-1]
    assert outcome["refused"] and outcome["changed"] and not outcome.get("restored")


# ---- Finding 6: existing ids keep their value; revisions only where KiCad quotes them ------------

def test_pointing_at_the_wrong_existing_item_is_caught():
    python = [call("live_schematic_move", text({"moved": [EXISTING[0]]}))]
    rust = [call("live_schematic_move", text({"moved": [EXISTING[1]]}))]
    assert compare(python, rust) == []  # unseeded, both are just "the first id"
    assert compare(python, rust, EXISTING)


def test_revisions_are_named_only_where_kicad_quotes_them():
    names = lw.Normaliser()
    assert names("lot 20261121-3 shipped") == "lot 20261121-3 shipped"
    assert names("the document is at revision 0a1b2c3d-5, not stale-0") == "the document is at revision <rev1>, not stale-0"
    assert names("(revision 0a1b2c3d-5 to 0a1b2c3d-7)") == "(revision <rev1> to <rev2>)"
    assert names({"revision": "0a1b2c3d-7"}) == {"revision": "<rev2>"}


def test_scratch_folder_is_kept_when_an_editor_is_not_released(tmp_path, monkeypatch):
    """Deleting a scratch library an editor still watches opens a modal error in KiCad."""
    import e2e_guard

    def park_fails(checker):
        raise RuntimeError("AS_BUSY")

    monkeypatch.setattr(e2e_guard, "park_footprint_editor", park_fails)
    monkeypatch.setattr(lw, "remove_scratch_libraries", lambda ctx: [])
    monkeypatch.setattr(lw, "restore_library_tables", lambda ctx, tables: None)
    scratch = tmp_path / "scratch"
    scratch.mkdir()
    ctx = SimpleNamespace(libraries_used=True, scratch=scratch,
                          checker=SimpleNamespace(edit_symbol=lambda lib_id: None))
    problems = lw.final_cleanup(ctx, {})
    assert scratch.is_dir()
    assert any("parking the Footprint Editor" in p for p in problems)
    assert any("was kept" in p for p in problems)
    # Released editors: the folder goes
    monkeypatch.setattr(e2e_guard, "park_footprint_editor", lambda checker: None)
    assert lw.final_cleanup(ctx, {}) == []
    assert not scratch.exists()
