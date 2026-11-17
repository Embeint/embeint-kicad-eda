# SPDX-License-Identifier: GPL-3.0-or-later
"""End-to-end check of the editing commands added on top of the base IPC API.

Run against a development KiCad with a schematic open. Exercises annotate, pin lookup, wiring helpers,
netlist, move-with-drag, undo/redo, update from library, autoplace, ERC, batch rollback, export and
the generic execute path. Never saves; checks the files on disk are unchanged.

    python e2e_features.py [output_dir]
"""

from hashlib import sha256
from pathlib import Path
import random
import sys
import tempfile

from client import LiveSchematic, as_dict, document, kiid, point, sch, types
from e2e_guard import test_document


def wires(client, doc):
    result = {}
    for packed in client.items(doc).items:
        line = sch.SchematicLine()
        if packed.Unpack(line):
            result[line.id.value] = ((line.start.x_nm, line.start.y_nm), (line.end.x_nm, line.end.y_nm))
    return result


def check(condition, message):
    if not condition:
        raise SystemExit("FAILED: " + message)
    print("ok  ", message)


def main():
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "live-smoke-output")
    out.mkdir(parents=True, exist_ok=True)
    client = LiveSchematic()
    listing = client.documents()
    root = test_document(client)
    project_dir = Path(root.project.path)
    on_disk = {p: sha256(p.read_bytes()).hexdigest() for p in project_dir.glob("*.kicad_sch")}

    # Two resistors, deliberately given duplicate-style references, then renumbered
    # A fresh grid-aligned area per run, so leftovers from an earlier run never overlap
    ox, oy = 2.54 * random.randint(0, 60), 2.54 * random.randint(0, 40)
    r_a = client.place_symbol(root, "Device:R", 40 + ox, 40 + oy, reference="R?")
    r_b = client.place_symbol(root, "Device:R", 70 + ox, 40 + oy, reference="R?")
    annotated = client.annotate(root, scope="symbols", symbols=[r_a["id"], r_b["id"]])
    refs = {c["id"]: c["new"] for c in annotated["changes"]}
    check(len(refs) == 2 and all(r.startswith("R") and r[1:].isdigit() for r in refs.values()),
          f"annotate numbered both new resistors: {annotated['changes']}")
    ref_a, ref_b = refs[r_a["id"]], refs[r_b["id"]]

    pins = {s["reference"]: s for s in client.symbol_pins(root, [r_a["id"], r_b["id"]])}
    check(set(pins) == {ref_a, ref_b} and len(pins[ref_a]["pins"]) == 2,
          f"symbol pins found for {ref_a} and {ref_b}: {pins[ref_a]['pins']}")

    wired = client.connect_pins(root, ref_a, "2", ref_b, "2")
    labelled = client.connect_pin_to_net(root, ref_a, "1", "VIN_TEST")
    client.netlist(root)
    pin_net = {p["number"]: p["net"] for p in client.symbol_pins(root, [r_a["id"]])[0]["pins"]}
    check(pin_net["1"] == "VIN_TEST" or (pin_net["1"] or "").endswith("/VIN_TEST"),
          f"{ref_a}.1 is on the labelled net (got {pin_net['1']!r})")
    other = {p["number"]: p["net"] for p in client.symbol_pins(root, [r_b["id"]])[0]["pins"]}
    check(pin_net["2"] and pin_net["2"] == other["2"], f"{ref_a}.2 and {ref_b}.2 share net {pin_net['2']!r}")

    # Move the second resistor; the wire to it must follow
    before = wires(client, root)
    moved = client.transform(root, [r_b["id"]], dx=0, dy=20, drag_wires=True)
    after = wires(client, root)
    check(set(moved["dragged"]) & set(wired["wires"]), f"move dragged the connecting wire: {moved}")
    dragged = [after[w] for w in moved["dragged"] if w in after]
    new_pin = next(p for p in client.symbol_pins(root, [r_b["id"]], with_nets=False)[0]["pins"]
                   if p["number"] == "2")["position_mm"]
    pin_nm = (round(new_pin[0] * 1e6), round(new_pin[1] * 1e6))
    check(any(pin_nm in ends for ends in dragged), f"a dragged wire ends at {ref_b}.2's new position {new_pin}")
    check(dragged and all(s[0] == e[0] or s[1] == e[1] for s, e in dragged),
          f"dragged wires stayed horizontal/vertical: {dragged}")
    still = {p["number"]: p["net"] for p in client.symbol_pins(root, [r_b["id"]])[0]["pins"]}
    check(still["2"] == pin_net["2"], f"{ref_b}.2 is still connected after the move ({still['2']!r})")

    rotated = client.transform(root, [r_b["id"]], rotation=90)
    check(rotated["moved"] == 1, "rotate accepted")

    undone = client.undo(root)
    check(undone.get("performed"), f"undo performed: {undone}")
    undone = client.undo(root)
    check(wires(client, root) == before, "two undos restored the wires to before the move")
    redone = client.undo(root, redo=True)
    check(redone.get("performed") and wires(client, root) == after, "redo reapplied the move")

    updated = client.update_from_library(root, [r_b["id"]], new_lib_id="Device:R_Small")
    check(updated["symbols"][0]["updated"], f"changed {ref_b} to Device:R_Small: {updated}")
    check(client.symbol_pins(root, [r_b["id"]])[0]["lib_id"] == "Device:R_Small", "lib id now R_Small")
    reloaded = client.update_from_library(root, [r_a["id"]], reset_field_text=True)
    check(reloaded["symbols"][0]["updated"], "reloaded R from library")

    check(client.autoplace(root, [r_a["id"], r_b["id"]])["success"], "autoplaced fields")

    erc = client.erc(root)
    check("violation_count" in erc, f"ERC ran: {erc['violation_count']} violations")

    count = len(client.items(root).items)
    good = {"@type": "type.googleapis.com/kiapi.schematic.types.SchematicLine",
            "id": {"value": kiid().value}, "type": "SLT_WIRE",
            "start": {"x_nm": round((10 + ox) * 1e6), "y_nm": round((150 + oy) * 1e6)},
            "end": {"x_nm": round((20 + ox) * 1e6), "y_nm": round((150 + oy) * 1e6)}}
    missing = dict(good, id={"value": kiid().value})  # updating an item that does not exist is rejected
    error = None
    try:
        client.batch(root, [{"command": "CreateItems", "payload": {"header": {"document": as_dict(root)},
                                                                    "items": [good]}},
                            {"command": "UpdateItems", "payload": {"header": {"document": as_dict(root)},
                                                                    "items": [missing]}}])
    except RuntimeError as failure:
        error = str(failure)
    check(error and "rolled back" in error, f"batch with a rejected step failed: {error}")
    check(len(client.items(root).items) == count, "failed batch left no items behind")

    good = dict(good, id={"value": kiid().value})
    committed = client.batch(root, [{"command": "CreateItems",
                                     "payload": {"header": {"document": as_dict(root)}, "items": [good]}}])
    check(committed["committed"] and len(client.items(root).items) == count + 1, "good batch committed")

    listed = client.execute("GetOpenDocuments", {"type": "DOCTYPE_SCHEMATIC"})
    check(listed["result"].get("documents"), "generic execute ran GetOpenDocuments")

    with tempfile.TemporaryDirectory() as scratch:
        exported = client.export(root, "netlist", Path(scratch) / "live.net")
        produced = [Path(p) for p in exported["outputs"]] or [Path(scratch) / "live.net"]
        text = produced[0].read_text(errors="replace") if produced[0].exists() else ""
        check("VIN_TEST" in text, f"netlist export from memory contains VIN_TEST ({produced[0]})")

    (out / "features.png").write_bytes(client.render(root, dpi=100))
    print("rendered", out / "features.png")

    # Page layout: the drawing area, title block and items that break out of them
    page = client.page_layout(root)
    area, block = page["drawing_area_mm"], page["title_block_mm"]
    check(page["page_size_mm"][0] > page["page_size_mm"][1] and area and block
          and area[0] < block[0] < block[2] <= area[2] + 0.01 and block[3] <= area[3] + 0.01,
          f"page {page['paper']} {page['page_size_mm']}: drawing area {area}, title block {block}")
    original = {"size": page["paper"], "orientation": page["orientation"], **page["title_block"]}
    margin = (page["page_size_mm"][0] - 2, page["page_size_mm"][1] - 2)        # outside the border
    corner = ((block[0] + block[2]) / 2, (block[1] + block[3]) / 2)            # in the title block
    strays = [sch.SchematicText(id=kiid(), text=types.Text(text="OUTSIDE", position=point(*margin))),
              sch.SchematicText(id=kiid(), text=types.Text(text="IN TITLE", position=point(*corner)))]
    client.mutate(root, strays)
    flagged = {p["id"]: p for p in client.page_layout(root)["problems"]}
    check(flagged.get(strays[0].id.value, {}).get("outside_drawing_area")
          and flagged.get(strays[1].id.value, {}).get("overlaps_title_block"),
          "items outside the border and in the title block are reported")
    framed, bare = client.render(root, dpi=100, frame=True), client.render(root, dpi=100, frame=False)
    (out / "page_framed.png").write_bytes(framed)
    check(framed != bare, "view(frame=True) draws the drawing sheet; frame=False leaves it out")
    client.delete(root, [s.id.value for s in strays])

    a3 = client.set_page(root, size="A3", title="live-demo - Root", revision="T1", comments=["MCP page test"])
    check(a3["paper"] == "A3" and abs(a3["page_size_mm"][0] - 420) < 0.5
          and a3["title_block"]["title"] == "live-demo - Root" and a3["title_block"]["comments"] == ["MCP page test"],
          f"set the page to A3 {a3['page_size_mm']} and filled the title block")
    restored = client.set_page(root, size=original["size"], orientation=original["orientation"],
                               title=original["title"], revision=original["revision"],
                               comments=original["comments"])
    check(restored["paper"] == original["size"] and restored["title_block"]["title"] == original["title"],
          f"restored the page to {original['size']}")

    unchanged = {p: sha256(p.read_bytes()).hexdigest() for p in project_dir.glob("*.kicad_sch")}
    check(unchanged == on_disk, "nothing written to the schematic files")
    print("OK: all live editing features verified")


if __name__ == "__main__":
    main()
