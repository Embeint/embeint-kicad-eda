# SPDX-License-Identifier: GPL-3.0-or-later
"""End-to-end check of board design setup and obstacle-avoiding routing against a running KiCad.

Same setup as e2e_pcb (the live-demo project in the project manager).  Exercises the stackup
(layer count, dielectrics, thickness, refusing to drop an occupied layer), controlled-impedance
widths, built-in and custom design rules, net classes, keepouts (by points and around a footprint),
routing around another net's copper with and without vias, routing a whole net, the path check,
grouped DRC checks (silkscreen) and the RF net setup.  Never saves the board; puts the rules back.

    python e2e_design.py [output_dir]
"""

from hashlib import sha256
from pathlib import Path
import sys

from client import LiveSchematic
from board_client import bt
from e2e_guard import test_board

RESISTOR = "Resistor_SMD:R_0603_1608Metric"


def check(condition, message):
    if not condition:
        raise SystemExit("FAILED: " + message)
    print("ok  ", message)


def set_pad_nets(client, board, nets):
    """{"R911.2": "E2E_SIG"}: name pad nets on the scratch board (normally they come from the schematic)."""
    by_ref = {}
    for key, net in nets.items():
        ref, number = key.split(".")
        by_ref.setdefault(ref, {})[number] = net
    for ref, numbers in by_ref.items():
        fp = client.board_footprint(board, ref)
        for packed in fp.definition.items:
            pad = bt.Pad()
            if packed.Is(bt.Pad.DESCRIPTOR) and packed.Unpack(pad) and pad.number in numbers:
                pad.net.name = numbers[pad.number]
                packed.Pack(pad)
        client.mutate(board, [fp], update=True, description="e2e: pad nets")


def main():
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "live-design-output")
    out.mkdir(parents=True, exist_ok=True)
    client = LiveSchematic(timeout_ms=60000)
    board = test_board(client)
    board_file = Path(board.project.path) / board.board_filename
    on_disk = sha256(board_file.read_bytes()).hexdigest() if board_file.exists() else None
    created = []

    # Stackup: 4 layers, dielectrics, 1.6 mm, then refuse to drop an occupied layer, then back to 2
    stackup = client.board_set_stackup(board, copper_layers=4, soldermask_color="#1A4D1A", finish="ENIG",
                                       dielectrics=[{"type": "prepreg", "thickness": 0.2, "epsilon_r": 4.4},
                                                    {"type": "core", "thickness": 1.0, "epsilon_r": 4.6},
                                                    {"type": "prepreg", "thickness": 0.2, "epsilon_r": 4.4}],
                                       board_thickness=1.6, impedance_controlled=True)
    copper = [e["layer"] for e in stackup["layers"] if e["type"] == "copper"]
    check(copper == ["F.Cu", "In1.Cu", "In2.Cu", "B.Cu"], f"4-layer stackup: {copper}")
    check(abs(stackup["board_thickness_mm"] - 1.6) < 0.01, f"board thickness 1.6 mm: {stackup['board_thickness_mm']}")
    check(stackup["finish"] == "ENIG" and stackup["impedance_controlled"], "finish and impedance control set")
    first = next(e for e in stackup["layers"] if e["type"] == "dielectric")
    check(first["dielectric"] == "prepreg" and abs(first["sublayers"][0]["epsilon_r"] - 4.4) < 1e-6,
          f"top dielectric is 4.4 prepreg: {first}")
    ms = client.board_impedance_width(board, 50, "F.Cu")
    check(ms["reference_layer"] == "In1.Cu" and 0.2 < ms["width_mm"] < 0.6, f"50 ohm microstrip over In1: {ms['width_mm']} mm")
    cp = client.board_impedance_width(board, 50, "F.Cu", "cpwg", gap=0.15)
    check(cp["width_mm"] < ms["width_mm"] + 0.2, f"50 ohm CPWG with 0.15 gap: {cp['width_mm']} mm")
    inner = client.board_add_tracks(board, [[60, 45], [70, 45]], "In1.Cu", 0.2)
    try:
        client.board_set_stackup(board, copper_layers=2)
        check(False, "dropping In1.Cu with a track on it is refused")
    except RuntimeError as error:
        check("still have items" in str(error), "dropping a layer that has items is refused")
    client.delete(board, inner["tracks"])
    two = client.board_set_stackup(board, copper_layers=2)
    check(sum(1 for e in two["layers"] if e["type"] == "copper") == 2, "back to 2 layers")

    # Built-in rules, then put them back
    rules0 = client.board_rules(board)
    rules = client.board_rules(board, track_widths=[0.2, 0.3, 0.5], via_sizes=[[0.6, 0.3], [0.8, 0.4]],
                               severities={"silk_clearance": "error"}, min_clearance=0.15, silk_clearance=0.12)
    check(rules["min_clearance"] == 0.15 and rules["silk_clearance"] == 0.12, "minimum clearances set")
    check(rules["track_widths_mm"] == [0.2, 0.3, 0.5] and rules["via_sizes_mm"] == [[0.6, 0.3], [0.8, 0.4]],
          "predefined track widths and via sizes set")
    check(rules["severities"].get("silk_clearance") == "error", "silk clearance is an error")

    # Custom rules
    custom = client.board_custom_rules(board, "add", [{"name": "E2E RF clearance",
                                                       "condition": "A.NetClass == 'E2E_RF'",
                                                       "constraints": [{"type": "clearance", "min": 0.5}]}])
    check(any(r["name"] == "E2E RF clearance" for r in custom["rules"]) and custom["status"] == "valid",
          f"custom rule added: {custom['status']}")

    # Net classes: defaults and an RF class
    classes0 = client.board_netclasses(board)
    default0 = next(c for c in classes0["classes"] if c["name"] == "Default")
    classes = client.board_netclasses(board, classes=[{"name": "Default", "track_width": 0.25, "clearance": 0.2,
                                                       "via_diameter": 0.6, "via_drill": 0.3}])
    default = next(c for c in classes["classes"] if c["name"] == "Default")
    check(default.get("track_width_mm") == 0.25 and default.get("via_diameter_mm") == 0.6,
          f"default net class widths: {default}")

    # A board, two resistors on E2E_SIG and another net's track between them
    outline = client.board_outline(board, 60, 40, 50, 40, replace=False)
    created += outline["items"]
    for ref, x, y in (("R911", 60, 60), ("R912", 95, 60), ("R913", 95, 72)):
        created.append(client.board_place_footprint(board, RESISTOR, x, y, reference=ref)["id"])
    set_pad_nets(client, board, {"R911.1": "E2E_A", "R911.2": "E2E_SIG", "R912.1": "E2E_SIG", "R912.2": "E2E_B",
                                 "R913.1": "E2E_SIG", "R913.2": "E2E_B"})
    blocker = client.board_add_tracks(board, [[78, 43], [78, 77]], "F.Cu", 0.3, "E2E_BLOCK")
    created += blocker["tracks"]

    path = client.board_clear_path(board, "E2E_SIG", [[61, 60], [94, 60]], "F.Cu", 0.25)
    check(not path["clear"] and any(c["net"] == "E2E_BLOCK" for c in path["conflicts"]),
          "the straight path is reported as crossing E2E_BLOCK")
    check(client.board_clear_path(board, "E2E_SIG", [[61, 50], [70, 50]], "F.Cu", 0.25)["clear"],
          "an open path is clear")

    routed = client.board_route_avoiding(board, "R911", "2", "R912", "1", layers=["F.Cu"], vias=False)
    points = routed["segments"][0]["path_mm"]
    check(routed["drc_clean"] and len(points) > 2, f"routed around the blocker on F.Cu ({routed['length_mm']} mm)")
    check(all(abs(y - 60) > 1 for x, y in points if 77 < x < 79), "the route passes the blocker at its ends, not through")
    client.undo(board)
    via_route = client.board_route_avoiding(board, "R911", "2", "R912", "1")
    check(via_route["drc_clean"] and (via_route["vias"] or via_route["length_mm"] < routed["length_mm"] + 1),
          f"with vias allowed: {len(via_route['vias'])} via(s), {via_route['length_mm']} mm")
    client.undo(board)

    # Keepouts: a full-height band stops the route; one around a footprint has the right box
    band = client.board_add_keepout(board, [[84, 38], [88, 38], [88, 82], [84, 82]], ["F.Cu", "B.Cu"], "E2E band")
    try:
        client.board_route_avoiding(board, "R911", "2", "R912", "1", verify=False)
        check(False, "routing through a full-height keepout is refused")
    except Exception as error:      # NoRoute
        check("no path" in str(error) or "no legal" in str(error), "the router stays out of the keepout")
    client.delete(board, [band["rule_area"]])
    around = client.board_footprint_keepout(board, "R913", margin=0.5)
    created.append(around["rule_area"])
    x1, y1, x2, y2 = around["box_mm"]
    check(x1 < 95 < x2 and y1 < 72 < y2 and len(around["layers"]) == 2, f"keepout around R913: {around['box_mm']}")
    client.delete(board, [around["rule_area"]])
    created.remove(around["rule_area"])

    # Whole net: R911.2, R912.1 and R913.1
    net = client.board_route_net(board, "E2E_SIG")
    created += net["items"]
    check(net["drc_clean"] and len(net["legs"]) == 2, f"routed net E2E_SIG in {len(net['legs'])} legs")
    again = client.board_route_net(board, "E2E_SIG")
    check(again.get("already_connected"), "a routed net reports already connected")

    # Silkscreen check: text across a pad
    silk = client.board_add_graphics(board, [{"kind": "text", "text": "XXXX", "position": [60.8, 60],
                                              "layer": "F.SilkS", "size": 1.2}])
    report = client.board_check(board, ["silkscreen", "clearance"])
    check(report["counts"]["silkscreen"] >= 1, f"silk over a pad is reported: {report['counts']}")
    check(report["counts"]["clearance"] == 0, "no clearance problems from the routing")
    client.delete(board, silk["items"])

    # RF net set-up
    rf = client.board_rf_net(board, ["E2E_SIG"], netclass="E2E_RF", impedance=50, keep_others_away=0.5)
    check(rf["track_width_mm"] > 1 and rf["class"]["track_width_mm"] == rf["track_width_mm"],
          f"RF class width from the 2-layer stackup: {rf['track_width_mm']} mm")

    # Clean up
    for start in range(0, len(created), 200):
        client.delete(board, created[start:start + 200])
    client.board_custom_rules(board, "remove", names=[r["name"] for r in client.board_custom_rules(board)["rules"]
                                                      if r["name"].startswith(("E2E", "E2E_RF"))])
    client.board_rules(board, track_widths=rules0["track_widths_mm"], via_sizes=rules0["via_sizes_mm"],
                       severities={"silk_clearance": rules0["severities"].get("silk_clearance", "warning")},
                       min_clearance=rules0["min_clearance"], silk_clearance=rules0["silk_clearance"])
    client.board_netclasses(board, classes=[{"name": "Default",
                                             **{k.removesuffix("_mm"): v for k, v in default0.items()
                                                if k.endswith("_mm")}}])
    now = sha256(board_file.read_bytes()).hexdigest() if board_file.exists() else None
    check(now == on_disk, "the board on disk is unchanged (nothing was saved)")
    print("PASSED e2e_design")


if __name__ == "__main__":
    main()
