# SPDX-License-Identifier: GPL-3.0-or-later
"""Safety checks for the isolated qualification launcher; no native application."""

from types import SimpleNamespace

import pynng
import pytest

import client
from qualification import run
from qualification.packaged_main import configure_desktop, run_stdio


def test_readiness_retries_reads_but_never_replays_an_unknown_open(monkeypatch, tmp_path):
    requests = []
    stopped = []
    reads = 0

    class Peer:
        def __init__(self, **kwargs):
            pass

        def send(self, request, response):
            nonlocal reads
            requests.append(request.DESCRIPTOR.name)
            reads += 1
            if reads == 1:
                raise pynng.Timeout("readiness timeout", 5)
            return response()

        def open_schematic(self):
            requests.append("OpenDocument")
            raise pynng.Timeout("unknown open outcome", 5)

    proc = SimpleNamespace(poll=lambda: None)
    monkeypatch.setattr(client, "LiveSchematic", Peer)
    monkeypatch.setattr(run.subprocess, "Popen", lambda *args, **kwargs: proc)
    monkeypatch.setattr(run.time, "sleep", lambda _: None)
    monkeypatch.setattr(run, "stop", lambda process, log: (stopped.append(process), log.close()))
    with pytest.raises(pynng.Timeout):
        run.start(tmp_path, tmp_path, tmp_path, {}, "unknown-open")
    assert requests == ["GetVersion", "GetVersion", "OpenDocument"]
    assert stopped == [proc]


def test_fixture_configuration_and_designs_are_confined_to_the_supplied_directory(tmp_path):
    project = run.prepare(tmp_path)
    assert project.is_relative_to(tmp_path)
    assert set(run.disk_hashes(project)) == {"live-demo.kicad_sch"}
    config = tmp_path / "config/10.99"
    assert '"enable_server": true' in (config / "kicad_common.json").read_text()
    assert "Regulator_Linear" in (config / "sym-lib-table").read_text()
    assert "Resistor_SMD" in (config / "fp-lib-table").read_text()


def test_packaged_initialization_preserves_preferences_and_library_tables(tmp_path):
    import json

    config = tmp_path / "10.99"
    config.mkdir()
    settings = config / "kicad_common.json"
    settings.write_text(json.dumps({"api": {"enable_server": False}, "user_preference": 42}))
    library = config / "sym-lib-table"
    library.write_text("user-maintained table\n")
    configure_desktop(tmp_path)
    first = settings.read_bytes()
    configure_desktop(tmp_path)
    assert settings.read_bytes() == first
    assert json.loads(first)["api"]["enable_server"] is False
    assert json.loads(first)["user_preference"] == 42
    assert library.read_text() == "user-maintained table\n"


def test_packaged_initialization_refuses_invalid_settings_without_replacing_them(tmp_path):
    import json

    config = tmp_path / "10.99"
    config.mkdir()
    settings = config / "kicad_common.json"
    settings.write_text("invalid json\n")
    with pytest.raises(json.JSONDecodeError):
        configure_desktop(tmp_path)
    assert settings.read_text() == "invalid json\n"
    assert not (config / "sym-lib-table").exists()


def test_stdio_sdk_can_close_its_buffers_without_closing_process_streams(monkeypatch, tmp_path):
    import sys

    with (tmp_path / "stdin").open("w+") as stdin, (tmp_path / "stdout").open("w+") as stdout:
        monkeypatch.setattr(sys, "stdin", stdin)
        monkeypatch.setattr(sys, "stdout", stdout)

        def sdk():
            sys.stdout.buffer.write(b'{"jsonrpc":"2.0"}\n')
            sys.stdout.buffer.close()
            sys.stdin.buffer.close()

        run_stdio(sdk)
        assert sys.stdin is stdin and sys.stdout is stdout
        assert not stdin.closed and not stdout.closed
        stdout.flush()
        stdout.seek(0)
        assert stdout.read() == '{"jsonrpc":"2.0"}\n'
