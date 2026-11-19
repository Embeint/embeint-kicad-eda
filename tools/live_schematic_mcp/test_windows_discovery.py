# SPDX-License-Identifier: GPL-3.0-or-later
"""Windows named-pipe endpoint contracts.

The first tests replace the pipe listing and run on every platform.  The last ones listen on
real local named pipes with disposable NNG peers, so they run on Windows only; they are
synthetic protocol tests, not native KiCad evidence.
"""

import asyncio
import json
import os
from pathlib import Path, PureWindowsPath
import subprocess
import sys
import tempfile

import pytest

import client
import diagnose
import instance_discovery as discovery
from model_handoff import kicad_path


TEMP = r"C:\Users\LONGNA~1\AppData\Local\Temp"
OURS = TEMP + r"\kicad"
windows_only = pytest.mark.skipif(sys.platform != "win32", reason="real named pipes need Windows")


@pytest.fixture
def fake_windows(monkeypatch):
    """Windows rules with a controllable pipe listing; nothing can be dialed."""
    monkeypatch.delenv("KICAD_API_SOCKET", raising=False)
    monkeypatch.delenv("KICAD_API_TOKEN", raising=False)
    monkeypatch.setattr(discovery.sys, "platform", "win32")
    monkeypatch.setattr(discovery, "default_socket_url", lambda: "ipc://" + OURS + r"\api.sock")
    # The short (8.3) and long spellings of the same directory resolve to one key
    monkeypatch.setattr(discovery, "directory_key",
                        lambda path: str(path).lower().replace("longna~1", "longname"))
    monkeypatch.setattr(client.pynng, "Req0", lambda **_: pytest.fail("no connection may be opened"))
    pipes = [r"C_\repo\.git\fsmonitor--daemon.ipc", "mojo.1234.5678", r"C:\Users\Other\AppData\Local\Temp\kicad\api.sock"]
    monkeypatch.setattr(discovery, "list_pipes", lambda: list(pipes))
    return pipes


@pytest.mark.parametrize("url", [
    "ipc://C:\\Users\\me\\AppData\\Local\\Temp\\kicad\\api.sock",
    "ipc://d:\\kb\\live\\tmp\\kicad\\api-123.sock",
])
def test_windows_accepts_local_drive_pipe_urls(fake_windows, url):
    assert discovery.local_socket_path(url) == PureWindowsPath(url[len("ipc://"):])


@pytest.mark.parametrize("url", [
    "tcp://127.0.0.1:5555", "ws://host/kicad", "ipc:///tmp/kicad/api.sock", "ipc://relative.sock",
    "ipc://\\\\server\\pipe\\kicad", "ipc://\\\\.\\pipe\\kicad", "ipc://\\\\?\\C:\\kicad\\api.sock",
    "ipc://C:\\Temp\\..\\..\\UNC\\server\\pipe\\x", "ipc://C:\\Temp\\.\\api.sock", "ipc://C:/Temp/api.sock",
    "ipc://C:\\Temp\\kicad\\api.sock?token=secret", "ipc://C:\\Temp\\api.sock\nsecret",
    "ipc://C:\\Temp\\kicad.\\api.sock", "ipc://C:\\Temp\\a:b\\api.sock", "ipc://C:\\Temp\\\\api.sock",
    "ipc://C:\\" + "x" * 300,
])
def test_windows_rejects_network_device_and_traversal_urls_without_echo(fake_windows, url):
    with pytest.raises(discovery.DiscoveryError) as error:
        discovery.select_socket(url)
    assert error.value.code == "invalid_socket"
    assert url not in str(error.value) and "secret" not in str(error.value)
    assert "named-pipe" in str(error.value)


def test_windows_without_kicad_keeps_default_and_ignores_other_users(fake_windows):
    inventory = discovery.discover_instances()
    assert inventory == {"selection": "default_missing", "candidates": [],
                         "selected_socket": "ipc://" + OURS + r"\api.sock"}


def test_windows_single_pipe_matches_short_and_long_temp_spellings(fake_windows):
    long_name = r"C:\Users\LongName\AppData\Local\Temp\kicad\api.sock"
    fake_windows.append(long_name)
    inventory = discovery.discover_instances()
    assert inventory["selection"] == "single_candidate"
    assert inventory["candidates"] == [{"socket_url": "ipc://" + long_name, "kind": "named_pipe"}]
    assert discovery.select_socket(check_liveness=True) == "ipc://" + long_name


def test_windows_pid_pipes_are_ambiguous_and_deterministic(fake_windows):
    fake_windows.extend([OURS + r"\api.sock", OURS + r"\API-200.SOCK", OURS + r"\api-3.sock.extra",
                         OURS + r"\api-other.sock", TEMP + r"\elsewhere\api.sock"])
    inventory = discovery.discover_instances()
    assert inventory["selection"] == "ambiguous"
    assert [entry["socket_url"] for entry in inventory["candidates"]] == [
        "ipc://" + OURS + r"\API-200.SOCK", "ipc://" + OURS + r"\api.sock"]
    with pytest.raises(discovery.DiscoveryError, match="ambiguous"):
        discovery.select_socket(check_liveness=True)


def test_windows_reachability_is_listing_only(fake_windows, monkeypatch):
    fake_windows.append(OURS + r"\api.sock")
    assert discovery.socket_reachability("ipc://" + OURS.upper() + r"\API.SOCK") == "connectable"
    assert discovery.socket_reachability("ipc://" + OURS + r"\api-9.sock") == "not_listening"

    def denied():
        raise PermissionError("private detail")
    monkeypatch.setattr(discovery, "list_pipes", denied)
    assert discovery.socket_reachability("ipc://" + OURS + r"\api.sock") == "unknown"
    assert discovery.socket_entry("ipc://" + OURS + r"\api.sock")["kind"] == "unavailable"
    with pytest.raises(discovery.DiscoveryError) as error:
        discovery.select_socket()
    assert error.value.code == "discovery_unavailable"
    assert "private" not in str(error.value)


def test_windows_explicit_selection_never_lists_pipes(fake_windows, monkeypatch):
    explicit = r"ipc://C:\kb\live\tmp\kicad\api.sock"
    monkeypatch.setenv("KICAD_API_SOCKET", explicit)
    calls = []
    monkeypatch.setattr(discovery, "list_pipes", lambda: calls.append(1) or [])
    assert discovery.select_socket() == explicit
    assert client.LiveSchematic()._socket_selected
    assert not calls[1:]  # the explicit entry's own existence check only


def test_windows_client_selects_lazily_and_dead_pinned_pipe_fails_before_send(fake_windows, monkeypatch):
    fake_windows.append(OURS + r"\api.sock")
    dialed = []

    class Connection:
        def __init__(self, **_kwargs):
            pass

        def __enter__(self):
            return self

        def __exit__(self, *_args):
            pass

        def dial(self, url, **_kwargs):
            dialed.append(url)

        def send(self, _raw):
            pass

        def recv(self):
            raise client.pynng.Timeout("synthetic timeout", 5)

    live = client.LiveSchematic()
    assert not live._socket_selected  # MCP startup stays offline
    monkeypatch.setattr(client.pynng, "Req0", Connection)
    with pytest.raises(client.pynng.Timeout):
        live.send(client.base.GetVersion(), client.base.GetVersionResponse)
    assert dialed == ["ipc://" + OURS + r"\api.sock"]

    fake_windows.pop()
    pinned = client.LiveSchematic("ipc://" + OURS + r"\api.sock")
    pinned._received_reply = True  # a previously responsive instance has exited
    with pytest.raises(discovery.DiscoveryError, match="no command was sent"):
        pinned.send(client.base.GetVersion(), client.base.GetVersionResponse)


def test_windows_client_keeps_looking_until_kicad_starts(fake_windows, monkeypatch):
    """The missing default endpoint is tried but not pinned, so a KiCad started later is found."""
    dialed = []

    class Connection:
        def __init__(self, **_kwargs):
            pass

        def __enter__(self):
            return self

        def __exit__(self, *_args):
            pass

        def dial(self, url, **_kwargs):
            dialed.append(url)

        def send(self, _raw):
            pass

        def recv(self):
            raise client.pynng.Timeout("synthetic timeout", 5)

    live = client.LiveSchematic()
    monkeypatch.setattr(client.pynng, "Req0", Connection)
    with pytest.raises(client.pynng.Timeout):
        live.send(client.base.GetVersion(), client.base.GetVersionResponse)
    assert dialed == ["ipc://" + OURS + r"\api.sock"] and not live._socket_selected

    # KiCad starts, with the long spelling of the temp directory: found and then pinned
    long_name = r"C:\Users\LongName\AppData\Local\Temp\kicad\api.sock"
    fake_windows.append(long_name)
    with pytest.raises(client.pynng.Timeout):
        live.send(client.base.GetVersion(), client.base.GetVersionResponse)
    assert dialed[-1] == "ipc://" + long_name and live._socket_selected

    fake_windows.append(OURS + r"\api.sock")  # another instance no longer changes the pinned one
    with pytest.raises(client.pynng.Timeout):
        live.send(client.base.GetVersion(), client.base.GetVersionResponse)
    assert dialed[-1] == "ipc://" + long_name


@pytest.mark.parametrize("environ, expected", [
    ({"TMPDIR": r"C:\a", "TMP": r"C:\b", "TEMP": r"C:\c"}, r"C:\a"),
    ({"TMP": r"C:\b", "TEMP": r"C:\c"}, r"C:\b"),
    ({"TMPDIR": "", "TMP": "", "TEMP": "C:\\Users\\LONGNA~1\\AppData\\Local\\Temp\\"},
     r"C:\Users\LONGNA~1\AppData\Local\Temp"),
    ({"TEMP": "C:/Temp/"}, r"C:\Temp"),
    ({"USERPROFILE": r"C:\Users\me", "SystemRoot": r"C:\Windows"}, r"C:\Users\me"),
    ({"SystemRoot": r"C:\Windows"}, r"C:\Windows"),
])
def test_windows_temp_dir_follows_wx(environ, expected):
    """wxFileName::GetTempDir: TMPDIR, TMP, TEMP, then GetTempPath; Python's tempfile prefers TEMP."""
    assert discovery.windows_temp_dir(environ) == expected


def test_windows_default_endpoint_uses_kicads_temp_dir(monkeypatch):
    monkeypatch.setattr(discovery.sys, "platform", "win32")
    monkeypatch.delenv("TMPDIR", raising=False)
    monkeypatch.setenv("TEMP", r"C:\python\would\pick\this")
    monkeypatch.setenv("TMP", r"C:\Users\LONGNA~1\AppData\Local\Temp")
    assert discovery.default_socket_url() == r"ipc://C:\Users\LONGNA~1\AppData\Local\Temp\kicad\api.sock"


def test_kicad_path_follows_the_platform(monkeypatch):
    monkeypatch.setattr(sys, "platform", "win32")
    assert kicad_path(r"C:\Users\me\kicad-models\a.step") == r"C:\Users\me\kicad-models\a.step"
    assert kicad_path("/mnt/d/models/a.step") == r"D:\models\a.step"
    monkeypatch.setattr(sys, "platform", "linux")
    assert kicad_path(r"C:\Users\me\kicad-models\a.step") == "/mnt/c/Users/me/kicad-models/a.step"
    assert kicad_path("/home/me/a.step") == "/home/me/a.step"


# ---- Real named pipes (Windows) -------------------------------------------------------------

@pytest.fixture
def pipe_dir(monkeypatch):
    """A private temp\\kicad directory; pipes under it never collide with a running KiCad."""
    monkeypatch.delenv("KICAD_API_SOCKET", raising=False)
    monkeypatch.delenv("KICAD_API_TOKEN", raising=False)
    with tempfile.TemporaryDirectory(prefix="emcp-") as directory:
        root = Path(directory) / "kicad"
        root.mkdir()
        monkeypatch.setattr(discovery, "default_socket_url", lambda: "ipc://" + str(root / "api.sock"))
        yield root


@windows_only
def test_real_pipe_is_discovered_selected_and_vanishes_when_closed(pipe_dir):
    from test_diagnostics import fake_instance, version_or_documents
    with fake_instance(pipe_dir / "api.sock", version_or_documents("alpha", version=(10, 99))) as (url, requests):
        inventory = discovery.discover_instances()
        assert inventory["selection"] == "single_candidate"
        assert inventory["candidates"] == [{"socket_url": url, "kind": "named_pipe"}]
        assert discovery.socket_reachability(url) == "connectable"
        assert not requests  # discovery and reachability never dial
        live = client.LiveSchematic(timeout_ms=2000)
        assert live.documents()["documents"][0]["project"]["name"] == "alpha"
        assert live.socket_url == url
    assert discovery.socket_reachability(url) == "not_listening"
    assert discovery.discover_instances()["selection"] == "default_missing"
    with pytest.raises(discovery.DiscoveryError, match="no command was sent"):
        live.send(client.base.GetVersion(), client.base.GetVersionResponse)


@windows_only
def test_real_pipes_from_two_instances_need_explicit_selection(pipe_dir):
    from test_diagnostics import fake_instance, version_or_documents
    with fake_instance(pipe_dir / "api.sock", version_or_documents("alpha", version=(10, 99))) as (first, first_requests), \
            fake_instance(pipe_dir / f"api-{os.getpid()}.sock", version_or_documents("beta", version=(10, 99))) as (second, _):
        with pytest.raises(discovery.DiscoveryError, match="ambiguous"):
            client.LiveSchematic(timeout_ms=2000).documents()
        assert not first_requests
        report = diagnose.diagnose(probe=True, timeout_ms=1000)
        projects = {entry["socket_url"]: entry["probe"]["commands"][1]["response"]["documents"][0]
                    ["project"]["name"] for entry in report["discovery"]["candidates"]}
        assert projects == {first: "alpha", second: "beta"}
        assert "private-token" not in json.dumps(report)
        explicit = client.LiveSchematic(second, timeout_ms=2000)
        assert explicit.documents()["documents"][0]["project"]["name"] == "beta"


def run_launcher(temp_dir, root):
    """run_kicad_windows.ps1 with temp_dir as KiCad's temp directory; returns (exit code, message).

    root has no KiCad installed, so a launcher that passes the endpoint check stops at "not
    installed": no KiCad can start here.
    """
    launcher = Path(diagnose.__file__).with_name("run_kicad_windows.ps1")
    command = ("try { & '" + str(launcher) + "' } catch { [Console]::Error.WriteLine( $_.Exception.Message ); "
               "exit 1 }")
    env = {**os.environ, "TMPDIR": str(temp_dir), "KICAD_WINDOWS_ROOT": str(root),
           "KICAD_LIVE_PYTHON": sys.executable, "KICAD_API_SOCKET": r"ipc://C:\elsewhere\kicad\api.sock"}
    result = subprocess.run(["powershell.exe", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass",
                             "-Command", command], env=env, capture_output=True, text=True, timeout=120)
    return result.returncode, result.stdout + result.stderr


@windows_only
def test_launcher_refuses_while_a_pipe_holds_the_default_endpoint(pipe_dir, tmp_path):
    from test_diagnostics import fake_instance, version_or_documents
    # The pipe is served under the temp path as Python spells it (here the short 8.3 form if
    # there is one) while the launcher is given the long spelling: they must still match
    long_temp = os.path.realpath(str(pipe_dir.parent))
    with fake_instance(pipe_dir / "api.sock", version_or_documents("alpha")) as (url, requests):
        code, message = run_launcher(long_temp, tmp_path)
        assert code != 0
        assert "Another KiCad with the API enabled already serves " + url in message, message
        assert not requests  # the check never dials the other instance
    code, message = run_launcher(long_temp, tmp_path)
    assert code != 0 and "Native KiCad not installed" in message, message
    assert not (tmp_path / "live").exists()  # nothing set up before the checks


@windows_only
def test_stdio_mcp_reaches_a_named_pipe_instance(pipe_dir):
    from mcp import ClientSession, StdioServerParameters
    from mcp.client.stdio import stdio_client
    from test_diagnostics import fake_instance, version_or_documents
    # A KiCad 10.99 answer, so the client accepts it as the development build
    with fake_instance(pipe_dir / "api.sock", version_or_documents("gamma", version=(10, 99))) as (url, _):
        async def exercise():
            parameters = StdioServerParameters(
                command=sys.executable, args=[str(Path(diagnose.__file__).with_name("server.py"))],
                env={**os.environ, "KICAD_API_SOCKET": url, "KICAD_API_TOKEN": ""})
            async with stdio_client(parameters) as (reader, writer):
                async with ClientSession(reader, writer) as session:
                    await session.initialize()
                    result = await session.call_tool("live_schematic_documents", {})
                    assert not result.isError, result.content
                    payload = result.structuredContent or json.loads(result.content[0].text)
                    assert "gamma" in json.dumps(payload)
        asyncio.run(asyncio.wait_for(exercise(), timeout=60))
