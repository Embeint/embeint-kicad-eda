#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Real-process refusal/deferral controls, never a numerical reference implementation."""
import json
from pathlib import Path
import subprocess
import sys

UNIT = "libs.kimath.src.math.util.application.ui.p001"
CONTROLS = Path(__file__).parent / "fixtures/math-util-controls.jsonl"


def check(binary):
    controls = [json.loads(line) for line in CONTROLS.read_text().splitlines()]
    requests, expected = [], []
    refusals = deferred = 0
    for index, control in enumerate(controls):
        request = control["request"]
        requests.append(request)
        expected.append({"unit": UNIT, "case": request["case"], "error": control["error"]})
        if control["error"]["kind"] == "unported":
            deferred += 1
        else:
            refusals += 1
        # Verify that every refusal leaves the process able to perform real arithmetic.
        case = f"continuation-{index}"
        requests.append({"unit": UNIT, "case": case, "input": {"op": "hypot_wide", "x": 3, "y": 4}})
        expected.append({"unit": UNIT, "case": case, "result": {"value": 5}})
    payload = "".join(json.dumps(r, separators=(",", ":"))+"\n" for r in requests)
    # Keep these raw: json.dumps normalizes Python integer -0 to 0 and hides the
    # spelling-sensitive difference between signed integer and floating-point zero.
    literals = [
        ('{"op":"hypot_wide","x":-0,"y":4}', {"result": {"value": 4}}),
        ('{"op":"hypot_wide","x":3,"y":-0}', {"result": {"value": 3}}),
        ('{"op":"hypot_wide","x":-0,"y":-0}', {"result": {"value": 0}}),
    ]
    for coordinate in ("x", "y"):
        for token in ("-0.0", "-1"):
            x, y = (token, "4") if coordinate == "x" else ("3", token)
            literals.append((f'{{"op":"hypot_wide","x":{x},"y":{y}}}',
                             {"error": {"kind": "invalid_input",
                                        "message": f"integer out of range: {coordinate}"}}))
    for index, (literal, reply) in enumerate(literals):
        case = f"literal-integer-{index}"
        payload += f'{{"unit":"{UNIT}","case":"{case}","input":{literal}}}\n'
        expected.append({"unit": UNIT, "case": case, **reply})
    process = subprocess.run([binary], input=payload, text=True, capture_output=True, timeout=60, check=True)
    actual = [json.loads(line) for line in process.stdout.splitlines()]
    if len(actual) != len(expected):
        raise AssertionError(f"reply count {len(actual)} != {len(expected)}")
    for index, (got, want) in enumerate(zip(actual, expected)):
        if got != want:
            raise AssertionError(f"{binary} line {index+1}: {got} != {want}")
    print(f"{binary}: {refusals} refusals, {deferred} explicit host deferrals, {len(controls)} successful continuations")
    print(f"{binary}: 3 literal integer -0 successes and 4 literal float/negative refusals")


if __name__ == "__main__":
    for binary in sys.argv[1:]:
        check(binary)
    if len(sys.argv) < 2:
        raise SystemExit("usage: test_math_util_inputs.py ORACLE [RUST_SHIM]")
