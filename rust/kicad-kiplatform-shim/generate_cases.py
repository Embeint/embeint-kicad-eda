#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fixed public corpus and refusal controls for the Unix policy unit.

Writes port/cases/<unit>.jsonl (requests only) and
port/oracle/fixtures/kiplatform-policy-controls.jsonl (raw request lines that both
adapters must refuse, with the expected refusal envelope). It never computes a
reference reply. Both Unix bodies are constant, so the corpus is small: each key
below is sent to both functions. Keys are the public policy keys of
include/policy_keys.h, empty and NUL-bearing keys, wide-unit edges (lone
surrogates, beyond U+10FFFF, negative units down to INT32_MIN) and one
maximum-length key from a fixed-seed splitmix64 stream. Units are the signed 32-bit
wchar_t values of the pinned wxGTK wxString, encoded as the accepted map_helpers
adapter encodes them. `--check` verifies the committed files.
"""
import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
UNIT = "libs.kiplatform.os.unix.application.thin.replacement.p001"
CASES = ROOT / "port/cases" / (UNIT + ".jsonl")
CONTROLS = ROOT / "port/oracle/fixtures/kiplatform-policy-controls.jsonl"
OPS = ("get_policy_bool", "get_policy_enum_uint")
MAX_KEY_UNITS = 4096
INT32_MIN = -(1 << 31)
INT32_MAX = (1 << 31) - 1


def splitmix64(seed, count):
    state, words = seed, []
    for _ in range(count):
        state = (state + 0x9E3779B97F4A7C15) % (1 << 64)
        z = state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) % (1 << 64)
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) % (1 << 64)
        words.append(z ^ (z >> 31))
    return words


def signed32(word):
    """The low 32 bits of a generator word as a signed wchar_t unit."""
    low = word & 0xFFFFFFFF
    return low - (1 << 32) if low > INT32_MAX else low


def units(text):
    return [ord(c) for c in text]


def keys():
    return [
        ("empty", []),
        ("data-collection", units("DataCollection")),
        ("pcm", units("PluginAndContentManager")),
        ("curl-ssl-revoke", units("curl\\SslRevoke")),
        ("nul", [0]),
        ("embedded-nul", units("Data") + [0] + units("Collection")),
        ("trailing-nul", units("PCM") + [0]),
        ("latin-cjk-astral", [0xE9, 0x4E2D, 0x1F600]),
        ("lone-high-surrogate", [0xD800]),
        ("reversed-surrogates", [0xDFFF, 0xD800]),
        ("max-scalar", [0x10FFFF]),
        ("beyond-unicode", [0x110000]),
        ("int32-max", [INT32_MAX]),
        ("int32-min", [INT32_MIN]),
        ("minus-one", [-1]),
        ("mixed-sign", [INT32_MIN, -1, 0, 1, INT32_MAX]),
        ("maximum-length", [signed32(word) for word in splitmix64(0x504F4C4943590001, MAX_KEY_UNITS)]),
    ]


def line(value):
    return json.dumps(value, separators=(",", ":"), ensure_ascii=False, allow_nan=False)


def request(case, input):
    return {"unit": UNIT, "case": case, "input": input}


def corpus():
    lines = []
    for name, key in keys():
        for op in OPS:
            lines.append(line(request(f"{op}-{name}", {"op": op, "key": key})))
    # Integer -0 is a valid unit 0 for the C++ JSON reader; written literally.
    for op in OPS:
        lines.append('{"unit":"%s","case":"%s-negative-zero","input":{"op":"%s","key":[-0,65]}}'
                     % (UNIT, op, op))
    return lines


def controls():
    def invalid(case, message, raw):
        return {"line": raw, "reply": {"unit": UNIT, "case": case,
                                        "error": {"kind": "invalid_input", "message": message}}}

    def refuse(case, message, input):
        return invalid(case, message, line(request(case, input)))

    over = [65] * (MAX_KEY_UNITS + 1)
    result = [
        # Protocol envelope controls (shared dispatch, both sides).
        {"line": "{", "reply": {"unit": None, "case": None, "error": {
            "kind": "invalid_request", "message": "invalid JSON or duplicate key"}}},
        {"line": '{"unit":"%s","case":"refuse-duplicate","input":{"op":"get_policy_bool","op":"x","key":[]}}' % UNIT,
         "reply": {"unit": None, "case": None, "error": {
             "kind": "invalid_request", "message": "invalid JSON or duplicate key"}}},
        {"line": line({"unit": UNIT, "input": {"op": "get_policy_bool", "key": []}}),
         "reply": {"unit": UNIT, "case": None, "error": {
             "kind": "invalid_request", "message": "expected unit, case, input object"}}},
        {"line": line({"unit": UNIT, "case": "refuse-input-array", "input": []}),
         "reply": {"unit": UNIT, "case": "refuse-input-array", "error": {
             "kind": "invalid_request", "message": "expected unit, case, input object"}}},
        {"line": line({"unit": UNIT, "case": "refuse-extra", "input": {"op": "get_policy_bool", "key": []},
                       "extra": 1}),
         "reply": {"unit": UNIT, "case": "refuse-extra", "error": {
             "kind": "invalid_request", "message": "expected unit, case, input object"}}},
        {"line": line({"unit": UNIT + ".p999", "case": "refuse-unit", "input": {}}),
         "reply": {"unit": UNIT + ".p999", "case": "refuse-unit", "error": {
             "kind": "unknown_unit", "message": "unit has no registered shim"}}},
        # Operation input controls.
        refuse("refuse-missing-key", "unexpected or missing field", {"op": "get_policy_bool"}),
        refuse("refuse-missing-op", "unexpected or missing field", {"key": []}),
        refuse("refuse-extra-field", "unexpected or missing field",
               {"op": "get_policy_bool", "key": [], "value": 1}),
        refuse("refuse-op-number", "expected op string", {"op": 1, "key": []}),
        refuse("refuse-key-string", "expected key array", {"op": "get_policy_bool", "key": "PCM"}),
        refuse("refuse-key-object", "expected key array", {"op": "get_policy_enum_uint", "key": {}}),
        refuse("refuse-key-too-long", "key longer than 4096 units", {"op": "get_policy_bool", "key": over}),
        refuse("refuse-unit-below-int32", "key unit out of range",
               {"op": "get_policy_bool", "key": [INT32_MIN - 1]}),
        refuse("refuse-unit-above-int32", "key unit out of range",
               {"op": "get_policy_enum_uint", "key": [INT32_MAX + 1]}),
        refuse("refuse-unit-2-32", "key unit out of range", {"op": "get_policy_enum_uint", "key": [1 << 32]}),
        refuse("refuse-unit-minus-2-63", "key unit out of range",
               {"op": "get_policy_bool", "key": [-(1 << 63)]}),
        refuse("refuse-unit-2-64", "key unit out of range", {"op": "get_policy_bool", "key": [1 << 64]}),
        refuse("refuse-unit-float", "key unit out of range", {"op": "get_policy_bool", "key": [1.0]}),
        refuse("refuse-unit-bool", "key unit out of range", {"op": "get_policy_enum_uint", "key": [True]}),
        refuse("refuse-unit-null", "key unit out of range", {"op": "get_policy_bool", "key": [None]}),
        refuse("refuse-unit-string", "key unit out of range", {"op": "get_policy_bool", "key": ["A"]}),
        invalid("refuse-unit-exponent", "key unit out of range",
                '{"unit":"%s","case":"refuse-unit-exponent","input":{"op":"get_policy_bool","key":[1e2]}}' % UNIT),
        refuse("refuse-unknown-op", "unknown operation", {"op": "GetPolicyEnum", "key": []}),
        # The key is validated before the operation name.
        refuse("refuse-unknown-op-bad-key", "key unit out of range",
               {"op": "GetPolicyEnum", "key": [INT32_MAX + 1]}),
    ]
    return [line(control) for control in result]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    cases, refusals = corpus(), controls()
    outputs = [(CASES, "".join(l + "\n" for l in cases)), (CONTROLS, "".join(l + "\n" for l in refusals))]
    if args.check:
        if any(not path.exists() or path.read_bytes() != text.encode() for path, text in outputs):
            raise SystemExit("corpus differs from deterministic generator")
    else:
        for path, text in outputs:
            path.write_bytes(text.encode())
    print(json.dumps({"cases": len(cases), "controls": len(refusals)}))


if __name__ == "__main__":
    main()
