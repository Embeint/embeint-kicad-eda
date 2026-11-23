#!/usr/bin/env bash
# bless.sh SHA: clean export of commit SHA -> fresh oracle -> propose/bless baseline.
set -euo pipefail
sha=$1
source "$HOME/.cargo/env"
U=libs.kimath.include.geometry.rtree.application.pure.p003
W="$HOME/kicad-port/rtree-p003"
C="$W/clean-${sha:0:10}"
E=/mnt/c/kb/rtree-p003-export
L=/mnt/c/kb/rtree-p003-scripts/lock.sh
rm -rf "$C"; mkdir -p "$C/repo" "$C/stage"
tar -xf "$E/repo.tar" -C "$C/repo"
cp "$E/stage/"* "$C/stage/"
cp -a --reflink=auto "$W/repo/.port-deps" "$C/repo/"
python3 - "$C" "$sha" <<'PY'
import json, pathlib, sys
c, sha = pathlib.Path(sys.argv[1]), sys.argv[2]
m = json.loads((c / "stage/source-manifest.json").read_text())
assert m["overlay_revision"] == sha and m["overlay_dirty_at_export"] is False, m["overlay_revision"]
owned = ["rust/kicad-rtree-node", "rust/kicad-rtree-node-shim", "port/oracle/shims/rtree_node.cpp",
         "port/units/libs.kimath.include.geometry.rtree.application.pure.p003.toml",
         "port/cases/libs.kimath.include.geometry.rtree.application.pure.p003.jsonl",
         "rust/Cargo.toml", "rust/Cargo.lock"]
for root in owned:
    base = c / "repo" / root
    for p in ([base] if base.is_file() else base.rglob("*")):
        if p.is_file() and b"\r" in p.read_bytes():
            raise SystemExit(f"CR in export: {p}")
print("clean LF export of", sha)
PY
while true; do
    if ORACLE_JOBS=8 bash "$C/repo/port/oracle/build.sh" "$C/stage" "$C/repo/port/oracle" "$C"; then break; fi
    rc=$?; [[ $rc == 3 ]] || exit $rc; sleep 15
done
cd "$C/repo/rust"
bash $L cargo +1.99.1 build --locked -q -p kicad-rtree-node-shim -p kicad-difftest -j8
cd "$C/repo"
O="$C/oracle-build/kicad-oracle"; R="$C/repo/rust/target/debug/kicad-rtree-node-shim"; D=rust/target/debug/kicad-difftest
"$O" --provenance > "$C/oracle-provenance.json"
$D compare --unit $U --oracle "$O" --rust "$R" --report "$C/compare.json"
$D propose --unit $U --oracle "$O" --rust "$R" --provenance "$C/oracle-provenance.json" --approval "$C/approval.json"
python3 - "$C/approval.json" <<'PY'
import json, sys
p = sys.argv[1]; a = json.load(open(p))
a["reason"] = ("Initial baseline for libs.kimath.include.geometry.rtree.application.pure.p003: "
               "all 1448 fixed-seed cases agree exactly between the live own-library oracle "
               "(pinned rtree_node.h) and kicad-rtree-node-shim, with no errors or deferrals.")
open(p, "w").write(json.dumps(a, indent=2) + "\n")
PY
mkdir -p port/goldens/evidence
$D bless --unit $U --oracle "$O" --rust "$R" --provenance "$C/oracle-provenance.json" \
    --approval "$C/approval.json" --evidence "port/goldens/evidence/$U-baseline.json" --report "$C/bless.json"
$D check --unit $U --oracle "$O" --rust "$R" --report "$C/check.json"
"$O" < port/cases/$U.jsonl > "$C/oracle-actual.jsonl"
cmp port/goldens/$U.jsonl "$C/oracle-actual.jsonl" && echo "golden equals raw oracle output"
cp port/goldens/$U.jsonl /mnt/c/kb/worktrees/rtree-p003/port/goldens/
cp port/goldens/evidence/$U-baseline.json /mnt/c/kb/worktrees/rtree-p003/port/goldens/evidence/
cat "$C/compare.json" "$C/check.json"
sha256sum "$O" "$R" port/cases/$U.jsonl port/goldens/$U.jsonl
