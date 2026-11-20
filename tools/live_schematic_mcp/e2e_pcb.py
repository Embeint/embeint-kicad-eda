# SPDX-License-Identifier: GPL-3.0-or-later
"""End-to-end check of the PCB editor tools against a running development KiCad.

KiCad must run as the project manager with the test project (run_kicad_wsl.sh or
run_kicad_windows.ps1 without arguments); the test opens the schematic and board itself.  Exercises
page and title block, the page check, grid, both origins, outline, footprint placement and moves, a 3D body colour, routing, vias, zones, graphics, DRC,
undo, views, exports and update-from-schematic.  Never saves; checks the board on disk is unchanged
and removes what it added.

    python e2e_pcb.py [output_dir]
"""

from hashlib import sha256
from pathlib import Path
import sys
import tempfile
import uuid

from client import LiveSchematic
from board_client import bt
from e2e_guard import test_board, test_document
from item_roundtrip import save_string

RESISTOR = "Resistor_SMD:R_0603_1608Metric"


def check(condition, message):
    if not condition:
        raise SystemExit("FAILED: " + message)
    print("ok  ", message)


def digest(path):
    return sha256(path.read_bytes()).hexdigest() if path.exists() else None


def check_fab_package(client, board, schematic):
    """The manufacturing package from the edited (unsaved) test board, into a scratch folder: refused
    while unsaved, then exported with allow_unsaved and checked file by file."""
    import io
    import json
    import zipfile
    with tempfile.TemporaryDirectory(prefix="kicad-live-fab-") as folder:
        blocked = client.fab_package(board, folder, schematic, outputs=["position"])
        check(not blocked["exported"] and blocked["blocked_by"] == "unsaved" and not any(Path(folder).iterdir()),
              "fab package refuses the unsaved board and writes nothing")
        outputs = ["gerbers", "drill", "position", "step", "assembly_pdf", "ipc2581", "ipcd356", "bom",
                   "schematic_pdf"]
        result = client.fab_package(board, folder, schematic, outputs=outputs, checks=False, allow_unsaved=True)
        check(result["exported"] and list(result["outputs"]) == outputs,
              f"fab package exported {result['files']} files; warnings {result['warnings']}")
        copper = client.board_layers(board)["copper_layers"]
        check(result["verification"]["gerbers"]["copper_layers"] == copper,
              f"Gerbers plot all {copper} copper layers and a profile: {result['verification']['gerbers']['functions']}")
        check(result["verification"]["drill"]["files"], f"drill: {result['verification']['drill']}")
        check(result["verification"]["position"]["parts"] > 0, f"placements: {result['verification']['position']}")
        check("bom" in result["verification"], f"BOM: {result['verification']['bom']}")
        root = Path(folder)
        archive = zipfile.ZipFile(io.BytesIO((root / result["zip"]).read_bytes()))
        check(archive.testzip() is None and len(archive.namelist()) == len(result["outputs"]["gerbers"])
              + len(result["outputs"]["drill"]), f"Gerber zip holds {len(archive.namelist())} files")
        manifest = json.loads((root / "manifest.json").read_text(encoding="utf-8"))
        check(all((root / f["path"]).stat().st_size == f["bytes"] for f in manifest["files"]),
              "manifest lists every file with its size")


def check_body_colour(client, board, footprint_id):
    """An extruded 3D body colour is saved at full precision, so the API keeps it exactly rather
    than rounding it to the 8 bits per channel other colours are saved with."""
    footprint = next(f for f in client.board_unpacked(board, "footprints") if f.id.value == footprint_id)
    body = bt.Body3D(kind=bt.BDK_EXTRUDED, visible=True)
    body.id.value = str(uuid.uuid4())
    body.scale.x_nm = body.scale.y_nm = body.scale.z_nm = 1.0
    body.extruded.height.value_nm = 500000
    body.extruded.material = bt.EM_PLASTIC
    body.extruded.color.r, body.extruded.color.g, body.extruded.color.b, body.extruded.color.a = 0.3, 0.45, 0.7, 1.0
    footprint.definition.items.add().Pack(body)
    client.mutate(board, [footprint], update=True)

    footprint = next(f for f in client.board_unpacked(board, "footprints") if f.id.value == footprint_id)
    colours = []
    for packed in footprint.definition.items:
        found = bt.Body3D()
        if packed.Unpack(found) and found.id.value == body.id.value:
            colours.append((found.extruded.color.r, found.extruded.color.g, found.extruded.color.b))
    check(colours == [(0.3, 0.45, 0.7)], f"a 3D body colour is kept at full precision: {colours}")
    saved = save_string(client, board)
    check("(color 0.3 0.45 0.7 1)" in saved, "the 3D body colour is saved at full precision")


def main():
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "live-pcb-output")
    out.mkdir(parents=True, exist_ok=True)
    client = LiveSchematic(timeout_ms=60000)
    schematic = test_document(client)
    board = test_board(client)
    board_file = Path(board.project.path) / board.board_filename
    on_disk = digest(board_file)
    check(board.board_filename.endswith(".kicad_pcb"), f"board open in the PCB editor: {board.board_filename}")
    created = []

    # Page and title block (the drawing block at the bottom right)
    before_page = client.page_layout(board)
    page = client.set_page(board, size="A4", orientation="landscape", title="live-demo - PCB", date="today",
                           revision="A", company="Embeint Pty Ltd", comments=["PCB test board"])
    check(page["paper"] == "A4" and page["page_size_mm"] == [297.0, 210.0], f"page set to A4: {page['page_size_mm']}")
    check(page["title_block"]["title"] == "live-demo - PCB" and page["title_block"]["comments"] == ["PCB test board"],
          "title block fields set")
    check(page["title_block_mm"] and page["title_block_mm"][2] <= 297, f"title block measured: {page['title_block_mm']}")

    # Grid: a preset size, a new size (added as a preset), style; then put it back
    grid0 = client.editor_grid(board)
    grid = client.editor_grid(board, size_mm=0.5, style="lines")
    check(grid["size_mm"] == [0.5, 0.5] and grid["style"] == "lines", f"grid 0.5 mm lines: {grid['size_mm']}")
    odd = client.editor_grid(board, size_mm=0.15)
    check(odd["size_mm"] == [0.15, 0.15] and [0.15, 0.15] in odd["presets_mm"], "a new grid size becomes a preset")
    client.editor_grid(board, preset_index=grid0["preset_index"], style=grid0["style"])
    sch_grid = client.editor_grid(schematic)
    check(sch_grid["size_mm"][0] > 0, f"schematic grid readable: {sch_grid['size_mm']}")

    # Both origins
    origins0 = client.board_origins(board)
    origins = client.board_origins(board, grid=(50, 40), drill=(50, 80))
    check(origins == {"grid_origin_mm": [50.0, 40.0], "drill_origin_mm": [50.0, 80.0]}, f"origins set: {origins}")
    check(client.editor_grid(board)["origin_mm"] == [50.0, 40.0], "the grid starts at the grid origin")

    # Outline inside the drawing area
    outline = client.board_outline(board, 60, 40, 50, 40, corner_radius=2, replace=False)
    created += outline["items"]
    box = outline["outline_mm"]
    check(box and all(abs(a - b) < 0.1 for a, b in zip(box, [50, 40, 110, 80])), f"outline drawn: {box}")

    # Footprints: place, move, rotate, flip
    r1 = client.board_place_footprint(board, RESISTOR, 60, 50, reference="R901", value="10k")
    r2 = client.board_place_footprint(board, RESISTOR, 70, 55, reference="R902", value="1k")
    created += [r1["id"], r2["id"]]
    check(r1["reference"] == "R901" and r1["position_mm"] == [60.0, 50.0], f"placed {r1['reference']} at 60,50")
    moved = client.board_move_footprints(board, [{"reference": "R902", "x_mm": 80, "y_mm": 60, "rotation": 90}])
    r2_now = moved["footprints"][0]
    check(r2_now["position_mm"] == [80.0, 60.0] and r2_now["rotation"] == 90.0, f"R902 moved and rotated: {r2_now}")
    r2_pads = client.board_footprints(board, ["R902"], with_pads=True)[0]["pads"]
    check(all(abs(p["position_mm"][0] - 80) < 1.5 and abs(p["position_mm"][1] - 60) < 1.5 for p in r2_pads),
          f"R902's pads moved with it: {[p['position_mm'] for p in r2_pads]}")
    flipped = client.board_move_footprints(board, [{"reference": "R902", "side": "back"}])
    check(flipped["footprints"][0]["side"] == "back", "R902 flipped to the back")
    back_pads = client.board_footprints(board, ["R902"], with_pads=True)[0]["pads"]
    check(flipped["footprints"][0]["position_mm"] == [80.0, 60.0]
          and all(abs(p["position_mm"][0] - 80) < 1.5 and abs(p["position_mm"][1] - 60) < 1.5 for p in back_pads),
          "the flip kept R902 and its pads in place")
    client.board_move_footprints(board, [{"reference": "R902", "side": "front"}])
    pads = {f["reference"]: f for f in client.board_footprints(board, ["R901", "R902"], with_pads=True)}
    check(len(pads["R901"]["pads"]) == 2 and "size_mm" in pads["R901"]["pads"][0], "pads listed with sizes")
    check_body_colour(client, board, r1["id"])

    # Routing, via, zone, graphics
    # These manually placed fixture footprints have no schematic netlist. Give
    # the routed pads real nets, as production Update PCB from Schematic does.
    for reference, nets in (("R901", {"1": "E2E_B", "2": "E2E_A"}),
                             ("R902", {"1": "E2E_A", "2": "E2E_B"})):
        footprint = client.board_footprint(board, reference)
        for packed in footprint.definition.items:
            pad = bt.Pad()
            if packed.Unpack(pad) and pad.number in nets:
                pad.net.name = nets[pad.number]
                packed.Pack(pad)
        client.mutate(board, [footprint], update=True, description="e2e: pad nets")
    route = client.board_route(board, "R901", "2", "R902", "1", width=0.25)
    created += route["items"]
    check(route["drc_clean"] and route["segments"][0]["layer"] == "F.Cu",
          f"routed R901.2 to R902.1 around everything else: {route['segments'][0]['path_mm']}")
    leg = client.board_route(board, "R901", "1", "R902", "2", width=0.25, avoid=False)
    created += leg["items"]
    check(len(leg["path_mm"]) in (2, 3), f"plain dog-leg on request: {leg['path_mm']}")
    via = client.board_add_via(board, 95, 70, size=0.6, drill=0.3)
    created.append(via["via"])
    tracks = client.board_add_tracks(board, [[90, 70], [95, 70]], "F.Cu", 0.3)
    created += tracks["tracks"]
    zone = client.board_add_zone(board, "", ["B.Cu"], inset=1.0, fill=True)
    created.append(zone["zone"])
    text = client.board_add_graphics(board, [{"kind": "text", "text": "E2E", "position": [100, 45], "layer": "F.SilkS"}])
    created += text["items"]
    summary = client.board_summary(board)
    check(summary["counts"]["tracks"] >= 3 and summary["counts"]["vias"] >= 1 and summary["counts"]["zones"] >= 1,
          f"tracks, via and zone on the board: {summary['counts']}")

    # The page check flags a board item past the border or in the title block
    stray = client.board_add_graphics(board, [{"kind": "text", "text": "STRAY", "position": [5, 5], "layer": "F.SilkS"},
                                              {"kind": "text", "text": "TB", "position": [250, 195], "layer": "F.SilkS"}])
    layout = client.page_layout(board)
    flagged = {p["name"]: p for p in layout["problems"]}
    check(not layout["fits"] and flagged.get("STRAY", {}).get("outside_drawing_area"),
          "text past the border is flagged")
    check(flagged.get("TB", {}).get("overlaps_title_block"), "text in the title block is flagged")
    check(layout.get("board_outline_mm") is not None, f"board outline reported: {layout.get('board_outline_mm')}")
    client.delete(board, stray["items"])

    # Undo the last edit, redo it
    undone = client.undo(board)
    check(undone["performed"], "undo in the PCB editor")
    client.undo(board, redo=True)

    # See it
    framed = client.board_render(board, dpi=100, frame=True)
    bare = client.board_render(board, dpi=100, frame=False)
    (out / "pcb_framed.png").write_bytes(framed)
    (out / "pcb_bare.png").write_bytes(bare)
    check(framed[:4] == b"\x89PNG" and framed != bare, "framed board view differs from the bare one")
    try:
        (out / "pcb_3d.png").write_bytes(client.board_render_3d(board, width=800, height=600))
        print("ok   3D render")
    except RuntimeError as error:
        print(f"skip 3D render: {error}")

    # DRC and exports
    drc = client.board_drc(board)
    check("violation_count" in drc, f"DRC ran: {drc['violation_count']} violations {drc['by_type']}")
    with tempfile.TemporaryDirectory(prefix="kicad-live-gerbers-") as folder:
        client.board_export(board, "gerbers", folder + "/")
        files = list(Path(folder).glob("*"))
        check(files, f"gerbers written: {len(files)} files")
    check_fab_package(client, board, schematic)

    # Update PCB from schematic, dry run only
    update = client.board_update_from_schematic(board, schematic, dry_run=True)
    check("report" in update, f"update from schematic (dry run): {update['errors']} errors")

    # Clean up: everything added, both origins and the page
    for start in range(0, len(created), 200):
        client.delete(board, created[start:start + 200])
    client.board_origins(board, grid=origins0["grid_origin_mm"], drill=origins0["drill_origin_mm"])
    client.set_page(board, size=before_page["paper"], orientation=before_page["orientation"],
                    title=before_page["title_block"]["title"], date=before_page["title_block"]["date"],
                    revision=before_page["title_block"]["revision"],
                    company=before_page["title_block"]["company"],
                    comments=before_page["title_block"]["comments"])
    check(digest(board_file) == on_disk, "the board on disk is unchanged (nothing was saved)")
    print("PASSED e2e_pcb")


if __name__ == "__main__":
    main()
