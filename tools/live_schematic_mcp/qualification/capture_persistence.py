# SPDX-License-Identifier: GPL-3.0-or-later
"""Capture-specific persistence normalization, retaining physical identities.

The shared persistence helper handles library graphic IDs and footprint roots.
Multi-unit symbols additionally expose inactive-unit library pins whose UUIDs are
not written in the instance's pin records. Active/common pins remain exact.
"""

import copy
import json


def capture_schematic_item(item):
    item = copy.deepcopy(item)
    if item.get("@type", "").endswith(".SchematicSymbolInstance"):
        active = item.get("unit", {}).get("unit", 1)
        children = item.get("definition", {}).get("items", [])
        for child in children:
            pin = child.get("item", {})
            unit = child.get("unit", {}).get("unit", 0)
            if pin.get("@type", "").endswith(".SchematicPin") and unit not in (0, active):
                pin.pop("id", None)
        # Library definition children are a collection, retaining unit metadata,
        # all content and duplicate counts; native save/load can reorder them.
        children.sort(key=lambda value: json.dumps(value, sort_keys=True))
    return item


def capture_board_items(items, root, filename):
    items = copy.deepcopy(items)
    identities = {item.get("id", {}).get("value") for item in items}
    assert root and root not in identities, "board root must not identify an actual board item"
    for item in items:
        if item.get("parent", {}).get("value") == root:
            item["parent"] = {"value": "board:" + filename}
    return sorted(items, key=lambda value: json.dumps(value, sort_keys=True))


def snapshot(client):
    from board_client import board_document
    from client import as_dict
    from qualification import run

    value = run.snapshot(client)
    for key, items in value["schematics"].items():
        value["schematics"][key] = sorted((capture_schematic_item(item) for item in items),
                                          key=lambda item: json.dumps(item, sort_keys=True))
    for board in client.boards():
        doc = board_document(board)
        footprints = list(client.board_items(doc, ["footprints"]))
        roots = {as_dict(item)["parent"]["value"] for item in footprints}
        assert len(roots) == 1, "capture footprints must share a single board root"
        value["boards"][doc.board_filename] = capture_board_items(
            value["boards"][doc.board_filename], next(iter(roots)), doc.board_filename)
    return value
