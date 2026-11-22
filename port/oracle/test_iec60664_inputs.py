#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check complete IEC adapter refusals, integer bits, EOF and stream recovery."""
import argparse
import json
from pathlib import Path
import subprocess

UNIT = "pcb_calculator.calculator_panels.application.pure.p001"
FIXTURES = Path(__file__).with_name("iec60664-input-guards.jsonl")


def request(steps, case="guard"):
    return {"unit": UNIT, "case": case, "input": {"steps": steps}}


def refusals():
    items = []

    def add(wire, message, kind="invalid_input", unit=UNIT, case=None):
        items.append({"wire": wire, "expected": {"unit": unit, "case": case,
                      "error": {"kind": kind, "message": message}}})

    def bad(value, message):
        case = str(len(items))
        add(json.dumps({"unit": UNIT, "case": case, "input": value}), message, case=case)

    for value, message in [
        ({}, "missing field"), ({"steps": []}, "expected 1..256 steps"),
        ({"steps": {}}, "expected 1..256 steps"),
        ({"steps": [{"op": "state"}] * 257}, "expected 1..256 steps"),
        ({"steps": [{"op": "state", "extra": 0}]}, "unexpected field"),
        ({"steps": [{"op": "missing"}]}, "unknown operation"),
        ({"steps": [None]}, "expected string: op"),
        ({"steps": [{"op": "set", "field": "coated", "value": True}]}, "unknown setter"),
    ]:
        bad(value, message)
    for value in (-1, 1.0, True, "0", None, 2**64, 0x7FF0000000000000, 0x7FF8000000000001):
        bad({"steps": [{"op": "set", "field": "altitude", "value": value}]},
            "expected finite bits" if type(value) is int and value in
            (0x7FF0000000000000, 0x7FF8000000000001) else "expected u64 bits")
    for value in (-2**31 - 1, 2**31, 1.0, True, "1", None):
        bad({"steps": [{"op": "set", "field": "pollution_degree", "value": value}]},
            "integer out of range: value")
    bad({"steps": [{"op": "set", "field": "pcb_material", "value": 1}]},
        "expected boolean: value")
    for step, message in [
        ({"op": "groove", "pd": "bad", "distance": -1}, "expected u64 bits"),
        ({"op": "rms", "voltage": -1, "field": "bad"}, "integer out of range: field"),
        ({"op": "transient", "voltage": -1, "pd": "bad", "field": "bad"}, "expected u64 bits"),
        ({"op": "peaks", "voltage": -1, "field": "bad"}, "expected u64 bits"),
        ({"op": "basic_creepage", "voltage": -1, "pd": "bad", "mg": "bad"}, "expected u64 bits"),
        ({"op": "clearance", "pd": "bad", "field": "bad", "altitude": -1}, "integer out of range: pd"),
        ({"op": "creepage", "pd": "bad", "mg": "bad"}, "integer out of range: pd"),
    ]:
        bad({"steps": [step]}, message)
    for wire in ("", "{", '{"unit":"x","unit":"y","case":"c","input":{}}',
                 '{"unit":"' + UNIT + '","case":"duplicate","input":{"steps":[{"op":"state","op":"compute"}]}}'):
        add(wire, "invalid JSON or duplicate key", "invalid_request", unit=None)
    add("[]", "expected unit, case, input object", "invalid_request", unit=None)
    add(json.dumps(request([{"op": "state"}], "")), "expected unit, case, input object",
        "invalid_request", case="")
    extra = request([{"op": "state"}], "extra-top")
    extra["extra"] = 0
    add(json.dumps(extra), "expected unit, case, input object", "invalid_request", case="extra-top")
    unknown = request([{"op": "state"}], "unknown")
    unknown["unit"] = "unregistered-iec-unit"
    add(json.dumps(unknown), "unit has no registered shim", "unknown_unit",
        unit=unknown["unit"], case="unknown")

    # Construct numeric requests as raw text. Python float conversion/serialization
    # would turn overflowing JSON numbers into Infinity or lose the integer form.
    def raw_steps(token, field="altitude"):
        return ('{"unit":"' + UNIT + '","case":"numeric-domain","input":{"steps":'
                '[{"op":"set","field":"' + field + '","value":' + token + '}]}}')

    for token in ("1e1000", "-1e1000", "9" * 400, "-" + "9" * 400,
                  "1.7976931348623159e308"):
        for wire in (
            token,
            raw_steps(token),
            '{"unit":"' + UNIT + '","case":"nested","input":{"steps":[{"op":"state","extra":[{"n":' + token + '}]}]}}',
            '{"unit":"' + UNIT + '","case":"unknown-input","input":{"extra":' + token + '}}',
            '{"unit":"' + UNIT + '","case":"unknown-top","input":{},"extra":' + token + '}',
            '{"unit":' + token + ',"case":"numeric-identity","input":{}}',
            '{"unit":"' + UNIT + '","case":' + token + ',"input":{}}',
            '{"unit":"unregistered-iec-unit","case":"overflow","input":{"n":' + token + '}}',
        ):
            add(wire, "invalid JSON or duplicate key", "invalid_request", unit=None)

    for token in ("18446744073709551616", "-9223372036854775809", "9" * 100,
                  "9" * 308, "1e100", "1.7976931348623158e308", "1e-1000",
                  "-1e-1000", "0e1000", "-0.0", "1.0"):
        add(raw_steps(token), "expected u64 bits", case="numeric-domain")
        add(raw_steps(token, "pollution_degree"), "integer out of range: value", case="numeric-domain")

    finite = "9" * 100
    add('{"unit":"' + UNIT + '","case":"finite-extra","input":{"steps":[{"op":"state","extra":' + finite + '}]}}',
        "unexpected field", case="finite-extra")
    add('{"unit":"' + UNIT + '","case":"finite-top","input":{},"extra":' + finite + '}',
        "expected unit, case, input object", "invalid_request", case="finite-top")
    add('{"unit":' + finite + ',"case":"numeric-identity","input":{}}',
        "expected unit, case, input object", "invalid_request", unit=None, case="numeric-identity")
    add('{"unit":"' + UNIT + '","case":' + finite + ',"input":{}}',
        "expected unit, case, input object", "invalid_request")
    add('{"unit":"unregistered-iec-unit","case":"finite","input":{"n":' + finite + '}}',
        "unit has no registered shim", "unknown_unit", unit="unregistered-iec-unit", case="finite")
    add(raw_steps("18446744073709551615"), "expected finite bits", case="numeric-domain")
    return items


def check(binary, fixtures):
    zero = json.dumps(request([{"op": "set", "field": "altitude", "value": 0}], "integer-zero"),
                      separators=(",", ":")).replace('"value":0', '"value":-0')
    wires = [item["wire"] for item in fixtures] + [zero,
        json.dumps(request([{"op": "set", "field": "altitude", "value": 2**63}], "signed-zero-bits")),
        json.dumps(request([{"op": "set", "field": "altitude", "value": 0xFFEFFFFFFFFFFFFF}], "unsigned-finite-bits")),
        json.dumps(request([{"op": "state"}] * 256, "maximum")),
        json.dumps(request([{"op": "compute"}], "recovery"))]
    process = subprocess.run([str(binary)], input="\n".join(wires), capture_output=True,
                             text=True, timeout=30, check=True)
    assert not process.stderr, process.stderr
    replies = [json.loads(line) for line in process.stdout.split("\n") if line]
    assert len(replies) == len(wires)
    for fixture, reply in zip(fixtures, replies):
        assert reply == fixture["expected"], (fixture["wire"], reply, fixture["expected"])
    assert replies[-5]["result"][0]["state"]["altitude"] == 0
    assert replies[-4]["result"][0]["state"]["altitude"] == 2**63
    assert replies[-3]["result"][0]["state"]["altitude"] == 0xFFEFFFFFFFFFFFFF
    assert len(replies[-2]["result"]) == 256
    assert replies[-1]["result"][0]["value"] is True
    print(f"{binary.name}: {len(fixtures)} complete refusal envelopes, raw -0, u64 bits and recovery passed")
    return replies


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("oracle", type=Path, nargs="?")
    parser.add_argument("rust", type=Path, nargs="?")
    parser.add_argument("--write-fixtures", action="store_true")
    args = parser.parse_args()
    generated = refusals()
    if args.write_fixtures:
        FIXTURES.write_text("".join(json.dumps(item, separators=(",", ":")) + "\n" for item in generated),
                            encoding="utf-8", newline="\n")
    fixtures = [json.loads(line) for line in FIXTURES.read_text(encoding="utf-8").split("\n") if line]
    assert fixtures == generated, "regenerate the shared input contract with --write-fixtures"
    if args.oracle is not None:
        actual = check(args.oracle.resolve(), fixtures)
        if args.rust is not None:
            assert check(args.rust.resolve(), fixtures) == actual
    elif not args.write_fixtures:
        parser.error("provide an oracle executable or --write-fixtures")


if __name__ == "__main__":
    main()
