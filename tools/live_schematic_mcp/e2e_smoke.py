# SPDX-License-Identifier: GPL-3.0-or-later
"""End-to-end check against a running development KiCad with a schematic open.

Builds a small hierarchy live (sheet + pin, child hierarchical label, symbol, wire), renders
both sheets from memory, and verifies nothing was written to disk. Never saves.

    python e2e_smoke.py [output_dir]
"""

from hashlib import sha256
from pathlib import Path
import sys
import uuid

from client import LiveSchematic, as_dict, document, point, sch, types


def kiid():
    return types.KIID(value=str(uuid.uuid4()))


def find_child(nodes, name):
    for node in nodes:
        if node.name == name:
            return node
        found = find_child(node.children, name)
        if found:
            return found
    return None


def main():
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "live-smoke-output")
    out.mkdir(parents=True, exist_ok=True)
    client = LiveSchematic()

    listing = client.documents()
    print("KiCad", listing["version"].get("full_version", listing["version"]))
    if not listing["documents"]:
        raise SystemExit("No schematic open in the development KiCad")
    root = document(listing["documents"][0])
    project_dir = Path(root.project.path)
    on_disk = {p: sha256(p.read_bytes()).hexdigest() for p in project_dir.glob("*.kicad_sch")}

    tag = uuid.uuid4().hex[:6]
    sheet_name, sheet_file = f"Power_{tag}", f"power_{tag}.kicad_sch"

    sheet = sch.SheetSymbol(id=kiid(), position=point(100, 50), size=point(40, 30), page_number="2",
                            fields_autoplaced=True)
    sheet.name_field.text.text = sheet_name
    sheet.name_field.visible = True
    sheet.filename_field.text.text = sheet_file
    sheet.filename_field.visible = True
    sheet.pins.add(id=kiid(), position=point(100, 60), text=types.Text(text="VIN"),
                   side=sch.SHS_LEFT, shape=sch.SLSH_INPUT)
    client.mutate(root, [sheet])
    print("created sheet", sheet_name)

    hierarchy = client.hierarchy(root)
    child_node = find_child(hierarchy.top_level_sheets, sheet_name)
    if child_node is None:
        raise SystemExit(f"new sheet missing from live hierarchy: {as_dict(hierarchy)}")
    child = types.DocumentSpecifier()
    child.CopyFrom(root)
    child.sheet_path.CopyFrom(child_node.path)
    print("child sheet path", child_node.path.path_human_readable)

    label = sch.HierarchicalLabel(id=kiid(), position=point(50, 50), text=types.Text(text="VIN"),
                                  shape=sch.SLSH_INPUT)
    client.mutate(child, [label])

    placed = client.place_symbol(root, "Device:R", 60, 60)
    print("placed symbol", placed)
    assert placed["reference"] and placed["position_mm"] == [60.0, 60.0]

    wire = sch.SchematicLine(id=kiid(), start=point(60, 80), end=point(100, 80), type=sch.SLT_WIRE)
    client.mutate(root, [wire])

    root_types = [i.TypeName().rsplit(".", 1)[-1] for i in client.items(root).items]
    child_types = [i.TypeName().rsplit(".", 1)[-1] for i in client.items(child).items]
    print("root items:", sorted(root_types))
    print("child items:", sorted(child_types))
    assert "SheetSymbol" in root_types and "SchematicLine" in root_types
    assert "HierarchicalLabel" in child_types

    for name, spec in (("root", root), ("child", child)):
        png = client.render(spec, dpi=100)
        assert png.startswith(b"\x89PNG"), f"{name} render is not a PNG"
        (out / f"{name}.png").write_bytes(png)
        print(f"rendered {name}: {len(png)} bytes -> {out / (name + '.png')}")

    unchanged = {p: sha256(p.read_bytes()).hexdigest() for p in project_dir.glob("*.kicad_sch")}
    assert unchanged == on_disk, "schematic files on disk changed"
    assert not (project_dir / sheet_file).exists(), "child sheet was written to disk"
    print("OK: live edits visible, renders from memory, nothing saved")


if __name__ == "__main__":
    main()
