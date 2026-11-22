#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Reproduce public IEC60664 cases from the exact pinned C++ source, not Rust tables."""
import argparse
import hashlib
import itertools
import json
import math
from pathlib import Path
import random
import re
import struct
import subprocess
import sys

UNIT = "pcb_calculator.calculator_panels.application.pure.p001"
PIN = "4cf939778df04d2b747bb2c4714fc5014a7565c7"
SOURCE = "pcb_calculator/calculator_panels/iec60664.cpp"
ROOT = Path(__file__).resolve().parents[2]


def bits(value):
    return struct.unpack("<Q", struct.pack("<d", value))[0]


def setter(field, value):
    if field in {"altitude", "frequency", "rated_voltage", "rms_voltage", "peak_voltage", "transient_voltage"}:
        value = bits(value)
    return {"op": "set", "field": field, "value": value}


def generate(source):
    cases = []

    def case(name, *steps):
        assert 1 <= len(steps) <= 256
        cases.append({"unit": UNIT, "case": name, "input": {"steps": steps}})

    def body(name):
        start = source.index("IEC60664::" + name + "(")
        start = source.index("{", start)
        depth, end = 1, start + 1
        while depth:
            depth += (source[end] == "{") - (source[end] == "}")
            end += 1
        return source[start + 1:end - 1]

    def values(name, variable):
        thresholds = sorted({float(n) for n in re.findall(variable + r" (?:<=|>=|==|>|<) ([0-9.e]+)", body(name))})
        assert thresholds
        result = [(f"t{i}-{side}", v) for i, threshold in enumerate(thresholds)
                  for side, v in (("below", math.nextafter(threshold, -math.inf)),
                                  ("at", threshold), ("above", math.nextafter(threshold, math.inf)))]
        result += list(zip(("neg-max", "neg-one", "neg-min-normal", "neg-subnormal", "neg-zero", "zero", "subnormal", "min-normal", "max"),
                           (-sys.float_info.max, -1.0, -sys.float_info.min, -math.ulp(0.0), -0.0, 0.0, math.ulp(0.0), sys.float_info.min, sys.float_info.max)))
        return result

    case("default-state-and-repeat", {"op": "state"}, {"op": "categories"}, {"op": "impulse"}, {"op": "compute"}, {"op": "compute"})
    for pd in (1, 2, 3, 4, 0, -2147483648, 2147483647):
        case(f"groove-default-pd{pd}", {"op": "groove", "pd": pd})
        for label, v in values("GetMinGrooveWidth", "aDistIso"):
            case(f"groove-pd{pd}-{label}", {"op": "groove", "pd": pd, "distance": bits(v)})
        # abs(aDistIso) < 3 is not matched by the direct-variable threshold scanner.
        for side, v in (("below", math.nextafter(3.0, -math.inf)), ("at", 3.0), ("above", math.nextafter(3.0, math.inf))):
            case(f"groove-pd{pd}-three-{side}", {"op": "groove", "pd": pd, "distance": bits(v)})

    for op, name, variable in (("altitude", "GetClearanceAltitudeCorrectionFactor", "aAltitude"),
                              ("cti", "GetMaterialGroupFromCTI", "aCTI")):
        for label, v in values(name, variable):
            case(f"{op}-{label}", {"op": op, "voltage": bits(v)})

    for op, name, variable in (("rms", "GetClearanceForRMSVoltage", "aVRMS"),
                              ("peaks", "GetClearanceToWithstandPeaks", "aVoltage")):
        for field in (1, 2, 0, -2147483648, 2147483647):
            for label, v in values(name, variable):
                case(f"{op}-f{field}-{label}", {"op": op, "voltage": bits(v), "field": field})
        for label, v in values(name, variable):
            if op == "rms":
                case(f"rms-default-{label}", {"op": op, "voltage": bits(v)})

    for field, pd in itertools.product((1, 2, 0), (1, 2, 3, 4, 0)):
        for label, v in values("GetClearanceToWithstandTransientVoltage", "aVoltage"):
            case(f"transient-f{field}-pd{pd}-{label}", {"op": "transient", "field": field, "pd": pd, "voltage": bits(v)})

    for ov in (1, 2, 3, 4, 0, -2147483648, 2147483647):
        for label, v in values("GetRatedImpulseWithstandVoltage", "voltage"):
            case(f"impulse-ov{ov}-{label}", setter("overvoltage_category", ov), setter("rated_voltage", v), {"op": "impulse"})

    for pcb, pd, mg in itertools.product((False, True), (1, 2, 3, 4, 0), (1, 2, 3, 4, 5, 0)):
        for label, v in values("GetBasicCreepageDistance", "aVoltage"):
            case(f"creepage-pcb{int(pcb)}-pd{pd}-mg{mg}-{label}", setter("pcb_material", pcb),
                 {"op": "basic_creepage", "voltage": bits(v), "pd": pd, "mg": mg})

    # All named enum combinations through real successful calculations and their
    # continuations; OV and waveform do not enter Compute at this pin.
    base = [setter("altitude", 2000.0), setter("rms_voltage", 230.0), setter("peak_voltage", 0.4), setter("transient_voltage", 0.33)]
    for pcb, pd, mg, field, ins in itertools.product((False, True), range(1, 5), range(1, 6), (1, 2), (1, 2, 3)):
        case(f"compute-pcb{int(pcb)}-pd{pd}-mg{mg}-f{field}-ins{ins}", *base,
             setter("pcb_material", pcb), setter("pollution_degree", pd), setter("material_group", mg),
             setter("field", field), setter("insulation_type", ins),
             {"op": "clearance", "pd": pd, "field": field, "altitude": bits(2000.0)},
             {"op": "creepage", "pd": pd, "mg": mg}, {"op": "compute"}, {"op": "compute"})

    preferred = values("ComputeClearanceDistance", "transientVoltage")
    transient = values("GetClearanceToWithstandTransientVoltage", "aVoltage")
    peaks = values("GetClearanceToWithstandPeaks", "aVoltage")
    # Division here generates inputs only; the shims never normalize arithmetic.
    for ins, field in itertools.product((1, 2, 3, 0), (1, 2)):
        for axis, boundaries in (("transient_voltage", preferred + transient), ("peak_voltage", peaks)):
            for label, v in [(f"b{i}-{label}", v) for i, (label, v) in enumerate(boundaries)]:
                case(f"clearance-{axis}-ins{ins}-f{field}-{label}", *base, setter("field", field),
                     setter("insulation_type", ins), setter(axis, v),
                     {"op": "clearance", "pd": 2, "field": field, "altitude": bits(2000.0)}, {"op": "compute"})
        if ins == 3:
            for i, boundary in enumerate(sorted({v for _, v in transient + peaks if 0 < v < 1e5})):
                for side, value in (("below", math.nextafter(boundary / 1.6, -math.inf)),
                                    ("at", boundary / 1.6), ("above", math.nextafter(boundary / 1.6, math.inf))):
                    case(f"reinforced-scaled-f{field}-{i}-{side}", *base, setter("field", field),
                         setter("transient_voltage", value), setter("peak_voltage", value), {"op": "compute"})

    for label, altitude in values("GetClearanceAltitudeCorrectionFactor", "aAltitude"):
        for invalid in (False, True):
            case(f"compute-altitude-{label}-invalid{int(invalid)}", *base, setter("altitude", altitude),
                 setter("peak_voltage", 1e5 if invalid else 0.4), {"op": "compute"})
    for i, frequency in enumerate((-sys.float_info.max, -1.0, -0.0, 0.0, 50.0, 60.0,
                                    math.nextafter(30000.0, -math.inf), 30000.0,
                                    math.nextafter(30000.0, math.inf), sys.float_info.max)):
        case(f"inactive-frequency-cutoff-{i}", *base, setter("frequency", frequency), {"op": "compute"})

    # Setters preserve previously computed outputs until Compute is called again.
    for field, value in (("altitude", 9000.0), ("frequency", 40000.0), ("rated_voltage", 1500.0),
                         ("rms_voltage", 160.1), ("peak_voltage", 2.0), ("transient_voltage", 4.0),
                         ("pollution_degree", 4), ("material_group", 5), ("field", 2),
                         ("insulation_type", 1), ("overvoltage_category", 1), ("pcb_material", True)):
        case(f"mutation-{field}", *base, {"op": "compute"}, setter(field, value), {"op": "state"},
             {"op": "impulse"}, {"op": "compute"})
    for field in ("pollution_degree", "material_group", "field", "insulation_type", "overvoltage_category"):
        for value in (-2147483648, 0, 2147483647):
            case(f"unnamed-enum-{field}-{value}", *base, setter(field, value), {"op": "compute"}, {"op": "impulse"})

    rng = random.Random(60664)
    for i in range(128):
        steps = list(base)
        for _ in range(8):
            field = rng.choice(("rms_voltage", "peak_voltage", "transient_voltage", "altitude", "frequency"))
            steps += [setter(field, rng.uniform(-1.0, 100000.0)), {"op": "compute"}]
        case(f"fixed-seed-state-{i}", *steps)
    case("integer-negative-zero-bits", {"op": "set", "field": "altitude", "value": 0})
    assert len({c["case"] for c in cases}) == len(cases)
    return cases


def serialize(cases):
    lines = []
    for case in cases:
        text = json.dumps(case, separators=(",", ":"))
        if case["case"] == "integer-negative-zero-bits":
            # Preserve the raw integer token that Python otherwise normalizes to zero.
            text = text.replace('"value":0', '"value":-0', 1)
        lines.append(text + "\n")
    return "".join(lines).encode()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    source = subprocess.check_output(["git", "show", f"{PIN}:{SOURCE}"], cwd=ROOT)
    data = serialize(generate(source.decode("utf-8")))
    path = ROOT / "port/cases" / (UNIT + ".jsonl")
    if args.check:
        assert path.read_bytes() == data, "case corpus differs; regenerate and repeat live comparison"
    else:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    print(f"{len(data.splitlines())} cases; {len(data)} bytes; sha256 {hashlib.sha256(data).hexdigest()}")


if __name__ == "__main__":
    main()
