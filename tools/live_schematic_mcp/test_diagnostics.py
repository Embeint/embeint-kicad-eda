# SPDX-License-Identifier: GPL-3.0-or-later
"""Synthetic NNG/protobuf and CLI/MCP protocol tests, never native/live evidence."""

import asyncio
from contextlib import contextmanager
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import threading

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client
import pynng
import pytest

from client import base, editor, envelope, types
import diagnose
import instance_discovery as discovery


def test_native_windows_reports_transport_scope_without_discovery(monkeypatch):
    monkeypatch.setattr(diagnose.sys, "platform", "win32")
    monkeypatch.setattr(diagnose, "discover_instances", lambda *_: pytest.fail("Windows discovery"))
    report = diagnose.diagnose(probe=True)
    assert report["runtime"]["protocols"]["state"] == "ready"
    assert report["discovery"]["selection"] == "unsupported_platform"
    assert report["probe"]["state"] == "unsupported_platform"
    assert report["connection_attempted"] is False


@pytest.fixture
def isolated(monkeypatch, socket_tmp_path):
    monkeypatch.delenv("KICAD_API_SOCKET", raising=False)
    monkeypatch.delenv("KICAD_API_TOKEN", raising=False)
    monkeypatch.setattr(diagnose.sys, "platform", "linux")
    root = socket_tmp_path / "kicad"
    root.mkdir()
    monkeypatch.setattr(discovery, "default_socket_url", lambda: "ipc://" + str(root / "api.sock"))
    return root


@contextmanager
def fake_instance(path, respond):
    """A disposable local REP peer speaking actual generated protobuf envelopes."""
    url = "ipc://" + str(path)
    requests, errors = [], []
    stop = threading.Event()
    with pynng.Rep0(listen=url, recv_timeout=50, send_timeout=1000) as peer:
        def serve():
            while not stop.is_set():
                try:
                    raw = peer.recv()
                except pynng.Timeout:
                    continue
                try:
                    request = envelope.ApiRequest.FromString(raw)
                    requests.append(request)
                    response = respond(request, len(requests))
                    peer.send(response if isinstance(response, bytes) else response.SerializeToString())
                except Exception as error:
                    errors.append(error)
                    break
        thread = threading.Thread(target=serve)
        thread.start()
        try:
            yield url, requests
        finally:
            stop.set()
            thread.join(timeout=2)
            assert not thread.is_alive()
            assert not errors


def version_or_documents(project, *, version=(10, 0), status=None):
    def respond(request, _count):
        reply = envelope.ApiResponse()
        reply.header.kicad_token = "private-token-" + project
        reply.status.status = envelope.AS_OK
        if request.message.Is(base.GetVersion.DESCRIPTOR):
            response = base.GetVersionResponse()
            response.version.major, response.version.minor = version
        else:
            assert request.message.Is(editor.GetOpenDocuments.DESCRIPTOR)
            if status is not None:
                reply.status.status = status
                reply.status.error_message = "DO NOT PRINT private-token-" + project
                return reply
            command = editor.GetOpenDocuments()
            request.message.Unpack(command)
            response = editor.GetOpenDocumentsResponse()
            response.documents.add(type=command.type, project=types.ProjectSpecifier(
                name=project, path="/synthetic/" + project))
        reply.message.Pack(response)
        return reply
    return respond


def test_offline_inventory_never_dials_even_with_probe_candidates(monkeypatch, isolated):
    with socket.socket(socket.AF_UNIX) as sock:
        sock.bind(str(isolated / "api.sock"))
        monkeypatch.setattr(diagnose, "_probe_instance", lambda *_: pytest.fail("unexpected IPC"))
        report = diagnose.diagnose()
    assert report["connection_attempted"] is False
    assert report["discovery"]["selection"] == "single_candidate"
    assert "probe" not in report


def test_two_projects_and_stale_socket_with_observed_support_only(isolated):
    stale = isolated / "api-999.sock"
    with socket.socket(socket.AF_UNIX) as sock:
        sock.bind(str(stale))
    with fake_instance(isolated / "api.sock", version_or_documents("alpha")) as first:
        with fake_instance(isolated / "api-123.sock", version_or_documents("beta", version=(99, 99))) as second:
            report = diagnose.diagnose(probe=True, timeout_ms=100)
    assert report["connection_attempted"] is True
    assert report["live_support_verified"] is False
    assert report["discovery"]["selection"] == "ambiguous"
    assert report["discovery"]["selected_socket"] is None
    candidates = {Path(entry["socket_url"][6:]).name: entry for entry in report["discovery"]["candidates"]}
    assert candidates["api-999.sock"]["probe"]["state"] in {"unresponsive", "unreachable"}
    assert stale.exists()  # Diagnostics never clean up another process's files.
    for name, project in (("api.sock", "alpha"), ("api-123.sock", "beta")):
        commands = candidates[name]["probe"]["commands"]
        assert len(commands) == 5
        assert commands[1]["response"]["documents"][0]["project"]["name"] == project
        assert all(command["support"] == "observed_for_this_read_only_request" for command in commands)
    for _url, requests in (first, second):
        assert len(requests) == 5
        assert requests[0].header.kicad_token == ""
        assert all(request.header.kicad_token for request in requests[1:])
    assert "private-token" not in json.dumps(report)


@pytest.mark.parametrize("status", [envelope.AS_UNHANDLED, envelope.AS_UNIMPLEMENTED])
def test_new_version_does_not_imply_missing_command_support(isolated, status):
    with fake_instance(isolated / "api.sock", version_or_documents("new", version=(99, 99), status=status)):
        report = diagnose.diagnose(probe=True)
    commands = report["discovery"]["candidates"][0]["probe"]["commands"]
    assert commands[0]["response"]["version"]["major"] == 99
    expected = "editor_not_open_or_unsupported" if status == envelope.AS_UNHANDLED else "unsupported"
    assert all(command["support"] == expected for command in commands[1:])
    assert "DO NOT PRINT" not in json.dumps(report)
    assert "private-token" not in json.dumps(report)


@pytest.mark.parametrize("status", [envelope.AS_BUSY, envelope.AS_NOT_READY, envelope.AS_TOKEN_MISMATCH])
def test_busy_not_ready_and_token_rejection_are_not_unsupported(isolated, status):
    def reject(_request, _count):
        reply = envelope.ApiResponse()
        reply.status.status = status
        reply.status.error_message = "sensitive peer text"
        return reply
    with fake_instance(isolated / "api.sock", reject) as (_url, requests):
        report = diagnose.diagnose(probe=True)
    commands = report["discovery"]["candidates"][0]["probe"]["commands"]
    assert len(requests) == len(commands) == 1
    assert commands[0]["support"] == "unverified"
    assert "sensitive" not in json.dumps(report)


@pytest.mark.parametrize("raw, expected", [(b"invalid", "invalid_envelope"), (b"", "invalid_envelope"),
                                             (envelope.ApiResponse(header=envelope.ApiResponseHeader(
                                                 kicad_token="private-token-invalid")).SerializeToString(),
                                              "invalid_status"),
                                             (None, "unexpected_reply_type")])
def test_malformed_or_incompatible_responses_are_safe(isolated, raw, expected):
    def respond(_request, _count):
        if raw is not None:
            return raw
        reply = envelope.ApiResponse()
        reply.status.status = envelope.AS_OK
        reply.message.Pack(editor.GetOpenDocumentsResponse())
        return reply
    with fake_instance(isolated / "api.sock", respond) as (_url, requests):
        report = diagnose.diagnose(probe=True)
    probe = report["discovery"]["candidates"][0]["probe"]
    assert probe["state"] == "incompatible_response"
    assert probe["commands"][0]["status"] == expected
    assert len(requests) == 1


def test_explicit_selection_token_and_redaction(monkeypatch, isolated):
    responder = version_or_documents("private-token-configured")
    def respond(request, count):
        reply = responder(request, count)
        reply.header.kicad_token = "private-token-configured"
        return reply
    with fake_instance(isolated / "api.sock", respond) as (url, requests):
        monkeypatch.setenv("KICAD_API_SOCKET", url)
        monkeypatch.setenv("KICAD_API_TOKEN", "private-token-configured")
        report = diagnose.diagnose(probe=True)
    assert len(requests) == 5
    assert all(request.header.kicad_token == "private-token-configured" for request in requests)
    assert "private-token-configured" not in json.dumps(report)
    assert "[redacted]" in json.dumps(report)


def test_instance_change_stops_probe_without_retry(isolated):
    responder = version_or_documents("before")
    def changed(request, count):
        reply = responder(request, count)
        if count > 1:
            reply.header.kicad_token = "private-token-after"
        return reply
    with fake_instance(isolated / "api.sock", changed) as (_url, requests):
        report = diagnose.diagnose(probe=True)
    assert len(requests) == 2
    assert report["discovery"]["candidates"][0]["probe"]["state"] == "identity_mismatch"
    assert "private-token" not in json.dumps(report)


def test_malformed_embedded_protobuf_does_not_escape_diagnostics(isolated):
    def malformed(_request, _count):
        reply = envelope.ApiResponse()
        reply.status.status = envelope.AS_OK
        reply.message.type_url = "type.googleapis.com/" + base.GetVersionResponse.DESCRIPTOR.full_name
        reply.message.value = b"invalid"
        return reply
    with fake_instance(isolated / "api.sock", malformed):
        report = diagnose.diagnose(probe=True)
    assert report["discovery"]["candidates"][0]["probe"]["state"] == "incompatible_response"


@pytest.mark.skipif(sys.platform != "linux", reason="CLI probes are scoped to Linux/WSL")
def test_opt_in_cli_uses_explicit_endpoint_and_keeps_tokens_private(isolated):
    with fake_instance(isolated / "api.sock", version_or_documents("cli")) as (url, requests):
        result = subprocess.run([sys.executable, diagnose.__file__, "--probe", "--socket", url],
                                env={**os.environ, "KICAD_API_SOCKET": "ipc:///unused.sock",
                                     "KICAD_API_TOKEN": ""},
                                capture_output=True, text=True, check=True, timeout=20)
    report = json.loads(result.stdout)
    assert report["discovery"]["selected_socket"] == url
    assert report["connection_attempted"] is True
    assert len(requests) == 5
    assert "private-token" not in result.stdout + result.stderr
    assert not result.stderr


def test_probe_refuses_network_transports_without_echoing_token(isolated):
    report = diagnose.diagnose(probe=True, socket_url="tcp://user:private-token@host:1234")
    assert not report["connection_attempted"]
    assert report["discovery"]["selection"] == "invalid_socket"
    assert "private-token" not in json.dumps(report)


def test_missing_protocols_cli_runs_with_standard_library_only(tmp_path):
    here = Path(diagnose.__file__).parent
    scratch = tmp_path / "tools" / "live_schematic_mcp"
    scratch.mkdir(parents=True)
    proto = tmp_path / "api" / "proto"
    proto.mkdir(parents=True)
    (proto / "missing.proto").write_text('syntax = "proto3";')
    for name in ("diagnose.py", "instance_discovery.py"):
        shutil.copyfile(here / name, scratch / name)
    result = subprocess.run([sys.executable, "-S", str(scratch / "diagnose.py"), "--probe",
                             "--socket", "ipc:///nonexistent-embeint-test.sock"],
                            capture_output=True, text=True, timeout=20, check=True)
    report = json.loads(result.stdout)
    assert report["runtime"]["protocols"]["state"] == "missing"
    assert report["runtime"]["protocols"]["missing_modules"] == ["_generated/missing_pb2.py"]
    assert "generate_proto.py" in report["runtime"]["protocols"]["action"]
    assert not report["connection_attempted"]
    assert not result.stderr


def test_runtime_import_failure_is_actionable_and_not_echoed(monkeypatch, isolated):
    original = diagnose.importlib.import_module
    def broken(name):
        if name == "client":
            raise RuntimeError("private-token-runtime")
        return original(name)
    monkeypatch.setattr(diagnose.importlib, "import_module", broken)
    report = diagnose.diagnose(probe=True)
    assert report["runtime"]["protocols"]["state"] == "incompatible"
    assert report["probe"]["state"] == "runtime_unavailable"
    assert "private-token" not in json.dumps(report)


def test_missing_native_library_has_a_specific_safe_action(monkeypatch, isolated):
    original = diagnose.importlib.import_module
    def missing_library(name):
        if name == "pynng":
            raise ImportError("libatomic.so.1: missing; private-token-runtime")
        return original(name)
    monkeypatch.setattr(diagnose.importlib, "import_module", missing_library)
    report = diagnose.diagnose(probe=True)
    assert report["runtime"]["dependencies"]["pynng"] == "unavailable"
    assert "libatomic1" in report["runtime"]["dependency_actions"]["pynng"]
    assert report["runtime"]["protocols"]["state"] == "unchecked"
    assert report["probe"]["state"] == "runtime_unavailable"
    assert not report["connection_attempted"]
    assert "private-token" not in json.dumps(report)


def test_stdio_startup_with_multiple_socket_candidates_and_no_kicad(socket_tmp_path):
    root = socket_tmp_path / "kicad"
    root.mkdir()
    with socket.socket(socket.AF_UNIX) as first, socket.socket(socket.AF_UNIX) as second:
        first.bind(str(root / "api.sock"))
        second.bind(str(root / "api-123.sock"))
        async def exercise():
            # No request is sent to either inert socket during startup or inventory.
            parameters = StdioServerParameters(command=sys.executable,
                args=[str(Path(diagnose.__file__).with_name("server.py"))],
                env={**os.environ, "TMPDIR": str(socket_tmp_path), "KICAD_API_SOCKET": "",
                     "KICAD_API_TOKEN": ""})
            async with stdio_client(parameters) as (reader, writer):
                async with ClientSession(reader, writer) as session:
                    await session.initialize()
                    result = await session.call_tool("live_mcp_capabilities", {})
                    assert not result.isError
                    manifest = result.structuredContent or json.loads(result.content[0].text)
                    assert manifest["editor_connection_attempted"] is False
        asyncio.run(asyncio.wait_for(exercise(), timeout=30))
