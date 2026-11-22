#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Author the literal private-marker-key request lines for the resistor adapters.

Every line is one complete JSONL request envelope.  Bodies are written as raw
text so escaped key spellings, duplicate keys and numeric tokens stay literal.
"""
import sys

UNIT = "pcb_calculator.application.pure.p001"
N = "$serde_json::private::Number"
R = "$serde_json::private::RawValue"
N_ESC = r"$serde_json::private::Number"
R_ESC = r"$serde_json::private::RawValue"
N_ALL_ESC = r"$serde_json::private::Number"

lines = []


def wire(name, body):
    lines.append('{"unit":"' + UNIT + '","case":"pk-' + str(len(lines)).zfill(3) + "-" + name
                 + '","input":' + body + "}")


def raw(text):
    lines.append(text)


def obj(key, literal):
    return '{"' + key + '":' + literal + "}"


def success(tag):
    wire("success-" + tag, '{"op":"format","values_bits":[0,4607182418800017408]}')


# Values a marker key may carry: encoded valid/invalid JSON, numeric and
# overflow spellings that must stay strings, and nonstring values.
VALUES = [
    '"0"', '"1"', '"-0"', '"18446744073709551615"', '"18446744073709551616"', '"1e1000"',
    '"bad"', '"\\"' + UNIT + '\\""', '"[0]"', '"[]"', '"{}"', '"{\\"op\\":\\"unit_scales\\"}"',
    '"{"', '""', "0", "true", "null", "[]", "{}", '{"' + N + '":"0"}',
]
MATRIX = [obj(key, value) for key in (N, R) for value in VALUES]
MULTI = [
    '{"' + N + '":"0","' + R + '":"0"}',
    '{"' + R + '":"[0]","' + N + '":"1"}',
    '{"' + N + '":"0","x":1}',
    '{"x":1,"' + R + '":"\\"u\\""}',
    '{"op":"format","' + R + '":"[0]"}',
]
ESCAPED = [obj(N_ESC, '"0"'), obj(R_ESC, '"[0]"'), obj(N_ALL_ESC, '"1"'),
           obj(R_ESC, '"\\"' + UNIT + '\\""')]
NEAR = [obj("$serde_json::private::number", '"0"'), obj("serde_json::private::Number", '"0"'),
        obj(N + " ", '"0"'), obj("$serde_json::private::RawValu", '"[0]"'),
        obj("$serde_json::private::RawValues", '"[0]"'), obj("$serde_json::private:Number", '"0"')]
FULL = MATRIX + MULTI + ESCAPED + NEAR

success("first")

# Full matrix at the highest-value positions: bits element, collector target,
# unit/case identities and the input root.
for i, marker in enumerate(FULL):
    wire("format-bits-" + str(i), '{"op":"format","values_bits":[' + marker + "]}")
success("after-format-bits")
for i, marker in enumerate(FULL):
    wire("collector-target-" + str(i), '{"op":"collector","target_bits":' + marker + ',"candidates":[]}')
success("after-collector-target")
for i, marker in enumerate(FULL):
    raw('{"unit":' + marker + ',"case":"pk-unit-' + str(i) + '","input":{"op":"unit_scales"}}')
success("after-unit")
for i, marker in enumerate(FULL):
    raw('{"unit":"' + UNIT + '","case":' + marker + ',"input":{"op":"format","values_bits":[0]}}')
success("after-case")
for i, marker in enumerate(FULL):
    wire("input-root-" + str(i), marker)
success("after-input-root")

# Representative markers at every other meaningful wire position.
REP = [
    obj(N, '"0"'), obj(N, '"1"'), obj(R, '"0"'), obj(R, '"[0]"'), obj(R, '"\\"x\\""'),
    obj(R, '"true"'), obj(R, '"{\\"value_bits\\":0,\\"name\\":\\"r\\"}"'), obj(N, '"bad"'),
    obj(R, '"{"'), obj(N, "0"), obj(N_ESC, '"0"'), '{"' + N + '":"0","' + R + '":"0"}',
]
POSITIONS = [
    ("op", '{"op":@}'),
    ("op-format", '{"op":@,"values_bits":[0]}'),
    ("format-array", '{"op":"format","values_bits":@}'),
    ("format-second", '{"op":"format","values_bits":[0,@]}'),
    ("resistance-array", '{"op":"resistance","values":@}'),
    ("resistance-item", '{"op":"resistance","values":[@]}'),
    ("resistance-value", '{"op":"resistance","values":[{"value_bits":@,"name":"r"}]}'),
    ("resistance-name", '{"op":"resistance","values":[{"value_bits":0,"name":@}]}'),
    ("resistance-parts", '{"op":"resistance","values":[{"value_bits":0,"name":"r","parts_bits":@}]}'),
    ("resistance-part", '{"op":"resistance","values":[{"value_bits":0,"name":"r","parts_bits":[0,@]}]}'),
    ("resistance-clear", '{"op":"resistance","values":[{"value_bits":0,"name":"r","clear_parts":@}]}'),
    ("calc-actions", '{"op":"calculator","actions":@}'),
    ("calc-action", '{"op":"calculator","actions":[{"op":"state"},@]}'),
    ("calc-action-op", '{"op":"calculator","actions":[{"op":@}]}'),
    ("set-series", '{"op":"calculator","actions":[{"op":"set_series","series":@}]}'),
    ("new-target", '{"op":"calculator","actions":[{"op":"new","target_bits":@}]}'),
    ("exclude-value", '{"op":"calculator","actions":[{"op":"new","target_bits":4607182418800017408},'
     '{"op":"exclude","values_bits":[@]}]}'),
    ("find-value", '{"op":"calculator","actions":[{"op":"new","target_bits":4607182418800017408},'
     '{"op":"prepare"},{"op":"find","values_bits":[@]}]}'),
    ("solution-level", '{"op":"calculator","actions":[{"op":"new","target_bits":4607182418800017408},'
     '{"op":"prepare"},{"op":"solution","level":@}]}'),
    ("build-series", '{"op":"calculator","actions":[{"op":"build","series":@}]}'),
    ("build-values", '{"op":"calculator","actions":[{"op":"build_values","values":[100,@]}]}'),
    ("series-selector", '{"op":"series","series":@,"decades":[]}'),
    ("series-decades", '{"op":"series","series":0,"decades":@}'),
    ("series-decade", '{"op":"series","series":0,"decades":[0,@]}'),
    ("collector-candidates", '{"op":"collector","target_bits":0,"candidates":@}'),
    ("collector-candidate", '{"op":"collector","target_bits":0,"candidates":[@]}'),
    ("candidate-value", '{"op":"collector","target_bits":0,"candidates":[{"value_bits":@,"name":"c"}]}'),
    ("candidate-name", '{"op":"collector","target_bits":0,"candidates":[{"value_bits":0,"name":@}]}'),
    ("candidate-parts", '{"op":"collector","target_bits":0,"candidates":[{"value_bits":0,"name":"c","parts_bits":@}]}'),
    ("candidate-part", '{"op":"collector","target_bits":0,"candidates":[{"value_bits":0,"name":"c","parts_bits":[@]}]}'),
    ("candidate-clear", '{"op":"collector","target_bits":0,"candidates":[{"value_bits":0,"name":"c","clear_parts":@}]}'),
    ("reader-text", '{"op":"reader","readers":[{"text":@,"source":"s"}],"parsers":[],"actions":[]}'),
    ("reader-source", '{"op":"reader","readers":[{"text":"t","source":@}],"parsers":[],"actions":[]}'),
    ("reader-parser", '{"op":"reader","readers":[{"text":"t","source":"s"}],"parsers":[@],"actions":[]}'),
    ("reader-action-parser", '{"op":"reader","readers":[{"text":"t","source":"s"}],"parsers":[0],'
     '"actions":[{"op":"get","parser":@}]}'),
    ("reader-bind", '{"op":"reader","readers":[{"text":"t","source":"s"}],"parsers":[null],'
     '"actions":[{"op":"bind","parser":0,"reader":@}]}'),
    ("helpers-first", '{"op":"helpers","first":@,"second":{"value_bits":0,"name":"b"},"text":"t","symbol":"+"}'),
    ("helpers-text", '{"op":"helpers","first":{"value_bits":0,"name":"a"},"second":{"value_bits":0,"name":"b"},'
     '"text":@,"symbol":"+"}'),
    ("helpers-symbol", '{"op":"helpers","first":{"value_bits":0,"name":"a"},"second":{"value_bits":0,"name":"b"},'
     '"text":"t","symbol":@}'),
]
for name, template in POSITIONS:
    for i, marker in enumerate(REP):
        wire(name + "-" + str(i), template.replace("@", marker))
    success("after-" + name)

# Marker keys as extra fields beside otherwise valid objects.
EXTRA = [('"' + N + '":"0"'), ('"' + R + '":"[0]"'), ('"' + N_ESC + '":"0"')]
for i, member in enumerate(EXTRA):
    raw('{"unit":"' + UNIT + '","case":"pk-envelope-extra-' + str(i) + '","input":{"op":"unit_scales"},'
        + member + "}")
    raw("{" + member + ',"unit":"' + UNIT + '","case":"pk-envelope-lead-' + str(i)
        + '","input":{"op":"unit_scales"}}')
    wire("input-extra-" + str(i), '{"op":"format","values_bits":[0],' + member + "}")
    wire("resistance-extra-" + str(i), '{"op":"resistance","values":[{"value_bits":0,"name":"r",' + member + "}]}")
    wire("action-extra-" + str(i), '{"op":"calculator","actions":[{"op":"state",' + member + "}]}")
    wire("candidate-extra-" + str(i), '{"op":"collector","target_bits":0,"candidates":[{"value_bits":0,'
         '"name":"c",' + member + "}]}")
success("after-extra")

# Accepted positions: unknown units carry arbitrary inputs; names echo strings.
for i, marker in enumerate([obj(N, '"0"'), obj(R, '"[0]"'), obj(N, '"bad"'), obj(R, '"{"'), obj(N_ESC, "0")]):
    raw('{"unit":"unknown","case":"pk-unknown-unit-' + str(i) + '","input":{"op":"format","values_bits":['
        + marker + '],"nested":[{"deep":' + marker + "}]}}")
for i, text in enumerate([N, R, "\\u0024serde_json::private::Number", '{\\"' + N + '\\":\\"0\\"}',
                          '{\\"' + R + '\\":\\"[0]\\"}', "1e1000", "18446744073709551616", "-0", "[0]"]):
    wire("name-string-" + str(i), '{"op":"resistance","values":[{"value_bits":0,"name":"' + text + '"}]}')
success("after-names")

# Plain strings, including numeric/overflow/JSON-looking ones, remain strings.
for i, text in enumerate(["0", "1e1000", "18446744073709551616", "-0", "[0]", N, '{\\"' + N + '\\":\\"0\\"}']):
    wire("string-bits-" + str(i), '{"op":"format","values_bits":["' + text + '"]}')
    wire("string-target-" + str(i), '{"op":"collector","target_bits":"' + text + '","candidates":[]}')
    wire("string-series-" + str(i), '{"op":"calculator","actions":[{"op":"set_series","series":"' + text + '"}]}')
success("after-strings")

# Duplicate keys remain refusals, including escaped spellings of the same key.
DUPS = [
    '{"' + N + '":"0","' + N + '":"1"}',
    '{"' + N + '":"0","' + N_ESC + '":"0"}',
    '{"' + R + '":"[0]","' + R_ESC + '":"[0]"}',
    '{"' + N_ALL_ESC + '":"0","' + N + '":"0"}',
]
for i, dup in enumerate(DUPS):
    wire("duplicate-bits-" + str(i), '{"op":"format","values_bits":[' + dup + "]}")
    raw('{"unit":' + dup + ',"case":"pk-duplicate-unit-' + str(i) + '","input":{}}')
    raw('{"unit":"' + UNIT + '","case":"pk-duplicate-root-' + str(i) + '","input":{},"' + N + '":"0","'
        + N + '":"0"}')
    raw(dup)
success("after-duplicates")

# Literal number tokens inside marker objects keep the transport rules.
for i, token in enumerate(["1e1000", "1" + "0" * 399, "1.7976931348623159e308"]):
    wire("marker-overflow-" + str(i), '{"op":"format","values_bits":[' + obj(N, token) + "]}")
    raw('{"unit":' + obj(R, token) + ',"case":"pk-marker-overflow-unit","input":{}}')
for i, token in enumerate(["-0", "18446744073709551615", "18446744073709551616", "1e-1000", "-0.0"]):
    wire("marker-finite-" + str(i), '{"op":"format","values_bits":[' + obj(N, token) + "]}")
    wire("marker-finite-collector-" + str(i), '{"op":"collector","target_bits":' + obj(R, token)
         + ',"candidates":[]}')
success("after-marker-numbers")

# Literal number controls beside the marker cases.
wire("literal-minus-zero", '{"op":"format","values_bits":[-0]}')
wire("literal-full-u64", '{"op":"format","values_bits":[18446744073709551615]}')
wire("literal-u64-overflow", '{"op":"format","values_bits":[18446744073709551616]}')
wire("literal-underflow", '{"op":"format","values_bits":[1e-1000]}')
wire("literal-negative-zero-float", '{"op":"format","values_bits":[-0.0]}')
wire("literal-finite-max", '{"op":"format","values_bits":[1.7976931348623157e308]}')
wire("literal-infinite", '{"op":"format","values_bits":[1e1000]}')
wire("collector-empty", '{"op":"collector","target_bits":0,"candidates":[]}')
wire("collector-target-null", '{"op":"collector","target_bits":null,"candidates":[]}')
wire("calculator-series", '{"op":"calculator","actions":[{"op":"set_series","series":-0},{"op":"state"}]}')
success("last")

open(sys.argv[1], "wb").write(("\n".join(lines) + "\n").encode())
