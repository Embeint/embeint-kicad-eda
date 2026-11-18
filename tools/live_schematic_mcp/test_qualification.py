# SPDX-License-Identifier: GPL-3.0-or-later
"""Safety checks for the isolated qualification launcher; no native application."""

from types import SimpleNamespace

import pynng
import pytest

import client
from qualification import run
from qualification.packaged_main import configure_desktop, run_stdio
from qualification.package_linux import native_provenance, private_shlibs_entry


def test_freeze_identity_uses_only_the_source_repository(monkeypatch, tmp_path):
    from pathlib import Path
    from qualification import package_mcp

    source = tmp_path / "archive/tools/live_schematic_mcp"
    source.mkdir(parents=True)
    monkeypatch.setattr(package_mcp, "MCP", source)
    revision = "a" * 40
    calls = []

    def git(command, **kwargs):
        calls.append(command[1:])
        assert Path(kwargs["cwd"]) == source
        if command[1:] == ["rev-parse", "--show-toplevel"]:
            return SimpleNamespace(returncode=0, stdout=str(repository) + "\n")
        assert command[1:] == ["rev-parse", "HEAD"]
        return SimpleNamespace(returncode=0, stdout=revision + "\n")

    monkeypatch.setattr(package_mcp.subprocess, "run", git)
    monkeypatch.setattr(package_mcp.subprocess, "check_output", lambda *args, **kwargs: b"")
    repository = tmp_path  # Git discovered outside the extracted source root.
    with pytest.raises(ValueError, match="source archive requires"):
        package_mcp.source_identity(None)
    assert package_mcp.source_identity("b" * 40) == (
        "b" * 40, None, "caller-supplied archive revision; inspect source checksums")
    assert all(command == ["rev-parse", "--show-toplevel"] for command in calls)
    repository = source.parents[1]
    assert package_mcp.source_identity(None) == (revision, False, "git")
    with pytest.raises(ValueError, match="differs from Git HEAD"):
        package_mcp.source_identity("b" * 40)


def test_bundled_numpy_openblas_uses_dpkg_unstable_soname_mapping():
    assert private_shlibs_entry("libscipy_openblas64_-4b78795a.so") == "libscipy_openblas64_ 4b78795a embeint-kicad"
    assert private_shlibs_entry("libkiapi.so.10.99.0") == "libkiapi 10.99.0 embeint-kicad"
    assert private_shlibs_entry("unversioned.so") is None


def test_installed_persistence_ignores_only_unstored_library_graphic_ids():
    import copy
    from qualification.check_install import canonical_items

    original = {"items": [{"id": {"value": "placed-item"}, "definition": {"items": [{"item": {
        "@type": "type.googleapis.com/kiapi.schematic.types.SchematicGraphicShape",
        "id": {"value": "library-graphic"}, "shape": {"width": 10}}}]}}]}
    reopened = copy.deepcopy(original)
    graphic = reopened["items"][0]["definition"]["items"][0]["item"]
    graphic["id"]["value"] = "regenerated-library-graphic"
    assert canonical_items(original) != canonical_items(reopened)
    assert canonical_items(original, True) == canonical_items(reopened, True)
    assert original["items"][0]["definition"]["items"][0]["item"]["id"]["value"] == "library-graphic"
    graphic["shape"]["width"] = 11
    assert canonical_items(original, True) != canonical_items(reopened, True)
    graphic["shape"]["width"] = 10
    reopened["items"][0]["id"]["value"] = "different-placed-item"
    assert canonical_items(original, True) != canonical_items(reopened, True)


def test_persistence_retains_effective_symbol_ids_and_distinct_overrides():
    from qualification.persistence import persisted_schematic_item

    symbol = {"@type":"type.googleapis.com/kiapi.schematic.types.SchematicSymbolInstance",
              "id":{"value":"placed"}, "definition":{"id":{"entry_name":"R_1"}},
              "lib_id":{"entry_name":"R_1"}}
    normalized = persisted_schematic_item(symbol)
    assert "lib_id" not in normalized
    assert normalized["definition"]["id"] == {"entry_name":"R_1"}
    symbol["lib_id"] = {"library_nickname":"Device", "entry_name":"R"}
    assert persisted_schematic_item(symbol)["lib_id"] == symbol["lib_id"]


def test_board_persistence_retains_placed_ids_pad_parents_and_geometry():
    import copy
    from qualification.persistence import persisted_board_items

    before = [{"@type":"type.googleapis.com/kiapi.board.types.FootprintInstance",
               "id":{"value":"footprint"},"parent":{"value":"board-root-before"},"position":{"x_nm":"10"}},
              {"@type":"type.googleapis.com/kiapi.board.types.Pad", "id":{"value":"pad"},
               "parent":{"value":"footprint"}}]
    after = copy.deepcopy(before)
    after[0]["parent"]["value"] = "board-root-after"
    assert persisted_board_items(before,"test.kicad_pcb") == persisted_board_items(after,"test.kicad_pcb")
    after[1]["parent"]["value"] = "other-footprint"
    assert persisted_board_items(before,"test.kicad_pcb") != persisted_board_items(after,"test.kicad_pcb")
    after[1]["parent"]["value"] = "footprint"
    after[0]["position"]["x_nm"] = "11"
    assert persisted_board_items(before,"test.kicad_pcb") != persisted_board_items(after,"test.kicad_pcb")
    after[0]["parent"]["value"] = "pad"
    with pytest.raises(AssertionError, match="root points at an item"):
        persisted_board_items(after,"test.kicad_pcb")


def test_board_persistence_ignores_definition_order_but_retains_all_content():
    import copy
    from qualification.persistence import persisted_board_items

    before = [{"@type": "type.googleapis.com/kiapi.board.types.FootprintInstance",
               "id": {"value": "footprint"}, "parent": {"value": "board"},
               "definition": {"items": [
                   {"id": {"value": "text"}, "text": "R1"},
                   {"id": {"value": "line"}, "position": {"x_nm": "10"}},
                   {"model": "fixture.step"}]}}]
    after = copy.deepcopy(before)
    after[0]["definition"]["items"].reverse()
    canonical = lambda items: persisted_board_items(items, "test.kicad_pcb")
    assert canonical(before) == canonical(after)
    assert before[0]["definition"]["items"][0]["id"]["value"] == "text"
    after[0]["definition"]["items"][1]["position"]["x_nm"] = "11"
    assert canonical(before) != canonical(after)
    after[0]["definition"]["items"][1]["position"]["x_nm"] = "10"
    after[0]["definition"]["items"].append({"model": "fixture.step"})
    assert canonical(before) != canonical(after)


@pytest.fixture
def native_payload(tmp_path):
    from hashlib import sha256
    import json

    install = tmp_path / "install"
    hashes = {}
    for name in ("kicad", "kicad-cli", "_eeschema.kiface", "_pcbnew.kiface"):
        path = install / "bin" / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(name.encode())
        hashes[f"bin/{name}"] = sha256(path.read_bytes()).hexdigest()
    info = {"source_revision": "a" * 40, "application_version": "10.99-test",
            "build": {"compiler": "test"}, "platform": {"os": "test"},
            "source_archive": {"filename": "source.tar.gz", "sha256": "b" * 64},
            "files_sha256": hashes}
    manifest = tmp_path / "native.json"
    manifest.write_text(json.dumps(info))
    return install, manifest, info


def test_native_provenance_refuses_missing_or_mismatched_payload_identity(native_payload):
    import json

    install, manifest, info = native_payload
    assert native_provenance(install, manifest) == info
    leftover = install / "bin/stale-helper"
    leftover.write_bytes(b"left over from an earlier staged install")
    with pytest.raises(ValueError, match="unrecorded=.*bin/stale-helper"):
        native_provenance(install, manifest)
    leftover.unlink()
    (install / "bin/kicad").write_bytes(b"other binary")
    with pytest.raises(ValueError, match="does not match provenance"):
        native_provenance(install, manifest)
    info["files_sha256"] = {}
    manifest.write_text(json.dumps(info))
    with pytest.raises(ValueError, match="core executable hashes"):
        native_provenance(install, manifest)


def test_native_provenance_records_only_aliases_to_hashed_payload_files(native_payload, tmp_path):
    import os

    if os.name != "posix":
        pytest.skip("staged Linux library symlinks require Unix")
    install, manifest, info = native_payload
    alias = install / "bin/kicad-alias"
    alias.symlink_to("kicad")
    assert native_provenance(install, manifest) == {**info, "symlinks": {"bin/kicad-alias": "kicad"}}
    alias.unlink()
    for target in (tmp_path / "outside", install / "missing"):
        target.write_bytes(b"not a recorded payload file")
        alias.symlink_to(target)
        with pytest.raises(ValueError, match="requires a recorded file target"):
            native_provenance(install, manifest)
        alias.unlink()
        target.unlink()


def test_installed_open_failure_is_not_replayed_and_owned_app_is_stopped(monkeypatch):
    import asyncio
    from contextlib import asynccontextmanager
    from qualification import check_install

    requests, stopped = [], []

    class Peer:
        async def call_tool(self, name, arguments):
            requests.append(name)
            return SimpleNamespace(isError=name == "live_schematic_open",
                                   structuredContent={"documents": []})

    @asynccontextmanager
    async def session(runtime):
        yield Peer()

    runtime = SimpleNamespace(kind="docker", execute=lambda *args: "123")
    monkeypatch.setattr(check_install, "session", session)
    monkeypatch.setattr(check_install, "stop", lambda runtime, pid: stopped.append(pid))
    with pytest.raises(AssertionError):
        asyncio.run(check_install.start(runtime))
    assert requests == ["live_schematic_documents", "live_schematic_open"]
    assert stopped == ["123"]


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


@pytest.mark.parametrize("contents", ["invalid json\n", "[]", '{"api": null}',
                                      '{"do_not_show_again": false}'])
def test_packaged_initialization_warns_without_blocking_or_replacing_invalid_settings(tmp_path, capsys, contents):
    config = tmp_path / "10.99"
    config.mkdir()
    settings = config / "kicad_common.json"
    settings.write_text(contents)
    configure_desktop(tmp_path)
    assert settings.read_text() == contents
    assert "Warning: leaving" in capsys.readouterr().err
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
