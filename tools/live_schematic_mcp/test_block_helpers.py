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


def test_block_revision_changes_only_route_connectivity_and_keeps_frame_and_manual():
    note = manual()
    client = Editor([note])
    first = revision()
    state = apply(client, doc(), plan(first, Manifest.empty(NAMESPACE, doc()), doc(), client.items(doc()).items))
    before = dict(client.live)
    pending = plan(revision(False), state, doc(), client.items(doc()).items)
    assert pending.updates == ("route/trunk/0/0",)
    assert set(pending.deletes) == {"route/branch/0/0", "route/trunk/0/1",
                                    "junction/SIGNAL/55000000/50000000"}
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


def test_failed_update_after_delete_rolls_back_entire_revision():
    client = Editor([manual()])
    state = apply(client, doc(), plan(revision(), Manifest.empty(NAMESPACE, doc()), doc(), client.items(doc()).items))
    before = dict(client.live)
    client.fail, client.failure_command = "reject", "UpdateItems"
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
