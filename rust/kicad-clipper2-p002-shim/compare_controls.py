#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Refusal controls (input domain, resource and malformed-request guards of the engine
remainder and offset adapters) never enter the public corpus: both programs must refuse each
one with the identical error envelope, return neither a result nor a deferral, and then answer
a successful continuation request in the same process with identical results."""
import argparse
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
UNIT = "thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p002"
CONTROLS = ROOT / "port/oracle/fixtures/clipper2-engine-p002-controls.jsonl"
CONTINUATION = {
    "op": "offset",
    "groups": [{"paths": [[[0, 0, 0], [40, 0, 0], [40, 30, 0], [0, 30, 0]]], "join": 3,
                "end": 0, "single": True}],
    "miter_limit": "4000000000000000", "arc_tolerance": "0000000000000000",
    "preserve_collinear": False, "reverse_solution": False, "z": "none", "delta_table": None,
    "set_callback": False, "executes": [{"kind": "paths", "delta": "4014000000000000"}],
}

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--oracle", required=True)
parser.add_argument("--rust", required=True)
args = parser.parse_args()
controls = [json.loads(line) for line in CONTROLS.read_text(encoding="utf-8").splitlines()]
requests = []
for index, control in enumerate(controls):
    requests.append(control["request"])
    requests.append({"unit": UNIT, "case": f"continuation-{index}", "input": CONTINUATION})
stream = "".join(json.dumps(r, ensure_ascii=False) + "\n" for r in requests)
continuations = {}
for binary in [args.oracle, args.rust]:
    run = subprocess.run([binary], input=stream, text=True, capture_output=True, check=True,
                         timeout=120)
    replies = [json.loads(line) for line in run.stdout.splitlines()]
    assert len(replies) == len(requests), (binary, len(replies), len(requests))
    for i, control in enumerate(controls):
        reply, after = replies[2 * i], replies[2 * i + 1]
        assert reply["unit"] == control["request"]["unit"], (binary, reply)
        assert reply["case"] == control["request"]["case"], (binary, reply)
        assert set(reply) == {"unit", "case", "error"}, (binary, control["request"]["case"], reply)
        assert reply["error"] == control["error"], (binary, control["request"]["case"], reply)
        assert set(after) == {"unit", "case", "result"}, (binary, after)
        continuations.setdefault(i, []).append(after["result"])
for i, results in continuations.items():
    assert results[0] == results[1], ("continuation mismatch after control", i)
print(f"{len(controls)} controls refused identically by the oracle and the Rust shim, each "
      "followed by an identical successful continuation")
