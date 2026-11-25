#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Replay each control on both real processes; error agreement never counts as a ported case."""
import argparse
import json
from pathlib import Path
import subprocess
p=argparse.ArgumentParser();p.add_argument('--oracle',required=True);p.add_argument('--rust',required=True)
a=p.parse_args();root=Path(__file__).resolve().parent
requests=(root/'fixtures/refusals.requests.jsonl').read_bytes()
expected=[json.loads(l) for l in (root/'fixtures/refusals.replies.jsonl').read_text().splitlines()]
for exe in [a.oracle,a.rust]:
    result=subprocess.run([exe],input=requests,stdout=subprocess.PIPE,stderr=subprocess.PIPE,check=True,timeout=90)
    replies=[json.loads(l) for l in result.stdout.splitlines()]
    assert len(replies)==len(expected),(exe,'missing replies',len(replies))
    for want,got in zip(expected,replies):assert want==got,(exe,want,got)
    assert not result.stderr,(exe,result.stderr.decode())
print(f'{len(expected)} refusal controls match on both processes')
