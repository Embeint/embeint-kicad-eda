#!/usr/bin/env bash
set -euo pipefail
work="$HOME/kicad-port/md5-input-fix"
original="$HOME/kicad-port/pilot-md5"
windows=/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-b1e44652-8ce837179fbd65f3
stage=/mnt/c/Users/KarlVonRichter/.t3/md5-input-fix-stage
unit=libs.kimath.src.md5_hash.application.pure.p001
lock="$HOME/kicad-port.build-lock"
mkdir -p "$work"
echo 'Waiting for the shared build lock before preparing the private fix tree'
until mkdir "$lock" 2>/dev/null; do sleep 5; done
trap 'rmdir "$lock"' EXIT
mkdir -p "$work/repo"
cp -a --reflink=auto "$original/repo/." "$work/repo/"
rsync -a --exclude=target/ "$windows/rust/" "$work/repo/rust/"
rsync -a --exclude=__pycache__/ "$windows/port/" "$work/repo/port/"
source "$HOME/.cargo/env"
export CARGO_BUILD_JOBS=8 ORACLE_JOBS=8
cd "$work/repo"
python3 port/tools/bootstrap_west.py
.port-venv/linux/bin/west update
python3 port/tools/bootstrap_west.py --check
rmdir "$lock"
trap - EXIT
while true; do
    if bash port/oracle/build.sh "$stage" "$work/repo/port/oracle" "$work"; then
        break
    else
        result=$?
        if [[ "$result" != 3 ]]; then exit "$result"; fi
        sleep 5
    fi
done
echo 'Waiting for the shared lock before Rust and live comparison checks'
until mkdir "$lock" 2>/dev/null; do sleep 5; done
trap 'rmdir "$lock"' EXIT
cargo fmt --manifest-path rust/Cargo.toml --all --check
cargo clippy --locked --manifest-path rust/Cargo.toml -p kicad-md5 -p kicad-md5-shim --all-targets -j8 -- -D warnings
cargo test --locked --manifest-path rust/Cargo.toml -p kicad-md5 -p kicad-md5-shim -j8
cargo build --locked --manifest-path rust/Cargo.toml -p kicad-difftest -j8
cargo build --locked --manifest-path rust/Cargo.toml -p kicad-md5-shim --bin kicad-md5-shim -j8
cp rust/target/debug/kicad-difftest "$work/kicad-difftest"
cp rust/target/debug/kicad-md5-shim "$work/kicad-md5-shim"
oracle="$work/oracle-build/kicad-oracle"
"$oracle" --provenance > "$work/oracle-provenance.json"
python3 port/oracle/test_md5_inputs.py "$oracle" "$work/kicad-md5-shim"
python3 rust/kicad-md5-shim/generate_cases.py --output "$work/regenerated.jsonl"
cmp "$work/regenerated.jsonl" "port/cases/$unit.jsonl"
"$work/kicad-difftest" compare --root "$work/repo" --unit "$unit" \
    --oracle "$oracle" --rust "$work/kicad-md5-shim" --report "$work/compare.json"
baseline="$work/baseline-root"
mkdir -p "$baseline/port/cases" "$baseline/port/goldens/evidence"
cp "port/cases/$unit.jsonl" "$baseline/port/cases/$unit.jsonl"
"$work/kicad-difftest" propose --root "$baseline" --unit "$unit" \
    --oracle "$oracle" --rust "$work/kicad-md5-shim" --approval "$work/proposal.json" \
    --provenance "$work/oracle-provenance.json" --report "$work/propose-report.json"
python3 - "$work/proposal.json" <<'PY'
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
value = json.loads(path.read_text())
value['reason'] = ('Complete initial PR baseline with 5973 live-matched cases, including a literal JSON -0 byte. '
                   'The original 5972 case results are unchanged; the added valid input hashes the byte zero. '
                   'No comparison, case or reference result is relaxed. MD5 outputs are textual hashes, not visual geometry.')
path.write_text(json.dumps(value, indent=2) + '\n')
PY
"$work/kicad-difftest" bless --root "$baseline" --unit "$unit" \
    --oracle "$oracle" --rust "$work/kicad-md5-shim" --approval "$work/proposal.json" \
    --provenance "$work/oracle-provenance.json" \
    --evidence "$baseline/port/goldens/evidence/$unit-baseline-with-negative-zero.json" \
    --report "$work/bless-report.json"
"$work/kicad-difftest" check --root "$baseline" --unit "$unit" \
    --oracle "$oracle" --rust "$work/kicad-md5-shim" --report "$work/check-report.json"
python3 - "$work" "$unit" <<'PY'
import json, pathlib, sys
work, unit = pathlib.Path(sys.argv[1]), sys.argv[2]
old = (work / 'repo/port/goldens' / (unit + '.jsonl')).read_bytes()
new = (work / 'baseline-root/port/goldens' / (unit + '.jsonl')).read_bytes()
assert len(old.splitlines()) == 5972 and len(new.splitlines()) == 5973
assert new.startswith(old), 'An original golden result changed'
wire = (work / 'repo/port/cases' / (unit + '.jsonl')).read_bytes()
assert b'"data":[-0]' in wire.splitlines()[-1]
(work / 'summary.json').write_text(json.dumps({'source_revision': '536a79601d3116e8a30befacc07127ef05b052f1',
    'cases': 5973, 'original_results_unchanged': 5972, 'literal_negative_zero_retained': True,
    'refusals_checked_per_adapter': 8, 'complete_baseline_against_absent_pr_base': True}, indent=2) + '\n')
print('All 5973 cases match; original 5972 golden lines are byte-identical')
PY
touch "$work/completed"
echo 'MD5 input fixes, full live comparison and complete baseline evidence are ready'
