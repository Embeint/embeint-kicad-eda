# SPDX-License-Identifier: GPL-3.0-or-later
"""Ownership, connectivity geometry and failure contracts; no live editor required."""

from dataclasses import FrozenInstanceError
import uuid

from google.protobuf import any_pb2
import pytest

from block_helpers import (ApplyUncertain, Block, Manifest, StaleState, apply,
                           fingerprint, plan)
from client import LiveSchematic, editor, kiid, point, sch, types


NAMESPACE = "71f9c108-b834-4cf7-a26c-6886521bdbca"


def doc():
    return types.DocumentSpecifier(type=types.DOCTYPE_SCHEMATIC,
                                   project=types.ProjectSpecifier(name="synthetic", path="/tmp/fixture/"),
                                   sheet_path=types.SheetPath(path=[kiid(NAMESPACE)]))


def pack(item):
    result = any_pb2.Any()
    result.Pack(item)
    return result


def unpack(packed):
    result = getattr(sch, packed.TypeName().rsplit(".", 1)[-1])()
    assert packed.Unpack(result)
    return result


class Editor(LiveSchematic):
    """Exercise real batch/document/status/transaction code over an in-memory editor."""

    def __init__(self, items=()):
        super().__init__()
        self.live = {item.id.value: pack(item) for item in items}
        self.requests = []
        self.fail = None
        self.failure_command = "CreateItems"
        self.saved = None
        self.normalize = False
        self.side_effect = None

    def send(self, request, response_type):
        self.requests.append(request)
        assert request.header.document == doc()
        if isinstance(request, editor.GetItems):
            return editor.GetItemsResponse(status=types.IRS_OK, items=list(self.live.values()))
        if isinstance(request, editor.BeginCommit):
            self.saved = dict(self.live)
            return editor.BeginCommitResponse(id=kiid())
        if isinstance(request, editor.EndCommit):
            if self.fail == "end_timeout" and request.action == editor.CMA_COMMIT:
                raise TimeoutError("commit response lost after apply")
            if request.action == editor.CMA_DROP:
                if self.fail == "rollback_timeout":
                    raise TimeoutError("drop response lost")
                self.live = self.saved
            return editor.EndCommitResponse()
        if isinstance(request, editor.DeleteItems):
            result = editor.DeleteItemsResponse(status=types.IRS_OK)
            for value in request.item_ids:
                del self.live[value.value]
                result.deleted_items.add(id=value, status=editor.IDS_OK)
            return result
        if isinstance(request, (editor.CreateItems, editor.UpdateItems)):
            result = response_type(status=types.IRS_OK)
            results = result.created_items if isinstance(request, editor.CreateItems) else result.updated_items
            for index, value in enumerate(request.items):
                item = unpack(value)
                if self.normalize:
                    item.locked = types.LS_UNLOCKED
                self.live[item.id.value] = pack(item)
                rejected = (self.fail in ("reject", "rollback_timeout")
                            and type(request).__name__ == self.failure_command and index == 0)
                results.add(status=editor.ItemStatus(code=editor.ISC_INVALID_DATA if rejected else editor.ISC_OK,
                                                     error_message="fixture rejected" if rejected else ""),
                            item=pack(item))
            if self.side_effect:
                self.side_effect(self.live)
            if self.fail == "mutation_timeout":
                raise TimeoutError("mutation response lost after apply")
            return result
        raise AssertionError(type(request))


def manual():
    return sch.SchematicText(id=kiid(), text=types.Text(text="Keep this manual note", position=point(20, 20)))


def revision(branch=True):
    block = Block(NAMESPACE)
    block.frame("frame", "Synthetic interface", (25, 25, 100, 80))
    block.wire("trunk", "SIGNAL", [(40, 50), (70, 50)])
    if branch:
        block.wire("branch", "SIGNAL", [(55, 50), (55, 65)])
    return block


def test_repeated_apply_normalized_editor_and_manual_item_preserved():
    note = manual()
    client = Editor([note])
    client.normalize = True
    block = revision()
    state = apply(client, doc(), plan(block, Manifest.empty(NAMESPACE, doc()), doc(), client.items(doc()).items))
    assert client.live[note.id.value] == pack(note)
    restored = Manifest.from_json(state.to_json())
    assert restored == state
    pending = plan(revision(), restored, doc(), client.items(doc()).items)
    assert not pending.creates and not pending.updates and not pending.deletes
    client.requests.clear()
    assert apply(client, doc(), pending) == state
    assert [type(r).__name__ for r in client.requests] == ["GetItems"]
    assert client.live[note.id.value] == pack(note)


def route(name, a, b):
    """Logical key of a wire segment from a to b (mm); wire keys include geometry."""
    (x1, y1), (x2, y2) = (tuple(round(v * 1e6) for v in p) for p in (a, b))
    return f"route/{name}/{x1}/{y1}/{x2}/{y2}"


def test_block_revision_changes_only_route_connectivity_and_keeps_frame_and_manual():
    note = manual()
    client = Editor([note])
    first = revision()
    state = apply(client, doc(), plan(first, Manifest.empty(NAMESPACE, doc()), doc(), client.items(doc()).items))
    before = dict(client.live)
    pending = plan(revision(False), state, doc(), client.items(doc()).items)
    # Wires are replaced rather than updated, so KiCad's mid-batch cleanup never runs.
    assert pending.creates == (route("trunk", (40, 50), (70, 50)),) and not pending.updates
    assert set(pending.deletes) == {route("branch", (55, 50), (55, 65)), route("trunk", (40, 50), (55, 50)),
                                    route("trunk", (55, 50), (70, 50)), "junction/SIGNAL/55000000/50000000"}
    state2 = apply(client, doc(), pending)
    assert client.live[note.id.value] == before[note.id.value]
    for key in ("frame/box", "frame/title"):
        assert client.live[first.id(key)] == before[first.id(key)]
    wires = [unpack(value) for value in client.live.values() if value.Is(sch.SchematicLine.DESCRIPTOR)]
    assert len(wires) == 1
    assert wires[0].start == point(40, 50) and wires[0].end == point(70, 50)
    assert not plan(revision(False), state2, doc(), client.items(doc()).items).updates
    assert [r.action for r in client.requests if isinstance(r, editor.EndCommit)] == [editor.CMA_COMMIT] * 2


def test_changed_or_missing_owned_state_and_unowned_collision_refused():
    client = Editor()
    block = revision()
    initial = Manifest.empty(NAMESPACE, doc())
    state = apply(client, doc(), plan(block, initial, doc(), client.items(doc()).items))
    with pytest.raises(StaleState, match="collision"):
        plan(block, initial, doc(), client.items(doc()).items)
    key = block.id("frame/title")
    item = unpack(client.live[key])
    item.text.text = "Manual edit inside generated block"
    client.live[key] = pack(item)
    with pytest.raises(StaleState, match="changed or missing"):
        plan(block, state, doc(), client.items(doc()).items)
    del client.live[key]
    with pytest.raises(StaleState, match="changed or missing"):
        plan(block, state, doc(), client.items(doc()).items)


def test_fingerprint_detects_manual_edit_after_plan_before_any_mutation():
    note = manual()
    client = Editor([note])
    pending = plan(revision(), Manifest.empty(NAMESPACE, doc()), doc(), client.items(doc()).items)
    note.text.text = "User typed more"
    client.live[note.id.value] = pack(note)
    client.requests.clear()
    with pytest.raises(StaleState, match="Sheet changed"):
        apply(client, doc(), pending)
    assert [type(r).__name__ for r in client.requests] == ["GetItems"]


@pytest.mark.parametrize("fail", ["reject", "mutation_timeout", "end_timeout", "rollback_timeout"])
def test_failed_or_unknown_apply_is_consumed_and_never_replayed(fail):
    note = manual()
    client = Editor([note])
    client.fail = fail
    state = Manifest.empty(NAMESPACE, doc())
    pending = plan(revision(), state, doc(), client.items(doc()).items)
    with pytest.raises(ApplyUncertain, match="Do not replay"):
        apply(client, doc(), pending)
    assert sum(isinstance(r, editor.CreateItems) for r in client.requests) == 1
    count = len(client.requests)
    with pytest.raises(StaleState, match="already attempted"):
        apply(client, doc(), pending)
    assert len(client.requests) == count
    assert client.live[note.id.value] == pack(note)
    if fail in ("reject", "mutation_timeout"):
        # Only a new observed snapshot permits a fresh plan after confirmed rollback.
        new = plan(revision(), state, doc(), client.items(doc()).items)
        assert new is not pending and new.creates
    else:
        # Lost EndCommit/drop may leave the created IDs present. They cannot be
        # silently adopted from an old empty manifest or blindly created again.
        with pytest.raises(StaleState, match="collision"):
            plan(revision(), state, doc(), client.items(doc()).items)


def test_failed_create_after_delete_rolls_back_entire_revision():
    client = Editor([manual()])
    state = apply(client, doc(), plan(revision(), Manifest.empty(NAMESPACE, doc()), doc(), client.items(doc()).items))
    before = dict(client.live)
    client.fail, client.failure_command = "reject", "CreateItems"
    pending = plan(revision(False), state, doc(), client.items(doc()).items)
    with pytest.raises(ApplyUncertain):
        apply(client, doc(), pending)
    assert client.live == before
    assert client.requests[-1].action == editor.CMA_DROP


def test_postread_detects_external_writer_in_race_window():
    note = manual()
    client = Editor([note])
    def external_write(live):
        note.text.text = "Other writer after precheck"
        live[note.id.value] = pack(note)
    client.side_effect = external_write
    pending = plan(revision(), Manifest.empty(NAMESPACE, doc()), doc(), client.items(doc()).items)
    with pytest.raises(ApplyUncertain, match="unrelated item changed"):
        apply(client, doc(), pending)
    assert any(isinstance(r, editor.EndCommit) and r.action == editor.CMA_COMMIT for r in client.requests)


def test_exact_document_guard_and_plan_immutable_payload():
    block = revision()
    client = Editor()
    pending = plan(block, Manifest.empty(NAMESPACE, doc()), doc(), [])
    other = doc()
    other.project.path = "/different-project/"
    with pytest.raises(ValueError, match="exact document"):
        apply(client, other, pending)
    assert not client.requests
    with pytest.raises(ValueError, match="exact document"):
        plan(block, pending.manifest, other, [])
    with pytest.raises(FrozenInstanceError):
        pending.deletes = ("manual",)


def test_fingerprint_order_independent_and_duplicate_uuid_refused():
    first, second = pack(manual()), pack(manual())
    assert fingerprint([first, second]) == fingerprint([second, first])
    with pytest.raises(ValueError, match="Duplicate"):
        fingerprint([first, first])


def test_postread_failure_reports_unknown_and_prevents_replay():
    client = Editor()
    original_items = client.items
    reads = 0
    def items(target):
        nonlocal reads
        reads += 1
        if reads == 2:
            raise TimeoutError("post-commit read lost")
        return original_items(target)
    pending = plan(revision(), Manifest.empty(NAMESPACE, doc()), doc(), [])
    client.items = items
    with pytest.raises(ApplyUncertain, match="post-commit read lost"):
        apply(client, doc(), pending)
    assert client.live
    with pytest.raises(StaleState, match="already attempted"):
        apply(client, doc(), pending)


def definition():
    symbol = sch.SchematicSymbol(id=types.LibraryIdentifier(library_nickname="Synthetic", entry_name="Unit"),
                                 unit_count=2)
    for number, unit, at in (("1", 1, (-5.08, 0)), ("2", 1, (5.08, 0)), ("3", 2, (0, 2.54))):
        pin = sch.SchematicPin(id=kiid(), number=number, position=point(*at),
                               electrical_type=types.EPT_UNSPECIFIED)
        child = symbol.items.add(unit=sch.SchematicSymbolUnit(unit=unit))
        child.item.Pack(pin)
    symbol.items.add().item.Pack(sch.SchematicField(name="MPN", text=types.Text(text="SYNTHETIC")))
    return symbol


def test_symbol_placement_uses_real_pin_endpoints_unit_and_preserves_pin_types():
    original = definition()
    block = Block(NAMESPACE)
    pins = block.symbol("part", original, "U101", "1", (40, 50))
    assert pins == {"1": (40, 50), "2": (50.16, 50)}
    assert block.pin("part", "2") == (50.16, 50)
    instance = block.items()["part"]
    assert instance.position == point(45.08, 50)
    assert instance.definition.id == original.id and instance.lib_id == original.id
    assert instance.user_fields[0].name == "MPN"
    assert instance.user_fields[0].text.text == "SYNTHETIC"
    assert not instance.user_fields[0].visible
    p = sch.SchematicPin()
    assert instance.definition.items[0].item.Unpack(p) and p.electrical_type == types.EPT_UNSPECIFIED
    assert p.id != unpack(original.items[0].item).id
    assert original.items[0].item != instance.definition.items[0].item
    with pytest.raises(ValueError, match="absent or ambiguous"):
        block.pin("part", "3")
    with pytest.raises(ValueError, match="exactly one"):
        Block(NAMESPACE).symbol("part", original, "U101", "3", (40, 50), unit=1)


def test_ambiguous_stacked_anchor_is_refused():
    symbol = definition()
    symbol.items.add().CopyFrom(symbol.items[0])
    with pytest.raises(ValueError, match="exactly one"):
        Block(NAMESPACE).symbol("part", symbol, "U101", "1", (40, 50))


def test_logical_key_cannot_alias_a_generated_child_identity():
    block = Block(NAMESPACE)
    block.symbol("part", definition(), "U101", "1", (40, 50))
    block.add("part/child/0", manual())
    with pytest.raises(ValueError, match="collide with generated child"):
        plan(block, Manifest.empty(NAMESPACE, doc()), doc(), [])


def test_synthetic_example_is_offline(monkeypatch):
    import block_example
    def no_editor(*args, **kwargs):
        raise AssertionError("Offline example contacted the editor")
    monkeypatch.setattr(LiveSchematic, "send", no_editor)
    result = block_example.offline_example()
    assert result["construction"]["junctions"] == 1
    assert result["plan"]["create"]
    assert result["plan"]["atomic_revision_check"] is False


@pytest.mark.parametrize("paths", [
    [[(10, 10), (30, 10)], [(20, 10), (20, 20)]],
    [[(30, 10), (10, 10)], [(20, 20), (20, 10)]],
    [[(10, 20), (30, 20)], [(20, 30), (20, 10)]],
])
def test_manhattan_joins_split_wire_segments_and_add_one_junction(paths):
    block = Block(NAMESPACE)
    for index, path in enumerate(paths):
        block.wire(str(index), "SIGNAL", path)
    items = block.items()
    junctions = [item for item in items.values() if isinstance(item, sch.Junction)]
    wires = [item for item in items.values() if isinstance(item, sch.SchematicLine)]
    assert len(junctions) == 1
    pos = junctions[0].position
    assert sum(w.start == pos or w.end == pos for w in wires) in (3, 4)
    assert all(w.start.x_nm == w.end.x_nm or w.start.y_nm == w.end.y_nm for w in wires)


@pytest.mark.parametrize("second,net", [([(15, 10), (25, 10)], "A"),
                                         ([(20, 5), (20, 15)], "B")])
def test_overlapping_or_different_net_routes_are_invalid(second, net):
    block = Block(NAMESPACE)
    block.wire("first", "A", [(10, 10), (30, 10)])
    block.wire("second", net, second)
    assert not block.report()["valid"]
    with pytest.raises(ValueError, match="overlap or different-net"):
        plan(block, Manifest.empty(NAMESPACE, doc()), doc(), [])


@pytest.mark.parametrize("routes", [
    [[(10, 10), (20, 10), (30, 10)]],
    [[(10, 10), (20, 10)], [(20, 10), (30, 10)]],
    [[(10, 10), (10, 20)], [(10, 30), (10, 20)]],
])
def test_inline_wire_ends_that_kicad_would_merge_are_refused(routes):
    # KiCad's commit cleanup merges these into one wire, deleting an owned UUID.
    block = Block(NAMESPACE)
    for index, path in enumerate(routes):
        block.wire(str(index), "SIGNAL", path)
    report = block.report()
    assert not report["valid"] and "merge" in report["errors"][0]


def test_ports_and_nc_pins_are_not_wire_exits_like_kicad():
    # JUNCTION_HELPERS::AnalyzePoint ignores labels, and SCH_SYMBOL skips NC pins.
    block = Block(NAMESPACE)
    block.wire("left", "SIGNAL", [(10, 10), (20, 10)])
    block.wire("right", "SIGNAL", [(20, 10), (30, 10)])
    block.port("port", "SIGNAL", (20, 10))
    assert not block.report()["valid"]
    corner = Block(NAMESPACE)
    corner.wire("bend", "SIGNAL", [(10, 10), (20, 10), (20, 20)])
    corner.port("port", "SIGNAL", (20, 10))
    assert corner.report()["junctions"] == 0
    symbol = definition()
    pin = unpack(symbol.items[0].item)
    pin.electrical_type = types.EPT_NO_CONNECT
    symbol.items[0].item.Pack(pin)
    nc = Block(NAMESPACE)
    pins = nc.symbol("part", symbol, "U101", "1", (40, 50))
    nc.wire("a", "SIGNAL", [(30, 50), pins["1"]])
    nc.wire("b", "SIGNAL", [pins["1"], (40, 40)])
    assert nc.report()["junctions"] == 0


def test_inline_or_branching_wires_at_a_symbol_pin_get_a_junction_not_a_merge():
    block = Block(NAMESPACE)
    pins = block.symbol("part", definition(), "U101", "1", (40, 50))
    block.wire("a", "SIGNAL", [pins["1"], (30, 50)])
    block.wire("b", "SIGNAL", [pins["1"], (40, 60)])
    junctions = [i for i in block.items().values() if isinstance(i, sch.Junction)]
    assert [j.position for j in junctions] == [point(40, 50)]
    inline = Block(NAMESPACE)
    pins = inline.symbol("part", definition(), "U101", "2", (40, 50))
    inline.wire("a", "SIGNAL", [(30, 50), pins["2"]])
    inline.wire("b", "SIGNAL", [pins["2"], (50, 50)])
    assert inline.report()["junctions"] == 1


@pytest.mark.parametrize("revise", ["move", "delete"])
def test_moving_or_removing_a_pin_with_manual_wiring_attached_is_refused(revise):
    # CleanUp would drop the manual junction (and merge in-line manual wires) once
    # the block pin no longer supplies the third exit.
    client = Editor()
    first = Block(NAMESPACE)
    first.symbol("part", definition(), "U101", "1", (40, 50))
    state = apply(client, doc(), plan(first, Manifest.empty(NAMESPACE, doc()), doc(), []))
    assert Manifest.from_json(state.to_json()) == state
    manual_items = [sch.SchematicLine(id=kiid(), type=sch.SLT_WIRE, start=point(40, 50), end=point(30, 50)),
                    sch.SchematicLine(id=kiid(), type=sch.SLT_WIRE, start=point(40, 50), end=point(40, 40)),
                    sch.Junction(id=kiid(), position=point(40, 50))]
    client.live.update({item.id.value: pack(item) for item in manual_items})
    same = Block(NAMESPACE)
    same.symbol("part", definition(), "U102", "1", (40, 50))
    assert plan(same, state, doc(), client.items(doc()).items).updates == ("part",)
    revised = Block(NAMESPACE)
    if revise == "move":
        revised.symbol("part", definition(), "U101", "1", (40, 60))
    with pytest.raises(ValueError, match="manual wiring attached"):
        plan(revised, state, doc(), client.items(doc()).items)


@pytest.mark.parametrize("item", [
    sch.SchematicLine(type=sch.SLT_WIRE, start=point(60, 50), end=point(60, 40)),
    sch.SchematicLine(type=sch.SLT_WIRE, start=point(70, 50), end=point(80, 50)),
    sch.SchematicLine(type=sch.SLT_WIRE, start=point(30, 30), end=point(40, 50)),
    sch.Junction(position=point(55, 50)),
])
def test_block_wires_touching_manual_wires_or_junctions_are_refused(item):
    item.id.CopyFrom(kiid())
    client = Editor([item])
    with pytest.raises(ValueError, match="manual"):
        plan(revision(), Manifest.empty(NAMESPACE, doc()), doc(), client.items(doc()).items)
    assert not any(isinstance(r, editor.BeginCommit) for r in client.requests)


def test_rerouting_a_leg_plans_no_wire_updates():
    # A changed wire is a delete plus a create; UpdateItems would make KiCad clean up
    # mid-batch on a screen where the old wires and junction are still present.
    client = Editor()
    state = apply(client, doc(), plan(revision(), Manifest.empty(NAMESPACE, doc()), doc(), []))
    revised = Block(NAMESPACE)
    revised.frame("frame", "Synthetic interface", (25, 25, 100, 80))
    revised.wire("trunk", "SIGNAL", [(40, 45), (40, 50), (70, 50)])
    revised.wire("branch", "SIGNAL", [(55, 50), (55, 65)])
    pending = plan(revised, state, doc(), client.items(doc()).items)
    assert not pending.updates and "junction/SIGNAL/55000000/50000000" not in pending.deletes
    apply(client, doc(), pending)


def two_wire_pin(client):
    """Owned symbol whose pin 1 at (40, 50) is a branch of two owned wires."""
    first = Block(NAMESPACE)
    pins = first.symbol("part", definition(), "U101", "1", (40, 50))
    first.wire("a", "SIGNAL", [pins["1"], (30, 50)])
    first.wire("b", "SIGNAL", [pins["1"], (40, 40)])
    return apply(client, doc(), plan(first, Manifest.empty(NAMESPACE, doc()), doc(), []))


def test_moving_a_symbol_pin_at_a_wire_branch_is_refused_but_single_wire_moves_are_planned():
    client = Editor()
    state = two_wire_pin(client)
    moved = Block(NAMESPACE)
    pins = moved.symbol("part", definition(), "U101", "1", (40, 60))
    moved.wire("a", "SIGNAL", [pins["1"], (30, 60)])
    moved.wire("b", "SIGNAL", [pins["1"], (40, 70)])
    with pytest.raises(ValueError, match="wire branch"):
        plan(moved, state, doc(), client.items(doc()).items)
    single = Editor()
    first = Block(NAMESPACE)
    first.wire("a", "SIGNAL", [first.symbol("part", definition(), "U101", "1", (40, 50))["1"], (30, 50)])
    state = apply(single, doc(), plan(first, Manifest.empty(NAMESPACE, doc()), doc(), []))
    moved = Block(NAMESPACE)
    moved.wire("a", "SIGNAL", [moved.symbol("part", definition(), "U101", "1", (40, 60))["1"], (30, 60)])
    pending = plan(moved, state, doc(), single.items(doc()).items)
    assert pending.updates == ("part",)


def test_removing_an_owned_wire_with_manual_wiring_attached_is_refused():
    client = Editor()
    state = apply(client, doc(), plan(revision(), Manifest.empty(NAMESPACE, doc()), doc(), []))
    # The user later wired to the end of the block branch, with KiCad's junction.
    attached = [sch.SchematicLine(id=kiid(), type=sch.SLT_WIRE, start=point(45, 65), end=point(55, 65)),
                sch.SchematicLine(id=kiid(), type=sch.SLT_WIRE, start=point(55, 65), end=point(65, 65)),
                sch.Junction(id=kiid(), position=point(55, 65))]
    client.live.update({item.id.value: pack(item) for item in attached})
    assert not plan(revision(), state, doc(), client.items(doc()).items).deletes
    with pytest.raises(ValueError, match="manual wiring attached"):
        plan(revision(False), state, doc(), client.items(doc()).items)
    with pytest.raises(ValueError, match="manual wiring attached"):
        plan(Block(NAMESPACE), state, doc(), client.items(doc()).items)


@pytest.mark.parametrize("item", [
    sch.SchematicLine(type=sch.SLT_WIRE), sch.SchematicLine(type=sch.SLT_BUS), sch.Junction(),
    sch.SchematicSymbolInstance(), sch.SheetSymbol()])
def test_add_refuses_items_the_cleanup_and_pin_model_must_build(item):
    with pytest.raises(ValueError, match="wire\\(\\), symbol\\(\\) or sheet\\(\\)"):
        Block(NAMESPACE).add("item", item)
    Block(NAMESPACE).add("graphic", sch.SchematicLine(type=sch.SLT_GRAPHIC, start=point(0, 0), end=point(5, 0)))


def test_duplicate_no_connect_markers_are_refused():
    block = Block(NAMESPACE)
    block.add("nc1", sch.NoConnectMarker(position=point(10, 10)))
    block.add("nc2", sch.NoConnectMarker(position=point(10, 10)))
    assert not block.report()["valid"]
    existing = sch.NoConnectMarker(id=kiid(), position=point(20, 20))
    single = Block(NAMESPACE)
    single.add("nc", sch.NoConnectMarker(position=point(20, 20)))
    with pytest.raises(ValueError, match="would share"):
        plan(single, Manifest.empty(NAMESPACE, doc()), doc(), [pack(existing)])
    # Renaming an owned marker's key at the same place is safe: the delete lands at the
    # push, before the final cleanup, and the create never appears mid-batch.
    client = Editor()
    state = apply(client, doc(), plan(single, Manifest.empty(NAMESPACE, doc()), doc(), []))
    renamed = Block(NAMESPACE)
    renamed.add("nc-renamed", sch.NoConnectMarker(position=point(20, 20)))
    pending = plan(renamed, state, doc(), client.items(doc()).items)
    assert pending.creates == ("nc-renamed",) and pending.deletes == ("nc",)


def test_moving_a_block_label_with_manual_wiring_through_it_is_refused():
    # In KiCad's junction analysis a label makes wires passing through count twice,
    # so it can be what keeps manual in-line wires from merging.
    client = Editor()
    first = Block(NAMESPACE)
    first.port("port", "SIGNAL", (50, 50))
    state = apply(client, doc(), plan(first, Manifest.empty(NAMESPACE, doc()), doc(), []))
    for start, end in (((40, 50), (50, 50)), ((50, 50), (60, 50)), ((50, 40), (50, 60))):
        wire = sch.SchematicLine(id=kiid(), type=sch.SLT_WIRE, start=point(*start), end=point(*end))
        client.live[wire.id.value] = pack(wire)
    moved = Block(NAMESPACE)
    moved.port("port", "SIGNAL", (70, 70))
    with pytest.raises(ValueError, match="pin or label"):
        plan(moved, state, doc(), client.items(doc()).items)
    with pytest.raises(ValueError, match="pin or label"):
        plan(Block(NAMESPACE), state, doc(), client.items(doc()).items)
    renamed = Block(NAMESPACE)
    renamed.port("port", "OTHER", (50, 50))
    assert plan(renamed, state, doc(), client.items(doc()).items).updates == ("port",)


def test_manifest_without_pin_points_is_refused_with_a_version_error():
    state = apply(Editor(), doc(), plan(revision(), Manifest.empty(NAMESPACE, doc()), doc(), []))
    old = state.to_json().replace('"version": 2', '"version": 1')
    with pytest.raises(ValueError, match="version 1"):
        Manifest.from_json(old)


def test_plain_crossing_of_a_manual_wire_is_allowed_and_preserved():
    crossing = sch.SchematicLine(id=kiid(), type=sch.SLT_WIRE, start=point(65, 40), end=point(65, 60))
    client = Editor([crossing])
    apply(client, doc(), plan(revision(), Manifest.empty(NAMESPACE, doc()), doc(), client.items(doc()).items))
    assert client.live[crossing.id.value] == pack(crossing)


def test_stored_wire_geometry_differing_from_request_is_reported_uncertain():
    client = Editor()
    block = revision()
    target = block.id(route("trunk", (40, 50), (55, 50)))
    def shift(live):
        wire = unpack(live[target])
        wire.end.x_nm += 1270000
        live[target] = pack(wire)
    client.side_effect = shift
    with pytest.raises(ApplyUncertain, match="Stored geometry differs"):
        apply(client, doc(), plan(block, Manifest.empty(NAMESPACE, doc()), doc(), []))


def test_owned_hashes_with_map_fields_match_across_processes():
    # Manifests are written by one process and checked by another; map field order
    # must not change the recorded hash.
    import subprocess
    import sys
    from pathlib import Path
    script = ("import block_helpers as b; from client import sch\n"
              "s = sch.SheetSymbol(); v = s.variants.variants.add(name='A')\n"
              "[v.fields.__setitem__(f'F{i}', str(i)) for i in range(40)]\n"
              "print(b._hash(b._pack(s)))")
    here = Path(__file__).resolve().parent
    hashes = {subprocess.run([sys.executable, "-c", script], cwd=here, check=True,
                             capture_output=True, text=True).stdout for _ in range(3)}
    assert len(hashes) == 1


def test_hierarchy_ports_and_sheet_pins_have_matching_names_and_real_edges():
    parent, child = Block(NAMESPACE), Block(str(uuid.uuid4()))
    endpoints = parent.sheet("child", "Interface", "interface.kicad_sch", (100, 50), (30, 25), "2",
                             [("TX", "left", 5.08, "input"), ("RX", "right", 10.16, "output")])
    child.port("tx", "TX", (50, 50), "input")
    child.port("rx", "RX", (80, 50), "output", "right")
    sheet = parent.items()["child"]
    assert endpoints == {"TX": (100, 55.08), "RX": (130, 60.16)}
    assert sheet.pins[0].position == point(100, 55.08) and sheet.pins[0].side == sch.SHS_LEFT
    assert {pin.text.text for pin in sheet.pins} == {label.text.text for label in child.items().values()}
    client = Editor()
    state = apply(client, doc(), plan(parent, Manifest.empty(NAMESPACE, doc()), doc(), []))
    with pytest.raises(ValueError, match="Deleting a sheet"):
        plan(Block(NAMESPACE), state, doc(), client.items(doc()).items)
    retarget = Block(NAMESPACE)
    retarget.sheet("child", "Interface", "elsewhere.kicad_sch", (100, 50), (30, 25), "2", [])
    with pytest.raises(ValueError, match="Retargeting"):
        plan(retarget, state, doc(), client.items(doc()).items)
