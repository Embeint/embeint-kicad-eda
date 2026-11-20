# SPDX-License-Identifier: GPL-3.0-or-later
"""live_pcb_fab_package: checks before exporting, the exports, verification, manifest and zip."""

import io
import json
from pathlib import Path
import uuid
import zipfile

import pytest

from board_client import bc, board_tasks, bt, editor, enums, jobs, tasks, types
from client import LiveSchematic
import fab_fixture
import fab_package
from fake_endpoints import unused_socket

LAYER_NAMES = {value.number: value.name for value in bt.BoardLayer.DESCRIPTOR.values}
SHEET = str(uuid.uuid4())


def board_doc(path):
    return types.DocumentSpecifier(type=types.DOCTYPE_PCB, board_filename="demo.kicad_pcb",
                                   project=types.ProjectSpecifier(name="demo", path=str(path)))


def schematic_json(path):
    return {"type": "DOCTYPE_SCHEMATIC", "project": {"name": "demo", "path": str(path)},
            "sheet_path": {"path": [{"value": SHEET}]}}


class FakeKiCad(LiveSchematic):
    """Answers the package's requests and writes the outputs KiCad would."""

    def __init__(self, project, modified=(), violations=(), copper=4, drop_profile=False, schematics=None):
        super().__init__(unused_socket("unused-fab-socket"))
        self.project, self.modified, self.violations = project, modified, violations
        self.copper, self.drop_profile = copper, drop_profile
        self.schematics = [schematic_json(project)] if schematics is None else schematics
        self.requests = []

    def send(self, request, response_type):
        self.requests.append(request)
        name = type(request).__name__
        if name == "GetDocumentRevision":
            kind = "board" if request.document.type == types.DOCTYPE_PCB else "schematic"
            return editor.DocumentRevisionResponse(revision=f"{kind}-rev", modified=kind in self.modified)
        if name == "GetOpenDocuments":
            from google.protobuf import json_format
            return json_format.ParseDict({"documents": self.schematics}, editor.GetOpenDocumentsResponse())
        if name == "StartDrcTask":
            assert request.test_schematic_parity and request.require_zones_filled
            return tasks.StartTaskResponse(token=tasks.TaskToken(value="drc"))
        if name == "GetTaskStatus":
            return tasks.TaskStatusResponse(state=tasks.TS_COMPLETED)
        if name == "GetDrcTaskResult":
            result = board_tasks.DrcTaskResult(ignored_count=1)
            for kind, severity, text in self.violations:
                result.violations.add(error_type=kind, severity=severity, description=text)
            return result
        if name == "GetBoardEnabledLayers":
            return bc.BoardEnabledLayersResponse(copper_layer_count=self.copper)
        if name.startswith("Run"):
            fab_fixture.write_outputs(request, LAYER_NAMES, self.drop_profile)
            return jobs.RunJobResponse(status=jobs.JS_SUCCESS, output_path=[request.job_settings.output_path])
        raise AssertionError(f"unexpected {name}")

    def names(self):
        return [type(r).__name__ for r in self.requests]


def test_package_exports_checks_and_records_everything(tmp_path):
    kicad = FakeKiCad(tmp_path)
    result = kicad.fab_package(board_doc(tmp_path))
    out = tmp_path / "Outputs"
    assert result["exported"] and result["output_dir"] == f"{tmp_path}/Outputs" and result["project"] == "demo"
    assert kicad.names() == ["GetDocumentRevision", "GetOpenDocuments", "GetDocumentRevision", "StartDrcTask",
                             "GetTaskStatus", "GetDrcTaskResult", "GetBoardEnabledLayers",
                             "RunBoardJobExportGerbers", "RunBoardJobExportDrill", "RunBoardJobExportPosition",
                             "RunBoardJobExport3D", "RunBoardJobExportPdf", "RunSchematicJobExportBOM",
                             "RunSchematicJobExportPdf"]
    gerbers = kicad.requests[7]
    assert not gerbers.use_board_plot_params and gerbers.use_x2_format and gerbers.create_gerber_job_file
    assert [LAYER_NAMES[v] for v in gerbers.plot_settings.layers][:5] == ["BL_F_Cu", "BL_In1_Cu", "BL_In2_Cu",
                                                                        "BL_B_Cu", "BL_F_Mask"]
    assert gerbers.job_settings.output_path == f"{tmp_path}/Outputs/Gerbers/"
    drill = kicad.requests[8]
    assert drill.units == enums.U_MM and drill.map_format and not drill.excellon.combine_pth_npth
    position = kicad.requests[9]
    assert position.job_settings.output_path == f"{tmp_path}/Outputs/PnP/demo-pos.csv" and position.single_file
    assert kicad.requests[12].job_settings.document.type == types.DOCTYPE_SCHEMATIC
    assert list(result["outputs"]) == ["gerbers", "drill", "position", "step", "assembly_pdf", "bom",
                                       "schematic_pdf"]
    assert "Gerbers/demo-job.gbrjob" in result["outputs"]["gerbers"]
    assert result["outputs"]["drill"] == ["Gerbers/demo-NPTH-drl_map.pdf", "Gerbers/demo-NPTH.drl",
                                          "Gerbers/demo-PTH-drl_map.pdf", "Gerbers/demo-PTH.drl",
                                          "Gerbers/demo-drl.rpt"]
    verification = result["verification"]
    assert verification["gerbers"]["copper_layers"] == 4 and verification["gerbers"]["files"] == 11
    assert verification["drill"]["holes"] == 5
    assert verification["drill"]["files"]["demo-PTH.drl"] == {"tools": 2, "holes": 4}
    assert verification["position"] == {"parts": 4, "top": 3, "bottom": 1}
    assert verification["bom"] == {"lines": 3, "parts": 4, "lines_without_mpn": 2}
    assert verification["bom_vs_position"] == {"bom_only": ["J1"], "position_only": ["FID1"]}
    assert result["warnings"] == ["bom: 2 part(s) without an MPN: R1, R2",
                                  "bom: 1 part(s) in the BOM have no placement: J1"]
    assert result["checks"]["drc"]["errors"] == 0 and result["checks"]["drc"]["ignored"] == 1
    # The zip holds the Gerbers folder, flat, and is reproducible
    assert result["zip"] == "demo-gerbers.zip"
    archive = zipfile.ZipFile(io.BytesIO((out / "demo-gerbers.zip").read_bytes()))
    assert archive.testzip() is None
    assert sorted(archive.namelist()) == sorted(p.split("/", 1)[1] for p in result["outputs"]["gerbers"]
                                                + result["outputs"]["drill"])
    assert archive.read("demo-PTH.drl").decode() == fab_fixture.EXCELLON_PTH
    manifest = json.loads((out / "manifest.json").read_text(encoding="utf-8"))
    assert manifest["board_revision"] == "board-rev" and manifest["warnings"] == result["warnings"]
    assert {f["path"] for f in manifest["files"]} == {p for v in result["outputs"].values() for p in v} | {
        "demo-gerbers.zip"}
    assert result["files"] == len(manifest["files"]) + 1
    entry = next(f for f in manifest["files"] if f["path"] == "PnP/demo-pos.csv")
    assert entry["bytes"] == len(fab_fixture.POSITION) and len(entry["sha256"]) == 64


def test_unsaved_design_blocks_the_package(tmp_path):
    kicad = FakeKiCad(tmp_path, modified=("board", "schematic"))
    result = kicad.fab_package(board_doc(tmp_path))
    assert not result["exported"] and result["blocked_by"] == "unsaved"
    assert result["unsaved"] == ["demo.kicad_pcb", "demo.kicad_sch"]
    assert not (tmp_path / "Outputs").exists() and "StartDrcTask" not in kicad.names()
    allowed = FakeKiCad(tmp_path, modified=("board",)).fab_package(board_doc(tmp_path), allow_unsaved=True)
    assert allowed["exported"] and allowed["unsaved"] == ["demo.kicad_pcb"]


def test_drc_errors_block_unless_checks_are_off(tmp_path):
    violations = [(5, types.RS_ERROR, "Clearance violation"), (5, types.RS_WARNING, "Silk over copper")]
    kicad = FakeKiCad(tmp_path, violations=violations)
    result = kicad.fab_package(board_doc(tmp_path), outputs=["gerbers"])
    assert not result["exported"] and result["blocked_by"] == "drc"
    drc = result["checks"]["drc"]
    assert drc["errors"] == 1 and drc["warnings"] == 1 and sum(drc["by_type"].values()) == 2
    assert drc["violations"][0]["severity"] == "ERROR" and not (tmp_path / "Outputs").exists()
    skipped = FakeKiCad(tmp_path, violations=violations)
    assert skipped.fab_package(board_doc(tmp_path), outputs=["gerbers"], checks=False)["exported"]
    assert "StartDrcTask" not in skipped.names() and "GetOpenDocuments" not in skipped.names()


def test_existing_outputs_are_replaced_only_with_overwrite(tmp_path):
    out = tmp_path / "Outputs"
    (out / "Gerbers").mkdir(parents=True)
    (out / "Gerbers" / "old-F_Cu.gbr").write_text("old")
    (out / "notes.txt").write_text("kept")
    with pytest.raises(ValueError, match="overwrite=true"):
        FakeKiCad(tmp_path).fab_package(board_doc(tmp_path), outputs=["gerbers"])
    result = FakeKiCad(tmp_path).fab_package(board_doc(tmp_path), outputs=["gerbers"], overwrite=True)
    assert result["exported"] and not (out / "Gerbers" / "old-F_Cu.gbr").exists()
    assert (out / "notes.txt").read_text() == "kept"
    (out / "Gerbers" / "sub").mkdir()
    with pytest.raises(ValueError, match="contains folders"):
        FakeKiCad(tmp_path).fab_package(board_doc(tmp_path), outputs=["gerbers"], overwrite=True)


def test_arguments_are_checked_before_kicad_is_asked(tmp_path):
    kicad = FakeKiCad(tmp_path)
    for kwargs, message in (({"outputs": ["gerber"]}, "Unknown outputs"), ({"outputs": []}, "at least one"),
                            ({"options": {"odb": {}}}, "not exported"),
                            ({"options": {"drill": 3}}, "must be an object"),
                            ({"options": {"drill": {"job_settings": {}}}}, "job_settings")):
        with pytest.raises(ValueError, match=message):
            kicad.fab_package(board_doc(tmp_path), **kwargs)
    from google.protobuf.json_format import ParseError
    with pytest.raises(ParseError):
        kicad.fab_package(board_doc(tmp_path), options={"drill": {"units": "U_FURLONG"}})
    nameless = types.DocumentSpecifier(type=types.DOCTYPE_PCB, board_filename="x.kicad_pcb")
    with pytest.raises(ValueError, match="output_dir is required"):
        kicad.fab_package(nameless)
    assert kicad.requests == []


def test_schematic_must_be_open_for_bom_and_pdf(tmp_path):
    kicad = FakeKiCad(tmp_path, schematics=[schematic_json("/elsewhere")])
    with pytest.raises(ValueError, match="No schematic of this project"):
        kicad.fab_package(board_doc(tmp_path), outputs=["bom"])
    result = FakeKiCad(tmp_path, schematics=[]).fab_package(board_doc(tmp_path), outputs=["position"],
                                                            checks=False)
    assert result["exported"] and result["zip"] is None and "bom_vs_position" not in result["verification"]


def test_schematic_path_with_a_trailing_separator_matches(tmp_path):
    # KiCad on Windows reports "Y:\\proj\\" for the schematic's project and "Y:\\proj" for the board's
    schematic = schematic_json(f"{tmp_path}\\")
    kicad = FakeKiCad(tmp_path, schematics=[schematic])
    result = kicad.fab_package(board_doc(tmp_path), outputs=["bom"], checks=False)
    assert result["exported"] and result["outputs"]["bom"] == ["BOM/demo-bom.csv"]


def test_placement_file_named_by_kicad_is_still_checked(tmp_path):
    # Without naked_filename KiCad writes demo-pos-all-pos.csv; the only CSV written is checked
    result = FakeKiCad(tmp_path).fab_package(board_doc(tmp_path), outputs=["position"], checks=False,
                                             options={"position": {"naked_filename": False}})
    assert result["outputs"]["position"] == ["PnP/demo-pos-all-pos.csv"]
    assert result["verification"]["position"]["parts"] == 4 and result["warnings"] == []
    assert fab_package.csv_file(["a.csv", "b.csv"], "c.csv") is None
    assert fab_package.csv_file(["x.txt", "b.CSV"], "c.csv") == "b.CSV"


class WarningKiCad(FakeKiCad):
    """KiCad warnings (JS_WARNING): the outputs are written, except STEP of a malformed outline."""

    def send(self, request, response_type):
        response = super().send(request, response_type)
        name = type(request).__name__
        if name == "RunBoardJobExport3D":
            Path(request.job_settings.output_path).unlink()
            return jobs.RunJobResponse(status=jobs.JS_WARNING,
                                       message="Board outline is malformed. Run DRC for a full analysis.")
        if name == "RunBoardJobExportPosition":
            response.status, response.message = jobs.JS_WARNING, "Odd footprint"
        return response


def test_kicad_warnings_are_reported_and_an_empty_export_fails(tmp_path):
    result = WarningKiCad(tmp_path).fab_package(board_doc(tmp_path), outputs=["position"], checks=False)
    assert result["exported"] and result["warnings"][0] == "position: KiCad: Odd footprint"
    with pytest.raises(RuntimeError, match="step export reported success but wrote nothing.*Board outline"):
        WarningKiCad(tmp_path).fab_package(board_doc(tmp_path), outputs=["step"], checks=False, overwrite=True)
    single = WarningKiCad(tmp_path).board_export(board_doc(tmp_path), "position", str(tmp_path / "p.csv"))
    assert single["message"] == "Odd footprint"


def test_verification_flags_missing_layers(tmp_path):
    kicad = FakeKiCad(tmp_path, copper=2, drop_profile=True)
    options = {"gerbers": {"plot_settings": {"layers": ["BL_F_Cu", "BL_Edge_Cuts"]}}}
    result = kicad.fab_package(board_doc(tmp_path), outputs=["gerbers"], options=options, checks=False)
    assert result["warnings"] == ["gerbers: 1 copper layers plotted, the board has 2",
                                  "gerbers: no board outline (Edge.Cuts profile) was plotted"]


def test_helpers_match_their_contract():
    assert fab_package.gerber_layers(1)[:2] == ["BL_F_Cu", "BL_F_Mask"]
    assert fab_package.gerber_layers(2)[:3] == ["BL_F_Cu", "BL_B_Cu", "BL_F_Mask"]
    assert fab_package.name_stem("C:\\x\\demo.kicad_pcb") == "demo" and fab_package.name_stem(".hidden") == ".hidden"
    assert fab_package.parse_csv('"a,b",c\r\n\r\n"d""e",\n') == [["a,b", "c"], ['d"e', ""]]
    assert fab_package.parse_excellon("M48\r\nT1C0.3\r\n%\r\nT1\r\nX1Y1G85X2Y1\r\nM30\r\n") == (1, 1)
    data = fab_package.stored_zip([("a.txt", b"hello"), ("dir/b.txt", b"")])
    archive = zipfile.ZipFile(io.BytesIO(data))
    assert archive.read("a.txt") == b"hello" and archive.read("dir/b.txt") == b""
    assert fab_package.stored_zip([("a.txt", b"hello")]) == fab_package.stored_zip([("a.txt", b"hello")])


def test_server_advertises_the_package_tool():
    import asyncio
    from server import mcp
    tools = {tool.name: tool for tool in asyncio.run(mcp.list_tools())}
    schema = tools["live_pcb_fab_package"].inputSchema["properties"]
    assert {"board_document", "output_dir", "outputs", "options", "checks", "allow_unsaved", "overwrite",
            "zip_gerbers"} <= set(schema)
    assert "options" in tools["live_pcb_export"].inputSchema["properties"]
    assert "options" in tools["live_schematic_export"].inputSchema["properties"]


def test_layer_names_cover_the_gerber_layers():
    for name in fab_package.gerber_layers(32):
        assert bt.BoardLayer.Value(name)
    assert Path(fab_package.__file__).name == "fab_package.py"
