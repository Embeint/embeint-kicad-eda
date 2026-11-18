# SPDX-License-Identifier: GPL-3.0-or-later
"""Regression contracts from the IPC/MCP review; no running editor is required."""

from types import SimpleNamespace
import uuid

from google.protobuf.any_pb2 import Any
import pytest

from board_client import bc, board_rules, board_tasks, bt, tasks
from client import as_dict, editor, types
from client import commands as sch_commands
from test_client import FakeClient, board_doc, doc, wire


@pytest.mark.parametrize("command", [
    "SetPageSettings", "SetTitleBlockInfo", "SetTextVariables", "SetNetClasses",
    "UpdateBoardStackup", "SetBoardOrigin", "RunAction", "StartRefillZonesTask",
    "NewLibrarySymbol", "EditLibraryFootprint",
])
def test_nontransactional_batch_commands_fail_before_opening_commit(command):
    client = FakeClient(lambda *_: pytest.fail("a rejected batch must not contact KiCad"))
    with pytest.raises(ValueError, match="cannot run inside a batch"):
        client.batch(doc(), [{"command": command, "payload": {}}])
    assert client.requests == []


@pytest.mark.parametrize("command,target_path", [
    ("GetItems", ("header", "document")),
    ("TransformItems", ("document",)),
    ("GetBoardStackup", ("board",)),
])
def test_missing_batch_document_is_bound_to_the_transaction(command, target_path):
    target = board_doc() if command == "GetBoardStackup" else doc()

    def reply(request, response_type):
        actual = request
        for name in target_path:
            actual = getattr(actual, name)
        assert actual == target
        if command == "GetItems":
            return editor.GetItemsResponse(status=types.IRS_OK)
        return response_type()

    client = FakeClient(reply)
    assert client.batch(target, [{"command": command}])["committed"]
    assert client.requests[0].header.document == target
    assert client.requests[-1].action == editor.CMA_COMMIT


@pytest.mark.parametrize("command,target_path", [
    ("GetItems", ("header", "document")),
    ("TransformItems", ("document",)),
    ("GetBoardStackup", ("board",)),
])
def test_explicit_cross_document_batch_target_is_rejected_before_commit(command, target_path):
    target = board_doc() if command == "GetBoardStackup" else doc()
    other = types.DocumentSpecifier()
    other.CopyFrom(target)
    if other.type == types.DOCTYPE_PCB:
        other.board_filename = "another-board.kicad_pcb"
    else:
        other.sheet_path.path[0].value = str(uuid.uuid4())
    payload = as_dict(other)
    for name in reversed(target_path):
        payload = {name: payload}
    client = FakeClient(lambda *_: pytest.fail("cross-document batches must be rejected locally"))
    with pytest.raises(ValueError, match="document"):
        client.batch(target, [{"command": command, "payload": payload}])
    assert client.requests == []


def _two_item_payload(command):
    identifiers = [str(uuid.uuid4()), str(uuid.uuid4())]
    if command in ("CreateItems", "UpdateItems"):
        items = []
        for identifier in identifiers:
            item = wire()
            item.id.value = identifier
            packed = Any()
            packed.Pack(item)
            items.append(as_dict(packed))
        return {"items": items}
    field = {"DeleteItems": "item_ids", "UpdateSymbolsFromLibrary": "symbols"}.get(command, "items")
    return {field: [{"value": value} for value in identifiers]}


def _acknowledgements(command, count, rejected=False):
    if command == "UpdateSymbolsFromLibrary":
        response = sch_commands.UpdateSymbolsFromLibraryResponse()
        for index in range(count):
            bad = rejected and index == count - 1
            response.symbols.add(updated=not bad, error_message="library symbol unavailable" if bad else "")
        return response
    response_type, field = {
        "CreateItems": (editor.CreateItemsResponse, "created_items"),
        "UpdateItems": (editor.UpdateItemsResponse, "updated_items"),
        "DeleteItems": (editor.DeleteItemsResponse, "deleted_items"),
        "TransformBoardItems": (bc.TransformBoardItemsResponse, "items"),
    }[command]
    response = response_type(status=types.IRS_OK)
    for index in range(count):
        result = getattr(response, field).add()
        if command == "DeleteItems":
            result.status = editor.IDS_OK
        else:
            bad = rejected and index == count - 1
            result.status.code = editor.ISC_INVALID_DATA if bad else editor.ISC_OK
            if bad:
                result.status.error_message = "item cannot be transformed"
    return response


@pytest.mark.parametrize("command", [
    "CreateItems", "UpdateItems", "DeleteItems", "TransformBoardItems", "UpdateSymbolsFromLibrary",
])
def test_missing_item_acknowledgement_drops_the_batch(command):
    client = FakeClient(lambda *_: _acknowledgements(command, 1))
    target = board_doc() if command == "TransformBoardItems" else doc()
    with pytest.raises(RuntimeError):
        client.batch(target, [{"command": command, "payload": _two_item_payload(command)}])
    assert client.requests[-1].action == editor.CMA_DROP
    assert not any(isinstance(request, editor.EndCommit) and request.action == editor.CMA_COMMIT
                   for request in client.requests)


@pytest.mark.parametrize("command", ["TransformBoardItems", "UpdateSymbolsFromLibrary"])
def test_additional_per_item_rejection_shapes_drop_the_batch(command):
    client = FakeClient(lambda *_: _acknowledgements(command, 2, rejected=True))
    target = board_doc() if command == "TransformBoardItems" else doc()
    with pytest.raises(RuntimeError):
        client.batch(target, [{"command": command, "payload": _two_item_payload(command)}])
    assert client.requests[-1].action == editor.CMA_DROP


class RouteReviewClient(FakeClient):
    """A committed route followed by an intervening manual edit during DRC."""

    def __init__(self, verification_error=None, cleanup_error=None):
        super().__init__(lambda *_: editor.GetItemsResponse(status=types.IRS_OK))
        self.verification_error = verification_error
        self.cleanup_error = cleanup_error
        self.live_ids = {"existing-user-track"}
        self.new_ids = []
        self.deleted = []

    def route_problem(self, target):
        return SimpleNamespace(layers=["F.Cu", "B.Cu"], pads={
            "pad-a": {"net": "SIG", "position": (0, 0)},
            "pad-b": {"net": "SIG", "position": (5, 0)},
        })

    def _find_pad_id(self, problem, target, reference, number):
        return "pad-a" if reference == "U1" else "pad-b"

    def _net_sizes(self, *args):
        return 0.25, (0.6, 0.3)

    def _route_items(self, *args):
        found = SimpleNamespace(length=5.0, vias=[(2, 0)], checked=True,
                                segments=[("F.Cu", [(0, 0), (5, 0)])])
        items = [bt.Track(id=types.KIID(value=str(uuid.uuid4()))),
                 bt.Via(id=types.KIID(value=str(uuid.uuid4())))]
        return found, items

    def _pad_label(self, target, problem, pad_id):
        return pad_id

    def mutate(self, target, items, **kwargs):
        self.new_ids = [item.id.value for item in items]
        self.live_ids.update(self.new_ids)

    def _verify(self, target, new_ids):
        self.live_ids.add("intervening-user-edit")
        if self.verification_error:
            raise self.verification_error
        return [{"type": "clearance", "items": list(new_ids), "description": "new route violates clearance"}]

    def delete(self, target, identifiers):
        self.deleted.append(list(identifiers))
        if self.cleanup_error:
            raise self.cleanup_error
        self.live_ids.difference_update(identifiers)
        return {"deleted": len(identifiers)}

    def undo(self, *args, **kwargs):
        pytest.fail("route rejection must not undo the latest manual edit")


def _run_route(client, whole_net):
    if whole_net:
        return client.board_route_net(board_doc(), "SIG")
    return client.board_route_avoiding(board_doc(), "U1", "1", "U2", "1")


@pytest.mark.parametrize("whole_net", [False, True])
def test_rejected_route_deletes_only_its_new_tracks_and_vias(whole_net):
    client = RouteReviewClient()
    with pytest.raises(RuntimeError):
        _run_route(client, whole_net)
    assert client.deleted == [client.new_ids]
    assert client.live_ids == {"existing-user-track", "intervening-user-edit"}


@pytest.mark.parametrize("whole_net", [False, True])
def test_incomplete_verification_preserves_and_identifies_the_unverified_route(whole_net):
    client = RouteReviewClient(verification_error=TimeoutError("DRC response lost"))
    with pytest.raises(RuntimeError, match="Route remains in the editor; DRC verification failed") as caught:
        _run_route(client, whole_net)
    assert "DRC response lost" in str(caught.value)
    assert all(identifier in str(caught.value) for identifier in client.new_ids)
    assert client.deleted == []
    assert client.live_ids == {"existing-user-track", "intervening-user-edit", *client.new_ids}


@pytest.mark.parametrize("whole_net", [False, True])
def test_route_cleanup_failure_is_reported_without_claiming_success(whole_net):
    client = RouteReviewClient(cleanup_error=RuntimeError("cleanup transport lost"))
    with pytest.raises(RuntimeError, match="cleanup transport lost"):
        _run_route(client, whole_net)
    assert client.deleted == [client.new_ids]
    assert set(client.new_ids) <= client.live_ids
    assert {"existing-user-track", "intervening-user-edit"} <= client.live_ids


def test_route_verification_sees_manufacturing_errors_after_the_display_limit():
    new_id = str(uuid.uuid4())
    result = board_tasks.DrcTaskResult()
    for index in range(201):
        marker = result.violations.add(error_type=board_rules.DRCET_CLEARANCE,
                                       severity=types.RS_ERROR, description="existing violation")
        marker.items.add(value=f"unrelated-{index}")
    marker = result.violations.add(error_type=board_rules.DRCET_TRACK_WIDTH,
                                   severity=types.RS_ERROR, description="new track below minimum width")
    marker.items.add(value=new_id)

    def reply(request, response_type):
        if isinstance(request, board_tasks.StartDrcTask):
            return tasks.StartTaskResponse(token=tasks.TaskToken(value="review-drc"))
        if isinstance(request, tasks.GetTaskStatus):
            return tasks.TaskStatusResponse(state=tasks.TS_COMPLETED)
        assert isinstance(request, board_tasks.GetDrcTaskResult)
        return result

    client = FakeClient(reply)
    displayed = client.board_drc(board_doc())
    assert displayed["violation_count"] == 202
    assert len(displayed["violations"]) == 200
    full = client.board_drc(board_doc(), limit=None)
    assert len(full["violations"]) == 202
    rejected = client._verify(board_doc(), [new_id])
    assert len(rejected) == 1
    assert rejected[0]["type"] == "track_width" and rejected[0]["items"] == [new_id]


@pytest.mark.parametrize("violation_type", ["track_width", "via_diameter", "annular_width", "drill_out_of_range"])
def test_route_verification_rejects_new_track_and_via_manufacturing_errors(violation_type):
    client = FakeClient()
    violation = {"type": violation_type, "items": ["new-copper"], "description": "manufacturing rule failed"}

    def report(*args, **kwargs):
        assert kwargs["limit"] is None
        return {"violations": [violation]}

    client.board_drc = report
    assert client._verify(board_doc(), ["new-copper"]) == [violation]
