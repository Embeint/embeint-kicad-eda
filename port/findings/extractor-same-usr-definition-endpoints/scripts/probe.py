import gzip, json, sys
from pathlib import Path
from collections import defaultdict
cache = Path(sys.argv[1])
U = "c:@S@SHAPE@F@~SHAPE#"
ext = defaultdict(int)
n = 0
for p in sorted(cache.glob("*.json.gz")):
    r = json.loads(gzip.decompress(p.read_bytes()))
    n += 1
    ext[(r.get("status"),)] += 1
    fn = r["functions"]
    local = fn.get(U)
    calls = [c for c in r["calls"] if c[1] == U]
    ov = [o for o in r["overrides"] if U in o]
    if local or calls or ov:
        if "vrml" in r["file"] or "shape_arc" in r["file"] or "shape_compound" in r["file"] or "pcb_painter" in r["file"]:
            print(r["file"], "local:", local and (local["file"], local["start"], local["virtual"]),
                  "callers:", sorted({c[0].split("@F@")[-1][:30] for c in calls})[:6], "ov:", len(ov))
print(n, dict(ext))
