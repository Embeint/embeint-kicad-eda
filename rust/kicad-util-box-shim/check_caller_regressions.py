#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Require all accepted passing cases to remain exact, with unchanged public inputs."""
import argparse,gzip,json,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
BASE='37f4bc0a32'
p=argparse.ArgumentParser();p.add_argument('--vector',type=Path,required=True);p.add_argument('--geometry',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
result={}
for name,new_path,old_path in [
 ('vector',a.vector,ROOT/'rust/kicad-vector-shim/validation/2026-11-25-bfc668cf/difftest-report.json.gz'),
 ('geometry',a.geometry,ROOT/'rust/kicad-geometry-shim/validation/2026-11-25-d221593095/compare.json.gz')]:
    old=json.loads(gzip.decompress(old_path.read_bytes()));new=json.loads(new_path.read_text())
    assert new['unit']==old['unit'] and new['failures']==0 and new['unprocessed']==0
    assert not new['run_faults'] and not new['extra_replies']
    old_compared={v['case'] for v in old['outcomes'] if v['outcome']=='compared'}
    old_deferred={v['case'] for v in old['outcomes'] if v['outcome']=='deferred'}
    compared={v['case'] for v in new['outcomes'] if v['outcome']=='compared'}
    assert old_compared<=compared,(name,sorted(old_compared-compared))
    path='port/cases/'+old['unit']+'.jsonl'
    before=[json.loads(l) for l in subprocess.check_output(['git','show',BASE+':'+path],cwd=ROOT,text=True).splitlines()]
    after=[json.loads(l) for l in (ROOT/path).read_text().splitlines()]
    by_id={v['case']:v for v in after}
    assert all(by_id.get(v['case'])==v for v in before),name+' changed accepted input'
    result[name]=dict(unit=new['unit'],accepted_compared_retained=len(old_compared),accepted_deferrals_unlocked=len(old_deferred & compared),added_cases=len(after)-len(before),cases=new['cases'],compared=new['compared'],deferred=new['deferred'],failures=new['failures'])
a.output.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
