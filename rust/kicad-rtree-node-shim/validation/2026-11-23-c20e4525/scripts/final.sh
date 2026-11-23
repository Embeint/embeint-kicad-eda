#!/usr/bin/env bash
# final.sh SHA: clean export of SHA, fresh oracle, every rust-port workflow step.
set -uo pipefail
sha=$1
source "$HOME/.cargo/env"
U=libs.kimath.include.geometry.rtree.application.pure.p003
W="$HOME/kicad-port/rtree-p003"; C="$W/final-${sha:0:10}"; E=/mnt/c/kb/rtree-p003-export
L=/mnt/c/kb/rtree-p003-scripts/lock.sh
OUT="$C/evidence"
rm -rf "$C"; mkdir -p "$C/repo" "$C/stage" "$OUT"
tar -xf "$E/repo.tar" -C "$C/repo"; cp "$E/stage/"* "$C/stage/"
cp -a --reflink=auto "$W/repo/.port-deps" "$C/repo/"
python3 -c "import json,sys; m=json.load(open('$C/stage/source-manifest.json')); assert m['overlay_revision']=='$sha' and not m['overlay_dirty_at_export']; print('clean export', m['overlay_revision'], m['archive_sha256'])" | tee "$OUT/export.txt" || exit 1
sha256sum "$E/repo.tar" "$C/stage/"* >> "$OUT/export.txt"
status=0
step() { local name=$1; shift; echo "== $name: $*"; if "$@" > "$OUT/$name.log" 2>&1; then echo "   ok"; else echo "   FAILED ($name)"; status=1; tail -20 "$OUT/$name.log"; fi; }
while true; do
    ORACLE_JOBS=8 bash "$C/repo/port/oracle/build.sh" "$C/stage" "$C/repo/port/oracle" "$C" > "$OUT/oracle-build.log" 2>&1 && break
    rc=$?; [[ $rc == 3 ]] || { echo "oracle build failed"; tail -30 "$OUT/oracle-build.log"; exit 1; }; sleep 15
done
grep -E "tests passed|Passed|Failed" "$OUT/oracle-build.log" | tail -10
O="$C/oracle-build/kicad-oracle"
"$O" --provenance > "$OUT/oracle-provenance.json"
cp "$C/oracle-build/binary.sha256" "$OUT/"
cd "$C/repo/rust"
export CARGO_BUILD_JOBS=8
step fmt cargo +1.99.1 fmt --all --check
step clippy bash $L cargo +1.99.1 clippy --locked --workspace --all-targets -j8 -- -D warnings
step test bash $L cargo +1.99.1 test --locked --workspace -j8
step build-difftest bash $L cargo +1.99.1 build --locked -p kicad-difftest -j8
cd "$C/repo"
export PATH="$HOME/.rustup/toolchains/1.99.1-x86_64-unknown-linux-gnu/bin:$PATH"
step wasm bash $L python3 port/tools/check_goldens_policy.py --wasm
step test-policy python3 -m unittest discover -s rust/kicad-difftest/tests -p test_policy.py -v
step validate-units python3 port/tools/validate_units.py
step generator-check python3 rust/kicad-rtree-node-shim/generate_cases.py --check
step ci bash $L python3 port/tools/check_goldens_policy.py --ci --runner "$C/repo/rust/target/debug/kicad-difftest" --oracle "$O" --reports "$OUT/difftest-reports" --require-phase0
R="$C/repo/rust/target/debug/kicad-rtree-node-shim"
"$O" < rust/kicad-rtree-node-shim/fixtures/refusals.requests.jsonl > "$OUT/refusals.oracle.jsonl"
"$R" < rust/kicad-rtree-node-shim/fixtures/refusals.requests.jsonl > "$OUT/refusals.rust.jsonl"
step refusals python3 - "$OUT" rust/kicad-rtree-node-shim/fixtures/refusals.replies.jsonl <<'PY'
import json, sys
out, expected = sys.argv[1], sys.argv[2]
load = lambda p: [json.loads(line) for line in open(p)]
exp, cxx, rust = load(expected), load(f"{out}/refusals.oracle.jsonl"), load(f"{out}/refusals.rust.jsonl")
assert len(exp) == len(cxx) == len(rust) == 108, (len(exp), len(cxx), len(rust))
bad = [e["case"] for e, c, r in zip(exp, cxx, rust) if not (e == c == r)]
print(json.dumps({"refusals": len(exp), "oracle_matches": sum(e == c for e, c in zip(exp, cxx)),
                  "rust_matches": sum(e == r for e, r in zip(exp, rust)), "mismatched": bad}))
assert not bad
PY
cat "$OUT/refusals.log"
sha256sum "$O" "$R" port/cases/$U.jsonl port/goldens/$U.jsonl rust/kicad-rtree-node-shim/fixtures/*.jsonl > "$OUT/hashes.txt"
cat "$OUT/difftest-reports/$U.json" "$OUT/difftest-reports/phase0.json" 2>/dev/null
grep -h "test result" "$OUT/test.log" | awk '{p+=$4; f+=$6} END {print "workspace tests passed", p, "failed", f}'
echo "final status=$status"
exit $status
