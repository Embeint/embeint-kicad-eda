"""Compare split unit records against the committed records they started from."""
import json, sys, tomllib
from collections import Counter
from pathlib import Path
base, new, out = map(Path, sys.argv[1:4])
load = lambda d: {p.stem: tomllib.loads(p.read_text(encoding="utf-8")) for p in sorted(d.glob("*.toml"))}
b, n = load(base), load(new)
assert set(b) == set(n), set(b) ^ set(n)
keys, changed = Counter(), {}
preserved = ("status", "claimed_by", "claimed_at", "notes", "tests", "rust_module", "functions",
             "source_ranges", "est_lines", "coverage_gaps", "wx_class", "source_category", "upstream")
for uid in b:
    diff = sorted(k for k in set(b[uid]) | set(n[uid]) if b[uid].get(k) != n[uid].get(k))
    keys.update(diff)
    if diff:
        changed[uid] = {k: {"removed": sorted(set(b[uid].get(k, [])) - set(n[uid].get(k, []))),
                            "added": sorted(set(n[uid].get(k, [])) - set(b[uid].get(k, [])))}
                        if isinstance(b[uid].get(k), list) else [b[uid].get(k), n[uid].get(k)] for k in diff}
bad = {u: d for u, d in changed.items() if set(d) & set(preserved)}
result = {"units": len(b), "changed_units": len(changed), "changed_keys": dict(keys),
          "preserved_field_changes": bad, "changes": changed}
out.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
print(json.dumps({k: v for k, v in result.items() if k != "changes"}, indent=1, sort_keys=True))
for uid in ("libs.kimath.include.geometry.application.ui.p003",):
    print(uid, json.dumps(changed.get(uid), indent=1))
print("non-green/claimed statuses:", Counter(u["status"] for u in n.values()))
for uid, d in sorted(changed.items()):
    if "depends_on" in d:
        print(uid, "depends_on", d["depends_on"])
