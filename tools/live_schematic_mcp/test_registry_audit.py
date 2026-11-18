# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline adapter contracts for the P1.2 omission audit; no native editor evidence."""

import base64

from google.protobuf import json_format
from google.protobuf.empty_pb2 import Empty
import pytest

import commands
from client import LiveSchematic, as_dict, bind_batch_document, check_item_statuses, editor, envelope, types
from common.types import embedded_files_pb2 as embedded
from test_client import FakeClient, board_doc


def test_embedded_read_schema_and_exact_target():
    target = board_doc()
    full, request, response_type, kind = commands.build_request(
        "GetEmbeddedFiles", {"board": as_dict(target)})
    assert full == commands.B + "GetEmbeddedFiles"
    assert request.board == target
    assert response_type is embedded.EmbeddedFiles
    assert kind == "read"
    schema = commands.describe(full)
    assert schema["batchable"] is False
    assert schema["returns"] == "kiapi.common.types.EmbeddedFiles"
    assert schema["fields"] == [{"name": "board", "repeated": False,
                                 "type": "kiapi.common.types.DocumentSpecifier"}]


@pytest.mark.parametrize("populated", [False, True])
def test_embedded_execute_preserves_opaque_payload_and_never_opens_commit(populated):
    target = board_doc()
    response = embedded.EmbeddedFiles()
    # Synthetic stored bytes, deliberately not a claim of valid native zstd content.
    stored = b"opaque stored base64: \x00\xff"
    if populated:
        response.files.add(name="part.step", type=embedded.EFT_MODEL,
                           data=stored, data_hash="synthetic-hash")

    def reply(request, response_type):
        assert request.board == target
        assert response_type is embedded.EmbeddedFiles
        return response

    client = FakeClient(reply)
    result = client.execute("GetEmbeddedFiles", {"board": as_dict(target)})
    assert result["kind"] == "read"
    assert json_format.ParseDict(result["result"], embedded.EmbeddedFiles()) == response
    if populated:
        assert result["result"]["files"][0]["data"] == base64.b64encode(stored).decode()
    else:
        assert result["result"] == {}  # protobuf JSON omits the empty repeated field
    assert [request.DESCRIPTOR.name for request in client.requests] == ["GetEmbeddedFiles"]


def test_embedded_batch_is_rejected_before_opening_an_empty_commit():
    target = board_doc()

    def reply(request, response_type):
        assert request.board == target
        return embedded.EmbeddedFiles()

    client = FakeClient(reply)
    with pytest.raises(ValueError, match="cannot run inside a batch"):
        client.batch(target, [{"command": "GetEmbeddedFiles"}])
    assert client.requests == []
    assert commands.B + "GetEmbeddedFiles" not in commands.TRANSACTIONAL_COMMANDS


@pytest.mark.parametrize("change", ["filename", "project_name", "project_path", "type"])
def test_embedded_batch_rejects_different_target_before_contact(change):
    target = board_doc()
    other = types.DocumentSpecifier()
    other.CopyFrom(target)
    if change == "filename":
        other.board_filename = "other.kicad_pcb"
    elif change == "project_name":
        other.project.name = "other"
    elif change == "project_path":
        other.project.path = "/other/"
    else:
        other.type = types.DOCTYPE_SCHEMATIC
    _, request, _, _ = commands.build_request("GetEmbeddedFiles", {"board": as_dict(other)})
    with pytest.raises(ValueError, match="document exactly"):
        bind_batch_document(request, target)


@pytest.mark.parametrize("name", [
    "OpenLibraryItem", "ParseAndCreateItemsFromString", "LoadAllLibraries",
    "AddEmbeddedFiles", "SetEmbeddedFiles", "BeginCommit", "EndCommit", "InjectDrcError",
])
def test_unsafe_or_client_owned_commands_remain_unavailable(name):
    client = FakeClient(lambda *_: pytest.fail("excluded operation must not reach KiCad"))
    with pytest.raises(ValueError, match="Unknown command"):
        commands.describe(name)
    with pytest.raises(ValueError, match="Unknown command"):
        client.execute(name)
    with pytest.raises(ValueError, match="Unknown command"):
        client.batch(board_doc(), [{"command": name}])
    assert client.requests == []


@pytest.mark.parametrize("status", [types.IRS_UNKNOWN, types.IRS_DOCUMENT_NOT_FOUND,
                                    types.IRS_FIELD_MASK_INVALID, 999])
def test_parsed_creation_checks_overall_status_without_relying_on_command_suffix(status):
    response = editor.CreateItemsResponse(status=status)
    with pytest.raises(RuntimeError, match="returned"):
        check_item_statuses(0, commands.C + "ParseAndCreateItemsFromString", response)


@pytest.mark.parametrize("code", [editor.ISC_UNKNOWN, editor.ISC_INVALID_DATA])
def test_parsed_creation_rejects_partial_item_failure(code):
    response = editor.CreateItemsResponse(status=types.IRS_OK)
    response.created_items.add(status=editor.ItemStatus(code=editor.ISC_OK))
    response.created_items.add(status=editor.ItemStatus(code=code, error_message="parse child rejected"))
    with pytest.raises(RuntimeError, match="parse child rejected"):
        check_item_statuses(0, commands.C + "ParseAndCreateItemsFromString", response)


def test_unknown_creation_item_status_is_reported_as_rejection():
    response = editor.CreateItemsResponse(status=types.IRS_OK)
    response.created_items.add(status=editor.ItemStatus(code=999))
    with pytest.raises(RuntimeError, match="999"):
        check_item_statuses(0, commands.C + "ParseAndCreateItemsFromString", response)


def test_empty_unknown_creation_is_not_success_but_explicit_ok_can_mean_zero_items():
    with pytest.raises(RuntimeError, match="IRS_UNKNOWN"):
        check_item_statuses(0, commands.C + "ParseAndCreateItemsFromString", editor.CreateItemsResponse())
    # The protocol expressly permits zero items with IRS_OK; there is no input item count
    # for native strings. This cannot qualify the native parser, which remains excluded.
    check_item_statuses(0, commands.C + "ParseAndCreateItemsFromString",
                        editor.CreateItemsResponse(status=types.IRS_OK))


def test_get_items_by_id_also_checks_overall_status_and_drops_batch():
    # This supported command also lacks the "Items" suffix used by the old checker.
    client = FakeClient(lambda *_: editor.GetItemsResponse(status=types.IRS_DOCUMENT_NOT_FOUND))
    with pytest.raises(RuntimeError, match="IRS_DOCUMENT_NOT_FOUND"):
        client.batch(board_doc(), [{"command": "GetItemsById"}])
    assert client.requests[-1].action == editor.CMA_DROP


@pytest.mark.parametrize("status,wrong_reply", [
    (envelope.AS_BAD_REQUEST, False), (envelope.AS_UNHANDLED, False),
    (envelope.AS_BUSY, False), (envelope.AS_OK, True),
])
def test_embedded_execute_requires_ok_envelope_and_correct_reply(monkeypatch, status, wrong_reply):
    reply = envelope.ApiResponse()
    reply.status.status = status
    reply.status.error_message = "synthetic rejection"
    reply.message.Pack(Empty() if wrong_reply else embedded.EmbeddedFiles())
    calls = []

    class Connection:
        def __init__(self, **kwargs):
            pass

        def __enter__(self):
            return self

        def __exit__(self, *args):
            pass

        def dial(self, url, block):
            pass

        def send(self, data):
            calls.append(envelope.ApiRequest.FromString(data))

        def recv(self):
            return reply.SerializeToString()

    monkeypatch.setattr("client.pynng.Req0", Connection)
    client = LiveSchematic("ipc:///unused-p12-test-socket")
    with pytest.raises(RuntimeError, match="Unexpected reply" if wrong_reply else "AS_"):
        client.execute("GetEmbeddedFiles", {"board": as_dict(board_doc())})
    assert len(calls) == 1  # no retry, commit or save


def test_live_audit_requires_explicit_isolation_before_contact(monkeypatch):
    import e2e_registry_audit
    monkeypatch.delenv("KICAD_P12_ISOLATED", raising=False)
    monkeypatch.setattr(e2e_registry_audit, "LiveSchematic",
                        lambda: pytest.fail("must not connect without explicit isolation"))
    with pytest.raises(SystemExit, match="REFUSED"):
        e2e_registry_audit.main()


def test_live_audit_refuses_customer_board_without_opening_another_editor(monkeypatch):
    import e2e_registry_audit
    monkeypatch.setenv("KICAD_P12_ISOLATED", "1")
    monkeypatch.setenv("KICAD_API_SOCKET", "ipc:///unused-isolated-test-socket")
    monkeypatch.setenv("KICAD_LIVE_TEST_PROJECT", "live-demo")

    class CustomerSession:
        def boards(self):
            return [{"project": {"name": "customer"}}]

    monkeypatch.setattr(e2e_registry_audit, "LiveSchematic", CustomerSession)
    with pytest.raises(SystemExit, match="REFUSED"):
        e2e_registry_audit.main()


def test_live_audit_handles_fixture_named_like_old_negative_target(monkeypatch, tmp_path, capsys):
    import e2e_registry_audit
    monkeypatch.setenv("KICAD_P12_ISOLATED", "1")
    monkeypatch.setenv("KICAD_API_SOCKET", "ipc:///unused-isolated-test-socket")
    monkeypatch.setenv("KICAD_LIVE_TEST_PROJECT", "live-demo")
    monkeypatch.delenv("KICAD_P12_EXPECT_FILE", raising=False)
    target = board_doc()
    target.project.name = "live-demo"
    target.project.path = str(tmp_path)
    target.board_filename = "p12-not-open.kicad_pcb"
    board_file = tmp_path / target.board_filename
    board_file.write_bytes(b"synthetic disposable fixture")
    rejected_filenames = []

    def reply(request, response_type):
        if request.DESCRIPTOR.name == "GetEmbeddedFiles":
            if request.board.type != types.DOCTYPE_PCB:
                raise RuntimeError("AS_UNHANDLED: wrong type")
            if request.board.board_filename != target.board_filename:
                rejected_filenames.append(request.board.board_filename)
                raise RuntimeError("AS_BAD_REQUEST: board is not open")
            return embedded.EmbeddedFiles()
        if isinstance(request, editor.SaveDocumentToString):
            return editor.SavedDocumentResponse(document=target, contents="synthetic live board")
        return response_type()

    class FixtureClient(FakeClient):
        def boards(self):
            return [as_dict(target)]

    client = FixtureClient(reply)
    monkeypatch.setattr(e2e_registry_audit, "LiveSchematic", lambda: client)
    e2e_registry_audit.main()
    assert len(rejected_filenames) == 1
    assert "ok: embedded read" in capsys.readouterr().out
    assert board_file.read_bytes() == b"synthetic disposable fixture"
