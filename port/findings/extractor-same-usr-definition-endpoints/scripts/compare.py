"""Compare the V5 original AST graph with its location-aware re-aggregation."""
import gzip, hashlib, json, sys
from collections import Counter
from pathlib import Path

old_path, new_path, out_path = map(Path, sys.argv[1:4])
load = lambda p: json.loads(gzip.decompress(p.read_bytes()))
old, new = load(old_path), load(new_path)
summary = {"old_graph_sha256": hashlib.sha256(old_path.read_bytes()).hexdigest(),
           "new_graph_sha256": hashlib.sha256(new_path.read_bytes()).hexdigest()}
assert set(old) == set(new), set(old) ^ set(new)
summary["unchanged_top_level"] = sorted(k for k in old if k not in ("edges", "overrides", "functions", "provenance")
                                        and old[k] == new[k])
summary["changed_top_level"] = sorted(k for k in old if old[k] != new[k])
assert set(old["functions"]) == set(new["functions"])
strip = lambda f: {k: v for k, v in f.items() if k != "wx_class"}
assert all(strip(old["functions"][k]) == strip(new["functions"][k]) for k in old["functions"])
wx = Counter((old["functions"][k]["wx_class"], new["functions"][k]["wx_class"]) for k in old["functions"]
             if old["functions"][k]["wx_class"] != new["functions"][k]["wx_class"])
summary["wx_class_changes"] = {f"{a}->{b}": n for (a, b), n in sorted(wx.items())}
oe, ne = set(map(tuple, old["edges"])), set(map(tuple, new["edges"]))
oo, no = set(map(tuple, old["overrides"])), set(map(tuple, new["overrides"]))
removed, added = sorted(oe - ne), sorted(ne - oe)
summary.update(old_edges=len(oe), new_edges=len(ne), removed_edges=len(removed), added_edges=len(added),
               old_overrides=len(oo), new_overrides=len(no), removed_overrides=len(oo - no),
               added_overrides=len(no - oo))
copies = old["definition_copies"]
usr = lambda k: copies.get(k, k)
summary["removed_edges_by_usr"] = dict(Counter(usr(b) for _, b, _ in removed).most_common(40))
summary["removed_edges_by_kind"] = dict(Counter(k for _, _, k in removed))
summary["removed_edges_without_copy_endpoint"] = sum(a not in copies and b not in copies and
    a not in set(copies.values()) and b not in set(copies.values()) for a, b, _ in removed)
summary["copies"] = len(copies)
f = new["functions"]
shape = "c:@S@SHAPE@F@~SHAPE#"
summary["shape_destructor_removed_edges"] = [[a, f.get(a, {}).get("file"), b, k] for a, b, k in removed
                                             if usr(b) == shape or usr(a) == shape]
summary["shape_destructor_remaining_vrml_edges"] = [[a, f.get(a, {}).get("file"), b, k] for a, b, k in sorted(ne)
                                                    if b == shape + "@definition:861533d9c907d7fb"]
summary["provenance_added"] = sorted(set(new["provenance"]) - set(old["provenance"]))
summary["provenance_changed"] = sorted(k for k in old["provenance"] if k in new["provenance"]
                                       and old["provenance"][k] != new["provenance"][k])
summary["parse_cache_replay"] = new["provenance"].get("parse_cache_replay")
summary["extractor_sha256"] = [old["provenance"]["extractor_sha256"], new["provenance"]["extractor_sha256"]]
out_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
print(json.dumps({k: v for k, v in summary.items() if not isinstance(v, list)}, indent=1, sort_keys=True))
