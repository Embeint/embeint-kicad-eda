#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Compare complete native refusal envelopes and raw-token boundaries live."""
import argparse
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "port/tools"))
from check_goldens_policy import exact

UNIT = "pcb_calculator.application.pure.p001"


def corpus():
    cases = []

    def add(name, value, message=None, kind="invalid_input", null_identity=False, raw=False, result=None):
        body = value if raw else json.dumps(value, separators=(",", ":"))
        wire = '{"unit":"' + UNIT + '","case":"' + name + '","input":' + body + '}'
        expected = None if message is None else {
            "unit": None if null_identity else UNIT, "case": None if null_identity else name,
            "error": {"kind": kind, "message": message}}
        if result is not None:
            expected = {"unit": UNIT, "case": name, "result": result}
        cases.append((wire, expected))

    add("missing-op", {}, "missing field")
    for value in [None, False, 3, [], {}]:
        add("wrong-op-" + str(len(cases)), {"op": value}, "expected string")
    add("reader-text-bool", {"op":"reader", "readers":[{"text":False,"source":"public"}],
        "parsers":[],"actions":[]}, "expected string")
    add("clear-parts-number", {"op":"resistance", "values":[{"value_bits":0,"name":"zero","clear_parts":1}]}, "expected boolean")
    add("initial-solution-missing-level", {"op":"calculator", "actions":[{"op":"solution"}]}, "missing field")
    add("initial-action-wrong-type", {"op":"calculator", "actions":[1]}, "expected object")
    add("initial-solution-wrong-level", {"op":"calculator", "actions":[{"op":"solution","level":False}]}, "integer out of range: level")
    add("resistance-validation-order", {"op":"resistance","values":[{"value_bits":False,"name":False}]}, "expected u64 IEEE754 bits")
    add("collector-validation-order", {"op":"collector","target_bits":0,"candidates":[
        {"value_bits":0,"name":"bad-order","parts_bits":[0x7ff8000000000001,0,0x3ff0000000000000]},
        {"value_bits":False,"name":"later"}]}, "parts violate sort precondition")
    for operation, field in [("format","values_bits"),("series","decades"),("resistance","values"),("calculator","actions")]:
        value={"op":operation,field:False}
        if operation=="series": value["series"]=0
        add("wrong-array-"+operation,value,"array bound exceeded or wrong type")
    for value in [None, False, [], {}, "0", -1, 1.0]:
        add("wrong-bits-"+str(len(cases)),{"op":"format","values_bits":[value]},"expected u64 IEEE754 bits")
        add("collector-target-"+str(len(cases)),
            {"op":"collector","target_bits":value,"candidates":[]},"expected u64 IEEE754 bits")
    add("collector-empty-result", {"op":"collector","target_bits":0,"candidates":[]},
        result={"exception":"Empty solution collector"})
    add("literal-minus-zero-bits", '{"op":"format","values_bits":[-0]}', raw=True)
    add("literal-minus-zero-series", '{"op":"calculator","actions":[{"op":"set_series","series":-0}]}', raw=True)
    add("full-u64", '{"op":"resistance","values":[{"value_bits":18446744073709551615,"name":"u64"}]}', raw=True)
    add("full-u32", '{"op":"calculator","actions":[{"op":"set_series","series":4294967295}]}', raw=True)
    for i, token in enumerate(["-0.0","18446744073709551616","-9223372036854775809","1e308","1e-1000",
            "1.7976931348623157e308","1.7976931348623158e308","1"+"0"*399+"e-100"]):
        add("finite-non-u64-"+str(i),'{"op":"format","values_bits":['+token+']}',"expected u64 IEEE754 bits",raw=True)
    for i, token in enumerate(["1e309","1e1000","1"+"0"*399,"1.7976931348623159e308"]):
        add("overflow-before-identity-"+str(i),'{"op":"format","values_bits":['+token+']}',
            "invalid JSON or duplicate key","invalid_request",True,True)
        null_reply = {"unit":None,"case":None,"error":{
            "kind":"invalid_request","message":"invalid JSON or duplicate key"}}
        for wire in [
            '{"unit":'+token+',"case":"overflow","input":{}}',
            '{"unit":"'+UNIT+'","case":'+token+',"input":{}}',
            '{"unit":"unknown","case":"overflow","input":{"op":"unknown","nested":[{"value":'+token+'}]}}',
            '{"unit":"'+UNIT+'","case":"overflow","input":{},"extra":'+token+'}',
            token,
        ]:
            cases.append((wire,null_reply))
    # Finite numeric fallback is accepted even outside integer storage ranges.
    add("finite-unused-number",'{"op":"unknown","number":18446744073709551616}',"unknown operation",raw=True)
    add("finite-long-token",'{"op":"unknown","number":'+'1'+'0'*399+'e-100}',"unknown operation",raw=True)
    add("valid-after-refusals",{"op":"unit_scales"})
    # Literal serde private-marker-key envelopes with replies recorded from the
    # pinned C++ adapter; port/oracle/test_resistor_private_keys.py pins their bytes.
    fixtures=ROOT/"port/oracle/fixtures"
    requests=(fixtures/"resistor-private-keys.requests.jsonl").read_text(encoding="utf-8").split("\n")[:-1]
    replies=(fixtures/"resistor-private-keys.replies.jsonl").read_text(encoding="utf-8").split("\n")[:-1]
    assert len(requests)==len(replies)==889
    cases.extend(zip(requests,replies))
    escaped_requests=(fixtures/"resistor-private-keys-escaped.requests.jsonl").read_text(encoding="utf-8").split("\n")[:-1]
    escaped_replies=(fixtures/"resistor-private-keys-escaped.replies.jsonl").read_text(encoding="utf-8").split("\n")[:-1]
    assert len(escaped_requests)==len(escaped_replies)==199
    cases.extend(zip(escaped_requests,escaped_replies))
    return cases


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--oracle",type=Path,required=True)
    parser.add_argument("--rust",type=Path,action="append",required=True)
    args=parser.parse_args()
    cases=corpus()
    data=("\n".join(wire for wire,_ in cases)+"\n").encode()
    outputs=[]
    for binary in [args.oracle,*args.rust]:
        run=subprocess.run([str(binary.resolve())],input=data,capture_output=True,check=True,timeout=60)
        assert not run.stderr, run.stderr
        lines=run.stdout.decode().splitlines()
        assert len(lines)==len(cases), (binary,len(lines),len(cases))
        for line,(_,expected) in zip(lines,cases):
            if expected is not None:
                expected_text=expected if isinstance(expected,str) else json.dumps(expected)
                assert exact(line)==exact(expected_text), (binary,line,expected)
        outputs.append([exact(line) for line in lines])
    for binary,output in zip(args.rust,outputs[1:]):
        for (wire,_),oracle,rust in zip(cases,outputs[0],output):
            assert oracle==rust,(binary,wire,oracle,rust)
    print(f"{len(cases)} complete replies match: raw -0/full-u64, typed diagnostics, numeric overflow/fallback, "
          "private marker keys and recovery")


if __name__=="__main__":
    main()
