# SPDX-License-Identifier: GPL-3.0-or-later
"""Disposable synthetic capture, not a customer circuit or production device model.

The six-pin ESD model has the USBLC6 flow-through topology (1/6 and 3/4).
Its passive pins intentionally require external wires: ERC cannot infer intent.
Every electrical pin type below is explicit; no ERC or DRC setting is changed.
"""

from client import commands, enums, kiid, lib_identifier, library, point as native_point, sch, types
from qualification.connectivity_intent import observe_pins, require_connectivity, validate_connectivity


INTENT = {
    "nets": {
        "VDD": ["P1.1", "R1.1", "U1.1", "U3.5", "J1.1"],
        "GND": ["P1.2", "R1.2", "U1.2", "U1.7", "U3.2", "J1.4", "J2.2"],
        "DM": ["U1.3", "U3.1", "U3.6", "J1.2"],
        "DP": ["U1.4", "U3.3", "U3.4", "J1.3"],
        "TX_INTERNAL": ["U1.5", "U2.1"],
        "TX": ["U2.2", "J2.1"],
    },
    "no_connect": ["U1.6", "U1.8", "U2.3", "U2.4"],
    "stacked": [["U1.2", "U1.7"]],
    "pin_types": {"U1.1": "EPT_POWER_INPUT", "U1.2": "EPT_POWER_INPUT",
                  "U1.7": "EPT_PASSIVE", "U1.3": "EPT_BIDIRECTIONAL",
                  "U1.4": "EPT_BIDIRECTIONAL", "U1.5": "EPT_OUTPUT",
                  "U1.6": "EPT_NO_CONNECT", "U1.8": "EPT_INPUT"},
}

HEADER2 = "Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical"
HEADER4 = "Connector_PinHeader_2.54mm:PinHeader_1x04_P2.54mm_Vertical"
# number, name, electrical type, x/y in library coordinates, angle, hidden
PARTS = {
    "Source": ("P", HEADER2, [[
        ("1", "VDD", "power_out", 10.16, 0, 180, False),
        ("2", "GND", "power_out", 10.16, -5.08, 180, False)]]),
    "Load": ("R", "Resistor_SMD:R_0603_1608Metric", [[
        ("1", "1", "passive", -10.16, 0, 0, False),
        ("2", "2", "passive", 10.16, 0, 180, False)]]),
    "MCU": ("U", "Package_SO:SOIC-8_3.9x4.9mm_P1.27mm", [[
        ("1", "VDD", "power_in", -10.16, 10.16, 0, False),
        ("2", "GND", "power_in", -10.16, -10.16, 0, False),
        ("3", "DM", "bidirectional", 10.16, 5.08, 180, False),
        ("4", "DP", "bidirectional", 10.16, 0, 180, False),
        ("5", "TX", "output", 10.16, -5.08, 180, False),
        ("6", "NC", "no_connect", -10.16, -5.08, 0, True),
        # A duplicate stacked pad is an explicit passive alias, as in KiCad
        # library stacking conventions. Pin 2 remains the power input.
        ("7", "GND", "passive", -10.16, -10.16, 0, True),
        ("8", "RESET", "input", -10.16, 0, 0, False)]]),
    "Dual": ("U", HEADER4, [
        [("1", "A", "input", -10.16, 0, 0, False),
         ("2", "Y", "output", 10.16, 0, 180, False)],
        [("3", "A", "input", -10.16, 0, 0, False),
         ("4", "Y", "output", 10.16, 0, 180, False)]]),
    "FlowESD6": ("U", "Package_TO_SOT_SMD:SOT-23-6", [[
        ("1", "IO1", "passive", -10.16, 5.08, 0, False),
        ("6", "IO1", "passive", 10.16, 5.08, 180, False),
        ("3", "IO2", "passive", -10.16, -5.08, 0, False),
        ("4", "IO2", "passive", 10.16, -5.08, 180, False),
        ("5", "VDD", "passive", 0, 10.16, 270, False),
        ("2", "GND", "passive", 0, -10.16, 90, False)]]),
    "Connector": ("J", HEADER4, [[
        ("1", "VDD", "passive", 10.16, 10.16, 180, False),
        ("2", "DM", "passive", 10.16, 5.08, 180, False),
        ("3", "DP", "passive", 10.16, -5.08, 180, False),
        ("4", "GND", "passive", 10.16, -10.16, 180, False)]]),
    "Sink": ("J", HEADER2, [[
        ("1", "RX", "input", -10.16, 0, 0, False),
        ("2", "GND", "passive", 10.16, 0, 180, False)]]),
}


def point(x, y):
    # All anchors share KiCad's default 50 mil connection grid. Offsets in the
    # library definitions are exact multiples of this grid as well.
    return native_point(round(x / 1.27) * 1.27, round(y / 1.27) * 1.27)


def library_text():
    """Create only the synthetic project library, before any native editor opens it."""
    symbols = []
    for name, (prefix, footprint, units) in PARTS.items():
        fields = []
        for field, value, y, hide in (("Reference", prefix, 15, False), ("Value", name, 12.7, False),
                                      ("Footprint", footprint, 0, True), ("Datasheet", "", 0, True),
                                      ("Manufacturer", "Synthetic acceptance fixture", 0, True),
                                      ("MPN", "FIXTURE-" + name, 0, True)):
            fields.append(f'(property "{field}" "{value}" (at 0 {y} 0) '
                          f'(effects (font (size 1.27 1.27)){" (hide yes)" if hide else ""}))')
        bodies = []
        for unit, pins in enumerate(units, 1):
            pin_text = []
            for number, pin_name, kind, x, y, angle, hidden in pins:
                pin_text.append(f'(pin {kind} line (at {x} {y} {angle}) (length 2.54) '
                                f'{"(hide yes)" if hidden else ""} '
                                f'(name "{pin_name}" (effects (font (size 1.27 1.27)))) '
                                f'(number "{number}" (effects (font (size 1.27 1.27)))))')
            extent = 12.7 if name == "MCU" else 7.62
            bodies.append(f'(symbol "{name}_{unit}_1" '
                          f'(rectangle (start -7.62 {extent}) (end 7.62 {-extent}) '
                          '(stroke (width 0.254) (type default)) (fill (type background))) '
                          + " ".join(pin_text) + ")")
        symbols.append(f'(symbol "{name}" (pin_names (offset 1.016)) '
                       '(in_bom yes) (on_board yes) ' + " ".join(fields + bodies) + ")")
    return '(kicad_symbol_lib (version 20241209) (generator "acceptance_fixture")\n' + "\n".join(symbols) + ")\n"


def prepare_library(project):
    (project / "Acceptance.kicad_sym").write_text(library_text())
    (project / "sym-lib-table").write_text(
        '(sym_lib_table (version 7) (lib (name "Acceptance") (type "KiCad") '
        '(uri "${KIPRJMOD}/Acceptance.kicad_sym") (options "") '
        '(descr "Disposable synthetic capture acceptance parts")))\n')


def wire(client, doc, points):
    assert all(a[0] == b[0] or a[1] == b[1] for a, b in zip(points, points[1:])), points
    items = [sch.SchematicLine(id=kiid(), start=point(*a), end=point(*b), type=sch.SLT_WIRE)
             for a, b in zip(points, points[1:]) if a != b]
    if items:
        client.mutate(doc, items)
    return [item.id.value for item in items]


def label(client, doc, name, at, hierarchical=False):
    kind = sch.HierarchicalLabel if hierarchical else sch.LocalLabel
    item = kind(id=kiid(), position=point(*at), text=types.Text(text=name),
                spin_style=sch.SLSS_RIGHT, fields_autoplaced=True)
    if hierarchical:
        item.shape = sch.SLSH_BIDI
    client.mutate(doc, [item])


def junction(client, doc, at):
    client.mutate(doc, [sch.Junction(id=kiid(), position=point(*at),
                                   diameter=types.Distance(value_nm=914400))])


def power(client, doc, name, at):
    pos = point(*at)
    return client.place_symbol(doc, "power:" + name, pos.x_nm / 1e6, pos.y_nm / 1e6)


def block(client, doc, name, bounds):
    x1, y1, x2, y2 = bounds
    edges = [(x1, y1), (x2, y1), (x2, y2), (x1, y2), (x1, y1)]
    items = [sch.SchematicLine(id=kiid(), start=point(*a), end=point(*b), type=sch.SLT_GRAPHIC)
             for a, b in zip(edges, edges[1:])]
    for item in items:
        item.stroke.width.value_nm = 254000
        item.stroke.style = enums.SLS_DASH
    caption = sch.SchematicText(id=kiid(), text=types.Text(text=name, position=point(x1 + 4, y1 + 5)))
    caption.text.attributes.horizontal_alignment = enums.HA_LEFT
    caption.text.attributes.multiline = True
    caption.text.attributes.size.CopyFrom(native_point(1.27, 1.27))
    items.append(caption)
    client.mutate(doc, items)


def symbol(client, doc, ref):
    for packed in client.items(doc).items:
        item = sch.SchematicSymbolInstance()
        if packed.Unpack(item) and item.reference_field.text.text == ref:
            return item
    raise AssertionError(ref)


def place(client, doc, name, ref, x, y):
    at = point(x, y)
    result = client.place_symbol(doc, "Acceptance:" + name, at.x_nm / 1e6, at.y_nm / 1e6, reference=ref)
    assert result["reference"] == ref
    return result


def documents(client, root):
    result = {"Root": root}

    def visit(nodes):
        for node in nodes:
            if node.name in ("MCU", "Interface"):
                doc = type(root)()
                doc.CopyFrom(root)
                doc.sheet_path.CopyFrom(node.path)
                result[node.name] = doc
            visit(node.children)
    visit(client.hierarchy(root).top_level_sheets)
    assert set(result) == {"Root", "MCU", "Interface"}
    return result


def build(client, root):
    assert not client.items(root).items, "capture requires an empty owned project"
    place(client, root, "Source", "P1", 40, 45)
    place(client, root, "Load", "R1", 80, 45)
    wire(client, root, [(50.16, 45), (60, 45), (69.84, 45)])
    junction(client, root, (60, 45))
    wire(client, root, [(60, 45), (60, 35)])
    power(client, root, "VDD", (60, 35))
    wire(client, root, [(50.16, 50.08), (55, 50.08), (55, 65), (90.16, 65), (90.16, 45)])
    power(client, root, "GND", (55, 65))
    block(client, root, "Power / synthetic source and load", (20, 20, 100, 100))
    for name, x in (("MCU", 120), ("Interface", 200)):
        item = sch.SheetSymbol(id=kiid(), position=point(x, 40), size=point(40, 65),
                               fields_autoplaced=True, page_number="2" if name == "MCU" else "3")
        item.name_field.text.text = name
        item.name_field.visible = True
        item.filename_field.text.text = name.lower() + ".kicad_sch"
        item.filename_field.visible = True
        for net, y in (("VDD", 50), ("GND", 60), ("DM", 70), ("DP", 80), ("TX", 90)):
            right = name == "MCU" and net in ("DM", "DP", "TX")
            item.pins.add(id=kiid(), position=point(x + 40 if right else x, y),
                          text=types.Text(text=net), side=sch.SHS_RIGHT if right else sch.SHS_LEFT,
                          shape=sch.SLSH_BIDI)
        client.mutate(root, [item])
        for net, y in (("VDD", 50), ("GND", 60)):
            wire(client, root, [(x - 10, y), (x, y)])
            power(client, root, net, (x - 10, y))
    parent_pins = {}
    for packed in client.items(root).items:
        sheet = sch.SheetSymbol()
        if packed.Unpack(sheet):
            parent_pins[sheet.name_field.text.text] = {
                pin.text.text: (pin.position.x_nm / 1e6, pin.position.y_nm / 1e6) for pin in sheet.pins}
    # Sheet pins are constrained to their native sheet edge. Use readback rather
    # than independently rounding a requested sheet width and its right edge.
    for net in ("DM", "DP", "TX"):
        wire(client, root, [parent_pins["MCU"][net], parent_pins["Interface"][net]])
    docs = documents(client, root)
    mcu, interface = docs["MCU"], docs["Interface"]
    place(client, mcu, "MCU", "U1", 65, 60)
    place(client, mcu, "Dual", "U2", 125, 65.08)
    # The native placement command already supports selecting a unit. Preserve
    # its library definition; the separate probe tests serialized UpdateItems.
    client.run_edit(mcu, commands.PlaceSymbolFromLibrary(
        header=types.ItemHeader(document=mcu), lib_id=lib_identifier("Acceptance:Dual"),
        position=point(125, 95.56), unit=sch.SchematicSymbolUnit(unit=2), reference="U2"),
        library.PlaceFromLibraryResponse, "Acceptance: place unit B")
    for net, path in (("VDD", [(30, 49.84), (54.84, 49.84)]),
                      ("GND", [(30, 70.16), (54.84, 70.16)]),
                      ("DM", [(75.16, 54.92), (90, 54.92), (90, 40), (160, 40)]),
                      ("DP", [(75.16, 60), (95, 60), (95, 45), (160, 45)]),
                      ("TX", [(135.16, 65.08), (160, 65.08)])):
        wire(client, mcu, path)
        label(client, mcu, net, path[0] if net in ("VDD", "GND") else path[-1], hierarchical=True)
    wire(client, mcu, [(75.16, 65.08), (114.84, 65.08)])
    power(client, mcu, "VDD", (54.84, 49.84))
    power(client, mcu, "GND", (54.84, 70.16))
    for ref, number in (("U1", "6"), ("U1", "8"), ("U2B", "3"), ("U2B", "4")):
        pin = client.find_pin(mcu, ref, number)[1]
        client.mutate(mcu, [sch.NoConnectMarker(id=kiid(), position=point(*pin["position_mm"]),
                                               size=types.Distance(value_nm=1219200))])
    block(client, mcu, "MCU / stacked ground / two-unit buffer", (20, 20, 190, 125))
    place(client, interface, "Connector", "J1", 50, 65)
    place(client, interface, "FlowESD6", "U3", 100, 65)
    place(client, interface, "Sink", "J2", 175, 110)
    bridges = []
    for left, right, y, detour, net in ((1, 6, 59.92, 45, "DM"), (3, 4, 70.08, 85, "DP")):
        wire(client, interface, [(60.16, y), (80, y), (89.84, y)])
        # External bridge is mandatory even though the ESD symbol looks flow-through.
        bridges += wire(client, interface, [(80, y), (80, detour), (120, detour), (120, y)])
        wire(client, interface, [(110.16, y), (120, y), (145, y)])
        junction(client, interface, (80, y))
        junction(client, interface, (120, y))
        label(client, interface, net, (145, y), hierarchical=True)
    wire(client, interface, [(60.16, 54.84), (100, 54.84), (100, 35), (145, 35)])
    junction(client, interface, (100, 54.84))
    label(client, interface, "VDD", (145, 35), hierarchical=True)
    power(client, interface, "VDD", (100, 35))
    wire(client, interface, [(60.16, 75.16), (100, 75.16), (100, 125), (145, 125), (185.16, 125), (185.16, 110)])
    junction(client, interface, (100, 75.16))
    label(client, interface, "GND", (145, 125), hierarchical=True)
    power(client, interface, "GND", (100, 125))
    wire(client, interface, [(145, 110), (164.84, 110)])
    label(client, interface, "TX", (145, 110), hierarchical=True)
    block(client, interface, "Interface / explicit ESD flow-through bridges", (20, 20, 215, 135))
    observations = observe_pins(client, docs.values())
    require_connectivity(INTENT, observations)
    return docs, bridges


def erc_summary(result):
    assert result["violation_count"] == len(result["violations"]), result
    # ignored_checks is the native default count of disabled check categories,
    # not the number of suppressed findings. Preserve it; never change rules.
    assert result["excluded"] == 0, result
    return result


def negative_flow_through(client, docs, bridges):
    """Reproduce the failure class: zero ERC still permits split protection pins."""
    doc = docs["Interface"]
    before = observe_pins(client, docs.values())
    client.delete(doc, bridges)
    try:
        observed = observe_pins(client, docs.values())
        findings = validate_connectivity(INTENT, observed)
        assert {item["net"] for item in findings if item["kind"] == "split_net"} == {"DM", "DP"}, findings
        erc = erc_summary(client.erc(docs["Root"]))
        assert erc["violation_count"] == 0, erc
        return {"status": "reproduced", "scope": "synthetic USBLC6 flow-through topology; not client design",
                "erc": erc, "connectivity_findings": findings, "observations": observed}
    finally:
        assert client.undo(doc).get("performed")
        assert observe_pins(client, docs.values()) == before, "negative probe undo changed native pins/nets"
