import json, sys, tomllib
from collections import Counter
from pathlib import Path
base, new, diff_path = map(Path, sys.argv[1:4])
load = lambda d: {p.stem: tomllib.loads(p.read_text(encoding="utf-8")) for p in sorted(d.glob("*.toml"))}
b, n = load(base), load(new)
summary = Counter()
promoted, fresh = [], []
for uid in b:
    added = set(n[uid]["depends_on"]) - set(b[uid]["depends_on"])
    for dep in added:
        (promoted if dep in b[uid]["soft_depends_on"] else fresh).append((uid, dep))
    removed = set(b[uid]["depends_on"]) - set(n[uid]["depends_on"])
    for dep in removed:
        summary["hard_removed_now_soft" if dep in n[uid]["soft_depends_on"] else "hard_removed_entirely"] += 1
    for dep in set(b[uid]["soft_depends_on"]) - set(n[uid]["soft_depends_on"]):
        summary["soft_removed_now_hard" if dep in n[uid]["depends_on"] else "soft_removed_entirely"] += 1
summary["hard_added_from_soft"] = len(promoted)
summary["hard_added_new"] = len(fresh)
print(dict(summary), fresh[:10])
interesting = [u for u in b if u.startswith("libs.kimath") or u.startswith("thirdparty.clipper2") or u.startswith("libs.core")]
for uid in interesting:
    if b[uid] != n[uid]:
        d = {}
        for k in ("depends_on", "soft_depends_on"):
            r = sorted(set(b[uid][k]) - set(n[uid][k])); a = sorted(set(n[uid][k]) - set(b[uid][k]))
            if r or a: d[k] = {"-": r, "+": a}
        if b[uid]["layer"] != n[uid]["layer"]: d["layer"] = [b[uid]["layer"], n[uid]["layer"]]
        print(uid, b[uid]["status"], json.dumps(d))
statuses = {u: b[u]["status"] for u in b if b[u]["status"] != "todo" and b[u]["status"] != "blocked"}
print("non-todo units with dependency changes:", {u: s for u, s in statuses.items()
      if b[u]["depends_on"] != n[u]["depends_on"]})
vrml = "plugins.3d.vrml.application.ui.p001"
print("units hard-depending on vrml before/after:", sum(vrml in u["depends_on"] for u in b.values()),
      sum(vrml in u["depends_on"] for u in n.values()))
print("vrml p001 deps after:", n[vrml]["depends_on"], "layer", b[vrml]["layer"], "->", n[vrml]["layer"])
print("max layer", max(u["layer"] for u in b.values()), "->", max(u["layer"] for u in n.values()))
