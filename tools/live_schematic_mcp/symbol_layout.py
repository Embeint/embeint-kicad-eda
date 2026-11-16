# SPDX-License-Identifier: GPL-3.0-or-later
"""Lay out a rectangular library symbol from a pin list, on KiCad's 100 mil (2.54 mm) grid.

Pure geometry (no KiCad imports), in millimeters with Y pointing down as the IPC API uses.
Pin orientation names the direction from the pin's connection point toward the body.
"""

import math

GRID = 2.54
PIN_LENGTH = 2.54
CHAR_WIDTH = 1.0     # approx. width of one character of 1.27 mm pin-name text

SIDES = ("left", "right", "top", "bottom")
ORIENTATION = {"left": "SPO_RIGHT", "right": "SPO_LEFT", "top": "SPO_DOWN", "bottom": "SPO_UP"}

ELECTRICAL_TYPES = {
    "input": "EPT_INPUT", "output": "EPT_OUTPUT", "bidirectional": "EPT_BIDIRECTIONAL",
    "tri_state": "EPT_TRISTATE", "passive": "EPT_PASSIVE", "free": "EPT_FREE",
    "unspecified": "EPT_UNSPECIFIED", "power_in": "EPT_POWER_INPUT", "power_out": "EPT_POWER_OUTPUT",
    "open_collector": "EPT_OPEN_COLLECTOR", "open_emitter": "EPT_OPEN_EMITTER",
    "no_connect": "EPT_NO_CONNECT",
}


def _round_up(value, step):
    return math.ceil(round(value / step, 6)) * step


def default_side(pin):
    """Power inputs on top, grounds at the bottom, outputs right, everything else left."""
    name = str(pin.get("name", "")).upper()
    kind = pin.get("type", "passive")
    if kind == "power_in":
        return "bottom" if name in {"GND", "VSS", "AGND", "DGND", "PGND", "GNDA", "GNDD", "EP", "PAD"} else "top"
    if kind in ("output", "power_out", "open_collector", "open_emitter", "tri_state"):
        return "right"
    return "left"


def layout_box(pins, pin_length=PIN_LENGTH):
    """Return {'body': (x1, y1, x2, y2), 'pins': [...]} for pins around a centred rectangle.

    Each pin is {"number", "name", "type", "side"}; side defaults by type (see default_side).
    Pins on each side keep their given order: top-to-bottom on the left/right, left-to-right on
    the top/bottom. Gaps can be inserted with {"gap": true}.
    """
    if not pins:
        raise ValueError("A symbol needs at least one pin")
    by_side = {side: [] for side in SIDES}
    numbers = set()
    for pin in pins:
        if pin.get("gap"):
            by_side[pin.get("side", "left")].append(None)
            continue
        number = str(pin.get("number", "")).strip()
        if not number:
            raise ValueError(f"Pin {pin} needs a number")
        if number in numbers:
            raise ValueError(f"Pin number {number} is used twice")
        numbers.add(number)
        kind = pin.get("type", "passive")
        if kind not in ELECTRICAL_TYPES:
            raise ValueError(f"Pin {number}: type must be one of {sorted(ELECTRICAL_TYPES)}")
        side = pin.get("side") or default_side(pin)
        if side not in SIDES:
            raise ValueError(f"Pin {number}: side must be one of {SIDES}")
        by_side[side].append({**pin, "number": number, "type": kind, "side": side})

    def label_width(side):
        names = [len(str(p.get("name", ""))) for p in by_side[side] if p]
        return max(names, default=0) * CHAR_WIDTH

    rows = max(len(by_side["left"]), len(by_side["right"]), 1)

    def centred(count):
        """Grid x positions for top/bottom pins, symmetric about the centre line."""
        if count % 2:
            return [(i - count // 2) * GRID for i in range(count)]
        half = count // 2
        return [(i - half) * GRID if i < half else (i - half + 1) * GRID for i in range(count)]

    top_x, bottom_x = centred(len(by_side["top"])), centred(len(by_side["bottom"]))
    column_extent = max((abs(x) for x in top_x + bottom_x), default=-GRID / 2) + GRID / 2

    # Side-pin names run inward from the edges and top/bottom names run vertically down the middle;
    # size the body so neither crosses the other.  Edges sit on the grid with the centre at the
    # origin, so every pin end lands on the grid.
    side_names = max(label_width("left"), label_width("right")) + GRID / 2
    width = max(2 * 5.08, label_width("left") + label_width("right") + 2 * GRID,
                2 * (column_extent + side_names))
    height = max(5.08, (rows + 1) * GRID, label_width("top") + label_width("bottom") + 2 * GRID)
    width = _round_up(width, 2 * GRID)
    height = _round_up(height, 2 * GRID)
    x1, y1, x2, y2 = -width / 2, -height / 2, width / 2, height / 2

    placed = []
    for index, pin in enumerate(by_side["left"]):
        if pin:
            placed.append({**pin, "position": (x1 - pin_length, y1 + GRID * (index + 1))})
    for index, pin in enumerate(by_side["right"]):
        if pin:
            placed.append({**pin, "position": (x2 + pin_length, y1 + GRID * (index + 1))})
    for x, pin in zip(top_x, by_side["top"]):
        if pin:
            placed.append({**pin, "position": (x, y1 - pin_length)})
    for x, pin in zip(bottom_x, by_side["bottom"]):
        if pin:
            placed.append({**pin, "position": (x, y2 + pin_length)})

    for pin in placed:
        pin["position"] = tuple(round(v, 4) for v in pin["position"])
        pin["orientation"] = ORIENTATION[pin["side"]]
        pin["electrical_type"] = ELECTRICAL_TYPES[pin["type"]]
        pin["length"] = pin_length
    return {"body": tuple(round(v, 4) for v in (x1, y1, x2, y2)), "pins": placed}
