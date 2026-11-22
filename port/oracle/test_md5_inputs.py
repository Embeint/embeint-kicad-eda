#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check real MD5 adapter input regressions; optionally compare the Rust adapter."""
import json
import subprocess
import sys

UNIT = "libs.kimath.src.md5_hash.application.pure.p001"


def main():
    refusals = [
        ("missing-id", [{"op": "new"}], "integer out of range: id"),
        ("self-unconstructed", [{"op": "assign", "id": 0, "source": 0}],
         "self-assignment overlaps reference memcpy"),
        ("source-type-before-state", [{"op": "assign", "id": 0, "source": "zero"}],
         "integer out of range: source"),
        ("source-range-before-state", [{"op": "assign", "id": 0, "source": 16}],
         "integer out of range: source"),
    ]
    for token in [-1, 256, 0.0, -0.0]:
        refusals.append((f"byte-{token}", [{"op": "new", "id": 0},
            {"op": "bytes", "id": 0, "data": [token]}], "byte out of range"))
    requests = [{"unit": UNIT, "case": name, "input": {"steps": steps}}
                for name, steps, _ in refusals]
    positive = {"unit": UNIT, "case": "negative-zero-byte", "input": {"steps": [
        {"op": "new", "id": 0}, {"op": "bytes", "id": 0, "data": [0]},
        {"op": "finalize", "id": 0}]}}
    wire = "\n".join(json.dumps(value, separators=(",", ":")) for value in requests) + "\n"
    wire += json.dumps(positive, separators=(",", ":")).replace('"data":[0]', '"data":[-0]') + "\n"
    previous = None
    for executable in sys.argv[1:]:
        process = subprocess.run([executable], input=wire, capture_output=True, text=True,
                                 check=True, timeout=30)
        replies = [json.loads(line) for line in process.stdout.split("\n") if line]
        assert len(replies) == len(refusals) + 1
        for (name, _, message), reply in zip(refusals, replies):
            assert reply == {"unit": UNIT, "case": name,
                             "error": {"kind": "invalid_input", "message": message}}, reply
        reply = replies[-1]
        assert reply["unit"] == UNIT and reply["case"] == "negative-zero-byte"
        assert reply["result"]["events"][-1]["compact"] == "93C885BEGF0EB089DEG634904GE59G71"
        if previous is not None:
            assert replies == previous
        previous = replies
    print(f"MD5 adapter: {len(refusals)} exact refusals and literal -0 byte passed")


if __name__ == "__main__":
    main()
