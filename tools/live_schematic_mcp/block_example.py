# SPDX-License-Identifier: GPL-3.0-or-later
"""Synthetic block example. Default: offline plan. --live: guarded disposable probe.

The native acceptance owner runs the live probe serially in its own test app.
No real circuit, file save, editor launch or process/configuration change occurs.
"""

import argparse
import json
import os
import uuid

from block_helpers import Block, Manifest, StaleState, apply, fingerprint, plan
from client import LiveSchematic, as_dict, kiid, point, sch, types


def synthetic_resistor():
    """Illustrative passive only; live checks use the installed Device:R definition."""
    definition = sch.SchematicSymbol(
        id=types.LibraryIdentifier(library_nickname="Synthetic", entry_name="Resistor"),
        unit_count=1)
    definition.reference_field.text.text = "R"
    definition.value_field.text.text = "SYNTHETIC"
    for number, y, orientation in (("1", -3.81, sch.SPO_DOWN), ("2", 3.81, sch.SPO_UP)):
        pin = sch.SchematicPin(id=kiid(str(uuid.uuid5(uuid.NAMESPACE_OID, "example-pin-" + number))),
                               number=number, position=point(0, y), orientation=orientation,
                               electrical_type=types.EPT_PASSIVE, shape=sch.SPS_LINE, visible=True)
        definition.items.add().item.Pack(pin)
    return definition


def example_block(namespace, definition, revised=False):
    block = Block(namespace)
    block.frame("interface", "Synthetic signal path", (35, 35, 95, 85))
    block.symbol("source", definition, "R901", "1", (50, 50))
    block.symbol("load", definition, "R902", "1", (80, 50))
    start, end = block.pin("source", "2"), block.pin("load", "2" if revised else "1")
    # Explicit route corridors keep wires out of the symbol bodies.
    path = [start, (start[0], 65), (80, 65), end] if revised else [
        start, (start[0], 65), (70, 65), (70, 45), (end[0], 45), end]
    block.wire("signal", "SIGNAL", path)
    block.wire("branch", "SIGNAL", [(60, 65), (60, 75)])
    block.port("port", "SIGNAL", (60, 75), "passive")
    return block


def offline_example():
    namespace = "1201fe9f-5e62-4070-983b-3373edb15efc"
    doc = types.DocumentSpecifier(type=types.DOCTYPE_SCHEMATIC,
                                   project=types.ProjectSpecifier(name="synthetic", path="/synthetic/"),
                                   sheet_path=types.SheetPath(path=[kiid(namespace)]))
    block = example_block(namespace, synthetic_resistor())
    pending = plan(block, Manifest.empty(namespace, doc), doc, [])
    return {"mode": "offline; no editor contacted", "construction": block.report(), "plan": pending.report()}


def run_probe(client, doc):
    """Owner-only probe on an isolated synthetic sheet; preserves every preexisting item.

    A failed/unknown apply stops immediately; cleanup is not attempted through an
    uncertain transaction. Only exact probe-owned UUIDs are cleaned on success.
    """
    namespace = str(uuid.uuid4())
    original = fingerprint(client.items(doc).items)
    if any(s["reference"] in ("R901", "R902") for s in client.symbol_pins(doc, with_nets=False)):
        raise ValueError("Synthetic probe references already exist")
    definitions = client.library_symbols(["Device:R"])
    if len(definitions) != 1:
        raise RuntimeError("Installed Device:R library definition is required")
    note = sch.SchematicText(id=kiid(), text=types.Text(text="Manual synthetic note", position=point(100, 85)))
    note.text.attributes.size.CopyFrom(point(1.27, 1.27))
    client.mutate(doc, [note], description="Probe: unrelated manual note")
    baseline = {item.id.value: item for item in _read(client, doc)}
    first = example_block(namespace, definitions[0])
    state = apply(client, doc, plan(first, Manifest.empty(namespace, doc), doc, client.items(doc).items))
    after = {item.id.value: item for item in _read(client, doc)}
    assert all(after[key] == value for key, value in baseline.items()), "manual content changed"
    assert sum(isinstance(item, sch.Junction) for item in after.values()) == (
        sum(isinstance(item, sch.Junction) for item in baseline.values()) + 1)
    observed_pins = client.symbol_pins(doc)
    by_ref = {item["reference"]: {p["number"]: p for p in item["pins"]} for item in observed_pins}
    for key, ref in (("source", "R901"), ("load", "R902")):
        for number in ("1", "2"):
            assert point(*by_ref[ref][number]["position_mm"]) == point(*first.pin(key, number))
    assert by_ref["R901"]["2"]["net"] and by_ref["R901"]["2"]["net"] == by_ref["R902"]["1"]["net"]
    assert by_ref["R901"]["2"]["net"] != by_ref["R902"]["2"]["net"]
    repeated = plan(first, state, doc, client.items(doc).items)
    assert not repeated.creates and not repeated.updates and not repeated.deletes
    assert apply(client, doc, repeated) == state
    second = example_block(namespace, definitions[0], revised=True)
    revision = plan(second, state, doc, client.items(doc).items)
    assert all(key.startswith(("route/", "junction/")) for key in revision.updates + revision.deletes + revision.creates)
    state = apply(client, doc, revision)
    by_ref = {item["reference"]: {p["number"]: p for p in item["pins"]}
              for item in client.symbol_pins(doc)}
    assert by_ref["R901"]["2"]["net"] == by_ref["R902"]["2"]["net"]
    assert by_ref["R901"]["2"]["net"] != by_ref["R902"]["1"]["net"]
    revised_items = {item.id.value: item for item in _read(client, doc)}
    assert all(revised_items[key] == value for key, value in baseline.items())
    # Exercise stale precondition without sending a stale helper mutation.
    stale = plan(second, state, doc, client.items(doc).items)
    note.text.text += " edited"
    client.mutate(doc, [note], update=True, description="Probe: user edit after planning")
    try:
        apply(client, doc, stale)
    except StaleState:
        pass
    else:
        raise AssertionError("stale plan accepted")
    page = client.page_layout(doc)
    assert page["fits"], page
    # No uncertain operation has occurred. Reconcile only the generated UUIDs away,
    # then remove the one synthetic manual note owned by this probe.
    apply(client, doc, plan(Block(namespace), state, doc, client.items(doc).items))
    client.delete(doc, [note.id.value])
    assert fingerprint(client.items(doc).items) == original
    return {"passed": True, "namespace": namespace, "document": as_dict(doc),
            "checks": ["manual items preserved", "real pin endpoints", "one junction at T join",
                       "repeat is no-op", "revision changes intended net membership",
                       "stale fingerprint refused", "page fits", "exact cleanup; no save"],
            "revision": revision.report(), "independent_agent_evidence": False}


def _read(client, doc):
    from block_helpers import _unpack
    return [_unpack(item) for item in client.items(doc).items]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--live", action="store_true")
    args = parser.parse_args()
    if args.live:
        if os.environ.get("KICAD_BLOCK_HELPER_ISOLATED") != "1" or not os.environ.get("KICAD_API_SOCKET"):
            parser.error("--live requires KICAD_BLOCK_HELPER_ISOLATED=1 and an explicit KICAD_API_SOCKET")
        from e2e_guard import test_document
        client = LiveSchematic(timeout_ms=15000)
        result = run_probe(client, test_document(client))
    else:
        result = offline_example()
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
