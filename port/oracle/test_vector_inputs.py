#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Guard controls are outside the success corpus: UB is refused, never counted green."""
import json
from pathlib import Path
import subprocess
import sys

def main():
    requests=[json.loads(s) for s in Path(__file__).with_name('vector-refusals.jsonl').read_text().splitlines()]
    survivor={'unit':'libs.kimath.include.math.application.ui.p002','case':'after-refusal','input':{'dim':2,'type':'i32','op':'new','a':[2,3]}}
    stream=''.join(json.dumps(r)+'\n'+json.dumps(dict(survivor,case=f'after-{n}'))+'\n' for n,r in enumerate(requests))
    p=subprocess.run([sys.argv[1]],input=stream,text=True,capture_output=True,check=True,timeout=30)
    replies=[json.loads(s) for s in p.stdout.splitlines()]
    assert len(replies)==2*len(requests), (len(replies),len(requests),p.stderr)
    for n,r in enumerate(requests):
        assert replies[2*n]=={'unit':r['unit'],'case':r['case'],'error':{'kind':'invalid_input','message':'undefined vector operation'}},replies[2*n]
        assert replies[2*n+1]=={'unit':survivor['unit'],'case':f'after-{n}','result':{'value':[2,3]}},replies[2*n+1]
    print(f'{len(requests)} vector UB guard refusals; {len(requests)} subsequent operations preserved')
if __name__=='__main__': main()
