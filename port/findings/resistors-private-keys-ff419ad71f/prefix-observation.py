import json, re, collections, sys
from pathlib import Path
d = Path(sys.argv[1])
req = (d / 'requests.jsonl').read_bytes().split(b'\n')[:-1]
cpp = (d / 'cpp.jsonl').read_bytes().split(b'\n')[:-1]
old = (d / 'prefix-debug.jsonl').read_bytes().split(b'\n')[:-1]
rel = (d / 'prefix-release.jsonl').read_bytes().split(b'\n')[:-1]
groups = collections.Counter(); diff = collections.Counter(); kinds = collections.Counter()
for r, a, b, c in zip(req, cpp, old, rel):
    assert json.loads(b) == json.loads(c)
    m = re.search(rb'"case":"pk-(?:\d{3}-)?([a-z-]+?)(?:-\d+)?"', r)
    g = m.group(1).decode() if m else ('case-position' if b'"case":{' in r or b'"case":{' in r else 'other')
    groups[g] += 1
    if json.loads(a) != json.loads(b):
        diff[g] += 1
        x, y = json.loads(a), json.loads(b)
        kinds['coerced into a result' if 'result' in y else ('null-identity parse refusal' if y.get('unit') is None and y.get('case') is None and not (x.get('unit') is None and x.get('case') is None) else 'different refusal')] += 1
print('lines', len(req), 'structural differences (pre-#48 debug == release)', sum(diff.values()))
for k, v in kinds.most_common(): print(' ', v, k)
for g in groups: print(f'{g:28} {diff[g]:4}/{groups[g]}')
