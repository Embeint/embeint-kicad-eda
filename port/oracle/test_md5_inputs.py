#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check real MD5 adapter input regressions; optionally compare the Rust adapter."""
import hashlib
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

    for leaf, leaf_containers in [("0", 0), ("[]", 1), ("{}", 1)]:
        for arrays in [124, 125, 126, 127, 128]:
            nested = "[" * arrays + leaf + "]" * arrays
            parsed = 2 + arrays + leaf_containers <= 127
            wire = request("nested-input", '{"steps":[{"op":"new","id":0}],"extra":' + nested + '}')
            expected = (failure(UNIT, "nested-input", "invalid_input", "unexpected or missing field") if parsed
                        else failure(None, None, "invalid_request", "invalid JSON or duplicate key"))
            checks.append((wire, expected))
            wire = request("nested-unknown-unit", '{"extra":' + nested + '}', unit='"unknown-unit"')
            expected = (failure("unknown-unit", "nested-unknown-unit", "unknown_unit", "unit has no registered shim") if parsed
                        else failure(None, None, "invalid_request", "invalid JSON or duplicate key"))
            checks.append((wire, expected))
            parsed = 1 + arrays + leaf_containers <= 127
            for wire, unit, case in [
                (request("nested-unit", '{}', unit=nested), None, "nested-unit"),
                (request("nested-case", '{}', case_token=nested), UNIT, None),
                (request("nested-extra", '{}')[:-1] + ',"extra":' + nested + '}', UNIT, "nested-extra"),
            ]:
                expected = (failure(unit, case, "invalid_request", "expected unit, case, input object") if parsed
                            else failure(None, None, "invalid_request", "invalid JSON or duplicate key"))
                checks.append((wire, expected))
    checks.extend(reserved_key_cases(zero))
    checks.extend(literal_escaped_key_cases(zero))
    return checks


# serde_json reserves these object keys internally. User objects that contain them
# must stay ordinary objects, as nlohmann sees them. Requests are literal wire text
# so key spelling, JSON escapes, member order and value types are exactly as tested.
# The first 14 are the independent review's retained reproducers, each with the
# complete reply the actual C++ oracle returned: (wire, unit, case, kind, message).
RESERVED_KEY_RETAINED = [
    (r'{"unit":"libs.kimath.src.md5_hash.application.pure.p001","case":"$serde_json::private::Number:data","input":{"steps":[{"op":"new","id":0},{"op":"bytes","id":0,"data":[{"$serde_json::private::Number":"0"}]},{"op":"finalize","id":0}]}}',
     UNIT, "$serde_json::private::Number:data", "invalid_input", "byte out of range"),
    (r'{"unit":"libs.kimath.src.md5_hash.application.pure.p001","case":"$serde_json::private::Number:id","input":{"steps":[{"op":"new","id":{"$serde_json::private::Number":"0"}}]}}',
     UNIT, "$serde_json::private::Number:id", "invalid_input", "integer out of range: id"),
    (r'{"unit":"libs.kimath.src.md5_hash.application.pure.p001","case":"$serde_json::private::Number:op","input":{"steps":[{"op":{"$serde_json::private::Number":"\"new\""},"id":0}]}}',
     UNIT, "$serde_json::private::Number:op", "invalid_input", "expected op string"),
    (r'{"unit":"libs.kimath.src.md5_hash.application.pure.p001","case":"$serde_json::private::Number:steps","input":{"steps":{"$serde_json::private::Number":"[{\"op\":\"new\",\"id\":0}]"}}}',
     UNIT, "$serde_json::private::Number:steps", "invalid_input", "expected steps array"),
    (r'{"unit":"libs.kimath.src.md5_hash.application.pure.p001","case":"$serde_json::private::Number:input","input":{"$serde_json::private::Number":"{\"steps\":[{\"op\":\"new\",\"id\":0}]}"}}',
     UNIT, "$serde_json::private::Number:input", "invalid_input", "unexpected or missing field"),
    (r'{"unit":{"$serde_json::private::Number":"\"libs.kimath.src.md5_hash.application.pure.p001\""},"case":"$serde_json::private::Number:unit","input":{"steps":[{"op":"new","id":0},{"op":"bytes","id":0,"data":[{"$serde_json::private::Number":"0"}]},{"op":"finalize","id":0}]}}',
     None, "$serde_json::private::Number:unit", "invalid_request", "expected unit, case, input object"),
    (r'{"unit":"libs.kimath.src.md5_hash.application.pure.p001","case":{"$serde_json::private::Number":"\"coerced\""},"input":{"steps":[{"op":"new","id":0},{"op":"bytes","id":0,"data":[{"$serde_json::private::Number":"0"}]},{"op":"finalize","id":0}]}}',
     UNIT, None, "invalid_request", "expected unit, case, input object"),
    (r'{"unit":"libs.kimath.src.md5_hash.application.pure.p001","case":"$serde_json::private::RawValue:data","input":{"steps":[{"op":"new","id":0},{"op":"bytes","id":0,"data":[{"$serde_json::private::RawValue":"0"}]},{"op":"finalize","id":0}]}}',
     UNIT, "$serde_json::private::RawValue:data", "invalid_input", "byte out of range"),
    (r'{"unit":"libs.kimath.src.md5_hash.application.pure.p001","case":"$serde_json::private::RawValue:id","input":{"steps":[{"op":"new","id":{"$serde_json::private::RawValue":"0"}}]}}',
     UNIT, "$serde_json::private::RawValue:id", "invalid_input", "integer out of range: id"),
    (r'{"unit":"libs.kimath.src.md5_hash.application.pure.p001","case":"$serde_json::private::RawValue:op","input":{"steps":[{"op":{"$serde_json::private::RawValue":"\"new\""},"id":0}]}}',
     UNIT, "$serde_json::private::RawValue:op", "invalid_input", "expected op string"),
    (r'{"unit":"libs.kimath.src.md5_hash.application.pure.p001","case":"$serde_json::private::RawValue:steps","input":{"steps":{"$serde_json::private::RawValue":"[{\"op\":\"new\",\"id\":0}]"}}}',
     UNIT, "$serde_json::private::RawValue:steps", "invalid_input", "expected steps array"),
    (r'{"unit":"libs.kimath.src.md5_hash.application.pure.p001","case":"$serde_json::private::RawValue:input","input":{"$serde_json::private::RawValue":"{\"steps\":[{\"op\":\"new\",\"id\":0}]}"}}',
     UNIT, "$serde_json::private::RawValue:input", "invalid_input", "unexpected or missing field"),
    (r'{"unit":{"$serde_json::private::RawValue":"\"libs.kimath.src.md5_hash.application.pure.p001\""},"case":"$serde_json::private::RawValue:unit","input":{"steps":[{"op":"new","id":0},{"op":"bytes","id":0,"data":[{"$serde_json::private::RawValue":"0"}]},{"op":"finalize","id":0}]}}',
     None, "$serde_json::private::RawValue:unit", "invalid_request", "expected unit, case, input object"),
    (r'{"unit":"libs.kimath.src.md5_hash.application.pure.p001","case":{"$serde_json::private::RawValue":"\"coerced\""},"input":{"steps":[{"op":"new","id":0},{"op":"bytes","id":0,"data":[{"$serde_json::private::RawValue":"0"}]},{"op":"finalize","id":0}]}}',
     UNIT, None, "invalid_request", "expected unit, case, input object"),
]

# (case, steps wire text, expected invalid_input message or None for the abc digest).
RESERVED_KEY_STEPS = [
    ("reserved-guard-before",
     r'[{"op":"new","id":0},{"op":"bytes","id":0,"data":[97,98,99]},{"op":"finalize","id":0}]', None),
    ("reserved-byte-between-valid",
     r'[{"op":"new","id":0},{"op":"bytes","id":0,"data":[97,{"$serde_json::private::Number":"98"},99]},{"op":"finalize","id":0}]',
     "byte out of range"),
    ("reserved-byte-after-finalize",
     r'[{"op":"new","id":0},{"op":"bytes","id":0,"data":[97,98,99]},{"op":"finalize","id":0},{"op":"bytes","id":0,"data":[{"$serde_json::private::RawValue":"0"}]}]',
     "byte out of range"),
    ("reserved-guard-between",
     r'[{"op":"new","id":0},{"op":"bytes","id":0,"data":[97,98,99]},{"op":"finalize","id":0}]', None),
    ("reserved-number-step",
     r'[{"op":"new","id":0},{"$serde_json::private::Number":"{\"op\":\"finalize\",\"id\":0}"}]',
     "expected op string"),
    ("reserved-rawvalue-step",
     r'[{"op":"new","id":0},{"$serde_json::private::RawValue":"{\"op\":\"finalize\",\"id\":0}"}]',
     "expected op string"),
    ("reserved-invalid-marker-step-field",
     r'[{"op":"new","id":0,"x":{"$serde_json::private::RawValue":"not json"}}]',
     "unexpected or missing field"),
    ("reserved-repeat",
     r'[{"op":"new","id":0},{"op":"bytes","id":0,"data":[1],"repeat":{"$serde_json::private::Number":"1"}}]',
     "integer out of range: repeat"),
    ("reserved-int-value",
     r'[{"op":"new","id":0},{"op":"int","id":0,"value":{"$serde_json::private::Number":"7"}}]',
     "integer out of range: value"),
    ("reserved-valid-value",
     r'[{"op":"new","id":0},{"op":"valid","id":0,"value":{"$serde_json::private::RawValue":"true"}}]',
     "expected boolean value"),
    ("reserved-clone-source",
     r'[{"op":"new","id":0},{"op":"clone","id":1,"source":{"$serde_json::private::Number":"0"}}]',
     "integer out of range: source"),
    ("reserved-compare-other",
     r'[{"op":"new","id":0},{"op":"compare","id":0,"other":{"$serde_json::private::RawValue":"0"}}]',
     "integer out of range: other"),
    ("reserved-multiple-keys-id",
     r'[{"op":"new","id":{"$serde_json::private::Number":"0","$serde_json::private::RawValue":"0"}}]',
     "integer out of range: id"),
    ("reserved-multiple-keys-byte",
     r'[{"op":"new","id":0},{"op":"bytes","id":0,"data":[{"$serde_json::private::RawValue":"0","$serde_json::private::Number":"0","other":1}]}]',
     "byte out of range"),
    ("reserved-escaped-id",
     r'[{"op":"new","id":{"$serde_json::private::Number":"0"}}]',
     "integer out of range: id"),
    ("reserved-escaped-byte",
     r'[{"op":"new","id":0},{"op":"bytes","id":0,"data":[{"$serde_json::private::RawValue":"0"}]}]',
     "byte out of range"),
    ("reserved-number-typed-id",
     r'[{"op":"new","id":{"$serde_json::private::Number":0}}]',
     "integer out of range: id"),
    ("reserved-rawvalue-array-byte",
     r'[{"op":"new","id":0},{"op":"bytes","id":0,"data":[{"$serde_json::private::RawValue":[0]}]}]',
     "byte out of range"),
    ("reserved-null-op",
     r'[{"op":{"$serde_json::private::Number":null},"id":0}]',
     "expected op string"),
    ("reserved-string-op",
     r'[{"op":"new","id":0},{"op":"$serde_json::private::Number","id":0}]',
     "unknown operation"),
    ("reserved-string-byte",
     r'[{"op":"new","id":0},{"op":"bytes","id":0,"data":["$serde_json::private::RawValue"]}]',
     "byte out of range"),
    ("reserved-near-miss-key",
     r'[{"op":"new","id":{"$serde_json::private::number":"0"}}]',
     "integer out of range: id"),
]


def kicad_format(data):
    """Return MD5 text in KiCad's pinned format (md5-format-nibble-offset: 10..15 map to B..G)."""
    text = "".join("0123456789BCDEFG"[nibble] for byte in hashlib.md5(data).digest()
                   for nibble in (byte >> 4, byte & 15))
    spaced = "".join(text[index:index + 2] + " " for index in range(0, 32, 2))
    return {"id": 0, "valid": True, "compact": text, "default": spaced, "spaced": spaced}


def reserved_key_cases(zero):
    abc = {"events": [zero, zero, kicad_format(b"abc")]}
    checks = []
    for wire, unit, case, kind, message in RESERVED_KEY_RETAINED:
        checks.append((wire, failure(unit, case, kind, message)))
    for case, steps, message in RESERVED_KEY_STEPS:
        wire = request(case, '{"steps":' + steps + '}')
        expected = ({"unit": UNIT, "case": case, "result": abc} if message is None
                    else failure(UNIT, case, "invalid_input", message))
        checks.append((wire, expected))

    envelope = failure(None, None, "invalid_request", "expected unit, case, input object")
    invalid = failure(None, None, "invalid_request", "invalid JSON or duplicate key")
    unknown = "unknown_unit", "unit has no registered shim"
    valid_steps = '{"steps":[{"op":"new","id":0},{"op":"bytes","id":0,"data":[0]},{"op":"finalize","id":0}]}'
    checks.extend([
        (r'{"$serde_json::private::Number":"0"}', envelope),
        (r'{"$serde_json::private::RawValue":"{\"unit\":\"libs.kimath.src.md5_hash.application.pure.p001\",\"case\":\"raw-root\",\"input\":{\"steps\":[{\"op\":\"new\",\"id\":0}]}}"}',
         envelope),
        (request("reserved-extra-invalid-marker", valid_steps)[:-1] + r',"extra":{"$serde_json::private::Number":"bad"}}',
         failure(UNIT, "reserved-extra-invalid-marker", "invalid_request", "expected unit, case, input object")),
        (request("reserved-escaped-unit", valid_steps, unit=r'{"$serde_json::private::RawValue":"\"libs.kimath.src.md5_hash.application.pure.p001\""}'),
         failure(None, "reserved-escaped-unit", "invalid_request", "expected unit, case, input object")),
        (request("reserved-duplicate", '{"steps":[{"op":"new","id":{"$serde_json::private::Number":"0","$serde_json::private::Number":"0"}}]}'),
         invalid),
        (request("reserved-escaped-duplicate", '{"steps":[{"op":"new","id":{"$serde_json::private::Number":"0","\\u0024serde_json::private::Number":"0"}}]}'),
         invalid),
        (request("reserved-unknown-unit", r'{"$serde_json::private::Number":"0"}', unit='"unknown-unit"'),
         failure("unknown-unit", "reserved-unknown-unit", *unknown)),
        (request("$serde_json::private::RawValue", valid_steps),
         {"unit": UNIT, "case": "$serde_json::private::RawValue",
          "result": {"events": [zero, zero, kicad_format(b"\0")]}}),
        (request("reserved-guard-after", '{"steps":[{"op":"new","id":0},{"op":"bytes","id":0,"data":[97,98,99]},{"op":"finalize","id":0}]}'),
         {"unit": UNIT, "case": "reserved-guard-after", "result": abc}),
    ])
    return checks


def failure(unit, case, kind, message):
    return {"unit": unit, "case": case, "error": {"kind": kind, "message": message}}


def literal_escaped_key_cases(zero):
    # Append after the historical 199 checks; raw strings retain KEY escapes.
    checks = [
        (request("reserved-literal-escaped-id",
                 r'{"steps":[{"op":"new","id":{"\u0024serde_json::private::Number":"0"}}]}'),
         failure(UNIT, "reserved-literal-escaped-id", "invalid_input", "integer out of range: id")),
        (request("reserved-literal-escaped-byte",
                 r'{"steps":[{"op":"new","id":0},{"op":"bytes","id":0,"data":[{"$serde_json::private::Raw\u0056alue":"0"}]}]}'),
         failure(UNIT, "reserved-literal-escaped-byte", "invalid_input", "byte out of range")),
        (request("reserved-literal-escaped-unit", '{"steps":[{"op":"new","id":0}]}',
                 unit=r'{"\u0024serde_json::private::RawValue":"\"libs.kimath.src.md5_hash.application.pure.p001\""}'),
         failure(None, "reserved-literal-escaped-unit", "invalid_request", "expected unit, case, input object")),
    ]
    spellings = [r'"\u0024serde_json::private::Number":',
                 r'"$serde_json::private::Raw\u0056alue":',
                 r'"\u0024serde_json::private::RawValue":']
    for (wire, _), spelling in zip(checks, spellings):
        assert spelling in wire, "The escaped marker key lost its literal wire spelling"
    decoded = [json.loads(wire) for wire, _ in checks]
    objects = [decoded[0]["input"]["steps"][0]["id"],
               decoded[1]["input"]["steps"][1]["data"][0], decoded[2]["unit"]]
    assert [list(value) for value in objects] == [
        ["$serde_json::private::Number"], ["$serde_json::private::RawValue"],
        ["$serde_json::private::RawValue"]]
    checks.append((
        request("reserved-literal-escaped-recovered",
                '{"steps":[{"op":"new","id":0},{"op":"bytes","id":0,"data":[97,98,99]},{"op":"finalize","id":0}]}'),
        {"unit": UNIT, "case": "reserved-literal-escaped-recovered",
         "result": {"events": [zero, zero, kicad_format(b"abc")]}}))
    return checks


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
    successes = len(checks) - refusals
    print(f"MD5 adapter: {refusals} complete refusal envelopes and {successes} complete successes passed per adapter")


if __name__ == "__main__":
    main()
