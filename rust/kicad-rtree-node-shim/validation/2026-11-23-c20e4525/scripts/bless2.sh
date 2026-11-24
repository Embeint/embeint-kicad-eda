#!/usr/bin/env bash
# bless2.sh SHA: clean export -> fresh oracle -> (a) delta blessing over the existing
# golden, then (b) the consolidated blessing against the PR base (no golden).
set -euo pipefail
sha=$1
source "$HOME/.cargo/env"
U=libs.kimath.include.geometry.rtree.application.pure.p003
W="$HOME/kicad-port/rtree-p003"; C="$W/clean-${sha:0:10}"; E=/mnt/c/kb/rtree-p003-export
L=/mnt/c/kb/rtree-p003-scripts/lock.sh
rm -rf "$C"; mkdir -p "$C/repo" "$C/stage"
tar -xf "$E/repo.tar" -C "$C/repo"; cp "$E/stage/"* "$C/stage/"
cp -a --reflink=auto "$W/repo/.port-deps" "$C/repo/"
python3 -c "import json; m=json.load(open('$C/stage/source-manifest.json')); assert m['overlay_revision']=='$sha' and not m['overlay_dirty_at_export']; print('clean export', m['overlay_revision'])"
while true; do
    if ORACLE_JOBS=8 bash "$C/repo/port/oracle/build.sh" "$C/stage" "$C/repo/port/oracle" "$C" > "$C/oracle-build.log" 2>&1; then break; fi
    rc=$?; [[ $rc == 3 ]] || { tail -30 "$C/oracle-build.log"; exit $rc; }; sleep 15
done
grep -E "tests passed" "$C/oracle-build.log"
cd "$C/repo/rust"; bash $L cargo +1.99.1 build --locked -q -p kicad-rtree-node-shim -p kicad-difftest -j8
cd "$C/repo"
O="$C/oracle-build/kicad-oracle"; R="$C/repo/rust/target/debug/kicad-rtree-node-shim"; D=rust/target/debug/kicad-difftest
G=port/goldens/$U.jsonl
"$O" --provenance > "$C/oracle-provenance.json"
cp "$G" "$C/golden-before.jsonl"
$D compare --unit $U --oracle "$O" --rust "$R" --report "$C/compare.json"
reason() { python3 - "$1" "$2" <<'PY'
import json, sys
p, text = sys.argv[1], sys.argv[2]; a = json.load(open(p)); a["reason"] = text
open(p, "w").write(json.dumps(a, indent=2) + "\n")
PY
}
# (a) Delta over the committed 1448-case golden.
$D propose --unit $U --oracle "$O" --rust "$R" --provenance "$C/oracle-provenance.json" --approval "$C/approval-additions.json"
reason "$C/approval-additions.json" "Append the 50 regression cases (case IDs review-*) from the first independent Sol 6.1 review of PR 52 after the unchanged original 1448 cases: Morton orders above 64 dimensions and at INT_MIN + 1, the 33-slot n-dimensional mask boundary and the i32.d3.m33 programs, RemoveChild at count - 1, and moves leaving stale child pointers. No existing reply changes; all 1498 cases agree exactly between the live own-library oracle and kicad-rtree-node-shim with no errors or deferrals."
$D bless --unit $U --oracle "$O" --rust "$R" --provenance "$C/oracle-provenance.json" --approval "$C/approval-additions.json" --evidence "port/goldens/evidence/$U-review-additions.json" --report "$C/bless-additions.json"
cp "$G" "$C/golden-after.jsonl"
python3 - "$C/golden-before.jsonl" "$C/golden-after.jsonl" <<'PY'
import sys
old, new = open(sys.argv[1], "rb").read(), open(sys.argv[2], "rb").read()
assert new.startswith(old), "original golden replies changed"
print("original", old.count(b"\n"), "replies byte-identical prefix of", new.count(b"\n"))
PY
# (b) Consolidated against the PR base, where the golden does not exist yet.
rm "$G"
$D propose --unit $U --oracle "$O" --rust "$R" --provenance "$C/oracle-provenance.json" --approval "$C/approval-consolidated.json"
reason "$C/approval-consolidated.json" "Complete golden consolidated against PR base 535a17e39cc77452ba3377a5afb4170c14e9ab81, as check_goldens_policy.py requires: 1498 cases. The first 1448 replies are byte-identical to the baseline blessed at 57352b787f (evidence $U-baseline.json, kept unchanged); the 50 appended review-* cases are enumerated in $U-review-additions.json. All agree exactly with the live oracle, with no errors or deferrals."
$D bless --unit $U --oracle "$O" --rust "$R" --provenance "$C/oracle-provenance.json" --approval "$C/approval-consolidated.json" --evidence "port/goldens/evidence/$U-review-consolidated.json" --report "$C/bless-consolidated.json"
cmp "$G" "$C/golden-after.jsonl" && echo "consolidated golden identical to delta golden"
$D check --unit $U --oracle "$O" --rust "$R" --report "$C/check.json"
"$O" < port/cases/$U.jsonl > "$C/oracle-actual.jsonl"; cmp "$G" "$C/oracle-actual.jsonl" && echo "golden equals raw oracle output"
cp "$G" /mnt/c/kb/worktrees/rtree-p003/port/goldens/
cp port/goldens/evidence/$U-review-additions.json port/goldens/evidence/$U-review-consolidated.json /mnt/c/kb/worktrees/rtree-p003/port/goldens/evidence/
cat "$C/check.json"; sha256sum "$O" "$R" port/cases/$U.jsonl "$G"
