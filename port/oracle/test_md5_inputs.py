#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check real MD5 adapter input regressions; optionally compare the Rust adapter."""
import json
import subprocess
import sys

UNIT = "libs.kimath.src.md5_hash.application.pure.p001"


def regression_cases():
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
    checks = [(json.dumps(request, separators=(",", ":")),
               failure(UNIT, name, "invalid_input", message))
              for request, (name, _, message) in zip(requests, refusals)]
    zero = {"id": 0, "valid": False, "compact": "0" * 32,
            "default": "00 " * 16, "spaced": "00 " * 16}
    digest = {"id": 0, "valid": True, "compact": "93C885BEGF0EB089DEG634904GE59G71",
              "default": "93 C8 85 BE GF 0E B0 89 DE G6 34 90 4G E5 9G 71 ",
              "spaced": "93 C8 85 BE GF 0E B0 89 DE G6 34 90 4G E5 9G 71 "}
    checks.append((json.dumps(positive, separators=(",", ":")).replace('"data":[0]', '"data":[-0]'),
                   {"unit": UNIT, "case": "negative-zero-byte", "result": {"events": [zero, zero, digest]}}))

    # Build raw request strings so neither Python's integer conversion nor float
    # serialization changes the numeric token under test.
    for token in ["1e1000", "-1e1000", "9" * 400]:
        requests = [
            request("overflow-byte", '{"steps":[{"op":"new","id":0},{"op":"bytes","id":0,"data":[' + token + ']}]}'),
            request("overflow-nested", '{"steps":[{"op":"new","id":0,"extra":[{"n":' + token + '}]}]}'),
            request("overflow-unknown-input", '{"unknown":' + token + '}'),
            request("overflow-unknown-unit", '{"unknown":' + token + '}', unit='"unknown-unit"'),
            request("overflow-extra-envelope", '{}')[:-1] + ',"extra":' + token + '}',
            request("overflow-unit", '{}', unit=token),
            request("overflow-case", '{}', case_token=token),
            '{"input":{"n":' + token + '},"unit":' + json.dumps(UNIT) + ',"case":"overflow-first"}',
        ]
        checks.extend((wire, failure(None, None, "invalid_request", "invalid JSON or duplicate key"))
                      for wire in requests)

    finite = ["18446744073709551615", "18446744073709551616", "-9223372036854775809",
              "9" * 308, "1.7976931348623158e308", "1e-1000", "-1e-1000", "0e1000",
              "0.0", "-0.0", "1e0"]
    for token in finite:
        checks.append((request("finite-byte-domain", '{"steps":[{"op":"new","id":0},{"op":"bytes","id":0,"data":[' + token + ']}]}'),
                       failure(UNIT, "finite-byte-domain", "invalid_input", "byte out of range")))
        checks.append((request("finite-id-domain", '{"steps":[{"op":"new","id":' + token + '}]}'),
                       failure(UNIT, "finite-id-domain", "invalid_input", "integer out of range: id")))
        checks.append((request("finite-unknown-unit", '{"n":' + token + '}', unit='"unknown-unit"'),
                       failure("unknown-unit", "finite-unknown-unit", "unknown_unit", "unit has no registered shim")))
        checks.append((request("finite-identity", '{}', case_token=token),
                       failure(UNIT, None, "invalid_request", "expected unit, case, input object")))
    checks.append((request("finite-ignored-field", '{"steps":[{"op":"new","id":0}],"n":18446744073709551615}'),
                   failure(UNIT, "finite-ignored-field", "invalid_input", "unexpected or missing field")))
    checks.append((request("numeric-looking-string", '{"steps":[{"op":"new","id":0,"n":"1e1000"}]}'),
                   failure(UNIT, "numeric-looking-string", "invalid_input", "unexpected or missing field")))
    return checks


def failure(unit, case, kind, message):
    return {"unit": unit, "case": case, "error": {"kind": kind, "message": message}}


def request(case, input_text, unit=None, case_token=None):
    return ('{"unit":' + (json.dumps(UNIT) if unit is None else unit)
            + ',"case":' + (json.dumps(case) if case_token is None else case_token)
            + ',"input":' + input_text + '}')


def main():
    if len(sys.argv) < 2:
        raise SystemExit("usage: test_md5_inputs.py ORACLE [RUST_SHIM]")
    checks = regression_cases()
    wire = "\n".join(text for text, _ in checks) + "\n"
    previous = None
    for executable in sys.argv[1:]:
        process = subprocess.run([executable], input=wire, capture_output=True, text=True,
                                 check=True, timeout=30)
        replies = [json.loads(line) for line in process.stdout.split("\n") if line]
        assert not process.stderr, process.stderr
        assert len(replies) == len(checks)
        for (text, expected), reply in zip(checks, replies):
            assert reply == expected, {"request": text, "expected": expected, "actual": reply}
        if previous is not None:
            assert replies == previous
        previous = replies
    refusals = sum("error" in expected for _, expected in checks)
    print(f"MD5 adapter: {refusals} complete refusal envelopes and literal -0 success passed per adapter")


if __name__ == "__main__":
    main()
