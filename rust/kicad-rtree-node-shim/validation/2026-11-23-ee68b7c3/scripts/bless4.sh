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
$D propose --unit $U --oracle "$O" --rust "$R" --provenance "$C/oracle-provenance.json" --approval "$C/approval-storage-additions.json"
reason "$C/approval-storage-additions.json" "Append the 13 regression cases (case IDs review3-*) from the third independent Sol 6.1 review of PR 52 after the unchanged 1512 cases: null Owns and findOwningPage before and after allocation and after moves, caller-owned external nodes freed, re-allocated, used as children and after their storage ends, the unused page slot alias sequence through two free lists, and six fixed-seed storage programs. No existing reply changes; all 1525 cases agree exactly between the live own-library oracle and kicad-rtree-node-shim with no errors or deferrals."
$D bless --unit $U --oracle "$O" --rust "$R" --provenance "$C/oracle-provenance.json" --approval "$C/approval-storage-additions.json" --evidence "port/goldens/evidence/$U-storage-additions.json" --report "$C/bless-storage-additions.json"
cp "$G" "$C/golden-after.jsonl"
python3 - "$C/golden-before.jsonl" "$C/golden-after.jsonl" <<'PY'
import sys
old, new = open(sys.argv[1], "rb").read(), open(sys.argv[2], "rb").read()
assert new.startswith(old), "original golden replies changed"
print("original", old.count(b"\n"), "replies byte-identical prefix of", new.count(b"\n"))
PY
# (b) Consolidated against the PR base, where the golden does not exist yet.
rm "$G"
$D propose --unit $U --oracle "$O" --rust "$R" --provenance "$C/oracle-provenance.json" --approval "$C/approval-storage-consolidated.json"
reason "$C/approval-storage-consolidated.json" "Complete golden consolidated against the PR base, where no golden exists, as check_goldens_policy.py requires: 1525 cases. The first 1448 replies are byte-identical to the baseline blessed at 57352b787f, the next 50 to the first review blessing, the next 14 to the foreign-allocation blessing, and the 13 appended review3-* cases are enumerated in $U-storage-additions.json; all earlier evidence is unchanged. All agree exactly with the live oracle, with no errors or deferrals."
$D bless --unit $U --oracle "$O" --rust "$R" --provenance "$C/oracle-provenance.json" --approval "$C/approval-storage-consolidated.json" --evidence "port/goldens/evidence/$U-storage-consolidated.json" --report "$C/bless-storage-consolidated.json"
cmp "$G" "$C/golden-after.jsonl" && echo "consolidated golden identical to delta golden"
$D check --unit $U --oracle "$O" --rust "$R" --report "$C/check.json"
"$O" < port/cases/$U.jsonl > "$C/oracle-actual.jsonl"; cmp "$G" "$C/oracle-actual.jsonl" && echo "golden equals raw oracle output"
cp "$G" /mnt/c/kb/worktrees/rtree-p003/port/goldens/
cp port/goldens/evidence/$U-storage-additions.json port/goldens/evidence/$U-storage-consolidated.json /mnt/c/kb/worktrees/rtree-p003/port/goldens/evidence/
cat "$C/check.json"; sha256sum "$O" "$R" port/cases/$U.jsonl "$G"
