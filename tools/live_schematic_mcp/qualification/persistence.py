# SPDX-License-Identifier: GPL-3.0-or-later
"""Compare persisted state while retaining item identities and document content."""

import copy
import json


def persisted_schematic_item(item):
    item = copy.deepcopy(item)
    # KiCad's symbol format does not persist library-definition graphic UUIDs.
    # Placed-item IDs, other child IDs, geometry and fields remain exact.
    for child in item.get("definition", {}).get("items", []):
        graphic = child.get("item", {})
        if graphic.get("@type") == "type.googleapis.com/kiapi.schematic.types.SchematicGraphicShape":
            graphic.pop("id", None)
    # The parser omits a library-name override identical to the definition ID.
    # Retain the effective definition ID and every distinct override exactly.
    if (item.get("@type") == "type.googleapis.com/kiapi.schematic.types.SchematicSymbolInstance"
            and item.get("lib_id") and item["lib_id"] == item.get("definition", {}).get("id")):
        item.pop("lib_id")
    return item


def canonical_items(value, persisted=False):
    return sorted(json.dumps(persisted_schematic_item(item) if persisted else item, sort_keys=True)
                  for item in value["items"])


def persisted_board_items(items, filename):
    items = copy.deepcopy(list(items))
    footprints = [item for item in items
                  if item.get("@type") == "type.googleapis.com/kiapi.board.types.FootprintInstance"]
    if footprints:
        # BOARD's root UUID is generated on load and is not a board-file item.
        # Assert a single root; retain footprint IDs and pad->footprint parents.
        roots = {item["parent"]["value"] for item in footprints}
        assert len(roots) == 1 and next(iter(roots)), "footprints do not share a board root"
        assert roots.isdisjoint(item.get("id", {}).get("value") for item in items), "board root points at an item"
        for item in footprints:
            item["parent"] = {"value": "board:" + filename}
    return sorted(items, key=lambda item: json.dumps(item, sort_keys=True))
