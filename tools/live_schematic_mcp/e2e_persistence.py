# SPDX-License-Identifier: GPL-3.0-or-later
"""Explicit-save persistence fixture for a disposable, separately launched app.

Run `seed state.json`, stop only that app, launch it again with the same project,
then run `verify state.json`. Requires KICAD_PERSISTENCE_ISOLATED=1 and an explicit
KICAD_API_SOCKET. The seed stage requires an empty live-demo schematic and board.
It saves a two-sheet circuit and a real schematic-to-board transfer; the symbols' Value,
Footprint and MPN are edited by sending back the whole symbol. Verification checks saved
UUIDs, cross-sheet pin connectivity, board pads, library links and the absence of library
mismatch findings without saving.
"""

import argparse
from hashlib import sha256
import json
import os
from pathlib import Path

from google.protobuf import empty_pb2

from client import LiveSchematic, as_dict, kiid, point, project, sch, types
from e2e_guard import test_board, test_document


def child_document(client, root):
    nodes = client.hierarchy(root).top_level_sheets
    pending = list(nodes)
    while pending:
        node = pending.pop()
        if node.name == "Persistence":
            child = types.DocumentSpecifier()
            child.CopyFrom(root)
            child.sheet_path.CopyFrom(node.path)
            return child
        pending.extend(node.children)
    raise AssertionError("persistence child missing from hierarchy")


def pin_net(client, doc, reference, number):
    for symbol in client.symbol_pins(doc):
        if symbol["reference"] == reference:
            return next(pin["net"] for pin in symbol["pins"] if pin["number"] == number)
    raise AssertionError("persistence symbol missing")


def library_state(client, root, child):
    """Library links and mismatch findings of the persistence symbols, which full updates edited."""
    symbols = {s["reference"]: s for doc in (root, child) for s in client.symbol_pins(doc, with_nets=False)}
    ids = {symbols[ref]["id"] for ref in ("R101", "R102")}
    mismatches = [v["description"] for v in client.erc(root)["violations"]
                  if v.get("error_type") == "ERCET_LIB_SYMBOL_MISMATCH"
                  and any(item.get("value") in ids for item in v.get("items", []))]
    assert not mismatches, mismatches
    links = {ref: symbols[ref]["lib_id"] for ref in ("R101", "R102")}
    assert links == {"R101": "Device:R", "R102": "Device:R"}, links
    return links


def snapshot(client, root, board):
    child = child_document(client, root)
    root_net = pin_net(client, root, "R101", "2")
    child_net = pin_net(client, child, "R102", "1")
    assert root_net and root_net == child_net, (root_net, child_net)
    links = library_state(client, root, child)
    refs = client.board_footprints(board, ["R101", "R102"], with_pads=True)
    assert len(refs) == 2
    pads = {f["reference"]: {p["number"]: p.get("net") for p in f["pads"]} for f in refs}
    assert pads["R101"]["2"] == pads["R102"]["1"] == root_net, pads
    assert client.page_layout(root)["fits"]
    assert client.page_layout(child)["fits"]
    assert client.page_layout(board)["fits"]
    folder = Path(root.project.path)
    return {
        "root": as_dict(root), "child": as_dict(child), "board": as_dict(board),
        "root_items": sorted(get_id(item) for item in client.items(root).items),
        "child_items": sorted(get_id(item) for item in client.items(child).items),
        "net": root_net, "pads": pads, "library_links": links,
        "board_footprints": sorted((f["reference"], f["id"]) for f in refs),
        "files": {p.name: sha256(p.read_bytes()).hexdigest()
                  for pattern in ("*.kicad_sch", "*.kicad_pcb") for p in folder.glob(pattern)},
    }


def get_id(packed):
    from commands import message_class
    message = message_class(packed.TypeName())()
    assert packed.Unpack(message)
    return message.id.value


def seed(client, root, board):
    assert not client.items(root).items, "seed requires an empty disposable schematic"
    assert not client.board_footprints(board), "seed requires an empty disposable board"
    folder = Path(root.project.path)
    original = {p.name: sha256(p.read_bytes()).hexdigest() for p in folder.glob("*.kicad_sch")}
    sheet = sch.SheetSymbol(id=kiid(), position=point(100, 50), size=point(40, 30),
                            page_number="2", fields_autoplaced=True)
    sheet.name_field.text.text = "Persistence"
    sheet.name_field.visible = True
    sheet.filename_field.text.text = "persistence-child.kicad_sch"
    sheet.filename_field.visible = True
    sheet.pins.add(id=kiid(), position=point(100, 60), text=types.Text(text="LINK"),
                   side=sch.SHS_LEFT, shape=sch.SLSH_INPUT)
    client.mutate(root, [sheet])
    child = child_document(client, root)
    first = client.place_symbol(root, "Device:R", 50, 60, reference="R101")
    second = client.place_symbol(child, "Device:R", 60, 60, reference="R102")
    for doc, placed in ((root, first), (child, second)):
        for packed in client.items(doc).items:
            symbol = sch.SchematicSymbolInstance()
            if packed.Unpack(symbol) and symbol.id.value == placed["id"]:
                # The whole symbol as read, with only placement fields edited
                symbol.value_field.text.text = "4k7"
                symbol.footprint_field.text.text = "Resistor_SMD:R_0603_1608Metric"
                field = symbol.user_fields.add()
                field.CopyFrom(symbol.value_field)
                field.name, field.text.text, field.visible = "MPN", "RC0603FR-074K7L", False
                client.mutate(doc, [symbol], update=True)
                break
        else:
            raise AssertionError("placed symbol not returned")
    _, pin = client.find_pin(root, "R101", "2")
    x, y = pin["position_mm"]
    points = [(x, y), (x, 75), (90, 75), (90, 60), (100, 60)]
    client.mutate(root, [sch.SchematicLine(id=kiid(), start=point(*a), end=point(*b), type=sch.SLT_WIRE)
                         for a, b in zip(points, points[1:]) if a != b])
    client.connect_pin_to_net(child, "R102", "1", "LINK", label="hierarchical")
    assert original == {p.name: sha256(p.read_bytes()).hexdigest() for p in folder.glob("*.kicad_sch")}
    transfer = client.board_update_from_schematic(board, root)
    assert transfer["errors"] == 0 and transfer["new_footprints"] == 2, transfer
    client.board_move_footprints(board, [{"reference": "R101", "x_mm": 60, "y_mm": 60},
                                        {"reference": "R102", "x_mm": 80, "y_mm": 60}])
    client.board_outline(board, 90, 80, 40, 40, replace=True)
    client.send(project.SaveDocument(document=root), empty_pb2.Empty)
    client.board_save(board)
    assert (folder / "persistence-child.kicad_sch").is_file()
    assert original != {p.name: sha256(p.read_bytes()).hexdigest() for p in folder.glob("*.kicad_sch")}
    print("actual schematic-to-board transfer:", json.dumps(transfer), flush=True)
    return snapshot(client, root, board)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stage", choices=("seed", "verify"))
    parser.add_argument("state", type=Path)
    args = parser.parse_args()
    if os.environ.get("KICAD_PERSISTENCE_ISOLATED") != "1" or not os.environ.get("KICAD_API_SOCKET"):
        raise SystemExit("REFUSED: select an isolated disposable app and explicit KICAD_API_SOCKET")
    client = LiveSchematic(timeout_ms=10000)
    root, board = test_document(client), test_board(client)
    if args.stage == "seed":
        args.state.write_text(json.dumps(seed(client, root, board), indent=2, sort_keys=True))
        print("PASSED persistence seed: explicit schematic/board saves completed")
    else:
        expected = json.loads(args.state.read_text())
        observed = json.loads(json.dumps(snapshot(client, root, board)))
        assert observed == expected, {"expected": expected, "observed": observed}
        print("PASSED persistence verify: reopened hierarchy, UUIDs, nets, board pads and disk hashes match")


if __name__ == "__main__":
    main()
