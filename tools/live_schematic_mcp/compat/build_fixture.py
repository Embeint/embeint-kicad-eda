# SPDX-License-Identifier: GPL-3.0-or-later
"""Author the KiCad 10 compatibility fixture through the live IPC API of the development KiCad.

Run against the fork started on an EMPTY disposable project called compat-fixture (the guard refuses
anything else):

    run_kicad_wsl.sh /scratch/compat-fixture/compat-fixture.kicad_pro &
    KICAD_LIVE_TEST_PROJECT=compat-fixture python compat/build_fixture.py STEP_FILE REPORT.json

It builds a two-sheet hierarchy (root with a divider, a "Power" child sheet with a regulator), a
project symbol library and a project footprint library whose SOT-23 footprint embeds STEP_FILE, the
board (outline, net classes, custom rules, routing and a GND pour), fixed Embeint title blocks, and
then saves every document explicitly.  The report records connectivity, ERC and DRC.  The folder is
the source of the committed fixture in compat/fixture/compat-fixture.
"""

import json
import os
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from client import LiveSchematic, OUTWARD, as_dict, footprint_document, kiid, point, sch, symbol_document, types  # noqa: E402,E501
from e2e_guard import test_board, test_document  # noqa: E402

PROJECT = "compat-fixture"
DATE = "2026-11-19"  # fixed so plotted title blocks are reproducible
SYMBOL_LIB, FOOTPRINT_LIB = "compat", "compat_fp"
REGULATOR = f"{SYMBOL_LIB}:COMPAT_LDO"
SOT23 = f"{FOOTPRINT_LIB}:SOT-23_COMPAT"
DATASHEET = "https://example.com/compat-ldo.pdf"
R_FP, C_FP = "Resistor_SMD:R_0603_1608Metric", "Capacitor_SMD:C_0603_1608Metric"


def check(condition, message):
    if not condition:
        raise SystemExit("FAILED: " + message)
    print("ok  ", message, flush=True)


def child_spec(client, root, name):
    from e2e_smoke import find_child
    node = find_child(client.hierarchy(root).top_level_sheets, name)
    check(node is not None, f"sheet {name} is in the hierarchy")
    spec = type(root)()
    spec.CopyFrom(root)
    spec.sheet_path.CopyFrom(node.path)
    return spec


def set_field(client, spec, reference, footprint=None, value=None):
    for packed in client.items(spec).items:
        symbol = sch.SchematicSymbolInstance()
        if packed.Unpack(symbol) and symbol.reference_field.text.text == reference:
            if footprint is not None:
                symbol.footprint_field.text.text = footprint
            if value is not None:
                symbol.value_field.text.text = value
            client.mutate(spec, [symbol], update=True)
            return
    raise SystemExit(f"FAILED: no symbol {reference}")


def power_at_pin(client, spec, reference, pin, symbols, counter):
    """A 5.08 mm wire leaving the pin and power symbols (pins at their origin) on its end."""
    _, found = client.find_pin(spec, reference, pin)
    x, y = found["position_mm"]
    dx, dy = OUTWARD[sch.SchematicPinOrientation.Value(found["orientation"])]
    end = (round(x + dx * 5.08, 4), round(y + dy * 5.08, 4))
    client.mutate(spec, [sch.SchematicLine(id=kiid(), start=point(x, y), end=point(*end), type=sch.SLT_WIRE)])
    for lib_id in symbols:
        # GND hangs below its pin; +5V and PWR_FLAG stand above theirs.  Turn them away from the wire.
        hangs_down = lib_id == "power:GND"
        rotation = 180 if (hangs_down and dy < 0) or (not hangs_down and dy > 0) else 0
        prefix = "#FLG" if lib_id.endswith("PWR_FLAG") else "#PWR"
        counter[prefix] = counter.get(prefix, 0) + 1
        client.place_symbol(spec, lib_id, *end, reference=f"{prefix}{counter[prefix]:02d}", rotation=rotation)


def libraries(client, project_dir, step_file):
    """Project symbol and footprint libraries; the footprint embeds the STEP model."""
    client.add_library(SYMBOL_LIB, "${KIPRJMOD}/compat.kicad_sym", scope="project",
                       description="Compatibility fixture symbols")
    client.add_library(FOOTPRINT_LIB, "${KIPRJMOD}/compat.pretty", scope="project",
                       description="Compatibility fixture footprints", kind="footprint")
    check((project_dir / "compat.kicad_sym").is_file() and (project_dir / "compat.pretty").is_dir(),
          "project libraries created")
    fp = footprint_document(client.new_footprint(FOOTPRINT_LIB, "SOT-23_COMPAT", mounting="smd"))
    client.footprint_generate(fp, "sot", {"variant": "23-3", "name": "SOT-23_COMPAT"})
    client.footprint_set_properties(fp, description="SOT-23 compatibility fixture footprint",
                                    keywords="SOT-23 compat", datasheet=DATASHEET)
    attached = client.footprint_attach_step(fp, str(step_file))
    client.footprint_save(fp)
    saved = (project_dir / "compat.pretty/SOT-23_COMPAT.kicad_mod").read_text()
    check("embedded_files" in saved and "kicad-embed://" in saved, f"footprint embeds its model: {attached}")

    sym = symbol_document(client.new_symbol(SYMBOL_LIB, "COMPAT_LDO", reference_prefix="U",
                                            manufacturer="Embeint Fixture", mpn="COMPAT-LDO-33"))
    client.symbol_add_pins(sym, [
        {"number": "1", "name": "VIN", "type": "power_in"},
        {"number": "2", "name": "GND", "type": "power_in"},
        {"number": "3", "name": "VOUT", "type": "power_out"}])
    client.symbol_set_properties(sym, value="COMPAT_LDO", footprint=SOT23, datasheet=DATASHEET,
                                 description="Fixed 3.3 V regulator (compatibility fixture)",
                                 footprint_filters=["SOT?23*"])
    client.symbol_save(sym)
    check(client.library_symbol(REGULATOR)["footprint"] == SOT23, "symbol links the embedded-model footprint")


def schematic(client, root):
    counter = {}
    sheet = sch.SheetSymbol(id=kiid(), position=point(127, 60.96), size=point(30.48, 20.32),
                            page_number="2", fields_autoplaced=True)
    sheet.name_field.text.text = "Power"
    sheet.name_field.visible = True
    sheet.filename_field.text.text = "power.kicad_sch"
    sheet.filename_field.visible = True
    sheet.pins.add(id=kiid(), position=point(127, 71.12), text=types.Text(text="VOUT"),
                   side=sch.SHS_LEFT, shape=sch.SLSH_INPUT)
    client.mutate(root, [sheet])
    child = child_spec(client, root, "Power")

    # Power sheet: regulator with input and output capacitors, output to the root sheet
    client.place_symbol(child, REGULATOR, 101.6, 76.2, reference="U1")
    client.place_symbol(child, "Device:C", 76.2, 81.28, reference="C1")
    client.place_symbol(child, "Device:C", 127, 81.28, reference="C2")
    set_field(client, child, "C1", C_FP, "10u")
    set_field(client, child, "C2", C_FP, "10u")
    client.connect_pins(child, "U1", "1", "C1", "1")
    vout = client.connect_pins(child, "U1", "3", "C2", "1")["path_mm"]
    power_at_pin(client, child, "C1", "1", ["power:+5V", "power:PWR_FLAG"], counter)
    power_at_pin(client, child, "C1", "2", ["power:GND", "power:PWR_FLAG"], counter)
    power_at_pin(client, child, "C2", "2", ["power:GND"], counter)
    power_at_pin(client, child, "U1", "2", ["power:GND"], counter)
    # The hierarchical label sits on the regulator-to-capacitor wire (its corner, or its end)
    corner = vout[1] if len(vout) > 2 else vout[-1]
    client.mutate(child, [sch.HierarchicalLabel(id=kiid(), position=point(*corner), text=types.Text(text="VOUT"),
                                                shape=sch.SLSH_OUTPUT, fields_autoplaced=True)])

    # Root sheet: divider on the regulated output
    client.place_symbol(root, "Device:R", 88.9, 78.74, reference="R1")
    client.place_symbol(root, "Device:R", 88.9, 93.98, reference="R2")
    set_field(client, root, "R1", R_FP, "10k")
    set_field(client, root, "R2", R_FP, "4k7")
    _, r1 = client.find_pin(root, "R1", "1")
    x, y = r1["position_mm"]
    client.mutate(root, [
        sch.SchematicLine(id=kiid(), start=point(127, 71.12), end=point(x, 71.12), type=sch.SLT_WIRE),
        sch.SchematicLine(id=kiid(), start=point(x, 71.12), end=point(x, y), type=sch.SLT_WIRE)])
    tap = client.connect_pins(root, "R1", "2", "R2", "1")["path_mm"]
    middle = (tap[0][0], round((tap[0][1] + tap[-1][1]) / 2 / 1.27) * 1.27)
    client.mutate(root, [sch.LocalLabel(id=kiid(), position=point(*middle), text=types.Text(text="SENSE"))])
    power_at_pin(client, root, "R2", "2", ["power:GND"], counter)

    # Expected connectivity, across the hierarchy
    nets = {}
    for spec in (root, child):
        for symbol in client.symbol_pins(spec):
            for pin in symbol["pins"]:
                nets[(symbol["reference"], pin["number"])] = pin["net"]
    check(nets[("U1", "1")] == nets[("C1", "1")] and "+5V" in (nets[("U1", "1")] or ""), "VIN is +5V")
    check(len({nets[k] for k in (("U1", "2"), ("C1", "2"), ("C2", "2"), ("R2", "2"))}) == 1
          and "GND" in nets[("U1", "2")], "one GND net")
    check(nets[("U1", "3")] == nets[("C2", "1")] == nets[("R1", "1")], f"VOUT crosses the hierarchy: {nets[('R1', '1')]}")
    check(nets[("R1", "2")] == nets[("R2", "1")] and "SENSE" in nets[("R1", "2")], "divider tap is SENSE")
    return child, {f"{k[0]}.{k[1]}": v for k, v in sorted(nets.items())}


def board(client, root, board_doc):
    client.board_netclasses(board_doc, classes=[
        {"name": "Default", "track_width": 0.25, "clearance": 0.2, "via_diameter": 0.6, "via_drill": 0.3},
        {"name": "Power", "track_width": 0.4, "clearance": 0.2, "via_diameter": 0.8, "via_drill": 0.4}],
        patterns={"+5V": "Power", "*VOUT*": "Power"})
    client.board_rules(board_doc, min_clearance=0.15, silk_clearance=0.1)
    client.board_custom_rules(board_doc, "add", [
        {"name": "Power clearance", "condition": "A.NetClass == 'Power'",
         "constraints": [{"type": "clearance", "min": 0.25}]},
        {"name": "No vias under U1", "condition": "A.intersectsCourtyard('U1')",
         "constraints": [{"type": "disallow", "items": ["through_vias"]}], "severity": "warning"}])
    imported = client.board_update_from_schematic(board_doc, root)
    check(imported["errors"] == 0 and imported["new_footprints"] == 5, f"board update from schematic: {imported}")
    client.board_outline(board_doc, 32, 20, 100, 80)
    client.board_move_footprints(board_doc, [
        {"reference": "C1", "x_mm": 104, "y_mm": 86, "rotation": 180},  # pad 1 faces U1
        {"reference": "U1", "x_mm": 112, "y_mm": 86},
        {"reference": "C2", "x_mm": 120, "y_mm": 86},
        {"reference": "R1", "x_mm": 115, "y_mm": 94},
        {"reference": "R2", "x_mm": 124, "y_mm": 94}])
    routed = {}
    for net in client.board_nets(board_doc):
        name = net["name"]
        if "GND" in name or name.startswith("unconnected"):
            continue
        try:
            result = client.board_route_net(board_doc, name, layers=["F.Cu"], vias=False)
            routed[name] = {"drc_clean": result.get("drc_clean"), "legs": len(result.get("legs", []))}
        except Exception as error:  # noqa: BLE001 - record and continue; DRC reports what is left
            routed[name] = {"error": str(error)}
    # Every GND pad is on F.Cu; a B.Cu pour without stitching vias would only be isolated copper
    client.board_add_zone(board_doc, "GND", layers=("F.Cu",), inset=0.5, name="GND pour")
    return {"update": {k: imported[k] for k in ("errors", "warnings", "new_footprints")}, "routed": routed}


def main():
    if os.environ.get("KICAD_LIVE_TEST_PROJECT") != PROJECT:
        raise SystemExit(f"Set KICAD_LIVE_TEST_PROJECT={PROJECT} and open the empty disposable project")
    step_file, report_path = Path(sys.argv[1]), Path(sys.argv[2])
    client = LiveSchematic(timeout_ms=120000)
    root = test_document(client)
    project_dir = Path(root.project.path)
    check(project_dir.name == PROJECT and not (project_dir / "power.kicad_sch").exists(),
          f"empty disposable project {project_dir}")
    libraries(client, project_dir, step_file)
    child, nets = schematic(client, root)
    board_doc = test_board(client)
    layout = board(client, root, board_doc)
    for name, spec in (("Root", root), ("Power", child), ("PCB", board_doc)):
        client.set_page(spec, size="A4", title=f"{PROJECT} - {name}", date=DATE, revision="A",
                        company="Embeint Pty Ltd", comments=["KiCad 10 compatibility fixture"])
        check(client.page_layout(spec)["fits"], f"{name} fits its page")
    report = {"nets": nets, "board": layout, "erc": client.erc(root), "drc": client.board_drc(board_doc)}
    # Explicit saves: schematic (every sheet), then the board (which also writes the project settings)
    client.execute("SaveDocument", {"document": as_dict(root)})
    client.board_save(board_doc)
    report["saved"] = sorted(p.relative_to(project_dir).as_posix() for p in project_dir.rglob("*") if p.is_file())
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True, default=str) + "\n")
    print("OK: fixture authored and saved in", project_dir)


if __name__ == "__main__":
    main()
