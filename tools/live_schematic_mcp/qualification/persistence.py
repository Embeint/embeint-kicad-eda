# SPDX-License-Identifier: GPL-3.0-or-later
"""Compare persisted document state without transient library-graphic UUIDs."""

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
    return item


def canonical_items(value, persisted=False):
    return sorted(json.dumps(persisted_schematic_item(item) if persisted else item, sort_keys=True)
                  for item in value["items"])
