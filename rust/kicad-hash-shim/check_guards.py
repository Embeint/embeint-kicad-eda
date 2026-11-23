#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Malformed-request guards for libs.kimath.include.application.pure.p001.

Refusals cannot enter the case corpus (every error envelope fails a difftest), so
they are checked here: each literal request has an expected complete reply, and
the oracle and the Rust shim must both produce it (compared as parsed JSON, so
key order and whitespace are not significant).

fixtures/mmh3-unwritten-reads.{requests,replies}.jsonl holds the 15 original
corpus requests whose reference execution would read never-written MMH3 block
bytes (port/findings/kimath-header-hash-mmh3-unwritten-reads.md); both programs
must refuse each before the read.

    python3 rust/kicad-hash-shim/check_guards.py ORACLE [RUST_SHIM]
"""
import json
import pathlib
import subprocess
import sys

FIXTURES = pathlib.Path(__file__).resolve().parent / "fixtures"

UNIT = "libs.kimath.include.application.pure.p001"
PARSE = {"kind": "invalid_request", "message": "invalid JSON or duplicate key"}


def wire(case, input_text, unit=None):
    unit_text = json.dumps(UNIT) if unit is None else unit
    return '{"unit":' + unit_text + ',"case":' + json.dumps(case) + ',"input":' + input_text + '}'


def refusal(case, message):
    return {"unit": UNIT, "case": case, "error": {"kind": "invalid_input", "message": message}}


def checks():
    out = []
    refusals = [
        ("no-op", '{}', "expected op string"),
        ("op-not-string", '{"op":1}', "expected op string"),
        ("unknown-op", '{"op":"md5"}', "unknown operation"),
        ("std-hash-extra-key", '{"op":"std_hash","values":[],"x":1}', "unexpected or missing field"),
        ("std-hash-values-object", '{"op":"std_hash","values":{}}', "expected values array"),
        ("std-hash-65-values", '{"op":"std_hash","values":[' + ",".join(['{"type":"bool","value":true}'] * 65) + ']}',
         "too many values"),
        ("value-not-object", '{"op":"std_hash","values":[1]}', "invalid value type"),
        ("value-unknown-type", '{"op":"std_hash","values":[{"type":"char","value":1}]}', "invalid value type"),
        ("value-type-not-string", '{"op":"std_hash","values":[{"type":1,"value":1}]}', "invalid value type"),
        ("value-extra-key", '{"op":"std_hash","values":[{"type":"i32","value":1,"x":0}]}', "unexpected or missing field"),
        ("i8-overflow", '{"op":"std_hash","values":[{"type":"i8","value":128}]}', "integer out of range: value"),
        ("u8-negative", '{"op":"std_hash","values":[{"type":"u8","value":-1}]}', "integer out of range: value"),
        ("i32-float", '{"op":"std_hash","values":[{"type":"i32","value":1.0}]}', "integer out of range: value"),
        ("i32-string", '{"op":"std_hash","values":[{"type":"i32","value":"1"}]}', "integer out of range: value"),
        ("i64-overflow", '{"op":"std_hash","values":[{"type":"i64","value":9223372036854775808}]}',
         "integer out of range: value"),
        ("u64-overflow", '{"op":"std_hash","values":[{"type":"u64","value":18446744073709551616}]}',
         "integer out of range: value"),
        ("u64-negative", '{"op":"std_hash","values":[{"type":"u64","value":-1}]}', "integer out of range: value"),
        ("bool-integer", '{"op":"std_hash","values":[{"type":"bool","value":1}]}', "expected boolean: value"),
        ("f64-short-bits", '{"op":"std_hash","values":[{"type":"f64","bits":"3ff0"}]}', "invalid hex: bits"),
        ("f32-upper-hex", '{"op":"std_hash","values":[{"type":"f32","bits":"3F800000"}]}', "invalid hex: bits"),
        ("f64-number-bits", '{"op":"std_hash","values":[{"type":"f64","bits":1}]}', "invalid hex: bits"),
        ("string-odd-hex", '{"op":"std_hash","values":[{"type":"string","hex":"abc"}]}', "invalid hex: hex"),
        ("string-too-long", '{"op":"std_hash","values":[{"type":"string","hex":"' + "00" * 4097 + '"}]}',
         "invalid hex: hex"),
        ("combine-4-values", '{"op":"hash_combine","seed":0,"values":[' + ",".join(['{"type":"i32","value":1}'] * 4) + ']}',
         "too many values"),
        ("combine-3-unsupported", '{"op":"hash_combine","seed":0,"values":[{"type":"i32","value":1},'
         '{"type":"u8","value":1},{"type":"i32","value":1}]}', "unsupported value type for three values"),
        ("val-3-unsupported", '{"op":"hash_val","values":[{"type":"f32","bits":"00000000"},'
         '{"type":"i32","value":1},{"type":"i32","value":1}]}', "unsupported value type for three values"),
        ("combine-seed-negative", '{"op":"hash_combine","seed":-1,"values":[]}', "integer out of range: seed"),
        ("combine-missing-seed", '{"op":"hash_combine","values":[]}', "unexpected or missing field"),
        ("each-257-values", '{"op":"hash_combine_each","seed":0,"values":[' + ",".join(['{"type":"i32","value":1}'] * 257) + ']}',
         "too many values"),
        ("hash128-short", '{"op":"hash128","a":"00","b":"' + "00" * 16 + '"}', "invalid hex: a"),
        ("hash128-long-b", '{"op":"hash128","a":"' + "00" * 16 + '","b":"' + "00" * 17 + '"}', "invalid hex: b"),
        ("mmh3-no-steps", '{"op":"mmh3"}', "unexpected or missing field"),
        ("mmh3-empty-steps", '{"op":"mmh3","steps":[]}', "steps outside 1..4096"),
        ("mmh3-4097-steps", '{"op":"mmh3","steps":[' + ",".join(['{"op":"new"}'] * 4097) + ']}', "steps outside 1..4096"),
        ("mmh3-step-not-object", '{"op":"mmh3","steps":[1]}', "expected op string"),
        ("mmh3-first-not-new", '{"op":"mmh3","steps":[{"op":"digest"}]}', "first step must be new"),
        ("mmh3-seed-overflow", '{"op":"mmh3","steps":[{"op":"new","seed":4294967296}]}', "integer out of range: seed"),
        ("mmh3-reset-seed-negative", '{"op":"mmh3","steps":[{"op":"new"},{"op":"reset","seed":-1}]}',
         "integer out of range: seed"),
        ("mmh3-data-upper-hex", '{"op":"mmh3","steps":[{"op":"new"},{"op":"data","hex":"AB"}]}', "invalid hex: hex"),
        ("mmh3-data-too-long", '{"op":"mmh3","steps":[{"op":"new"},{"op":"data","hex":"' + "00" * 65537 + '"}]}',
         "invalid hex: hex"),
        ("mmh3-repeat-4097", '{"op":"mmh3","steps":[{"op":"new"},{"op":"data","hex":"00","repeat":4097}]}',
         "integer out of range: repeat"),
        ("mmh3-budget", '{"op":"mmh3","steps":[{"op":"new"},' + ",".join(
            ['{"op":"data","hex":"' + "00" * 65536 + '","repeat":4096}'] * 2) + ']}', "byte budget exceeded"),
        ("mmh3-int-overflow", '{"op":"mmh3","steps":[{"op":"new"},{"op":"int","value":2147483648}]}',
         "integer out of range: value"),
        ("mmh3-digest-extra", '{"op":"mmh3","steps":[{"op":"new"},{"op":"digest","x":0}]}', "unexpected or missing field"),
        ("mmh3-getblock64-2", '{"op":"mmh3","steps":[{"op":"new"},{"op":"getblock64","index":2}]}',
         "integer out of range: index"),
        ("mmh3-getblock64-negative", '{"op":"mmh3","steps":[{"op":"new"},{"op":"getblock64","index":-1}]}',
         "integer out of range: index"),
        ("mmh3-state-len-overflow", '{"op":"mmh3","steps":[{"op":"new"},{"op":"state","h1":0,"h2":0,"blocks":"'
         + "00" * 16 + '","len":4294967296}]}', "integer out of range: len"),
        ("mmh3-state-short-blocks", '{"op":"mmh3","steps":[{"op":"new"},{"op":"state","h1":0,"h2":0,"blocks":"00",'
         '"len":0}]}', "invalid hex: blocks"),
        ("mmh3-unknown-step", '{"op":"mmh3","steps":[{"op":"new"},{"op":"addData"}]}', "unknown operation"),
        ("rotl-zero", '{"op":"rotl","x":1,"r":0}', "integer out of range: r"),
        ("rotl-64", '{"op":"rotl","x":1,"r":64}', "integer out of range: r"),
        ("rotl-negative", '{"op":"rotl","x":1,"r":-1}', "integer out of range: r"),
        ("rotl-x-negative", '{"op":"rotl","x":-1,"r":1}', "integer out of range: x"),
        ("fmix64-float", '{"op":"fmix64","k":1.5}', "integer out of range: k"),
        ("md5-first-not-new", '{"op":"md5_valid","steps":[{"op":"inspect"}]}', "first step must be new"),
        ("md5-set-valid-integer", '{"op":"md5_valid","steps":[{"op":"new"},{"op":"set_valid","value":1}]}',
         "expected boolean: value"),
        ("md5-bytes-too-long", '{"op":"md5_valid","steps":[{"op":"new"},{"op":"bytes","hex":"' + "00" * 4097 + '"}]}',
         "invalid hex: hex"),
        ("md5-unknown-step", '{"op":"md5_valid","steps":[{"op":"new"},{"op":"clone"}]}', "unknown operation"),
        ("transform-short", '{"op":"transform","a":[1,0,0],"b":null}', "expected transform: a"),
        ("transform-object", '{"op":"transform","a":null,"b":{}}', "expected transform: b"),
        ("transform-member-overflow", '{"op":"transform","a":[2147483648,0,0,1],"b":null}', "integer out of range: a"),
        ("transform-member-float", '{"op":"transform","a":null,"b":[1,0,0,1.0]}', "integer out of range: b"),
        ("transform-missing-b", '{"op":"transform","a":null}', "unexpected or missing field"),
        # HASH_128::ToString caller locale.
        ("locale-not-object", '{"op":"hash128","a":"' + "00" * 16 + '","b":"' + "00" * 16 + '","locale":3}',
         "expected object"),
        ("locale-missing-grouping", '{"op":"hash128","a":"' + "00" * 16 + '","b":"' + "00" * 16
         + '","locale":{"thousands_sep":44}}', "unexpected or missing field"),
        ("locale-sep-256", '{"op":"hash128","a":"' + "00" * 16 + '","b":"' + "00" * 16
         + '","locale":{"thousands_sep":256,"grouping":"03"}}', "integer out of range: thousands_sep"),
        ("locale-grouping-upper-hex", '{"op":"hash128","a":"' + "00" * 16 + '","b":"' + "00" * 16
         + '","locale":{"thousands_sep":44,"grouping":"0A"}}', "invalid hex: grouping"),
        ("locale-grouping-65", '{"op":"hash128","a":"' + "00" * 16 + '","b":"' + "00" * 16
         + '","locale":{"thousands_sep":44,"grouping":"' + "03" * 65 + '"}}', "grouping of 65 bytes exceeds 64"),
        # MMH3 reads of never-written block bytes, refused before the read.
        ("unwritten-new-getblock64", '{"op":"mmh3","steps":[{"op":"new"},{"op":"getblock64","index":1}]}',
         "getblock64 reads unwritten block bytes 8,9,10,11,12,13,14,15"),
        ("unwritten-new-hash-block", '{"op":"mmh3","steps":[{"op":"new","seed":1},{"op":"hash_block"}]}',
         "hashBlock reads unwritten block bytes 0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15"),
        ("unwritten-int-hash-tail", '{"op":"mmh3","steps":[{"op":"new"},{"op":"int","value":1},'
         '{"op":"data_v1","hex":"01"},{"op":"hash_tail"}]}', "hashTail reads unwritten block bytes 4,5,6,7"),
        ("unwritten-pending-add-data", '{"op":"mmh3","steps":[{"op":"new"},{"op":"int","value":1},'
         '{"op":"data_v1","hex":"01"},{"op":"data","hex":"' + "00" * 8 + '"}]}',
         "hashBlock reads unwritten block bytes 4,5,6,7"),
        # int writes 0..3 (len 4); addDataV1( 1 byte ) rewrites 0..3 and counts len 8,
        # leaving 4..7 unwritten; ints fill 8..15 and the last completes the block.
        ("unwritten-int-completes-block", '{"op":"mmh3","steps":[{"op":"new"},{"op":"int","value":1},'
         '{"op":"data_v1","hex":"01"},{"op":"int","value":3},{"op":"int","value":4}]}',
         "hashBlock reads unwritten block bytes 4,5,6,7"),
        ("unwritten-reset-keeps-unwritten", '{"op":"mmh3","steps":[{"op":"new"},{"op":"int","value":1},'
         '{"op":"reset"},{"op":"state","h1":0,"h2":0,"blocks":"' + "00" * 16 + '","len":0},'
         '{"op":"new"},{"op":"digest"},{"op":"getblock64","index":0}]}',
         "getblock64 reads unwritten block bytes 0,1,2,3,4,5,6,7"),
    ]
    out += [(wire(case, text), refusal(case, message)) for case, text, message in refusals]

    requests = (FIXTURES / "mmh3-unwritten-reads.requests.jsonl").read_text(encoding="utf-8").splitlines()
    replies = (FIXTURES / "mmh3-unwritten-reads.replies.jsonl").read_text(encoding="utf-8").splitlines()
    if len(requests) != 15 or len(replies) != 15:
        raise SystemExit("mmh3-unwritten-reads fixture must hold the 15 original requests and replies")
    out += [(request, json.loads(reply)) for request, reply in zip(requests, replies)]

    # Shared transport refusals (kicad_difftest::request::parse and port/oracle/protocol.cpp).
    for name, text in [("overflow", '{"op":"fmix64","k":1e1000}'), ("huge-integer", '{"op":"fmix64","k":' + "9" * 400 + '}'),
                       ("nested", '{"op":"fmix64","k":0,"x":' + "[" * 130 + "]" * 130 + '}')]:
        out.append((wire(name, text), {"unit": None, "case": None, "error": PARSE}))
    out.append(('{"unit":"' + UNIT + '","case":"dup","input":{"op":"fmix64","k":0,"k":1}}',
                {"unit": None, "case": None, "error": PARSE}))
    out.append((wire("other-unit", '{"op":"fmix64","k":0}', unit='"libs.kimath.other"'),
                {"unit": "libs.kimath.other", "case": "other-unit",
                 "error": {"kind": "unknown_unit", "message": "unit has no registered shim"}}))
    out.append(('{"unit":"' + UNIT + '","case":"no-input"}',
                {"unit": UNIT, "case": "no-input",
                 "error": {"kind": "invalid_request", "message": "expected unit, case, input object"}}))

    # Accepted boundary tokens: integer -0 is zero; the largest u64 and i64 extremes.
    out.append((wire("negative-zero-k", '{"op":"fmix64","k":-0}'),
                {"unit": UNIT, "case": "negative-zero-k", "result": {"value": 0}}))
    out.append((wire("max-k", '{"op":"rotl","x":18446744073709551615,"r":63}'),
                {"unit": UNIT, "case": "max-k", "result": {"value": 18446744073709551615}}))
    return out


def run(program, requests):
    result = subprocess.run([program], input="\n".join(requests) + "\n", capture_output=True,
                            text=True, timeout=120, check=False)
    if result.returncode != 0:
        raise SystemExit(f"{program} exited {result.returncode}: {result.stderr}")
    return result.stdout.splitlines()


def main():
    if len(sys.argv) not in (2, 3):
        raise SystemExit(__doc__)
    cases = checks()
    requests = [request for request, _ in cases]
    replies = {program: run(program, requests) for program in sys.argv[1:]}
    failures = 0
    for program, lines in replies.items():
        if len(lines) != len(cases):
            raise SystemExit(f"{program}: {len(lines)} replies for {len(cases)} requests")
        for line, (request, expected) in zip(lines, cases):
            if json.loads(line) != expected:
                failures += 1
                print(f"{program}: {request[:160]}\n  expected {expected}\n  actual   {line}")
    if len(replies) == 2:
        first, second = replies.values()
        for a, b in zip(first, second):
            if json.loads(a) != json.loads(b):
                failures += 1
                print(f"oracle/Rust reply mismatch:\n  {a}\n  {b}")
    print(f"{len(cases)} guard requests, {len(replies)} program(s), {failures} failure(s)")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
