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


GROUND_NAMES = {"GND", "VSS", "AGND", "DGND", "PGND", "SGND", "GNDA", "GNDD", "EP", "PAD", "EPAD", "0V"}
CONTROL_NAMES = {"EN", "ENABLE", "SHDN", "SD", "ON", "OFF", "CE", "INH", "UVLO", "SS", "TR", "SYNC", "MODE",
                 "RUN", "STBY", "STANDBY"}
FEEDBACK_NAMES = {"FB", "ADJ", "SENSE", "SNS", "VSENSE", "VFB", "BYP", "BYPASS", "NR", "PG", "PGOOD", "FLAG"}


def _base_name(pin):
    """Upper-case name without inversion marks, so ~{SHDN} and /EN classify like SHDN and EN."""
    name = str(pin.get("name", "")).upper()
    for mark in ("~{", "}", "/", "~", "#", "!"):
        name = name.replace(mark, "")
    return name


def is_ground(pin):
    name = _base_name(pin)
    return pin.get("type", "passive") in ("power_in", "passive") and (name in GROUND_NAMES or name.startswith("GND"))


def default_side(pin):
    """Datasheet-style placement: supplies and control inputs left, outputs and feedback right,
    grounds at the bottom. LDOs come out as VIN/EN on the left and VOUT/ADJ on the right."""
    name = _base_name(pin)
    kind = pin.get("type", "passive")
    if is_ground(pin):
        return "bottom"
    if name in FEEDBACK_NAMES or kind in ("output", "power_out", "open_collector", "open_emitter", "tri_state"):
        return "right"
    return "left"


NO_CONNECT_NAMES = {"NC", "DNC", "N.C.", "N/C", "NIC"}


def is_no_connect(pin):
    return pin.get("type") == "no_connect" or _base_name(pin) in NO_CONNECT_NAMES


def group_rank(pin):
    """Supplies first on each side, then signals, then no-connects."""
    if pin.get("type") in ("power_in", "power_out") and not is_ground(pin):
        return 0
    if is_no_connect(pin):
        return 2
    return 1


def layout_box(pins, pin_length=PIN_LENGTH, arrange="auto", group_gap=2):
    """Return {'body': (x1, y1, x2, y2), 'pins': [...]} for pins around a centred rectangle.

    Each pin is {"number", "name", "type", "side"}; side defaults datasheet-style (see default_side).
    With arrange="auto", each left/right side lists supply pins first, then signals, then
    no-connects, leaving group_gap empty rows between groups.  Two rows put EN 7.62 mm below VIN
    and ADJ 7.62 mm below VOUT, a resistor's pin spacing, so a pull-up or feedback divider drops
    straight in.  Within a group the given order is kept.  arrange="as_given" keeps the exact
    order; {"gap": true, "side": ...} inserts an empty row either way.  NC pins (named NC/DNC or
    typed no_connect) become hidden no-connect pins inside the body unless given "hidden": false.
    """
    if arrange not in ("auto", "as_given"):
        raise ValueError("arrange must be auto or as_given")
    if not 0 <= group_gap <= 6:
        raise ValueError("group_gap must be between 0 and 6 rows")
    if not pins:
        raise ValueError("A symbol needs at least one pin")
    by_side = {side: [] for side in SIDES}
    hidden = []
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
        if is_no_connect({**pin, "type": kind}):
            # As stock symbols do: an NC pin is a no-connect pin, and hidden unless asked otherwise.
            # Hidden pins take no row, so they neither spread the layout nor leave a gap.
            kind = "no_connect"
            if pin.get("hidden", True):
                hidden.append({**pin, "number": number, "type": kind, "side": "hidden", "hidden": True})
                continue
        side = pin.get("side") or default_side(pin)
        if side not in SIDES:
            raise ValueError(f"Pin {number}: side must be one of {SIDES}")
        by_side[side].append({**pin, "number": number, "type": kind, "side": side})

    if arrange == "auto":
        for side in ("left", "right"):
            entries = by_side[side]
            if None in entries:  # explicit gaps: the caller is arranging this side
                continue
            ordered = sorted(entries, key=group_rank)
            arranged = []
            for pin in ordered:
                if arranged and group_rank(pin) != group_rank(arranged[-1]):
                    arranged.extend([None] * group_gap)
                arranged.append(pin)
            by_side[side] = arranged

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

    # Hidden pins sit inside the body in rows no visible side pin uses (the gaps between groups
    # first), just inside an edge and pointing inward, so their names, shown greyed in the symbol
    # editor, cannot land on another pin's label.  Right side first, then left.
    total_rows = max(1, round(height / GRID) - 1)
    slots = []
    for side, edge_x, orientation in (("right", x2 - GRID, "SPO_LEFT"), ("left", x1 + GRID, "SPO_RIGHT")):
        used = {index for index, entry in enumerate(by_side[side]) if entry}
        slots += [(edge_x, y1 + GRID * (row + 1), orientation) for row in range(total_rows) if row not in used]
    for index, pin in enumerate(hidden):
        if index < len(slots):
            x, y, orientation = slots[index]
        else:  # more hidden pins than free rows: stack them in further columns
            column, row = divmod(index - len(slots), total_rows)
            x, y, orientation = x2 - GRID * (column + 2), y1 + GRID * (row + 1), "SPO_LEFT"
        placed.append({**pin, "position": (x, y), "orientation": orientation, "length": 0})

    for pin in placed:
        pin["position"] = tuple(round(v, 4) for v in pin["position"])
        pin.setdefault("orientation", ORIENTATION.get(pin["side"]))
        pin["electrical_type"] = ELECTRICAL_TYPES[pin["type"]]
        pin.setdefault("length", pin_length)
    return {"body": tuple(round(v, 4) for v in (x1, y1, x2, y2)), "pins": placed}
