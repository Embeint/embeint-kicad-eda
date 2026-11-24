#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Retain gcov observations for the pinned owning header without relabeling gaps."""
import gzip,json,sys
from pathlib import Path
with gzip.open(sys.argv[1],'rt') as f:data=json.load(f)
files=[f for f in data['files'] if f['file'].endswith('/geometry/rtree/dynamic_rtree.h')]
assert len(files)==1,files
f=files[0]
lines=f['lines'];functions=f['functions']
counts={}
for line in lines:counts[line['line_number']]=counts.get(line['line_number'],0)+line['count']
result={'source':f['file'],'gcc_version':data['gcc_version'],
        'line_records':len(lines),'distinct_executable_lines':len(counts),'distinct_hit_lines':sum(v>0 for v in counts.values()),
        'unhit_lines':sorted(k for k,v in counts.items() if not v),
        'functions':[{k:func[k] for k in ['demangled_name','start_line','end_line','execution_count','blocks','blocks_executed']} for func in functions],
        'branches':sum(len(l.get('branches',[])) for l in lines),
        'hit_branches':sum(b['count']>0 for l in lines for b in l.get('branches',[]))}
Path(sys.argv[2]).write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({k:v for k,v in result.items() if k!='functions'}))
