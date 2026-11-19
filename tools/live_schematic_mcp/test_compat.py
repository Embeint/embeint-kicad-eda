# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline checks of the KiCad 10 compatibility helpers and the committed fixture/golden files."""

import io
import json
from pathlib import Path
import zipfile

import pytest

from compat.check_compat import GOLDEN, FIXTURE, PCB_EXPORTS, SCH_EXPORTS, classify, reference_jobs
from compat.formats import file_versions, normalize_bytes, project_versions, tree_hash

# Versions the fork's writers declare (eeschema/sch_file_versions.h,
# pcbnew/pcb_io/kicad_sexpr/pcb_io_kicad_sexpr.h, common/project/*.cpp).  KiCad 10.0.6 writes
# 20260306 / 20260206 / 20251024 / 20260206, project 3 and local settings 5 (see the matrix).
FORK = {"kicad_sch": 20261106, "kicad_pcb": 20261031, "kicad_sym": 20261106, "kicad_mod": 20261031}
REFERENCE = {"kicad_sch": 20260306, "kicad_pcb": 20260206, "kicad_sym": 20251024, "kicad_mod": 20260206}


def test_sexpr_header(tmp_path):
    path = tmp_path / "a.kicad_sch"
    path.write_text('(kicad_sch\n\t(version 20261106)\n\t(generator "eeschema")\n\t(generator_version "10.99")\n'
                    '\t(uuid "x")\n)\n')
    assert file_versions(path) == {"root": "kicad_sch", "version": 20261106, "generator": "eeschema",
                                   "generator_version": "10.99", "kind": "kicad_sch", "embedded_files": False}


def test_json_and_table_versions(tmp_path):
    (tmp_path / "p.kicad_pro").write_text(json.dumps(
        {"meta": {"filename": "p.kicad_pro", "version": 4}, "board": {"design_settings": {"meta": {"version": 4}}}}))
    (tmp_path / "fp-lib-table").write_text('(fp_lib_table\n  (version 7)\n)\n')
    (tmp_path / "p.kicad_dru").write_text('(version 1)\n(rule "a" (constraint clearance (min 1mm)))\n')
    found = project_versions(tmp_path)
    assert found["p.kicad_pro"]["versions"] == {"/": 4, "/board/design_settings": 4}
    assert found["fp-lib-table"]["version"] == 7 and found["fp-lib-table"]["root"] == "fp_lib_table"
    assert found["p.kicad_dru"] == {"kind": "kicad_dru", "version": 1, "rules": 1}


@pytest.mark.parametrize("name,a,b", [
    ("x.net", b'(export (version "E")\n (design (source "/tmp/p1/x.kicad_sch")\n (date "2026-11-19T15:41:23")\n'
              b' (tool "Eeschema 10.99.0-123-gabc")))',
              b'(export (version "E")\n (design (source "/tmp/p2/x.kicad_sch")\n (date "2026-11-20T01:02:03")\n'
              b' (tool "Eeschema 10.0.6")))'),
    ("x-F_Cu.gbr", b"%TF.GenerationSoftware,KiCad,Pcbnew,10.99.0*%\n%TF.CreationDate,2026-11-19T15:41:27+10:00*%\n"
                   b"G04 Created by KiCad (PCBNEW 10.99.0) date 2026-11-19 15:41:27*\nX1Y2D02*\n",
                   b"%TF.GenerationSoftware,KiCad,Pcbnew,10.0.6*%\n%TF.CreationDate,2026-11-20T01:00:00+00:00*%\n"
                   b"G04 Created by KiCad (PCBNEW 10.0.6) date 2026-11-20 01:00:00*\nX1Y2D02*\n"),
    ("s.txt", b"PCB statistics report\n- Date: 19/11/2026 3:41:43 PM\n- Project: p\n",
              b"PCB statistics report\n- Date: 20/11/2026 11:02:00 AM\n- Project: p\n"),
    ("b.pdf", b"%PDF-1.5\n1 0 obj << /Producer (KiCad 10.99) /CreationDate (D:20261119154100+10'00') >>\n"
              b"xref\n0 2\n0000000000 65535 f\n0000000010 00000 n\ntrailer << /ID [<AB><CD>] >>\nstartxref\n123\n",
              b"%PDF-1.5\n1 0 obj << /Producer (KiCad 10.0.6) /CreationDate (D:20261120010000Z) >>\n"
              b"xref\n0 2\n0000000000 65535 f\n0000000011 00000 n\ntrailer << /ID [<EF><01>] >>\nstartxref\n124\n"),
])
def test_volatile_fields_are_normalized(name, a, b):
    assert normalize_bytes(a, name, ("/tmp/p1",)) == normalize_bytes(b, name, ("/tmp/p2",))


def test_content_changes_are_not_normalized():
    a = b"%TF.CreationDate,2026-11-19T15:41:27+10:00*%\nX1Y2D02*\n"
    assert normalize_bytes(a, "x.gbr") != normalize_bytes(a.replace(b"X1Y2", b"X1Y3"), "x.gbr")


def test_windows_paths_with_escaped_separators():
    data = b'(source "C:\\\\work\\\\p1\\\\x.kicad_sch") C:\\work\\p1\\y C:/work/p1/z'
    assert normalize_bytes(data, "x.net", ("C:\\work\\p1",)).count(b"<DIR0>") == 3


def test_structural_exports_ignore_order_but_not_counts():
    a = b'<?xml version="1.0"?><IPC-2581><Pad x="1"/><Pin n="2"/></IPC-2581>'
    b = b'<?xml version="1.0"?><IPC-2581><Pin n="7"/><Pad x="3"/></IPC-2581>'
    assert normalize_bytes(a, "b.xml") == normalize_bytes(b, "b.xml")
    assert normalize_bytes(a, "b.xml") != normalize_bytes(a.replace(b"<Pin", b"<Pad"), "b.xml")

    def archive(info):
        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, "w") as z:
            z.writestr("odb/misc/info", info)
            z.writestr("odb/steps/pcb/layers/top/features", b"$1 r100\n$2 rect1x1\nP 1 2 1 P 0 0\n")
        return buffer.getvalue()

    one = archive(b"CREATION_DATE=20261119.154139\nSAVE_APP=KiCad 10.0.6\n")
    two = archive(b"CREATION_DATE=20261120.010000\nSAVE_APP=KiCad 10.99.0\n")
    assert normalize_bytes(one, "board-odb.zip") == normalize_bytes(two, "board-odb.zip")


def test_classify():
    assert classify(1, "Error: file was created by a newer version of KiCad", False) == "refused-newer-format"
    assert classify(0, "", True) == "ok"
    assert classify(3, "failed to load", False) == "failed"


def test_reference_jobs_cover_every_mcp_export(tmp_path):
    for suffix in (".kicad_pro", ".kicad_sch", ".kicad_pcb"):
        (tmp_path / f"p{suffix}").write_text("")
    names = {name for name, _, _ in reference_jobs(tmp_path, tmp_path / "out")}
    assert {f"sch export {k}" for k in SCH_EXPORTS} <= names
    assert {f"pcb export {k}" for k in PCB_EXPORTS} <= names


def test_committed_fixture_declares_fork_versions():
    if not FIXTURE.exists():
        pytest.skip("fixture not generated")
    found = project_versions(FIXTURE)
    kinds = {}
    for value in found.values():
        kinds.setdefault(value["kind"], set()).add(value.get("version"))
    for kind, version in FORK.items():
        assert kinds.get(kind) == {version}, (kind, kinds.get(kind))
        assert version > REFERENCE[kind], "fork files are newer than KiCad 10.0.6 can declare"
    assert any(v.get("embedded_files") for k, v in found.items() if k.endswith(".kicad_mod"))
    assert {"sym-lib-table", "fp-lib-table", "compat-fixture.kicad_dru", "compat-fixture.kicad_pro",
            "power.kicad_sch"} <= set(found)
    # Small enough to keep in the repository
    assert sum(p.stat().st_size for p in FIXTURE.rglob("*") if p.is_file()) < 400_000


def test_golden_covers_every_export():
    if not Path(GOLDEN).exists():
        pytest.skip("golden hashes not generated")
    golden = json.loads(Path(GOLDEN).read_text())["exports"]
    assert set(golden) == {f"sch/{k}" for k in SCH_EXPORTS} | {f"pcb/{k}" for k in PCB_EXPORTS}
    for entry in golden.values():
        assert entry["files"] and entry["tree_sha256"] == tree_hash(entry["files"])
