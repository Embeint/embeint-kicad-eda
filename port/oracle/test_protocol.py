#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Exercise the real oracle process, including streaming, boundaries and failures."""
import json
from pathlib import Path
import random
import subprocess
import sys

from pin import KICAD_PIN


def main():
    binary = str(Path(sys.argv[1]).resolve())
    provenance = json.loads(subprocess.check_output([binary, "--provenance"], text=True))
    sidecar = json.loads(Path(binary).with_name("oracle-provenance.json").read_text(encoding="utf-8"))
    assert provenance == sidecar, "binary embeds stale build provenance"
    assert provenance["kicad_pin"] == KICAD_PIN
    assert provenance["floating_point"] == {"contract": "off", "fast_math": False}
    units = json.loads(subprocess.check_output([binary, "--list"], text=True))
    assert any(u["unit"] == "example.kimath-rescale" for u in units)
    inputs = [(1, 1, 2), (-1, 1, 2), (1, 1, -2), (-1, -1, -2), (0, 0, 1),
              (1000000, 2000, 1), (-1000000, 2000, 1)]
    rng = random.Random(0)
    inputs += [(rng.randint(-1000, 1000), rng.randint(-1000, 1000),
                rng.choice([-1, 1]) * rng.randint(1, 1000)) for _ in range(256)]
    requests = [json.dumps({"unit": "example.kimath-rescale", "case": str(i),
                           "input": {"numerator": n, "value": v, "denominator": d}})
                for i, (n, v, d) in enumerate(inputs)]
    invalid = [
        ("", "invalid_request"), ("{", "invalid_request"), ("[]", "invalid_request"),
        ('{"unit":"x","case":"c","input":{},"extra":0}', "invalid_request"),
        ('{"unit":"x","unit":"y","case":"c","input":{}}', "invalid_request"),
        ('{"unit":"x","case":"c","input":{"a":0,"a":1}}', "invalid_request"),
        ('{"unit":"x","case":"c","input":null}', "invalid_request"),
        ('{"unit":1,"case":"c","input":{}}', "invalid_request"),
        ('{"unit":"x","case":"","input":{}}', "invalid_request"),
        ('{"unit":"x","case":"c","input":{}} false', "invalid_request"),
        ('{"unit":"missing","case":"c","input":{}}', "unknown_unit"),
    ]
    for value in (0, 1.5, True, 2**64 - 1, -2**63, "2", None):
        invalid.append((json.dumps({"unit": "example.kimath-rescale", "case": "bad",
                                   "input": {"numerator": 1, "value": 1,
                                             "denominator": value}}), "invalid_input"))
    for bad_input in ({}, {"numerator": 1, "value": 1, "denominator": 1, "extra": 0},
                      {"numerator": 1000000, "value": 1000000, "denominator": 1}):
        invalid.append((json.dumps({"unit": "example.kimath-rescale", "case": "bad",
                                   "input": bad_input}), "invalid_input"))
    requests += [line for line, _ in invalid]
    result = subprocess.run([binary], input="\n".join(requests) + "\n", capture_output=True,
                            text=True, check=True, timeout=30)
    assert not result.stderr, result.stderr
    replies = [json.loads(line) for line in result.stdout.splitlines()]
    assert len(replies) == len(requests)
    for i, (n, v, d) in enumerate(inputs):
        # Independent rational nearest-integer check; ties go away from zero.
        magnitude = (2 * abs(n * v) + abs(d)) // (2 * abs(d))
        expected = -magnitude if (n * v < 0) != (d < 0) else magnitude
        assert replies[i] == {"unit": "example.kimath-rescale", "case": str(i),
                              "result": expected}, replies[i]
    for reply, (_, kind) in zip(replies[len(inputs):], invalid):
        assert set(reply) == {"unit", "case", "error"}, reply
        assert reply["error"]["kind"] == kind, reply
        assert reply["error"]["kind"] != "unported"
    # EOF without a newline is still one request; replies flush before EOF.
    with subprocess.Popen([binary], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, text=True) as process:
        process.stdin.write(requests[0] + "\n")
        process.stdin.flush()
        assert json.loads(process.stdout.readline())["case"] == "0"
        process.stdin.write(requests[1])
        process.stdin.close()
        assert json.loads(process.stdout.readline())["case"] == "1"
        assert process.wait(timeout=10) == 0
    print(f"Oracle protocol: {len(inputs)} actual kimath calls, {len(invalid)} refusals, streaming OK")


if __name__ == "__main__":
    main()
