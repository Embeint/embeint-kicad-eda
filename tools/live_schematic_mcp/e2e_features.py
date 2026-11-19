# SPDX-License-Identifier: GPL-3.0-or-later
"""End-to-end check of the editing commands added on top of the base IPC API.

Run against a development KiCad with a schematic open. Exercises annotate, pin lookup, wiring helpers,
netlist, move-with-drag, undo/redo, update from library, autoplace, ERC, full-symbol updates without
library mismatches (including a De Morgan symbol created from its definition), batch rollback, export
and the generic execute path. Never saves; checks the files on disk are unchanged.

    python e2e_features.py [output_dir]
"""

from hashlib import sha256
from pathlib import Path
import random
import sys
import tempfile
import uuid

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


def symbol_items(client, doc, ids):
    found = {}
    for packed in client.items(doc).items:
        symbol = sch.SchematicSymbolInstance()
        if packed.Unpack(symbol) and symbol.id.value in ids:
            found[symbol.id.value] = symbol
    check(set(found) == set(ids), f"read back {len(ids)} symbols")
    return found


def library_mismatches(client, doc, ids):
    return [v for v in client.erc(doc)["violations"]
            if v.get("error_type") == "ERCET_LIB_SYMBOL_MISMATCH"
            and any(item.get("value") in ids for item in v.get("items", []))]


def check_full_symbol_updates(client, root, ox, oy):
    """Editing placement fields by sending back the whole symbol must leave its library copy alone.

    Two resistors share one embedded library symbol, which is the case where a differing copy got
    renamed (R_1) and the next update then replaced the symbol's library link with that name.
    """
    stock = {"Device:R": 0, "Device:C": 25, "Transistor_BJT:2N3904": 40, "74xx:74LS00": 60}
    placed = {client.place_symbol(root, lib_id, 30 + x + ox, 110 + oy)["id"]: lib_id for lib_id, x in stock.items()}
    placed[client.place_symbol(root, "Device:R", 30 + ox, 135 + oy)["id"]] = "Device:R"
    ids = set(placed)

    def links():
        return {s["id"]: s["lib_id"] for s in client.symbol_pins(root, list(ids), with_nets=False)}

    def pin_ids():
        return {s["id"]: sorted(p["id"] for p in s["pins"]) for s in client.symbol_pins(root, list(ids), with_nets=False)}

    check(not library_mismatches(client, root, ids), "freshly placed symbols match their libraries")
    before, pins = symbol_items(client, root, ids), pin_ids()

    client.mutate(root, list(before.values()), update=True)
    after = symbol_items(client, root, ids)
    check(all(after[i].definition == before[i].definition for i in ids),
          "sending back unchanged symbols kept every definition")
    check(pin_ids() == pins, "and kept every pin identity")
    check(not library_mismatches(client, root, ids), "unchanged full-symbol update: no library mismatch")

    for symbol in after.values():
        symbol.value_field.text.text = "MCP-VALUE"
        symbol.footprint_field.text.text = "Resistor_SMD:R_0603_1608Metric"
        field = symbol.user_fields.add()
        field.CopyFrom(symbol.value_field)
        field.name, field.text.text, field.visible = "MPN", "MCP-MPN", False
    client.mutate(root, list(after.values()), update=True)
    edited = symbol_items(client, root, ids)
    check(all(s.value_field.text.text == "MCP-VALUE" and any(f.name == "MPN" for f in s.user_fields)
              for s in edited.values()), "Value, Footprint and a user field were applied")
    check(not library_mismatches(client, root, ids), "edited instance fields: no library mismatch")
    check(links() == placed, f"library links unchanged: {links()}")

    # Rebuild the ordinary symbols from their definitions: reordered items are not the packed
    # definition, so the editor replaces the embedded symbol with one built from the request
    plain = {i for i in ids if placed[i] != "74xx:74LS00"}
    for symbol_id in plain:
        symbol = edited[symbol_id]
        items = [type(child).FromString(child.SerializeToString()) for child in symbol.definition.items]
        del symbol.definition.items[:]
        symbol.definition.items.extend(reversed(items))
    client.mutate(root, [edited[i] for i in plain], update=True)
    check(not library_mismatches(client, root, plain), "a definition rebuilt from the request matches its library")
    again = symbol_items(client, root, plain)
    for symbol in again.values():
        symbol.value_field.text.text = "MCP-AGAIN"
    client.mutate(root, list(again.values()), update=True)
    check(links() == placed and not library_mismatches(client, root, ids),
          "a further update keeps the library links and matches")

    client.delete(root, list(ids))


def body_style_names(definition):
    return [style.name for style in definition.body_style]


def pin_body_styles(definition):
    return {child.body_style.style for child in definition.items
            if child.item.TypeName().endswith(".SchematicPin")}


DE_MORGAN_FONT = "(effects (font (size 1.27 1.27)))"
DE_MORGAN_LIBRARY = """(kicad_symbol_lib (version 20251024) (generator "kicad_symbol_editor")
  (symbol "MCP_DEMORGAN" (body_styles demorgan) (exclude_from_sim no) (in_bom yes) (on_board yes)
    (property "Reference" "U" (at 0 6.35 0) {font})
    (property "Value" "MCP_DEMORGAN" (at 0 -6.35 0) {font})
    (property "Footprint" "" (at 0 0 0) (hide yes) {font})
    (property "Datasheet" "" (at 0 0 0) (hide yes) {font})
    (property "Description" "De Morgan test gate" (at 0 0 0) (hide yes) {font})
    (symbol "MCP_DEMORGAN_1_1"
      (rectangle (start -3.81 3.81) (end 3.81 -3.81) (stroke (width 0.254) (type default))
        (fill (type background)))
      (pin input line (at -7.62 0 0) (length 3.81) (name "A" {font}) (number "1" {font}))
      (pin output line (at 7.62 0 180) (length 3.81) (name "Y" {font}) (number "2" {font})))
    (symbol "MCP_DEMORGAN_1_2"
      (circle (center 0 0) (radius 3.81) (stroke (width 0.254) (type default)) (fill (type background)))
      (pin input inverted (at -7.62 0 0) (length 3.81) (name "A" {font}) (number "1" {font}))
      (pin output inverted (at 7.62 0 180) (length 3.81) (name "Y" {font}) (number "2" {font})))))
""".format(font=DE_MORGAN_FONT)


def check_de_morgan_definition(client, root, ox, oy):
    """A De Morgan symbol created from its full library definition matches its library.

    A definition names every body style, so the De Morgan pair arrives as "Standard" and "Alternate",
    which the editor must store as the De Morgan flag: stored as names, the embedded copy differs
    from its library and ERC reports a mismatch.

    The symbol is a minimal gate in a scratch library rather than a stock one: the API drops repeated
    consecutive polyline vertices when it reads a shape, and every stock De Morgan gate (74xx, 4xxx)
    draws its alternate body with some, so a copy rebuilt from its definition always differs from
    the library.  Only a full library definition has the pins of both body styles; a placed symbol's
    definition lists those of its placed body style, so it cannot rebuild the other one.  For a
    placed symbol the guarantee is that sending its definition back unchanged keeps the embedded
    copy, which the last step checks.
    """
    lib_dir = Path.home() / "kicad-live" / "libs"
    lib_dir.mkdir(parents=True, exist_ok=True)
    nickname = f"mcp_demorgan_{uuid.uuid4().hex[:6]}"
    path = lib_dir / f"{nickname}.kicad_sym"
    path.write_text(DE_MORGAN_LIBRARY)
    client.add_library(nickname, str(path), scope="global", description="MCP De Morgan test", create=False)
    try:
        check_de_morgan_symbol(client, root, f"{nickname}:MCP_DEMORGAN", point(60 + ox, 135 + oy))
    finally:
        client.remove_library(nickname, scope="global")
        path.unlink()


def check_de_morgan_symbol(client, root, lib_id, origin):
    found = client.library_symbols([lib_id])
    check(len(found) == 1, f"read the {lib_id} library definition")
    full = found[0]
    names, pins = body_style_names(full), pin_body_styles(full)
    check(names == ["Standard", "Alternate"] and {1, 2} <= pins,
          f"library definition has both De Morgan body styles and their pins: {names} {pins}")

    symbol = sch.SchematicSymbolInstance(id=kiid(), position=origin, definition=full, lib_id=full.id)
    symbol.transform.orientation = sch.SSO_0
    symbol.unit.unit = 1
    symbol.body_style.style = 1
    symbol.attributes.CopyFrom(full.attributes)
    for name in ("reference", "value", "footprint", "datasheet", "description"):
        field = getattr(symbol, name + "_field")
        field.CopyFrom(getattr(full, name + "_field"))
        field.text.position.x_nm += origin.x_nm
        field.text.position.y_nm += origin.y_nm
    symbol.reference_field.text.text = "U901"
    client.mutate(root, [symbol])
    symbol_id = symbol.id.value
    ids = {symbol_id}

    check(not library_mismatches(client, root, ids), "De Morgan symbol from its full definition: no library mismatch")
    placed = symbol_items(client, root, ids)[symbol_id]
    check(body_style_names(placed.definition) == ["Standard", "Alternate"] and placed.HasField("body_style"),
          f"read back with two (De Morgan) body styles: {body_style_names(placed.definition)}")
    check(client.symbol_pins(root, [symbol_id], with_nets=False)[0]["lib_id"] == lib_id,
          f"and linked to {lib_id}")

    placed.value_field.text.text = "MCP-DEMORGAN"
    client.mutate(root, [placed], update=True)
    again = symbol_items(client, root, ids)[symbol_id]
    check(again.value_field.text.text == "MCP-DEMORGAN" and again.definition == placed.definition,
          "sending its definition back unchanged kept it")
    check(not library_mismatches(client, root, ids), "and still no library mismatch")

    client.delete(root, [symbol_id])


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

    check_full_symbol_updates(client, root, ox, oy)
    check_de_morgan_definition(client, root, ox, oy)

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
