# SPDX-License-Identifier: GPL-3.0-or-later
"""Native registration coverage contracts and synthetic drift cases; no live KiCad."""

from copy import deepcopy
import json
import socket

import pytest

from commands import COMMANDS
import operation_coverage as coverage


@pytest.fixture(scope="module")
def audited():
    messages = coverage.protocol_messages()
    native, parents = coverage.audit_native(coverage.ROOT, messages)
    return messages, native, parents, coverage.load_decisions()


def validate(audited, data=None, native=None, parents=None, commands=None):
    _, original_native, original_parents, original_data = audited
    coverage.validate_decisions(
        original_native if native is None else native,
        original_parents if parents is None else parents,
        original_data if data is None else data,
        COMMANDS if commands is None else commands,
        coverage.tool_names(coverage.HERE / "server.py"),
    )


def test_every_native_registration_has_an_explicit_review_decision(audited):
    validate(audited)
    _, native, parents, data = audited
    assert native.keys() == data["operations"].keys()
    assert coverage.lineage("API_HANDLER_FOOTPRINT", parents) == [
        "API_HANDLER_FOOTPRINT", "API_HANDLER_BOARD", "API_HANDLER_EDITOR"]
    assert coverage.lineage("API_HANDLER_PCB", parents) == [
        "API_HANDLER_PCB", "API_HANDLER_BOARD", "API_HANDLER_EDITOR"]
    assert coverage.lineage("API_HANDLER_SYMBOL", parents) == [
        "API_HANDLER_SYMBOL", "API_HANDLER_EDITOR"]
    assert {r.source for r in native.values()} >= {
        "common/api/api_handler_common.cpp", "common/api/api_handler_libraries.cpp",
        "common/api/api_handler_editor.cpp", "eeschema/api/api_handler_sch.cpp",
        "eeschema/api/api_handler_symbol.cpp", "pcbnew/api/api_handler_board.cpp",
        "pcbnew/api/api_handler_pcb.cpp", "pcbnew/api/api_handler_footprint.cpp"}


def test_new_native_handler_requires_decision_even_for_existing_proto(audited):
    messages, native, _, _ = audited
    new = coverage.parse_registrations("""
        registerHandler<kiapi::common::commands::Ping, google::protobuf::Empty>(
                &API_HANDLER_SCH::handleNewPing );
    """, "eeschema/api/new_handler.cpp", messages)[0]
    with pytest.raises(coverage.CoverageError, match="Missing native decision: API_HANDLER_SCH/.*Ping"):
        validate(audited, native={**native, new.key: new})


def test_new_source_and_inherited_only_handler_are_not_silently_ignored(tmp_path, audited):
    messages = audited[0]
    source = tmp_path / "pcbnew" / "new_api.cpp"
    source.parent.mkdir()
    source.write_text("""
        class API_HANDLER_NEW : public API_HANDLER {};
        registerHandler<kiapi::common::commands::Ping, google::protobuf::Empty>(
                &API_HANDLER_NEW::handlePing );
        HANDLER_RESULT<google::protobuf::Empty> API_HANDLER_NEW::handlePing(
                const HANDLER_CONTEXT<kiapi::common::commands::Ping>& ) { return {}; }
    """)
    native, parents = coverage.audit_native(tmp_path, messages)
    assert len(native) == 1
    assert parents == {"API_HANDLER_NEW": "API_HANDLER"}
    assert next(iter(native.values())).implementation_source == "pcbnew/new_api.cpp:5"
    with pytest.raises(coverage.CoverageError, match="No surface decision for handler class: API_HANDLER_NEW"):
        validate(audited, parents={**audited[2], "API_HANDLER_NEW": "API_HANDLER_EDITOR"})


def test_removed_registration_leaves_a_stale_decision(audited):
    native = dict(audited[1])
    key = next(iter(native))
    del native[key]
    with pytest.raises(coverage.CoverageError, match="Stale native decision"):
        validate(audited, native=native)


def test_duplicate_registration_and_missing_implementation_fail(tmp_path, audited):
    path = tmp_path / "common" / "api.cpp"
    path.parent.mkdir()
    declaration = """
        class API_HANDLER_NEW : public API_HANDLER {};
        registerHandler<Ping, google::protobuf::Empty>( &API_HANDLER_NEW::handlePing );
    """
    path.write_text(declaration)
    with pytest.raises(coverage.CoverageError, match="Cannot locate native implementation"):
        coverage.audit_native(tmp_path, audited[0])
    path.write_text(declaration + declaration)
    with pytest.raises(coverage.CoverageError, match="Duplicate native registration"):
        coverage.audit_native(tmp_path, audited[0])


def test_parser_ignores_comments_and_literals_preserves_evidence_and_task_flag(audited):
    source = '''// registerHandler<Missing, Nope>( &API_HANDLER_FAKE::fake );
        /* registerHandler<Missing, Nope>( &API_HANDLER_FAKE::fake ); */
        const char* s = "registerHandler<Missing, Nope>( &API_HANDLER_FAKE::fake );";
        const char* raw = R"tag(registerHandler<Missing, Nope>( &API_HANDLER_FAKE::fake );)tag";
        registerHandler < tasks::GetTaskStatus, kiapi::common::tasks::TaskStatusResponse > (
            &API_HANDLER_COMMON::handleGetTaskStatus, true );
    '''
    rows = coverage.parse_registrations(source, "synthetic.cpp", audited[0])
    assert len(rows) == 1
    assert rows[0].line == 5
    assert rows[0].request == "kiapi.common.tasks.GetTaskStatus"
    assert rows[0].allowed_during_task is True


@pytest.mark.parametrize("source", [
    "registerHandler<Ping, Empty>( customHandler );",
    "registerHandler<Ping, Empty>( &API_HANDLER_COMMON::handlePing, taskAllowed );",
    "registerHandler<Ping>( &API_HANDLER_COMMON::handlePing );",
])
def test_unrecognized_registration_syntax_fails_closed(source, audited):
    with pytest.raises(coverage.CoverageError, match="Unparsed registerHandler"):
        coverage.parse_registrations(source, "new.cpp", audited[0])


def test_unknown_or_ambiguous_proto_names_require_parser_review(audited):
    messages = {**audited[0], "other.commands.Ping": None}
    with pytest.raises(coverage.CoverageError, match="Cannot uniquely resolve"):
        coverage.resolve_cpp_type("Ping", messages)
    assert coverage.resolve_cpp_type("kiapi::common::commands::Ping", messages) == "kiapi.common.commands.Ping"
    with pytest.raises(coverage.CoverageError, match="Cannot uniquely resolve"):
        coverage.resolve_cpp_type("NotADeclaredMessage", messages)


@pytest.mark.parametrize("field,value", [
    ("decision", "automatic"), ("document", ""), ("disk_effects", ""),
    ("undo", "edit"), ("implementation", "probably"), ("registration_condition", ""),
    ("typed_tools", None), ("offline_tests", None),
])
def test_incomplete_or_invalid_decisions_fail(audited, field, value):
    data = deepcopy(audited[3])
    key = next(iter(data["operations"]))
    data["operations"][key][field] = value
    with pytest.raises(coverage.CoverageError, match="Invalid|Missing"):
        validate(audited, data=data)


@pytest.mark.parametrize("field,value,message", [
    ("typed_tools", ["live_deleted_tool"], "Stale typed tool"),
    ("offline_tests", ["test_client.py::test_deleted_case"], "Stale offline evidence"),
    ("surface_overrides", {"removed_editor": {"notes": "obsolete"}}, "Unknown override surface"),
])
def test_stale_mapping_and_evidence_references_fail(audited, field, value, message):
    data = deepcopy(audited[3])
    data["operations"][next(iter(data["operations"]))][field] = value
    with pytest.raises(coverage.CoverageError, match=message):
        validate(audited, data=data)


def test_duplicate_json_decisions_fail_instead_of_overwriting(tmp_path):
    path = tmp_path / "decisions.json"
    path.write_text('{"operations": {}, "operations": {}}')
    with pytest.raises(coverage.CoverageError, match="Duplicate decision key"):
        coverage.load_decisions(path)


def test_unknown_and_cyclic_inheritance_fail(audited):
    parents = dict(audited[2])
    parents["API_HANDLER_EDITOR"] = "API_HANDLER_SYMBOL"
    with pytest.raises(coverage.CoverageError, match="Unknown or cyclic"):
        validate(audited, parents=parents)


def test_adapter_response_drift_fails(audited):
    commands = {**COMMANDS, "kiapi.common.commands.GetItems": ("google.protobuf.Empty", "read")}
    with pytest.raises(coverage.CoverageError, match="Adapter response differs"):
        validate(audited, commands=commands)


def test_context_override_cannot_promote_a_stub_to_supported(audited):
    data = deepcopy(audited[3])
    key = "API_HANDLER_BOARD/kiapi.common.commands.ParseAndCreateItemsFromString"
    data["operations"][key]["surface_overrides"] = {"pcb": {
        "decision": "supported", "notes": "Still a stub; an adapter cannot qualify it."}}
    with pytest.raises(coverage.CoverageError, match="Stub cannot be supported for pcb"):
        validate(audited, data=data)


def test_new_adapter_for_deferred_operation_requires_explicit_exposure_reason(audited):
    request = "kiapi.common.commands.OpenLibraryItem"
    commands = {**COMMANDS, request: ("google.protobuf.Empty", "view")}
    with pytest.raises(coverage.CoverageError, match="Unexplained generic adapter exposure"):
        validate(audited, commands=commands)


def test_matrix_derives_adapter_additions_only_with_explicit_exposure_reason(audited, monkeypatch):
    def reject_network(*_args, **_kwargs):
        raise AssertionError("Matrix must remain offline")

    monkeypatch.setattr(socket, "socket", reject_network)
    monkeypatch.setattr(coverage, "audit_native", lambda *_args: (audited[1], audited[2]))
    request = "kiapi.common.commands.OpenLibraryItem"
    commands = {**COMMANDS, request: ("google.protobuf.Empty", "view")}
    data = deepcopy(audited[3])
    for key, entry in data["operations"].items():
        if key.endswith('/' + request):
            entry["adapter_exposure"] = "Synthetic adapter review; desktop inclusion is still deferred."
    matrix = coverage.build_matrix(data=data, commands=commands)
    rows = [r for r in matrix["operations"] if r["request"] == request]
    assert {row["surface"] for row in rows} == {"symbol", "footprint"}
    assert all(row["decision"] == "deferred" for row in rows)
    assert all(row["generic_adapter"]["command"] == request for row in rows)
    assert matrix["editor_connection_attempted"] is False
    assert matrix["live_support_verified"] is False
    assert all(row["live_support_verified"] is False for row in matrix["operations"])
    assert coverage.build_matrix(data=data, commands=commands) == matrix
    assert json.loads(json.dumps(matrix)) == matrix
    rendered = coverage.markdown_matrix(matrix)
    assert "No generic adapter" in rendered
    assert "no native/live qualification" in rendered
    assert "implementation" in rendered


def test_context_limits_keep_registered_stubs_and_unsupported_hooks_honest(audited, monkeypatch):
    monkeypatch.setattr(coverage, "audit_native", lambda *_args: (audited[1], audited[2]))
    rows = coverage.build_matrix()["operations"]
    parser_rows = [r for r in rows if r["request"].endswith(".ParseAndCreateItemsFromString")]
    assert parser_rows and all(r["implementation"] == "stub" and r["decision"] == "deferred"
                               for r in parser_rows)
    for row in rows:
        if row["request"].endswith((".GetTitleBlockInfo", ".SetTitleBlockInfo", ".GetPageLayout")):
            if row["surface"] in {"symbol", "footprint"}:
                assert row["decision"] == "deferred"
                assert row["implementation"] == "conditional"
        if row["owner"] == "API_HANDLER_BOARD" and row["surface"] == "footprint":
            assert not any(name.startswith("live_pcb_") for name in row["typed_tools"])
    pcb_save_as = next(r for r in rows if r["surface"] == "pcb" and r["request"].endswith(".SaveDocumentAs"))
    assert pcb_save_as["decision"] == "deferred"
    assert "GUI" in pcb_save_as["notes"]
    assert next(r for r in rows if r["request"].endswith(".BeginCommit"))["decision"] == "internal"
    assert next(r for r in rows if r["request"].endswith(".InteractiveMoveItems"))["decision"] == "interactive"
