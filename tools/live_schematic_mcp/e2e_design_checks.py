# SPDX-License-Identifier: GPL-3.0-or-later
"""End-to-end check of whole-design verification against a running development KiCad.

1. Hierarchical netlist: a net that crosses a sheet pin lists the pins of both sheets, with sheet
   paths, whichever sheet is asked; scope="sheet" keeps the sheet-only reply.
2. Page overlaps: overlapping schematic text, a wire through text and overlapping silkscreen text
   are reported with KiCad's boxes; separated text, labels on their wires and text on another layer
   are not, and `fits` is unchanged.
3. kicad-cli: KiCad reports its launch environment, and kicad-cli runs ERC and a netlist export of
   a disposable saved schematic in it with no library-configuration warnings.

Edits stay unsaved in the test project (live-demo); the CLI works on a scratch copy under the output
directory.  Checks the project's files on disk are unchanged.

    python e2e_design_checks.py [output_dir]
"""

from hashlib import sha256
import json
from pathlib import Path
import random
import re
import sys
import uuid

import kicad_cli
from board_client import layer, make_text
from client import LiveSchematic, kiid, point, sch, types
from e2e_guard import test_board, test_document


def check(condition, message):
    if not condition:
        raise SystemExit("FAILED: " + message)
    print("ok  ", message)


def child_document(client, root, name):
    for spec, path in client.sheet_instances(root):
        if path.strip("/").split("/")[-1] == name:
            return spec
    raise SystemExit(f"FAILED: sheet {name} missing from the hierarchy")


def schematic_text(content, x, y, size=1.27):
    text = sch.SchematicText(id=kiid())
    text.text.text = content
    text.text.position.CopyFrom(point(x, y))
    text.text.attributes.size.CopyFrom(point(size, size))
    return text


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


def page_overlaps(client, root, out, ox, oy):
    x, y = 30 + ox, 150 + oy
    first, second = schematic_text("OVERLAP_FIRST", x, y), schematic_text("OVERLAP_SECOND", x + 1.27, y)
    apart = schematic_text("STANDS_APART", x, y + 15.24)
    crossed = schematic_text("CROSSED_TEXT", x + 50.8, y)
    through = sch.SchematicLine(id=kiid(), start=point(x + 50.8 + 5.08, y - 5.08),
                                end=point(x + 50.8 + 5.08, y + 5.08), type=sch.SLT_WIRE)
    # Labels sitting on two wires at the usual 2.54 mm pitch: neither stacked label overlaps the
    # other (their boxes do, by the line spacing) and neither is "on" its own wire
    stacked_wires, stacked_labels = [], []
    for row in range(2):
        wy = y + 25.4 + 2.54 * row
        stacked_wires.append(sch.SchematicLine(id=kiid(), start=point(x, wy), end=point(x + 30.48, wy),
                                               type=sch.SLT_WIRE))
        stacked_labels.append(sch.LocalLabel(id=kiid(), position=point(x + 5.08, wy), spin_style=sch.SLSS_RIGHT,
                                             text=types.Text(text=f"STACKED_{row}_{uuid.uuid4().hex[:4]}")))
    client.mutate(root, [first, second, apart, crossed, through, *stacked_wires, *stacked_labels])
    resistor = client.place_symbol(root, "Device:R", x + 25.4, y + 7.62,
                                   reference=f"R{random.randint(5000, 8999) * 2}")
    labelled = client.connect_pin_to_net(root, resistor["reference"], "1", f"NET_{uuid.uuid4().hex[:6]}")

    page = client.page_layout(root)
    (out / "page-schematic.json").write_text(json.dumps(page, indent=2))
    check(page["fits"] == (not page["problems"]), "fits still reflects only the frame problems")
    check(page["overlap_free"] is False, "overlap_free is false when text overlaps")
    pairs = [{o["first"]["id"], o["second"]["id"]} for o in page["overlaps"]]
    found = next((o for o in page["overlaps"]
                  if {o["first"]["id"], o["second"]["id"]} == {first.id.value, second.id.value}), None)
    check(found is not None, "the two overlapping texts are reported as a pair")
    a, b, overlap = found["first"]["box_mm"], found["second"]["box_mm"], found["overlap_mm"]
    check(all(max(a[0], b[0]) - 1e-3 <= v <= min(a[2], b[2]) + 1e-3 for v in (overlap[0], overlap[2]))
          and a[2] - a[0] > 10, f"the overlap lies inside both KiCad text boxes: {a} {b} {overlap}")
    check({crossed.id.value, through.id.value} in pairs, "a wire through text is reported")
    check(not any(apart.id.value in p for p in pairs), "separated text is not reported")
    check({labelled["wire"], labelled["label"]} not in pairs, "a label on the end of its wire is not reported")
    stacked = {i.id.value for i in stacked_wires + stacked_labels}
    check(not any(p <= stacked for p in pairs),
          f"labels along their wires at 2.54 mm pitch are not reported: {[p for p in pairs if p <= stacked]}")
    own = [o for o in page["overlaps"] if o["first"]["id"] == o["second"]["id"] == resistor["id"]]
    check(not own, f"a freshly placed resistor's fields and body do not overlap each other: {own}")


def board_overlaps(client, out, ox, oy):
    board = test_board(client)
    x, y = 20 + ox / 4, 20 + oy / 4
    silk_a = make_text("SILK_A", (x, y), layer("F.SilkS"), 1.0)
    silk_b = make_text("SILK_B", (x + 1, y), layer("F.SilkS"), 1.0)
    fab = make_text("FAB_C", (x, y), layer("F.Fab"), 1.0)
    client.board_create(board, [silk_a, silk_b, fab])
    page = client.page_layout(board)
    (out / "page-board.json").write_text(json.dumps(page, indent=2))
    pairs = {frozenset((o["first"]["id"], o["second"]["id"])): o for o in page["overlaps"]}
    pair = pairs.get(frozenset((silk_a.id.value, silk_b.id.value)))
    check(pair is not None and "Silk" in pair.get("layer", ""),
          f"overlapping silkscreen text is reported on its layer: {pair}")
    check(not any(fab.id.value in p for p in pairs), "text on another layer (F.Fab) is not paired with it")
    check(page["fits"] == (not page["problems"]), "board fits still reflects only the frame problems")


def extract_symbol(library_text, name):
    """The top-level (symbol "name" ...) block of a .kicad_sym file."""
    start = library_text.index(f'(symbol "{name}"')
    depth, quoted, escaped = 0, False, False
    for index in range(start, len(library_text)):
        char = library_text[index]
        if quoted:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                quoted = False
            continue
        if char == '"':
            quoted = True
        elif char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
            if depth == 0:
                return library_text[start:index + 1]
    raise ValueError(f"unterminated symbol {name}")


def disposable_schematic(folder, symbol_dir):
    """A saved one-resistor project that uses the stock Device library, for kicad-cli."""
    symbol = extract_symbol((Path(symbol_dir) / "Device.kicad_sym").read_text(), "R")
    symbol = symbol.replace('(symbol "R"', '(symbol "Device:R"', 1)
    root = str(uuid.uuid4())
    folder.mkdir(parents=True, exist_ok=True)
    (folder / "cli-check.kicad_pro").write_text('{"meta": {"filename": "cli-check.kicad_pro", "version": 3}}\n')
    schematic = folder / "cli-check.kicad_sch"
    schematic.write_text(f'''(kicad_sch (version 20250114) (generator "eeschema") (generator_version "10.0")
  (uuid "{root}") (paper "A4")
  (lib_symbols {symbol})
  (symbol (lib_id "Device:R") (at 50.8 50.8 0) (unit 1) (in_bom yes) (on_board yes) (dnp no)
    (uuid "{uuid.uuid4()}")
    (property "Reference" "R1" (at 53.34 49.53 0) (effects (font (size 1.27 1.27)) (justify left)))
    (property "Value" "10k" (at 53.34 52.07 0) (effects (font (size 1.27 1.27)) (justify left)))
    (property "Footprint" "" (at 50.8 50.8 0) (effects (font (size 1.27 1.27)) (hide yes)))
    (property "Datasheet" "" (at 50.8 50.8 0) (effects (font (size 1.27 1.27)) (hide yes)))
    (pin "1" (uuid "{uuid.uuid4()}")) (pin "2" (uuid "{uuid.uuid4()}"))
    (instances (project "cli-check" (path "/{root}" (reference "R1") (unit 1)))))
  (sheet_instances (path "/" (page "1")))
)
''')
    return schematic


def erc_types(report_path):
    report = json.loads(Path(report_path).read_text())
    return sorted(v["type"] for sheet in report.get("sheets", []) for v in sheet.get("violations", []))


def cli_environment(client, out):
    variables = client.kicad_environment()
    check(variables is not None and "KICAD_CONFIG_HOME" in variables,
          f"KiCad reports its launch environment: {sorted(variables or [])}")
    check(not any(name.startswith("KICAD_API_") or name == "KIPRJMOD" for name in variables),
          "no API credentials or per-project variables are reported")
    version = client.run_kicad_cli(["version"])
    check(version["returncode"] == 0 and version["environment"]["source"] == "kicad",
          f"kicad-cli runs in KiCad's environment: {version['stdout'].strip()}")

    symbol_dir = next((variables[k] for k in sorted(variables) if re.fullmatch(r"KICAD\d+_SYMBOL_DIR", k)), None)
    check(symbol_dir is not None, f"KiCad reports its stock symbol directory: {symbol_dir}")
    folder = out / f"cli-{uuid.uuid4().hex[:6]}"
    schematic = disposable_schematic(folder, symbol_dir)
    report = folder / "erc.json"
    erc = client.run_kicad_cli(["sch", "erc", "--format", "json", "--severity-all", "-o", str(report),
                                str(schematic)])
    (out / "cli-erc.json").write_text(json.dumps(erc, indent=2))
    check(erc["returncode"] == 0 and report.is_file(), f"CLI ERC ran on the saved copy: {erc['stderr'][-300:]}")
    found = erc_types(report)
    check("lib_symbol_issues" not in found, f"CLI ERC resolves the stock library (violations: {found})")
    netlist = folder / "cli-check.net"
    exported = client.run_kicad_cli(["sch", "export", "netlist", "-o", str(netlist), str(schematic)])
    check(exported["returncode"] == 0 and '(ref "R1")' in netlist.read_text(),
          "CLI netlist export of the saved copy lists R1")

    # Controls, for the record: the same CLI without the launcher's library path or configuration
    path, env, _ = client.kicad_cli_launch()
    controls = {}
    bare = kicad_cli.child_environment()
    bare.pop("LD_LIBRARY_PATH", None)
    try:
        controls["no_library_path"] = kicad_cli.run(path, ["version"], bare, timeout_s=60)["returncode"]
    except OSError as error:
        controls["no_library_path"] = str(error)
    empty = folder / "empty-config"
    empty.mkdir()
    other = {**env, "KICAD_CONFIG_HOME": str(empty)}
    other_report = folder / "erc-empty-config.json"
    result = kicad_cli.run(path, ["sch", "erc", "--format", "json", "--severity-all", "-o", str(other_report),
                                  str(schematic)], other, timeout_s=120)
    controls["empty_config"] = {"returncode": result["returncode"],
                                "violations": erc_types(other_report) if other_report.is_file() else None}
    (out / "cli-controls.json").write_text(json.dumps(controls, indent=2))
    print("info controls without the launch environment:", json.dumps(controls))


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
    page_overlaps(client, root, out, ox, oy)
    board_overlaps(client, out, ox, oy)
    cli_environment(client, out)

    unchanged = {p: sha256(p.read_bytes()).hexdigest() for g in designs for p in project_dir.glob(g)}
    check(unchanged == on_disk, "the test project's files on disk are unchanged")
    print("OK: hierarchical netlist, page overlaps and kicad-cli environment verified")


if __name__ == "__main__":
    main()
