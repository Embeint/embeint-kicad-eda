# SPDX-License-Identifier: GPL-3.0-or-later
"""Generate KLC-style footprint geometry from datasheet package dimensions.

Pure geometry in millimeters (no KiCad imports), Y pointing down as the IPC API uses.  Follows the
KiCad Library Conventions used by the stock libraries:

- F.SilkS 0.12 mm lines kept >= 0.2 mm from copper, a pin-1 marker, reference above the courtyard
- F.Fab 0.10 mm body outline with a chamfered pin-1 corner, value below the courtyard and a
  ${REFERENCE} text in the middle
- F.CrtYd 0.05 mm rectangle 0.25 mm outside pads and body, on a 0.01 mm grid

Pad dimensions come from the datasheet's recommended land pattern when given (pad_length,
pad_width, row_pitch); otherwise they are derived from the lead dimensions with IPC-7351 nominal
toe/heel/side fillets.
"""

import math

SILK_WIDTH, FAB_WIDTH, COURTYARD_WIDTH = 0.12, 0.10, 0.05
SILK_PAD_CLEARANCE = 0.2
COURTYARD_GRID = 0.01
ROUNDRECT_RATIO = 0.25
MAX_CORNER_RADIUS = 0.25

# Nominal (density level B) solder fillets for gull-wing leads
TOE, HEEL, SIDE = 0.35, 0.35, 0.03
# No-lead (QFN/DFN) terminals: the pad extends past the body edge by the toe
NOLEAD_TOE, NOLEAD_HEEL, NOLEAD_SIDE = 0.3, 0.05, 0.0

# Stock-library land patterns (IPC-7351 nominal) for common chip sizes: (pad_w, pad_h, x, body_l, body_w)
CHIP_SIZES = {
    "0402": (0.54, 0.64, 0.51, 1.0, 0.5),
    "0603": (0.8, 0.95, 0.825, 1.6, 0.8),
    "0805": (1.025, 1.4, 0.9125, 2.0, 1.25),
    "1206": (1.125, 1.75, 1.4625, 3.2, 1.6),
    "1210": (1.125, 2.65, 1.4625, 3.2, 2.5),
    "2512": (1.525, 3.35, 3.1375, 6.3, 3.2),
}


def _grid_out(value, grid=COURTYARD_GRID):
    """Round away from zero to the grid (courtyards only ever grow)."""
    return math.copysign(math.ceil(round(abs(value) / grid, 6)) * grid, value)


def _r(value):
    return round(value, 4)


def smd_pad(number, x, y, w, h, shape="roundrect"):
    return {"number": str(number), "type": "smd", "shape": shape, "position": (_r(x), _r(y)),
            "size": (_r(w), _r(h))}


def _pad_box(pad):
    (x, y), (w, h) = pad["position"], pad["size"]
    return (x - w / 2, y - h / 2, x + w / 2, y + h / 2)


def _union(boxes):
    boxes = list(boxes)
    return (min(b[0] for b in boxes), min(b[1] for b in boxes), max(b[2] for b in boxes), max(b[3] for b in boxes))


def _clip_segment(start, end, keepouts):
    """Split an axis-aligned segment, dropping the parts inside any keep-out rectangle."""
    (x1, y1), (x2, y2) = start, end
    horizontal = abs(y1 - y2) < 1e-9
    lo, hi = (min(x1, x2), max(x1, x2)) if horizontal else (min(y1, y2), max(y1, y2))
    fixed = y1 if horizontal else x1
    cuts = []
    for kx1, ky1, kx2, ky2 in keepouts:
        if horizontal and ky1 < fixed < ky2:
            cuts.append((kx1, kx2))
        elif not horizontal and kx1 < fixed < kx2:
            cuts.append((ky1, ky2))
    pieces, cursor = [], lo
    for c1, c2 in sorted(cuts):
        if c1 > cursor:
            pieces.append((cursor, min(c1, hi)))
        cursor = max(cursor, c2)
        if cursor >= hi:
            break
    if cursor < hi:
        pieces.append((cursor, hi))
    out = []
    for a, b in pieces:
        if b - a >= 0.2:
            out.append(((a, fixed), (b, fixed)) if horizontal else ((fixed, a), (fixed, b)))
    return out


def _finish(name, pads, body, pin1_corner=True, courtyard_clearance=0.25, mounting="smd", model=None,
            silk_outline=None):
    """Add silkscreen, fabrication, courtyard and text around pads and a body rectangle."""
    bx1, by1, bx2, by2 = body
    pad_union = _union(_pad_box(p) for p in pads)
    cx1, cy1, cx2, cy2 = _union([pad_union, body])
    courtyard = tuple(_r(_grid_out(v)) for v in (cx1 - courtyard_clearance, cy1 - courtyard_clearance,
                                                    cx2 + courtyard_clearance, cy2 + courtyard_clearance))

    # Fabrication outline with the pin-1 corner chamfered (25 % of the short side, at most 1 mm)
    chamfer = min(1.0, 0.25 * min(bx2 - bx1, by2 - by1)) if pin1_corner else 0
    fab = [(bx1 + chamfer, by1), (bx2, by1), (bx2, by2), (bx1, by2), (bx1, by1 + chamfer)]
    fab_lines = [(fab[i], fab[(i + 1) % len(fab)]) for i in range(len(fab))]

    # Silkscreen just outside the body, cut back from copper
    offset = SILK_WIDTH / 2 + 0.01
    sx1, sy1, sx2, sy2 = silk_outline or (bx1 - offset, by1 - offset, bx2 + offset, by2 + offset)
    keepouts = [(x1 - SILK_PAD_CLEARANCE - SILK_WIDTH / 2, y1 - SILK_PAD_CLEARANCE - SILK_WIDTH / 2,
                 x2 + SILK_PAD_CLEARANCE + SILK_WIDTH / 2, y2 + SILK_PAD_CLEARANCE + SILK_WIDTH / 2)
                for x1, y1, x2, y2 in (_pad_box(p) for p in pads)]
    silk_lines = []
    for segment in (((sx1, sy1), (sx2, sy1)), ((sx2, sy1), (sx2, sy2)), ((sx2, sy2), (sx1, sy2)),
                    ((sx1, sy2), (sx1, sy1))):
        silk_lines += _clip_segment(*segment, keepouts)

    # Pin-1 marker: a small filled triangle pointing at pad 1 from outside it, clear of copper
    first = next(p for p in pads if p["number"] == "1")
    px1, py1, px2, py2 = _pad_box(first)
    size = 0.6
    if abs(first["position"][0] - (pad_union[0] + pad_union[2]) / 2) > abs(first["position"][1] -
                                                                           (pad_union[1] + pad_union[3]) / 2):
        # pad 1 on the left or right edge: point sideways from beyond its outer end
        tip_x = px1 - SILK_PAD_CLEARANCE if first["position"][0] < 0 else px2 + SILK_PAD_CLEARANCE
        back = -size if first["position"][0] < 0 else size
        y = first["position"][1]
        marker = [(tip_x, y), (tip_x + back, y - size / 2), (tip_x + back, y + size / 2)]
    else:
        tip_y = py1 - SILK_PAD_CLEARANCE
        x = first["position"][0]
        marker = [(x, tip_y), (x - size / 2, tip_y - size), (x + size / 2, tip_y - size)]
    marker = [(_r(x), _r(y)) for x, y in marker] if pin1_corner else None

    fab_text = _r(min(1.0, max(0.5, 0.25 * min(bx2 - bx1, by2 - by1))))
    return {
        "name": name,
        "mounting": mounting,
        "pads": pads,
        "body": tuple(_r(v) for v in body),
        "courtyard": courtyard,
        "fab_lines": [tuple((_r(x), _r(y)) for x, y in line) for line in fab_lines],
        "silk_lines": [tuple((_r(x), _r(y)) for x, y in line) for line in silk_lines],
        "pin1_marker": marker,
        "reference": {"position": (0.0, _r(courtyard[1] - 1.0)), "layer": "F.SilkS", "size": 1.0},
        "value": {"position": (0.0, _r(courtyard[3] + 1.0)), "layer": "F.Fab", "size": 1.0},
        "fab_reference": {"position": (_r((bx1 + bx2) / 2), _r((by1 + by2) / 2)), "size": fab_text},
        "model": model,
    }


def _dual_rows(left, right, pitch, row_center, pad_len, pad_w, numbering="ccw"):
    """Pads in two columns: left top-to-bottom, then right bottom-to-top (counter-clockwise)."""
    pads = []
    for side, count in (("left", left), ("right", right)):
        x = -row_center if side == "left" else row_center
        ys = [(i - (count - 1) / 2) * pitch for i in range(count)]
        if side == "right":
            ys = list(reversed(ys))
        for y in ys:
            pads.append((x, y))
    return [smd_pad(i + 1, x, y, pad_len, pad_w) for i, (x, y) in enumerate(pads)]


def gullwing(pins, pitch, body_width, body_length, lead_span=None, lead_length=0.6, lead_width=None,
             pad_length=None, pad_width=None, row_pitch=None, exposed_pad=None, name=None, model=None):
    """SOIC / SOP / SSOP / TSSOP / MSOP: two rows of gull-wing leads.

    body_width is across the rows (between lead rows), body_length along them; lead_span is lead tip
    to lead tip (datasheet E).  Give pad_length/pad_width/row_pitch from the datasheet land pattern
    to use it exactly.  exposed_pad=(w, h) adds a thermal pad numbered pins+1.
    """
    if pins % 2 or pins < 4:
        raise ValueError("gull-wing packages need an even pin count of at least 4")
    if pad_length is None or row_pitch is None:
        if lead_span is None:
            raise ValueError("give lead_span (tip to tip), or pad_length and row_pitch from the land pattern")
        pad_length = pad_length or lead_length + TOE + HEEL
        row_pitch = row_pitch or lead_span - lead_length + (TOE - HEEL)
    if pad_width is None:
        pad_width = min((lead_width or pitch * 0.5) + 2 * SIDE, pitch - 0.2)
    pads = _dual_rows(pins // 2, pins // 2, pitch, row_pitch / 2, pad_length, pad_width)
    if exposed_pad:
        pads.append(smd_pad(pins + 1, 0, 0, exposed_pad[0], exposed_pad[1], shape="rect"))
    body = (-body_width / 2, -body_length / 2, body_width / 2, body_length / 2)
    return _finish(name or f"SOIC-{pins}", pads, body, model=model)


def no_lead(pins_x, pins_y, pitch, body_x, body_y, terminal_length=0.4, terminal_width=None,
            pad_length=None, pad_width=None, exposed_pad=None, name=None, model=None):
    """QFN (pins on four sides) or DFN (pins_y only: two columns).

    pins_x counts pins on each of the top and bottom edges, pins_y on each of the left and right
    edges.  Numbering starts at the top of the left edge and runs counter-clockwise.
    exposed_pad=(w, h) adds a thermal pad numbered after the last terminal.
    """
    pad_length = pad_length or terminal_length + NOLEAD_TOE + NOLEAD_HEEL
    pad_width = pad_width or min((terminal_width or pitch * 0.5) + 2 * NOLEAD_SIDE, pitch - 0.2)
    outer_x, outer_y = body_x / 2 + NOLEAD_TOE, body_y / 2 + NOLEAD_TOE
    pads = []

    def span(count):
        return [(i - (count - 1) / 2) * pitch for i in range(count)]

    for y in span(pins_y):                       # left, top to bottom
        pads.append((-outer_x + pad_length / 2, y, pad_length, pad_width))
    for x in span(pins_x):                       # bottom, left to right
        pads.append((x, outer_y - pad_length / 2, pad_width, pad_length))
    for y in reversed(span(pins_y)):             # right, bottom to top
        pads.append((outer_x - pad_length / 2, y, pad_length, pad_width))
    for x in reversed(span(pins_x)):             # top, right to left
        pads.append((x, -outer_y + pad_length / 2, pad_width, pad_length))
    pads = [smd_pad(i + 1, x, y, w, h) for i, (x, y, w, h) in enumerate(pads)]
    if exposed_pad:
        pads.append(smd_pad(len(pads) + 1, 0, 0, exposed_pad[0], exposed_pad[1], shape="rect"))
    body = (-body_x / 2, -body_y / 2, body_x / 2, body_y / 2)
    total = 2 * (pins_x + pins_y)
    kind = "QFN" if pins_x else "DFN"
    return _finish(name or f"{kind}-{total}", pads, body, model=model)


def sot(variant, name=None, model=None):
    """SOT-23 (-3, -5, -6) and SOT-223 with stock-library land patterns."""
    variant = variant.upper().replace("SOT-", "").replace("SOT", "")
    if variant in ("23", "23-3"):
        pads = [smd_pad(1, -0.9375, -0.95, 1.475, 0.6), smd_pad(2, -0.9375, 0.95, 1.475, 0.6),
                smd_pad(3, 0.9375, 0, 1.475, 0.6)]
        body, default = (-0.65, -1.45, 0.65, 1.45), "SOT-23"
    elif variant in ("23-5", "23-6"):
        count = int(variant[-1])
        pads = _dual_rows(3, count - 3, 0.95, 1.1375, 1.325, 0.6)
        if count == 5:  # pins 4 and 5 sit at the outer positions on the right
            pads[3]["position"], pads[4]["position"] = (1.1375, 0.95), (1.1375, -0.95)
        body, default = (-0.8, -1.45, 0.8, 1.45), f"SOT-23-{count}"
    elif variant == "223":
        pads = [smd_pad(n, -3.15, y, 2.0, 1.5) for n, y in ((1, -2.3), (2, 0), (3, 2.3))]
        pads.append(smd_pad(4, 3.15, 0, 2.0, 3.8))
        body, default = (-1.75, -3.25, 1.75, 3.25), "SOT-223-3_TabPin2"
    else:
        raise ValueError("variant must be 23, 23-5, 23-6 or 223")
    return _finish(name or default, pads, body, model=model)


def chip(size, prefix="R", name=None, model=None):
    """Two-terminal chip parts (R, C, L, D) in imperial sizes 0402-2512."""
    if size not in CHIP_SIZES:
        raise ValueError(f"size must be one of {sorted(CHIP_SIZES)}")
    pad_w, pad_h, x, body_l, body_w = CHIP_SIZES[size]
    pads = [smd_pad(1, -x, 0, pad_w, pad_h), smd_pad(2, x, 0, pad_w, pad_h)]
    metric = {"0402": "1005", "0603": "1608", "0805": "2012", "1206": "3216", "1210": "3225", "2512": "6332"}[size]
    body = (-body_l / 2, -body_w / 2, body_l / 2, body_w / 2)
    return _finish(name or f"{prefix}_{size}_{metric}Metric", pads, body, pin1_corner=False, model=model)


def pin_header(pins_per_row, rows=1, pitch=2.54, drill=1.0, pad=1.7, name=None, model=None):
    """Through-hole pin header, pin 1 at the origin (square), odd pins in the first column."""
    if rows not in (1, 2):
        raise ValueError("rows must be 1 or 2")
    pads = []
    for index in range(pins_per_row * rows):
        row, column = divmod(index, rows)
        pads.append({"number": str(index + 1), "type": "pth", "shape": "rect" if index == 0 else "circle",
                     "position": (_r(column * pitch), _r(row * pitch)), "size": (pad, pad), "drill": drill})
    half = pitch / 2
    body = (-half, -half, (rows - 1) * pitch + half, (pins_per_row - 1) * pitch + half)
    default = f"PinHeader_{rows}x{pins_per_row:02d}_P{pitch:.2f}mm_Vertical"
    shape = _finish(name or default, pads, body, courtyard_clearance=0.5, mounting="tht", model=model)
    return shape


PACKAGES = {"gullwing": gullwing, "soic": gullwing, "tssop": gullwing, "msop": gullwing, "ssop": gullwing,
            "qfn": no_lead, "dfn": no_lead, "sot": sot, "chip": chip, "pin_header": pin_header}


def generate(package, **params):
    """generate("soic", pins=8, pitch=1.27, body_width=3.9, body_length=4.9, lead_span=6.0)"""
    key = package.lower()
    if key not in PACKAGES:
        raise ValueError(f"package must be one of {sorted(PACKAGES)}")
    if key == "dfn":
        params.setdefault("pins_x", 0)
    return PACKAGES[key](**params)
