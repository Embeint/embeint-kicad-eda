# SPDX-License-Identifier: GPL-3.0-or-later
"""Acceptance-runtime guards: native identity, owned lifetimes and honest probe labels."""

from hashlib import sha256
import json
import os
from types import SimpleNamespace

import pytest

from qualification.capture_runtime import (cleanup_steps, display_number, loaded_identity,
                                           native_identity)

posix_only = pytest.mark.skipif(os.name != "posix", reason="/proc maps and select() on pipes need Linux/WSL")
CORE = ("bin/kicad", "bin/kicad-cli", "bin/_eeschema.kiface", "bin/_pcbnew.kiface",
        "lib/libkicommon.so.10.99.0")


@pytest.fixture
def retained(tmp_path):
    """An install, its complete manifest and the source archive the manifest names."""
    install = tmp_path / "install"
    for name in CORE:
        (install / name).parent.mkdir(parents=True, exist_ok=True)
        (install / name).write_bytes(name.encode())
    archive = tmp_path / "artifacts" / "native-source.tar.gz"
    archive.parent.mkdir()
    archive.write_bytes(b"retained source")
    provenance = archive.parent / "native-provenance.json"
    provenance.write_text(json.dumps({
        "files_sha256": {name: sha256(name.encode()).hexdigest() for name in CORE},
        "source_revision": "a" * 40, "source_description": "synthetic", "source_changes": {},
        "source_archive": {"filename": archive.name, "sha256": sha256(b"retained source").hexdigest()}}))
    return install, provenance, archive


def test_native_identity_rejects_a_changed_library_not_only_editor_binaries(retained):
    install, provenance, _ = retained
    assert native_identity(install, provenance)["source_revision"] == "a" * 40
    # The same build installed under another prefix rebuilds libkicommon; the
    # editors and manager stay byte-identical, so a four-binary check passes.
    (install / "lib/libkicommon.so.10.99.0").write_bytes(b"other install prefix")
    with pytest.raises(ValueError, match="libkicommon"):
        native_identity(install, provenance)


def test_native_identity_ties_binaries_to_the_retained_source_archive(retained):
    install, provenance, archive = retained
    archive.write_bytes(b"a different source tree")
    with pytest.raises(ValueError, match="source archive"):
        native_identity(install, provenance)


def test_native_identity_requires_a_manifest_covering_libraries(retained):
    install, provenance, _ = retained
    record = json.loads(provenance.read_text())
    del record["files_sha256"]["lib/libkicommon.so.10.99.0"]
    provenance.write_text(json.dumps(record))
    with pytest.raises(ValueError, match="cover"):
        native_identity(install, provenance)


@posix_only
def test_loaded_modules_must_all_come_from_the_verified_manifest(retained, tmp_path):
    install, provenance, _ = retained
    native = native_identity(install, provenance)
    dependency = tmp_path / "deps" / "libwx.so.3"
    dependency.parent.mkdir()
    dependency.write_bytes(b"private wx")
    dependencies = {str(dependency.resolve()): sha256(b"private wx").hexdigest()}
    lines = [f"7f00-7f01 r-xp 00000000 08:01 1 {(install / name).resolve()}" for name in CORE]
    lines += [f"7f02-7f03 r-xp 00000000 08:01 2 {dependency.resolve()}",
              "7f04-7f05 rw-s 00000000 00:01 3 /memfd:mesa (deleted)",
              "7f06-7f07 rw-s 00000000 00:01 4 /SYSV00000000 (deleted)",
              "7f08-7f09 rw-p 00000000 00:00 0 [heap]"]
    loaded = loaded_identity(1, install, native, dependencies, maps="\n".join(lines))
    assert set(loaded["install"]) == set(CORE)
    assert list(loaded["private_dependencies"]) == [str(dependency.resolve())]
    assert loaded["anonymous_shared"] == ["/SYSV00000000 (deleted)", "/memfd:mesa (deleted)"]

    unlisted = install / "lib/libkiplugin.so"
    unlisted.write_bytes(b"not in manifest")
    with pytest.raises(ValueError, match="absent from or differs"):
        loaded_identity(1, install, native, dependencies,
                        maps="\n".join(lines + [f"7f0a-7f0b r-xp 0 08:01 5 {unlisted.resolve()}"]))
    with pytest.raises(ValueError, match="deleted file"):
        loaded_identity(1, install, native, dependencies,
                        maps="\n".join(lines + [f"7f0a-7f0b r-xp 0 08:01 5 {install.resolve()}/lib/old.so (deleted)"]))
    dependency.write_bytes(b"replaced during run")
    with pytest.raises(ValueError, match="private dependency changed"):
        loaded_identity(1, install, native, dependencies, maps="\n".join(lines))


@posix_only
def test_loaded_modules_must_include_editors_and_libkicommon(retained):
    install, provenance, _ = retained
    native = native_identity(install, provenance)
    maps = f"7f00-7f01 r-xp 00000000 08:01 1 {(install / 'bin/kicad').resolve()}"
    with pytest.raises(ValueError, match="did not load"):
        loaded_identity(1, install, native, {}, maps=maps)


@posix_only
def test_display_allocation_has_a_deadline_even_for_a_partial_line():
    import threading

    read_fd, write_fd = os.pipe()
    try:
        # A stalled Xvfb that wrote part of its line must not block forever.
        os.write(write_fd, b"12")
        with pytest.raises(TimeoutError):
            display_number(read_fd, 0.2)
    finally:
        os.close(read_fd)
        os.close(write_fd)
    read_fd, write_fd = os.pipe()
    finisher = threading.Timer(0.1, lambda: os.write(write_fd, b"\n"))
    try:
        os.write(write_fd, b"12")
        finisher.start()
        assert display_number(read_fd, 2) == "12"
    finally:
        finisher.join()
        os.close(read_fd)
        os.close(write_fd)
    read_fd, write_fd = os.pipe()
    os.close(write_fd)
    try:
        with pytest.raises(RuntimeError, match="closed"):
            display_number(read_fd, 1)
    finally:
        os.close(read_fd)


def test_cleanup_attempts_every_step_and_fails_the_receipt():
    report = {"result": "passed"}
    stopped = []

    def stuck():
        raise TimeoutError("manager ignored SIGKILL")

    cleanup_steps(report, [("KiCad manager", stuck), ("Xvfb", lambda: stopped.append("Xvfb"))])
    assert stopped == ["Xvfb"]
    assert report["result"] == "failed"
    assert report["cleanup_errors"] == ["KiCad manager: TimeoutError: manager ignored SIGKILL"]


class FakeEditor:
    """Two placed units of one physical U2; UpdateItems behaviour is configurable."""

    def __init__(self, mode):
        from client import kiid, sch, types

        self.mode, self.undos, self.history = mode, 0, []
        self.doc = types.DocumentSpecifier(sheet_path=types.SheetPath(path=[kiid()]))
        self.units = []
        for unit in (1, 2):
            item = sch.SchematicSymbolInstance(id=kiid())
            item.reference_field.text.text = "U2"
            item.value_field.text.text = "Dual"
            item.unit.unit = unit
            self.units.append(item)
        self.lib_ids = {item.id.value: "Acceptance:Dual" for item in self.units}

    def items(self, doc):
        from google.protobuf.any_pb2 import Any

        packed = []
        for item in self.units:
            value = Any()
            value.Pack(item)
            packed.append(value)
        return SimpleNamespace(items=packed)

    def symbol_pins(self, doc):
        return [{"id": item.id.value, "reference": "U2" + "AB"[item.unit.unit - 1],
                 "lib_id": self.lib_ids[item.id.value], "unit": item.unit.unit,
                 "pins": [{"number": str(number), "id": f"{item.id.value}-{number}", "net": f"N{number}",
                           "position_mm": [number, 0]}
                          for number in ((1, 2) if item.unit.unit == 1 else (3, 4))]}
                for item in self.units]

    def mutate(self, doc, items, update=False):
        from client import sch

        if self.mode == "reject":
            raise RuntimeError("KiCad did not acknowledge the entire item batch")
        self.history.append(([type(item)() for item in self.units], dict(self.lib_ids)))
        for saved, item in zip(self.history[-1][0], self.units):
            saved.CopyFrom(item)
        if self.mode == "ignore":
            return
        for index, item in enumerate(self.units):
            if item.id.value == items[0].id.value:
                replacement = sch.SchematicSymbolInstance()
                replacement.CopyFrom(items[0])
                self.units[index] = replacement
                if self.mode == "drop_nickname":
                    self.lib_ids[item.id.value] = "Dual"

    def undo(self, doc):
        self.undos += 1
        self.units, self.lib_ids = self.history.pop()
        return {"performed": True}


def probe(mode):
    from qualification.capture_acceptance import update_regressions

    editor = FakeEditor(mode)
    result = {}
    update_regressions(editor, {"MCU": editor.doc}, result)
    return editor, result


def statuses(result):
    return (result["multi_unit_update_duplicate_pin_ids"]["status"],
            result["update_dropped_library_nickname"]["status"])


def test_update_probe_covers_each_unit_and_requires_a_real_change():
    editor, result = probe("apply")
    assert statuses(result) == ("historical_not_reproduced", "historical_not_reproduced")
    assert [record["unit"] for record in result["units"]] == [1, 2]
    assert all(record["applied"] and record["restored"] for record in result["units"])
    assert editor.undos == 2


def test_ignored_update_is_inconclusive_not_evidence_of_absence():
    _, result = probe("ignore")
    assert statuses(result) == ("inconclusive", "inconclusive")
    assert not any(record["applied"] for record in result["units"])


def test_rejected_update_is_recorded_without_undoing_an_earlier_edit():
    editor, result = probe("reject")
    assert statuses(result) == ("inconclusive", "inconclusive")
    assert editor.undos == 0
    assert all("did not acknowledge" in record["rejected"] for record in result["units"])


def test_dropped_nickname_is_reproduced_with_its_unit_and_still_undone():
    editor, result = probe("drop_nickname")
    findings = result["update_dropped_library_nickname"]["findings"]
    assert result["update_dropped_library_nickname"]["status"] == "reproduced"
    assert {finding["probed_unit"] for finding in findings} == {1, 2}
    assert editor.undos == 2 and editor.lib_ids == {item.id.value: "Acceptance:Dual" for item in editor.units}
