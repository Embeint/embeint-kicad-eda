#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check both live adapters' input guards/stream recovery; errors never become goldens."""
import argparse
import json
from pathlib import Path
import subprocess

UNIT = "pcb_calculator.calculator_panels.application.pure.p001"


def request(steps):
    return {"unit": UNIT, "case": "guard", "input": {"steps": steps}}


def check(binary):
    bad_inputs = [
        {}, {"steps": []}, {"steps": {}}, {"steps": [{"op": "state"}] * 257},
        {"steps": [{"op": "state", "extra": 0}]}, {"steps": [{"op": "missing"}]},
        {"steps": [None]}, {"steps": [{"op": "set", "field": "coated", "value": True}]},
    ]
    for value in (-1, 1.0, True, "0", None, 2**64, 0x7FF0000000000000, 0x7FF8000000000001):
        bad_inputs.append({"steps": [{"op": "set", "field": "altitude", "value": value}]})
    for value in (-2**31 - 1, 2**31, 1.0, True, "1", None):
        bad_inputs.append({"steps": [{"op": "set", "field": "pollution_degree", "value": value}]})
    bad_inputs.append({"steps": [{"op": "set", "field": "pcb_material", "value": 1}]})
    requests = [json.dumps({"unit": UNIT, "case": str(i), "input": value}) for i, value in enumerate(bad_inputs)]
    kinds = ["invalid_input"] * len(requests)
    for text in ("", "{", "[]", '{"unit":"x","unit":"y","case":"c","input":{}}',
                 json.dumps({"unit": UNIT, "case": "", "input": {}})):
        requests.append(text)
        kinds.append("invalid_request")
    # Duplicate nested fields must be rejected before an adapter mutates state.
    requests.append('{"unit":"' + UNIT + '","case":"duplicate","input":{"steps":[{"op":"state","op":"compute"}]}}')
    kinds.append("invalid_request")
    valid = request([{"op": "state"}] * 256)
    requests += [json.dumps(valid), json.dumps(request([{"op": "compute"}]))]
    process = subprocess.run([str(binary)], input="\n".join(requests), capture_output=True,
                             text=True, timeout=30, check=True)
    assert not process.stderr, process.stderr
    replies = [json.loads(line) for line in process.stdout.splitlines()]
    assert len(replies) == len(requests)
    for reply, kind in zip(replies, kinds):
        assert reply["error"]["kind"] == kind, reply
        assert "result" not in reply
    assert len(replies[-2]["result"]) == 256
    assert replies[-1]["result"][0]["value"] is True
    print(f"{binary.name}: {len(kinds)} guard refusals, maximum step count, EOF without newline and recovery passed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("oracle", type=Path)
    parser.add_argument("rust", type=Path)
    args = parser.parse_args()
    for binary in (args.oracle, args.rust):
        check(binary.resolve())


if __name__ == "__main__":
    main()
