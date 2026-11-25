#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Undefined-domain controls never enter the successful public corpus."""
import argparse
import json
from pathlib import Path
import subprocess
ROOT=Path(__file__).resolve().parents[2]
p=argparse.ArgumentParser();p.add_argument('--oracle',required=True);p.add_argument('--rust',required=True);a=p.parse_args()
controls=[json.loads(s) for s in (ROOT/'port/oracle/fixtures/geometry-controls.jsonl').read_text().splitlines()]
requests=''.join(json.dumps(c['request'])+'\n' for c in controls)
for binary in [a.oracle,a.rust]:
    r=subprocess.run([binary],input=requests,text=True,capture_output=True,check=True,timeout=30)
    replies=[json.loads(s) for s in r.stdout.splitlines()]
    assert len(replies)==len(controls),(binary,r.stdout)
    for control,reply in zip(controls,replies):
        assert reply['case']==control['request']['case']
        assert reply['unit']==control['request']['unit']
        assert set(reply)=={'unit','case','error'}
        expected=control.get('rust_error',control['error']) if binary==a.rust else control['error']
        assert reply.get('error')==expected,(binary,control,reply)
print(f'{len(controls)} undefined-domain controls checked (earlier Rust dependencies remain deferred)')

priority=[json.loads(s) for s in (ROOT/'port/oracle/fixtures/geometry-priority-controls.jsonl').read_text().splitlines()]
requests=''.join(c['request_text']+'\n' for c in priority)
for binary in [a.oracle,a.rust]:
    r=subprocess.run([binary],input=requests,text=True,capture_output=True,check=True,timeout=30)
    replies=[json.loads(s) for s in r.stdout.splitlines()]
    assert len(replies)==len(priority)
    for control,reply in zip(priority,replies):
        key='result' if 'result' in control else 'error'
        request=json.loads(control['request_text'])
        assert reply['unit']==request['unit'] and reply['case']==request['case']
        assert set(reply)=={'unit','case',key},(binary,control,reply)
        # Python bool compares equal to integer 0/1; the protocol must not.
        assert type(reply.get(key)) is type(control[key]),(binary,control,reply)
        assert reply.get(key)==control[key],(binary,control,reply)
print('Priority lexical controls: four successful integer -0 results and two floating-input refusals agree')
