# SPDX-License-Identifier: GPL-3.0-or-later
"""Electrical regressions that an ERC total or a net-name check cannot establish."""

import copy

import pytest

from qualification.connectivity_intent import validate_board_parity, validate_connectivity


@pytest.fixture
def circuit():
    intent = {"nets": {"DM": ["J1.2", "U3.1", "U3.6", "U1.3"],
                       "GND": ["U1.2", "U1.7"]},
              "no_connect": ["U1.8"], "stacked": [["U1.2", "U1.7"]],
              "pin_types": {"U1.7": "EPT_POWER_INPUT"}}
    observations = [{"pin": pin, "net": net, "id": pin, "sheet": "sheet",
                     "position_mm": [10, 10], "type": "EPT_POWER_INPUT"}
                    for net, pins in intent["nets"].items() for pin in pins]
    observations.append({"pin": "U1.8", "net": "unconnected-(U1-RESET-Pad8)",
                         "id": "U1.8", "sheet": "sheet", "position_mm": [15, 15]})
    return intent, observations


def kinds(intent, observations):
    return {item["kind"] for item in validate_connectivity(intent, observations)}


def test_pin_partition_accepts_native_names_and_singleton_no_connect(circuit):
    intent, observations = circuit
    assert validate_connectivity(intent, observations) == []
    for entry in observations:
        entry["net"] = "native/" + entry["net"]
    assert validate_connectivity(intent, observations) == []


def test_zero_erc_does_not_prove_flow_through_connectivity(circuit):
    intent, observations = circuit
    # Both ESD ends are wired to something, so a zero ERC count cannot detect
    # the missing external link. Keep the expected name on the connector half.
    for entry in observations:
        if entry["pin"] in ("U3.6", "U1.3"):
            entry["net"] = "Net-(U1-DM)"
    erc = {"violation_count": 0}
    assert erc["violation_count"] == 0
    findings = validate_connectivity(intent, observations)
    assert findings == [{"kind": "split_net", "net": "DM",
                         "parts": [["J1.2", "U3.1"], ["U1.3", "U3.6"]]}]


def test_missing_hidden_stacked_pin_is_not_collapsed_by_position(circuit):
    intent, observations = circuit
    observations[:] = [entry for entry in observations if entry["pin"] != "U1.7"]
    assert {"missing_pin", "stack_position"} <= kinds(intent, observations)


def test_merged_nets_and_connected_nc_are_detected(circuit):
    intent, observations = circuit
    for entry in observations:
        entry["net"] = "GND"
    findings = validate_connectivity(intent, observations)
    assert {item["net"] for item in findings if item["kind"] == "merged_net"} == {"DM", "GND", "NC:U1.8"}


def test_extra_pin_and_duplicate_multunit_identity_are_retained(circuit):
    intent, observations = circuit
    extra = dict(observations[0], pin="U2.9")
    observations.append(extra)
    assert {"extra_pin", "duplicate_identity", "merged_net"} <= kinds(intent, observations)
    observations.append(copy.deepcopy(observations[0]))
    assert "duplicate_pin" in kinds(intent, observations)


def test_unknown_net_does_not_join_all_unconnected_pins(circuit):
    intent, observations = circuit
    for entry in observations:
        entry["net"] = None
    assert {"split_net", "unconnected_pin"} <= kinds(intent, observations)
    assert "merged_net" not in kinds(intent, observations)


def test_stack_position_and_unspecified_electrical_type_are_checked(circuit):
    intent, observations = circuit
    hidden = next(entry for entry in observations if entry["pin"] == "U1.7")
    hidden["position_mm"] = [20, 10]
    hidden["type"] = "EPT_UNKNOWN"
    assert {"stack_position", "pin_type"} <= kinds(intent, observations)


def test_ambiguous_intent_is_rejected(circuit):
    intent, observations = circuit
    intent["no_connect"].append("U3.1")
    with pytest.raises(ValueError, match="exactly once"):
        validate_connectivity(intent, observations)


def test_board_parity_checks_all_pads_and_duplicates(circuit):
    _, observations = circuit
    by_ref = {}
    for entry in observations:
        ref, number = entry["pin"].split(".")
        by_ref.setdefault(ref, {"reference": ref, "pads": []})["pads"].append(
            {"number": number, "net": entry["net"]})
    footprints = list(by_ref.values())
    assert validate_board_parity(observations, footprints) == []
    by_ref["U3"]["pads"][1]["net"] = "separated-flow-through"
    by_ref["U1"]["pads"] = [pad for pad in by_ref["U1"]["pads"] if pad["number"] != "7"]
    by_ref["J1"]["pads"].append({"number": "99", "net": "DM"})
    footprints.append({"reference": "J1", "pads": []})
    assert {item["kind"] for item in validate_board_parity(observations, footprints)} == {
        "pad_net", "missing_pad", "extra_pad", "duplicate_footprint"}


def test_native_unit_display_suffix_does_not_change_physical_reference():
    from types import SimpleNamespace
    from google.protobuf.any_pb2 import Any
    from client import kiid, sch, types
    from qualification.connectivity_intent import observe_pins

    first, second, power = kiid(), kiid(), kiid()
    items = []
    for identity in (first, second):
        instance = sch.SchematicSymbolInstance(id=identity)
        instance.reference_field.text.text = "U2"
        packed = Any()
        packed.Pack(instance)
        items.append(packed)
    marker = sch.SchematicSymbolInstance(id=power)
    marker.reference_field.text.text = "#PWR01"
    marker.definition.type = sch.SST_GLOBAL_POWER
    packed = Any()
    packed.Pack(marker)
    items.append(packed)
    client = SimpleNamespace(
        items=lambda doc: SimpleNamespace(items=items),
        symbol_pins=lambda doc: [
            {"id": identity.value, "reference": "U2" + suffix, "lib_id": "Fixture:Dual", "unit": unit,
             "pins": [{"number": number, "id": "same-native-id", "net": "signal", "position_mm": [unit, 0]}]}
            for identity, suffix, unit, number in ((first, "A", 1, "1"), (second, "B", 2, "3"))] + [
                {"id": power.value, "reference": "#PWR01", "pins": [{"number": "1"}]}])
    doc = types.DocumentSpecifier(sheet_path=types.SheetPath(path=[kiid()]))
    observations = observe_pins(client, [doc])
    assert [entry["pin"] for entry in observations] == ["U2.1", "U2.3"]
    assert len(observations) == 2
    assert "duplicate_identity" in kinds({"nets": {"signal": ["U2.1", "U2.3"]}}, observations)


def test_native_identity_rejects_changed_editor_before_launch(tmp_path):
    import json
    from hashlib import sha256
    from qualification.capture_acceptance import native_identity

    install = tmp_path / "install"
    (install / "bin").mkdir(parents=True)
    names = ("kicad", "kicad-cli", "_eeschema.kiface", "_pcbnew.kiface")
    for name in names:
        (install / "bin" / name).write_bytes(name.encode())
    hashes = tmp_path / "retained.txt"
    hashes.write_text("\n".join(sha256(name.encode()).hexdigest() + "  /retained/bin/" + name for name in names))
    source = tmp_path / "source.json"
    source.write_text(json.dumps({"native_base_revision": "a" * 40, "native_patch_blobs": {}, "version_stamp_note": "synthetic"}))
    measured = native_identity(install, hashes, source)
    assert measured["source_revision"] == "a" * 40
    (install / "bin/_eeschema.kiface").write_bytes(b"different native build")
    with pytest.raises(ValueError, match="does not match"):
        native_identity(install, hashes, source)


def test_roundtrip_only_ignores_inactive_library_pin_ids_and_child_order():
    from qualification.capture_persistence import capture_schematic_item

    original = {"@type": "kiapi.SchematicSymbolInstance", "id": {"value": "placed"},
                "unit": {"unit": 1}, "definition": {"items": [
                    {"unit": {"unit": unit}, "item": {"@type": "kiapi.SchematicPin",
                     "id": {"value": identity}, "number": str(unit), "position": {"x_nm": "10"}}}
                    for unit, identity in ((0, "common"), (1, "physical"), (2, "inactive"))]}}
    reopened = copy.deepcopy(original)
    children = reopened["definition"]["items"]
    children[2]["item"]["id"]["value"] = "regenerated-inactive"
    children.reverse()
    assert capture_schematic_item(original) == capture_schematic_item(reopened)
    assert original["definition"]["items"][2]["item"]["id"]["value"] == "inactive"
    for index in (1, 2):  # active and common physical pin identities remain exact
        changed = copy.deepcopy(reopened)
        changed["definition"]["items"][index]["item"]["id"]["value"] = "changed-physical"
        assert capture_schematic_item(original) != capture_schematic_item(changed)
    changed = copy.deepcopy(reopened)
    changed["definition"]["items"][0]["item"]["position"]["x_nm"] = "11"
    assert capture_schematic_item(original) != capture_schematic_item(changed)
    changed = copy.deepcopy(reopened)
    changed["definition"]["items"][0]["unit"]["unit"] = 1
    assert capture_schematic_item(original) != capture_schematic_item(changed)
    children.append(copy.deepcopy(children[0]))
    assert capture_schematic_item(original) != capture_schematic_item(reopened)


def test_roundtrip_board_root_normalization_retains_shape_and_pad_parents():
    from qualification.capture_persistence import capture_board_items

    before = [{"id": {"value": "outline"}, "parent": {"value": "old-root"}, "x": 10},
              {"id": {"value": "pad"}, "parent": {"value": "footprint"}}]
    after = copy.deepcopy(before)
    after[0]["parent"]["value"] = "new-root"
    expected = capture_board_items(before, "old-root", "board.kicad_pcb")
    assert expected == capture_board_items(after, "new-root", "board.kicad_pcb")
    after[1]["parent"]["value"] = "different-footprint"
    assert expected != capture_board_items(after, "new-root", "board.kicad_pcb")
    after[1]["parent"]["value"] = "footprint"
    after[0]["x"] = 11
    assert expected != capture_board_items(after, "new-root", "board.kicad_pcb")
    with pytest.raises(AssertionError, match="actual board item"):
        capture_board_items(before, "outline", "board.kicad_pcb")
