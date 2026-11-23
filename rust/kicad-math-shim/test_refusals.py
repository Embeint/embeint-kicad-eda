#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Require identical complete refusal replies from the C++ oracle and the Rust shim.

Usage: test_refusals.py ORACLE RUST

Refusals are not difftest cases (every error envelope fails a difftest run), so this script
checks them separately: both programs receive the same request bytes, every reply must be
byte-identical JSON, carry the expected kind and message, and a valid request after each
refusal must still succeed. Malformed input therefore never reaches the reference templates.
"""

import json
import subprocess
import sys

UNIT = "libs.kimath.include.math.application.pure.p001"
ONE, ZERO = "3ff0000000000000", "0000000000000000"
IDENTITY = [[ONE, ZERO, ZERO], [ZERO, ONE, ZERO], [ZERO, ZERO, ONE]]
INT64_MIN = -(1 << 63)
VALID = {"op": "determinant", "matrix": IDENTITY}


def probes():
    """(input, expected message); the kind is always invalid_input."""
    bad_doubles = [1.0, 1, None, "3FF0000000000000", "3ff000000000000", "3ff00000000000000",
                   "+ff0000000000000", " 3ff000000000000", "0x3ff00000000000", "3ff0000000000g00"]
    yield {}, "expected op string"
    yield {"op": 1}, "expected op string"
    yield {"op": "nope"}, "unknown operation"
    yield {"op": "matrix_default", "x": 1}, "unexpected or missing field"
    yield {"op": "determinant"}, "unexpected or missing field"
    yield {"op": "determinant", "matrix": IDENTITY, "extra": 0}, "unexpected or missing field"
    for name, fields in [("matrix", {"op": "determinant"}), ("elements", {"op": "matrix_from_elements"}),
                         ("a", {"op": "mul_matrix", "b": IDENTITY}),
                         ("b", {"op": "mul_matrix", "a": IDENTITY}),
                         ("other", {"op": "compare", "matrix": IDENTITY})]:
        for shape in [[], [[ONE, ONE, ONE]] * 2, [[ONE, ONE, ONE]] * 4, [[ONE, ONE]] * 3,
                      [[ONE, ONE, ONE], [ONE, ONE, ONE], ONE], {"0": 1}, ONE]:
            yield {**fields, name: shape}, f"expected matrix: {name}"
        for bad in bad_doubles:
            yield {**fields, name: [[ONE, ONE, ONE], [ONE, bad, ONE], [ONE, ONE, ONE]]}, \
                f"expected double bit string: {name}"
    for name, fields, size in [("translation", {"op": "set_translation", "matrix": IDENTITY}, 2),
                               ("scale", {"op": "set_scale", "matrix": IDENTITY}, 2),
                               ("vector", {"op": "mul_vector2", "matrix": IDENTITY}, 2),
                               ("vector", {"op": "mul_vector3", "matrix": IDENTITY}, 3),
                               ("a1", {"op": "matrix_from_vectors", "a2": [ONE] * 3, "a3": [ONE] * 3}, 3),
                               ("a3", {"op": "matrix_from_vectors", "a1": [ONE] * 3, "a2": [ONE] * 3}, 3)]:
        for shape in [[], [ONE] * (size - 1), [ONE] * (size + 1), ONE]:
            yield {**fields, name: shape}, f"expected vector: {name}"
        yield {**fields, name: [ONE] * (size - 1) + [1.0]}, f"expected double bit string: {name}"
    for name, fields in [("angle", {"op": "set_rotation", "matrix": IDENTITY}),
                         ("scalar", {"op": "mul_scalar", "matrix": IDENTITY}),
                         ("scalar", {"op": "scalar_mul", "matrix": IDENTITY})]:
        for bad in bad_doubles:
            yield {**fields, name: bad}, f"expected double bit string: {name}"
    # Field order: a malformed earlier field is reported first.
    yield {"op": "scalar_mul", "scalar": 1, "matrix": []}, "expected double bit string: scalar"
    yield {"op": "mul_scalar", "matrix": [], "scalar": 1}, "expected matrix: matrix"
    for bad in [(1 << 63), INT64_MIN - 1, 1.0, "0", None, True]:
        yield {"op": "words_to_double", "hi": bad, "lo": 0}, "integer out of range: hi"
    for bad in [-1, 1 << 64, 1.0, "0", -0.0]:
        yield {"op": "words_to_double", "hi": 0, "lo": bad}, "integer out of range: lo"
    for bad in [1, "", "-", "-0", "00", "01", "+1", " 1", "1 ", "1e3", "0x10",
                str(1 << 127), str(-(1 << 127) - 1), "9" * 50]:
        yield {"op": "to_double", "value": bad}, "expected int128 string: value"
    for op in ["cross_wide", "dot_wide"]:
        yield {"op": op, "a": [0], "b": [0, 0]}, "expected vector: a"
        yield {"op": op, "a": [0, 0], "b": [0, 0, 0]}, "expected vector: b"
        yield {"op": op, "a": [0, 1 << 63], "b": [0, 0]}, "integer out of range: a"
        yield {"op": op, "a": [0, 0], "b": [0.0, 0]}, "integer out of range: b"
    yield {"op": "dot_wide", "a": [INT64_MIN] * 2, "b": [INT64_MIN] * 2}, "DotWide overflows KI_INT128"
    yield from integer_and_stream_probes()


INT_MIN, INT_MAX = -(1 << 31), (1 << 31) - 1
QA = [[1, 1, 1], [2, 2, 2], [3, 3, 3]]
GOOD_STATE = {"flags": ["fixed"], "precision": 2, "width": 0, "fill": " "}


def integer_and_stream_probes():
    """Modifier validation, MATRIX3x3<int> undefined behaviour and stream state refusals."""
    for bad in ["float", "", 1, None, ["int"]]:
        yield {"op": "transpose", "type": bad, "matrix": IDENTITY}, "expected type: double or int"
        yield {"op": "mul_scalar", "scalar_type": bad, "matrix": IDENTITY, "scalar": ONE}, \
            "expected scalar_type: double or int"
    yield {"op": "words_to_double", "type": "int", "hi": 0, "lo": 0}, "unexpected or missing field"
    yield {"op": "determinant", "scalar_type": "int", "matrix": IDENTITY}, "unexpected or missing field"
    yield {"op": "transpose", "state": GOOD_STATE, "matrix": IDENTITY}, "unexpected or missing field"
    # Integer element validation.
    for bad in [INT_MAX + 1, INT_MIN - 1, 1.0, "1", None, -0.0]:
        yield {"op": "transpose", "type": "int", "matrix": [[0, 0, 0], [0, bad, 0], [0, 0, 0]]}, \
            "integer out of range: matrix"
    yield {"op": "transpose", "type": "int", "matrix": IDENTITY}, "integer out of range: matrix"
    yield {"op": "set_rotation", "type": "int", "matrix": QA, "angle": ONE}, \
        "integer out of range: angle"
    yield {"op": "mul_scalar", "type": "int", "scalar_type": "int", "matrix": QA, "scalar": ONE}, \
        "integer out of range: scalar"
    yield {"op": "mul_scalar", "type": "int", "matrix": QA, "scalar": 5}, \
        "expected double bit string: scalar"
    yield {"op": "mul_scalar", "scalar_type": "int", "matrix": IDENTITY, "scalar": INT_MAX + 1}, \
        "integer out of range: scalar"
    # Signed overflow (undefined behaviour) in each MATRIX3x3<int> operation.
    big = [[INT_MAX, 0, 0], [0, 2, 0], [0, 0, 1]]
    yield {"op": "determinant", "type": "int", "matrix": big}, \
        "MATRIX3x3<int>::Determinant overflows int"
    yield {"op": "determinant", "type": "int", "matrix": [[1, 0, 0], [0, 65536, 0], [0, 0, 65536]]}, \
        "MATRIX3x3<int>::Determinant overflows int"
    yield {"op": "determinant", "type": "int",
           "matrix": [[INT_MIN, 0, 0], [0, -1, 0], [0, 0, 1]]}, \
        "MATRIX3x3<int>::Determinant overflows int"
    yield {"op": "inverse", "type": "int", "matrix": [[1, 0, 0], [0, 65536, 0], [0, 0, 65536]]}, \
        "MATRIX3x3<int>::Inverse overflows int"
    yield {"op": "inverse", "type": "int", "matrix": big}, "MATRIX3x3<int>::Inverse overflows int"
    yield {"op": "inverse", "type": "int", "matrix": [[2, 0, 0], [0, 46340, 0], [0, 0, 46340]]}, \
        "MATRIX3x3<int>::Determinant overflows int"
    yield {"op": "inverse", "type": "int", "matrix": [[1, 2, 3], [2, 4, 6], [1, 1, 1]]}, \
        "operator*( MATRIX3x3<int>, double ) converts a value outside int"
    yield {"op": "inverse", "type": "int", "matrix": [[0] * 3] * 3}, \
        "operator*( MATRIX3x3<int>, double ) converts a value outside int"
    yield {"op": "mul_matrix", "type": "int", "a": big, "b": big}, \
        "operator*( MATRIX3x3<int>, MATRIX3x3<int> ) overflows int"
    yield {"op": "mul_matrix", "type": "int", "a": [[INT_MAX, 1, 0], [0, 1, 0], [0, 0, 1]],
           "b": [[1, 0, 0], [1, 1, 0], [0, 0, 1]]}, \
        "operator*( MATRIX3x3<int>, MATRIX3x3<int> ) overflows int"
    yield {"op": "mul_vector2", "type": "int", "matrix": [[1, 0, INT_MAX], [0, 1, 0], [0, 0, 1]],
           "vector": [1, 0]}, "operator*( MATRIX3x3<int>, VECTOR2<int> ) overflows int"
    yield {"op": "mul_vector3", "type": "int", "matrix": big, "vector": [2, 0, 0]}, \
        "operator*( MATRIX3x3<int>, VECTOR3<int> ) overflows int"
    yield {"op": "mul_scalar", "type": "int", "scalar_type": "int", "matrix": big, "scalar": 2}, \
        "operator*( MATRIX3x3<int>, int ) overflows int"
    yield {"op": "scalar_mul", "type": "int", "scalar_type": "int", "scalar": -1,
           "matrix": [[INT_MIN, 0, 0], [0, 1, 0], [0, 0, 1]]}, "operator*( MATRIX3x3<int>, int ) overflows int"
    # Non-representable double-to-int conversions (undefined behaviour).
    for scalar in ["7ff8000000000000", "7ff0000000000000", "41e0000000000000", "c1e0000000200000"]:
        yield {"op": "mul_scalar", "type": "int", "matrix": QA, "scalar": scalar}, \
            "operator*( MATRIX3x3<int>, double ) converts a value outside int"
        yield {"op": "scalar_mul", "type": "int", "scalar": scalar, "matrix": QA}, \
            "operator*( MATRIX3x3<int>, double ) converts a value outside int"
    # Stream state shape and resource bounds.
    for bad in [None, [], {}, {**GOOD_STATE, "extra": 1}, {k: v for k, v in GOOD_STATE.items() if k != "fill"},
                {**GOOD_STATE, "flags": "fixed"}, {**GOOD_STATE, "flags": ["fixed", "fixed"]},
                {**GOOD_STATE, "flags": ["bogus"]}, {**GOOD_STATE, "flags": [1]},
                {**GOOD_STATE, "fill": ""}, {**GOOD_STATE, "fill": "ab"}, {**GOOD_STATE, "fill": "\u00e9"},
                {**GOOD_STATE, "fill": "\n"}, {**GOOD_STATE, "fill": 32}]:
        yield {"op": "stream", "matrix": IDENTITY, "state": bad}, "expected stream state"
    for name in ["precision", "width"]:
        for bad in [513, -65, 1.0, "2", None, 1 << 63]:
            yield {"op": "stream", "matrix": IDENTITY, "state": {**GOOD_STATE, name: bad}}, \
                f"integer out of range: {name}"
    yield {"op": "stream", "type": "int", "matrix": IDENTITY, "state": GOOD_STATE}, \
        "integer out of range: matrix"


def run(program, text):
    completed = subprocess.run([program], input=text.encode(), capture_output=True, timeout=60)
    if completed.returncode != 0:
        raise SystemExit(f"{program} exited {completed.returncode}: {completed.stderr.decode()}")
    return completed.stdout.decode().splitlines()


def main():
    oracle, rust = sys.argv[1:3]
    lines, expected = [], []
    for index, (input_fields, message) in enumerate(probes()):
        lines.append(json.dumps({"unit": UNIT, "case": f"refusal-{index}", "input": input_fields}))
        expected.append(("invalid_input", message))
        lines.append(json.dumps({"unit": UNIT, "case": f"recovered-{index}", "input": VALID}))
        expected.append(None)
    text = "\n".join(lines) + "\n"
    replies = {name: run(program, text) for name, program in (("oracle", oracle), ("rust", rust))}
    failures = 0
    for index, want in enumerate(expected):
        cpp, rs = (json.loads(replies[name][index]) for name in ("oracle", "rust"))
        if cpp != rs:
            failures += 1
            print(f"differs: {lines[index]}\n  oracle {cpp}\n  rust   {rs}")
        elif want is None and "result" not in cpp:
            failures += 1
            print(f"not recovered: {cpp}")
        elif want is not None and (cpp.get("error", {}).get("kind"), cpp.get("error", {}).get("message")) != want:
            failures += 1
            print(f"unexpected refusal: {lines[index]}\n  {cpp}")
    count = len(expected) // 2
    print(json.dumps({"refusal_probes": count, "requests": len(expected), "failures": failures}))
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
