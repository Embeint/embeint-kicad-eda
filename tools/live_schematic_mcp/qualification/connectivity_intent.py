# SPDX-License-Identifier: GPL-3.0-or-later
"""Independent electrical intent: compare complete REF.PIN partitions, never ERC totals.

Input observations retain individual pin identities, including hidden/stacked pins.
Net names are deliberately not used as the electrical oracle: a correctly named net
can still omit half of a flow-through protection device.
"""

from collections import Counter, defaultdict


def validate_connectivity(intent, observations):
    """Return actionable findings for a complete design, without discarding duplicates.

    ``intent`` contains named ``nets`` and singleton ``no_connect`` pins. Every
    physical pin must occur exactly once. Observations contain pin, net, id and
    sheet; optional stacked groups additionally require coincident positions.
    """
    groups = {name: set(pins) for name, pins in intent["nets"].items()}
    groups.update({"NC:" + pin: {pin} for pin in intent.get("no_connect", [])})
    declared = [pin for pins in intent["nets"].values() for pin in pins]
    declared += intent.get("no_connect", [])
    if any(count != 1 for count in Counter(declared).values()):
        raise ValueError("intent must assign every pin exactly once")
    expected = set(declared)
    findings = []
    by_pin = defaultdict(list)
    by_net = defaultdict(set)
    identities = defaultdict(list)
    for entry in observations:
        pin = entry["pin"]
        by_pin[pin].append(entry)
        identities[(entry["sheet"], entry["id"])].append(pin)
        # An absent net is a distinct disconnected pin, never a shared empty net.
        key = entry.get("net") or ("unconnected", pin)
        by_net[key].add(pin)
    for pin in sorted(expected - by_pin.keys()):
        findings.append({"kind": "missing_pin", "pin": pin})
    for pin in sorted(by_pin.keys() - expected):
        findings.append({"kind": "extra_pin", "pin": pin})
    for pin, entries in sorted(by_pin.items()):
        if len(entries) != 1:
            findings.append({"kind": "duplicate_pin", "pin": pin, "count": len(entries)})
    for (sheet, identity), pins in sorted(identities.items()):
        if not identity or len(pins) != 1:
            findings.append({"kind": "duplicate_identity", "sheet": sheet,
                             "id": identity, "pins": sorted(pins)})
    for name, members in sorted(groups.items()):
        observed_groups = [pins for pins in by_net.values() if pins & members]
        if len(observed_groups) > 1:
            findings.append({"kind": "split_net", "net": name,
                             "parts": sorted(sorted(pins & members) for pins in observed_groups)})
        extra = set().union(*observed_groups) - members if observed_groups else set()
        if extra:
            findings.append({"kind": "merged_net", "net": name, "extra_pins": sorted(extra)})
        if len(members) > 1:
            disconnected = [pin for pin in members if pin in by_pin
                            and any(not entry.get("net") for entry in by_pin[pin])]
            if disconnected:
                findings.append({"kind": "unconnected_pin", "net": name, "pins": sorted(disconnected)})
    for stack in intent.get("stacked", []):
        entries = [by_pin[pin][0] for pin in stack if len(by_pin[pin]) == 1]
        if len(entries) != len(stack) or len({(e["sheet"], tuple(e["position_mm"])) for e in entries}) != 1:
            findings.append({"kind": "stack_position", "pins": stack})
    for pin, pin_type in intent.get("pin_types", {}).items():
        for entry in by_pin[pin]:
            if entry.get("type") != pin_type:
                findings.append({"kind": "pin_type", "pin": pin,
                                 "expected": pin_type, "actual": entry.get("type")})
    return findings


def require_connectivity(intent, observations):
    findings = validate_connectivity(intent, observations)
    if findings:
        raise AssertionError({"connectivity_findings": findings})


def observe_pins(client, documents):
    """Read all native pins; do not build a dictionary that hides duplicate identities."""
    from client import sch

    observations = []
    for doc in documents:
        sheet = "/".join(part.value for part in doc.sheet_path.path)
        # GetSymbolPins displays unit suffixes (U2A/U2B). GetItems carries the
        # physical reference (U2); never guess by stripping arbitrary letters.
        references = {}
        power_ids = set()
        for packed in client.items(doc).items:
            instance = sch.SchematicSymbolInstance()
            if packed.Unpack(instance):
                references[instance.id.value] = instance.reference_field.text.text
                if instance.definition.type in (sch.SST_GLOBAL_POWER, sch.SST_LOCAL_POWER):
                    power_ids.add(instance.id.value)
        for symbol in client.symbol_pins(doc):
            if symbol["id"] in power_ids:
                continue
            reference = references[symbol["id"]]
            for pin in symbol["pins"]:
                observations.append({**pin, "pin": reference + "." + pin["number"],
                                     "sheet": sheet, "unit": symbol["unit"],
                                     "display_reference": symbol["reference"],
                                     "symbol_id": symbol["id"], "lib_id": symbol["lib_id"]})
    return sorted(observations, key=lambda entry: (entry["pin"], entry["sheet"], entry["unit"],
                                                    entry["symbol_id"], entry["id"]))


def validate_board_parity(observations, footprints):
    """Compare every schematic pin to every physical numbered pad and its native net.

    Multiple pads with the same number are legitimate only when all share the
    schematic pin's net. Unnumbered mechanical pads have no electrical identity,
    but a footprint whose reference has no schematic pin is still reported.
    Standalone use must not hide duplicate schematic pins, so they are reported
    rather than collapsed into one expected net.
    """
    nets = defaultdict(list)
    for entry in observations:
        nets[entry["pin"]].append(entry.get("net") or "")
    findings = [{"kind": "duplicate_schematic_pin", "pin": pin, "nets": sorted(values)}
                for pin, values in sorted(nets.items()) if len(values) > 1]
    expected = {pin: values[0] for pin, values in nets.items() if len(values) == 1}
    observed = defaultdict(list)
    refs = Counter(footprint["reference"] for footprint in footprints)
    findings += [{"kind": "duplicate_footprint", "reference": ref}
                 for ref, count in sorted(refs.items()) if count > 1]
    schematic_refs = {pin.split(".", 1)[0] for pin in nets}
    findings += [{"kind": "extra_footprint", "reference": ref}
                 for ref in sorted(refs.keys() - schematic_refs)]
    for footprint in footprints:
        for pad in footprint["pads"]:
            if pad["number"]:
                observed[footprint["reference"] + "." + pad["number"]].append(pad.get("net") or "")
    for pin in sorted(expected.keys() - observed.keys()):
        findings.append({"kind": "missing_pad", "pin": pin})
    for pin in sorted(observed.keys() - nets.keys()):
        findings.append({"kind": "extra_pad", "pin": pin})
    for pin in sorted(expected.keys() & observed.keys()):
        if any(net != expected[pin] for net in observed[pin]):
            findings.append({"kind": "pad_net", "pin": pin,
                             "expected": expected[pin], "actual": observed[pin]})
    return findings
