# SPDX-License-Identifier: GPL-3.0-or-later
"""Controlled-impedance track widths from the board stackup (RF feeds, antennas, USB, ...).

Microstrip: Hammerstad and Jensen (1980) with their strip-thickness correction, accurate to about 1 %
for 0.1 <= w/h <= 20.  Grounded coplanar waveguide (CPWG): the conformal-mapping formula with a
ground plane below (Wen; Ghione), using complete elliptic integrals via the arithmetic-geometric mean.
Both ignore soldermask, which lowers a surface trace's impedance by roughly 1-3 ohm; ask the board
house to confirm a controlled-impedance stackup.  Units are millimetres.
"""

import math

ETA0 = 376.730313668


def _ellip_k(k):
    """Complete elliptic integral of the first kind K(k), modulus k."""
    if not 0 <= k < 1:
        raise ValueError("modulus out of range")
    a, b = 1.0, math.sqrt(1 - k * k)
    while abs(a - b) > 1e-15 * a:
        a, b = (a + b) / 2, math.sqrt(a * b)
    return math.pi / (2 * a)


def _k_ratio(k):
    """K(k) / K'(k), by Hilberg's approximation (within 8 ppm), which stays finite as k approaches 1."""
    k = min(max(k, 1e-12), 1 - 1e-15)
    if k <= 0.7:
        kp = math.sqrt(1 - k * k)
        return math.pi / math.log(2 * (1 + math.sqrt(kp)) / (1 - math.sqrt(kp)))
    return math.log(2 * (1 + math.sqrt(k)) / (1 - math.sqrt(k))) / math.pi


def _z01(u):
    f = 6 + (2 * math.pi - 6) * math.exp(-((30.666 / u) ** 0.7528))
    return ETA0 / (2 * math.pi) * math.log(f / u + math.sqrt(1 + (2 / u) ** 2))


def _eeff(u, er):
    a = 1 + math.log((u ** 4 + (u / 52) ** 2) / (u ** 4 + 0.432)) / 49 + math.log(1 + (u / 18.1) ** 3) / 18.7
    b = 0.564 * ((er - 0.9) / (er + 3)) ** 0.053
    return (er + 1) / 2 + (er - 1) / 2 * (1 + 10 / u) ** (-a * b)


def microstrip(width, height, er, thickness=0.035):
    """(Z0 ohm, effective permittivity) of a surface microstrip."""
    u = width / height
    if thickness > 0:
        t = thickness / height
        du1 = t / math.pi * math.log(1 + 4 * math.e / (t * (1 / math.tanh(math.sqrt(6.517 * u))) ** 2))
        dur = 0.5 * (1 + 1 / math.cosh(math.sqrt(er - 1))) * du1
        u1, ur = u + du1, u + dur
    else:
        u1 = ur = u
    eeff = _eeff(ur, er) * (_z01(u1) / _z01(ur)) ** 2
    return _z01(ur) / math.sqrt(_eeff(ur, er)), eeff


def cpwg(width, gap, height, er):
    """(Z0 ohm, effective permittivity) of a grounded coplanar waveguide (ground both sides and below)."""
    k = width / (width + 2 * gap)
    k1 = math.tanh(math.pi * width / (4 * height)) / math.tanh(math.pi * (width + 2 * gap) / (4 * height))
    r, r1 = _k_ratio(k), _k_ratio(k1)
    eeff = (1 + er * r1 / r) / (1 + r1 / r)
    return 60 * math.pi / math.sqrt(eeff) / (r + r1), eeff


def width_for(impedance, height, er, thickness=0.035, structure="microstrip", gap=None):
    """Track width giving `impedance` ohm (bisection; impedance falls as the track widens)."""
    if structure not in ("microstrip", "cpwg"):
        raise ValueError("structure must be microstrip or cpwg")
    if structure == "cpwg" and not gap:
        raise ValueError("cpwg needs the gap to the side grounds (gap_mm)")

    def z(w):
        return microstrip(w, height, er, thickness)[0] if structure == "microstrip" else cpwg(w, gap, height, er)[0]
    lo, hi = height * 0.01, height * 40
    if not z(hi) <= impedance <= z(lo):
        raise ValueError(f"{impedance} ohm is out of reach on this dielectric ({z(hi):.1f} to {z(lo):.1f} ohm)")
    for _ in range(100):
        mid = math.sqrt(lo * hi)
        if z(mid) > impedance:
            lo = mid
        else:
            hi = mid
    return (lo + hi) / 2


def dielectric_under(stackup_layers, signal_layer, reference_layer=None):
    """Height, epsilon_r (thickness-weighted) and copper thickness between a signal layer and its reference.

    stackup_layers: [{"type": "copper"|"dielectric"|..., "layer": "F.Cu", "thickness": mm,
    "sublayers": [{"thickness": mm, "epsilon_r": x}]}] top to bottom, as board_client reads them.
    The reference defaults to the nearest copper layer below (above, for the bottom layer).
    """
    copper = [i for i, layer in enumerate(stackup_layers) if layer["type"] == "copper"]
    names = [stackup_layers[i]["layer"] for i in copper]
    if signal_layer not in names:
        raise ValueError(f"{signal_layer} is not a copper layer of this stackup ({names})")
    s = copper[names.index(signal_layer)]
    if reference_layer is None:
        below = [i for i in copper if i > s]
        r = below[0] if below else max(i for i in copper if i < s)
    else:
        if reference_layer not in names:
            raise ValueError(f"{reference_layer} is not a copper layer of this stackup")
        r = copper[names.index(reference_layer)]
    lo, hi = sorted((s, r))
    height, weighted = 0.0, 0.0
    for layer in stackup_layers[lo + 1:hi]:
        if layer["type"] != "dielectric":
            continue
        for sub in layer.get("sublayers") or [{"thickness": layer["thickness"], "epsilon_r": layer.get("epsilon_r", 4.5)}]:
            height += sub["thickness"]
            weighted += sub["thickness"] * (sub.get("epsilon_r") or 4.5)
    if height <= 0:
        raise ValueError("no dielectric between the signal and reference layers")
    return {"height": height, "epsilon_r": weighted / height,
            "copper_thickness": stackup_layers[s].get("thickness") or 0.035,
            "reference_layer": stackup_layers[r]["layer"]}
