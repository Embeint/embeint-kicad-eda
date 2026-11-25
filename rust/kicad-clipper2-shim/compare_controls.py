#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Undefined-behaviour controls never enter the public corpus: both programs must refuse each
one with the identical error envelope, and neither may return a result or a deferral."""
import argparse
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CONTROLS = ROOT / "port/oracle/fixtures/clipper2-core-engine-controls.jsonl"

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--oracle", required=True)
parser.add_argument("--rust", required=True)
args = parser.parse_args()
controls = [json.loads(line) for line in CONTROLS.read_text(encoding="utf-8").splitlines()]
requests = "".join(json.dumps(c["request"], ensure_ascii=False) + "\n" for c in controls)
for binary in [args.oracle, args.rust]:
    run = subprocess.run([binary], input=requests, text=True, capture_output=True, check=True, timeout=60)
    replies = [json.loads(line) for line in run.stdout.splitlines()]
    assert len(replies) == len(controls), (binary, len(replies), len(controls))
    for control, reply in zip(controls, replies):
        assert reply["unit"] == control["request"]["unit"], (binary, reply)
        assert reply["case"] == control["request"]["case"], (binary, reply)
        assert set(reply) == {"unit", "case", "error"}, (binary, control["request"]["case"], reply)
        assert reply["error"] == control["error"], (binary, control["request"]["case"], reply)
print(f"{len(controls)} undefined-behaviour controls refused identically by the oracle and the Rust shim")
