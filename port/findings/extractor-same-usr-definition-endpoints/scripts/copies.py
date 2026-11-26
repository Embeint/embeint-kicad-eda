import gzip, json, sys
from collections import Counter, defaultdict
from pathlib import Path
g = json.loads(gzip.decompress(Path(sys.argv[1]).read_bytes()))
cmp = json.loads(Path(sys.argv[2]).read_text())
f, copies = g["functions"], g["definition_copies"]
groups = defaultdict(list)
for k, u in copies.items():
    groups[u].append(k)
kinds = Counter()
same_file = []
for u, ks in sorted(groups.items()):
    files = [f[u]["file"]] + [f[k]["file"] for k in ks]
    if len(set(files)) == 1:
        same_file.append((u, files[0], [(f[x]["start"], f[x]["end"]) for x in [u] + ks]))
    top = lambda p: p.split("/")[0] if not p.startswith("@build") else "@build"
    kinds[tuple(sorted({top(p) for p in files}))] += 1
print("copy groups", len(groups), "same-file groups", len(same_file))
for s in same_file[:15]:
    print("  same-file", s)
print("top-level dirs per group", kinds.most_common(20))
for u in ["c:@N@std@ST>2#T#T@vector@F@size#1", "c:@S@COMPONENT@F@GetReference#1", "c:@F@Kiface#", "c:@S@SHAPE@F@~SHAPE#"]:
    print(u, [(f[k]["file"], f[k]["start"]) for k in [u] + groups.get(u, []) if k in f])
print(json.dumps(cmp["shape_destructor_removed_edges"][:40], indent=0))
print("remaining vrml-body callers", cmp["shape_destructor_remaining_vrml_edges"])
