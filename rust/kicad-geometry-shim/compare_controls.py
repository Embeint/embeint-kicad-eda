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
        assert reply.get('error')==control['error'],(binary,control,reply)
print(f'{len(controls)} undefined-domain refusals agree in both executables')
