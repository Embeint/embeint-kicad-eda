# SPDX-License-Identifier: GPL-3.0-or-later
"""readiness.py: which failures make scripts/wait_ready.* keep waiting."""

from pathlib import Path

import pynng
import pytest

import instance_discovery
import readiness
from instance_discovery import DiscoveryError


@pytest.mark.parametrize("error", [
    DiscoveryError("no_editor", "No KiCad editor is listening. Nothing was sent."),
    DiscoveryError("unavailable", "KiCad socket selection is unavailable."),
    DiscoveryError("instance_unavailable", "Selected KiCad instance is unavailable"),
    RuntimeError("AS_NOT_READY: KiCad is not ready"),
    RuntimeError("AS_BUSY: KiCad is busy"),
    pynng.Timeout("Timed out", 5),
    pynng.ConnectionRefused("Connection refused", 6),
    ConnectionRefusedError(),
    FileNotFoundError(),
])
def test_not_started_or_loading_keeps_waiting(error):
    assert readiness.classify(error) == readiness.WAIT


@pytest.mark.parametrize("error", [
    DiscoveryError("ambiguous", "KiCad socket selection is ambiguous."),
    RuntimeError("Live schematic editing requires this KiCad development build (10.99/11+)"),
    RuntimeError("AS_BAD_REQUEST: nope (mentions AS_NOT_READY later)"),
    ValueError("KICAD_API_SOCKET must be a local ipc:// URL"),
])
def test_real_errors_stop_waiting(error):
    assert readiness.classify(error) == readiness.ERROR


def test_no_running_kicad_is_not_ready(monkeypatch, tmp_path):
    """Regression: the client's fast "no editor" failure must not be reported as ready."""
    monkeypatch.delenv("KICAD_API_SOCKET", raising=False)
    monkeypatch.setattr(instance_discovery, "discover_instances",
                        lambda *args, **kwargs: {"selection": "default_missing", "candidates": [],
                                                 "selected_socket": None})

    class Client:
        def __init__(self, timeout_ms):
            self.socket_url = "ipc:///absent"

        def documents(self):
            instance_discovery.select_socket(check_liveness=True, require_listener=True)

    status, lines = readiness.probe(client_factory=Client)
    assert status == readiness.WAIT
    assert lines[0].startswith("DiscoveryError(no_editor): ")


def test_ready_prints_documents_then_endpoint():
    class Client:
        socket_url = "ipc:///tmp/kicad/api.sock"

        def __init__(self, timeout_ms):
            assert timeout_ms == 1234

        def documents(self):
            return {"version": {"major": 10}, "documents": []}

    status, lines = readiness.probe(1234, client_factory=Client)
    assert status == readiness.READY
    assert lines == ["{'version': {'major': 10}, 'documents': []}", "endpoint: ipc:///tmp/kicad/api.sock"]


def test_wait_scripts_use_the_exit_status_not_message_text():
    scripts = Path(readiness.__file__).with_name("scripts")
    shell = (scripts / "wait_ready.sh").read_text(encoding="utf-8")
    powershell = (scripts / "wait_ready.ps1").read_text(encoding="utf-8")
    assert "readiness.py" in shell and "75) sleep" in shell and "*) break" in shell
    assert "readiness.py" in powershell and "$code -ne 75" in powershell
