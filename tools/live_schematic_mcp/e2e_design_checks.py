# SPDX-License-Identifier: GPL-3.0-or-later
"""End-to-end check of whole-design verification against a running development KiCad.

1. Hierarchical netlist: a net that crosses a sheet pin lists the pins of both sheets, with sheet
   paths, whichever sheet is asked; scope="sheet" keeps the sheet-only reply.

Edits stay unsaved in the test project (live-demo).  Checks the project's files on disk are unchanged.

    python e2e_design_checks.py [output_dir]
"""

from hashlib import sha256
import json
from pathlib import Path
import random
import sys
import uuid

from client import LiveSchematic, kiid, point, sch, types
from e2e_guard import test_document


def check(condition, message):
    if not condition:
        raise SystemExit("FAILED: " + message)
    print("ok  ", message)


def child_document(client, root, name):
    for spec, path in client.sheet_instances(root):
        if path.strip("/").split("/")[-1] == name:
            return spec
    raise SystemExit(f"FAILED: sheet {name} missing from the hierarchy")


def hierarchy_netlist(client, root, out, ox, oy):
    tag = uuid.uuid4().hex[:6]
    name, net = f"Chk_{tag}", f"LINK_{tag}"
    sx, sy = 120 + ox, 40 + oy
    sheet = sch.SheetSymbol(id=kiid(), position=point(sx, sy), size=point(30, 20), fields_autoplaced=True)
    sheet.name_field.text.text = name
    sheet.name_field.visible = True
    sheet.filename_field.text.text = f"chk_{tag}.kicad_sch"
    sheet.filename_field.visible = True
    sheet.pins.add(id=kiid(), position=point(sx, sy + 10.16), text=types.Text(text=net),
                   side=sch.SHS_LEFT, shape=sch.SLSH_INPUT)
    client.mutate(root, [sheet])
    child = child_document(client, root, name)
    number = random.randint(5000, 8999) * 2
    parent = client.place_symbol(root, "Device:R", sx - 20.32, sy + 10.16, reference=f"R{number}")
    inner = client.place_symbol(child, "Device:R", 60, 60, reference=f"R{number + 1}")
    _, pin = client.find_pin(root, parent["reference"], "2")
    x, y = pin["position_mm"]
    points = [(x, y), (x, sy + 17.78), (sx - 5.08, sy + 17.78), (sx - 5.08, sy + 10.16), (sx, sy + 10.16)]
    client.mutate(root, [sch.SchematicLine(id=kiid(), start=point(*a), end=point(*b), type=sch.SLT_WIRE)
                         for a, b in zip(points, points[1:]) if a != b])
    client.connect_pin_to_net(child, inner["reference"], "1", net, label="hierarchical")

    outer_pin, inner_pin = f'{parent["reference"]}.2', f'{inner["reference"]}.1'
    whole = client.netlist_pins(root)
    (out / "netlist-hierarchy.json").write_text(json.dumps(whole, indent=2))
    crossing = [n for n, v in whole.items() if {outer_pin, inner_pin} <= set(v["pins"])]
    check(len(crossing) == 1, f"one net holds {outer_pin} (root) and {inner_pin} (child): {crossing}")
    paths = {s["path"]: s["pins"] for s in whole[crossing[0]]["sheets"]}
    check(outer_pin in paths.get("/", []) and inner_pin in paths.get(f"/{name}/", []),
          f"the net lists each pin under its sheet path: {paths}")
    check(client.netlist_pins(child) == whole, "asking from the child sheet gives the same whole-design netlist")
    sheet_only = client.netlist_pins(root, scope="sheet")
    check(inner_pin not in sheet_only[crossing[0]]["pins"] and outer_pin in sheet_only[crossing[0]]["pins"],
          "scope=sheet keeps the sheet-only reply (child pin absent)")


def main():
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "live-smoke-output") / "design-checks"
    out.mkdir(parents=True, exist_ok=True)
    client = LiveSchematic()
    root = test_document(client)
    project_dir = Path(root.project.path)
    designs = ("*.kicad_sch", "*.kicad_pcb")
    on_disk = {p: sha256(p.read_bytes()).hexdigest() for g in designs for p in project_dir.glob(g)}
    ox, oy = 2.54 * random.randint(0, 20), 2.54 * random.randint(0, 10)

    hierarchy_netlist(client, root, out, ox, oy)

    unchanged = {p: sha256(p.read_bytes()).hexdigest() for g in designs for p in project_dir.glob(g)}
    check(unchanged == on_disk, "the test project's files on disk are unchanged")
    print("OK: hierarchical netlist verified")


if __name__ == "__main__":
    main()
