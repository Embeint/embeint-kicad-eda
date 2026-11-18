# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline socket-selection contracts; no KiCad processes are used."""

from contextlib import contextmanager
import socket
import threading

import pytest

import client
import instance_discovery as discovery


@contextmanager
def socket_file(path):
    with socket.socket(socket.AF_UNIX) as sock:
        sock.bind(str(path))
        yield "ipc://" + str(path)


@pytest.fixture(autouse=True)
def isolated_environment(monkeypatch, socket_tmp_path):
    monkeypatch.delenv("KICAD_API_SOCKET", raising=False)
    monkeypatch.delenv("KICAD_API_TOKEN", raising=False)
    root = socket_tmp_path / "kicad"
    root.mkdir()
    monkeypatch.setattr(discovery, "default_socket_url", lambda: "ipc://" + str(root / "api.sock"))
    return root


def test_default_missing_keeps_existing_default_without_connecting(isolated_environment):
    root = isolated_environment
    assert discovery.select_socket() == "ipc://" + str(root / "api.sock")
    assert discovery.discover_instances()["selection"] == "default_missing"


def test_exact_cpp_names_deterministic_and_stale_candidates_remain(isolated_environment):
    root = isolated_environment
    with socket_file(root / "api-200.sock"), socket_file(root / "api.sock"):
        pass  # Closed AF_UNIX sockets leave stale paths, just as a crashed process can.
    (root / "api.lock").touch()
    (root / "api-other.sock").touch()
    (root / "api-3.sock.extra").touch()
    inventory = discovery.discover_instances()
    assert inventory["selection"] == "ambiguous"
    assert [entry["socket_url"] for entry in inventory["candidates"]] == [
        "ipc://" + str(root / "api-200.sock"), "ipc://" + str(root / "api.sock")]
    with pytest.raises(discovery.DiscoveryError, match="ambiguous"):
        discovery.select_socket()
    assert (root / "api.lock").exists()
    assert (root / "api.sock").exists()


def test_single_pid_candidate_is_selected(isolated_environment):
    with socket_file(isolated_environment / "api-123.sock") as url:
        assert discovery.select_socket() == url


def test_explicit_argument_and_environment_override_all_candidates(monkeypatch, isolated_environment):
    with socket_file(isolated_environment / "api.sock"), socket_file(isolated_environment / "api-2.sock"):
        monkeypatch.setenv("KICAD_API_SOCKET", "ipc:///explicit-env.sock")
        assert discovery.select_socket() == "ipc:///explicit-env.sock"
        assert discovery.select_socket("ipc:///explicit-argument.sock") == "ipc:///explicit-argument.sock"
        inventory = discovery.discover_instances()
        assert len(inventory["candidates"]) == 1
        assert inventory["candidates"][0]["kind"] == "missing"


@pytest.mark.parametrize("url", ["tcp://127.0.0.1:5555", "ws://host", "ipc://relative.sock",
                                     "ipc:///tmp/api.sock?token=secret", "ipc:///tmp/secret\n", "ipc:///"])
def test_nonlocal_or_malformed_url_is_rejected_without_echoing_input(url):
    with pytest.raises(discovery.DiscoveryError) as error:
        discovery.select_socket(url)
    assert url not in str(error.value)


def test_non_sockets_and_symlinks_are_not_automatically_selected(isolated_environment):
    root = isolated_environment
    (root / "api.sock").write_text("not a socket")
    (root / "api-12.sock").symlink_to(root / "api.sock")
    inventory = discovery.discover_instances()
    assert inventory["selection"] == "unavailable"
    assert {entry["kind"] for entry in inventory["candidates"]} == {"not_socket", "symlink"}
    with pytest.raises(discovery.DiscoveryError):
        discovery.select_socket()


def test_unreadable_directory_never_falls_back(monkeypatch):
    def denied(_self):
        raise PermissionError("arbitrary sensitive text")
    monkeypatch.setattr(discovery.Path, "iterdir", denied)
    with pytest.raises(discovery.DiscoveryError) as error:
        discovery.select_socket()
    assert error.value.code == "discovery_unavailable"
    assert "sensitive" not in str(error.value)


def test_client_selects_lazily_and_never_sends_an_ambiguous_mutation(monkeypatch, isolated_environment):
    monkeypatch.setattr(client.sys, "platform", "linux")
    def reject_connection(**_kwargs):
        raise AssertionError("no connection should be opened")
    monkeypatch.setattr(client.pynng, "Req0", reject_connection)
    with socket_file(isolated_environment / "api.sock"), socket_file(isolated_environment / "api-2.sock"):
        live = client.LiveSchematic()  # Constructor/MCP startup remains offline.
        with pytest.raises(discovery.DiscoveryError, match="ambiguous"):
            live.send(client.project.SaveDocument(), client.editor.GetOpenDocumentsResponse)


def test_client_pins_selection_and_does_not_retry_timeout(monkeypatch):
    monkeypatch.setattr(client.sys, "platform", "linux")
    selected, sent, dialed, options = [], [], [], []
    def select():
        selected.append(True)
        return "ipc:///chosen-once.sock"
    class Connection:
        def __init__(self, **kwargs):
            options.append(kwargs)
        def __enter__(self):
            return self
        def __exit__(self, *_args):
            pass
        def dial(self, url, **_kwargs):
            dialed.append(url)
        def send(self, raw):
            sent.append(raw)
        def recv(self):
            raise client.pynng.Timeout("synthetic timeout", 5)
    monkeypatch.setattr(client, "select_socket", select)
    monkeypatch.setattr(client.pynng, "Req0", Connection)
    live = client.LiveSchematic()
    with pytest.raises(client.pynng.Timeout):
        live.send(client.project.SaveDocument(), client.editor.GetOpenDocumentsResponse)
    assert len(selected) == len(sent) == 1
    assert dialed == ["ipc:///chosen-once.sock"]
    assert live.socket_url == dialed[0]
    assert options[0]["resend_time"] == -1


def test_explicit_client_never_discovers(monkeypatch):
    def reject():
        raise AssertionError("explicit selection must not scan")
    monkeypatch.setattr(client, "select_socket", reject)
    live = client.LiveSchematic("ipc:///explicit.sock")
    assert live._socket_selected


def test_mutation_is_not_replayed_when_peer_disconnects_and_rebinds(isolated_environment):
    url = "ipc://" + str(isolated_environment / "api.sock")
    requests, errors = [], []
    original = client.pynng.Rep0(listen=url, recv_timeout=2000)
    def disconnect_and_rebind():
        try:
            requests.append(original.recv())
            original.close()  # Simulate applying the request and losing the reply.
            with client.pynng.Rep0(listen=url, recv_timeout=700) as replacement:
                try:
                    repeated = replacement.recv()
                except client.pynng.Timeout:
                    return
                requests.append(repeated)
                from google.protobuf import empty_pb2
                reply = client.envelope.ApiResponse()
                reply.status.status = client.envelope.AS_OK
                reply.message.Pack(empty_pb2.Empty())
                replacement.send(reply.SerializeToString())
        except Exception as error:
            errors.append(error)
    thread = threading.Thread(target=disconnect_and_rebind)
    thread.start()
    try:
        from google.protobuf import empty_pb2
        live = client.LiveSchematic(url, timeout_ms=1200)
        with pytest.raises(client.pynng.NNGException):
            live.send(client.project.SaveDocument(), empty_pb2.Empty)
    finally:
        thread.join(timeout=4)
        original.close()
    assert not thread.is_alive()
    assert not errors
    assert len(requests) == 1
