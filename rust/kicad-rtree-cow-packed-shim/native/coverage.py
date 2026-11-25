#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Retain gcov's actual pinned-header observations; counts are not full branch certification."""
import argparse
import gzip
import json
from pathlib import Path
import subprocess
p=argparse.ArgumentParser();p.add_argument('--out',required=True,type=Path);a=p.parse_args();a.out=a.out.resolve()
notes=list(a.out.glob('*-cow_packed_rtree.gcno'))
assert len(notes)==1,notes
subprocess.run(['gcov','--json-format','--branch-probabilities','--branch-counts',str(notes[0])],cwd=a.out,check=True,stdout=subprocess.PIPE)
files=list(a.out.glob('*.gcov.json.gz'))
observations=[]
for path in files:
    report=json.loads(gzip.decompress(path.read_bytes()))
    for f in report['files']:
        if f['file'].endswith(('/geometry/rtree/dynamic_rtree_cow.h','/geometry/rtree/packed_rtree.h')):
            observations.append(f)
assert len(observations)==2,'both actual owned headers must have coverage'
(a.out/'owned-header-coverage.json').write_text(json.dumps(observations,indent=2)+'\n')
functions=[fn for f in observations for fn in f['functions']]
lines=[line for f in observations for line in f['lines']]
summary=dict(headers=[f['file'] for f in observations],function_instantiations=len(functions),executed_function_instantiations=sum(fn['execution_count']>0 for fn in functions),
             line_entries=len(lines),executed_line_entries=sum(line['count']>0 for line in lines),
             branches=sum(len(line.get('branches',[])) for line in lines),
             taken_branches=sum(b['count']>0 for line in lines for b in line.get('branches',[])))
(a.out/'coverage-summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(summary)
