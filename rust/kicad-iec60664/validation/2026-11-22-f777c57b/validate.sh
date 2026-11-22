#!/usr/bin/env bash
set -euo pipefail
source "$HOME/.cargo/env"
work="$HOME/kicad-port/iec-request-parser"
retained="$HOME/kicad-port/iec-input-fix"
windows=/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-887c85bc-41544ce805373602
unit=pcb_calculator.calculator_panels.application.pure.p001
export ORACLE_JOBS=8 CARGO_BUILD_JOBS=8
mkdir -p "$work/repo" "$work/stage"
if [[ ! -f "$work/copied-retained-tree" ]]; then
    cp -a --reflink=auto "$retained/repo/." "$work/repo/"
    touch "$work/copied-retained-tree"
fi
rsync -a --delete --exclude=target/ "$windows/rust/" "$work/repo/rust/"
rsync -a --delete --exclude=__pycache__/ "$windows/port/" "$work/repo/port/"
rsync -a "$windows/.port-venv/iec60664/request-stage/" "$work/stage/"
cd "$work/repo"
echo 'Source f777c57baaf1813f210e3ce022db553b301c5dd6; waiting for the real shared build lock'
while true; do
    if bash port/oracle/build.sh "$work/stage" "$work/repo/port/oracle" "$work"; then
        break
    else
        result=$?
    fi
    if [[ "$result" != 3 ]]; then exit "$result"; fi
    sleep 15
done
until mkdir "$HOME/kicad-port.build-lock" 2>/dev/null; do sleep 15; done
trap 'rmdir "$HOME/kicad-port.build-lock"' EXIT
cargo fmt --manifest-path rust/Cargo.toml --all --check
cargo clippy --locked --manifest-path rust/Cargo.toml -p kicad-difftest -p kicad-iec60664-shim --all-targets -j8 -- -D warnings
cargo test --locked --manifest-path rust/Cargo.toml -p kicad-difftest -p kicad-iec60664-shim -j8
cargo build --locked --manifest-path rust/Cargo.toml -p kicad-difftest -p kicad-iec60664-shim -j8
cargo build --locked --manifest-path rust/Cargo.toml -p kicad-iec60664 --target wasm32-unknown-unknown -j8
oracle="$work/oracle-build/kicad-oracle"
rust="$work/repo/rust/target/debug/kicad-iec60664-shim"
runner="$work/repo/rust/target/debug/kicad-difftest"
"$oracle" --provenance > "$work/tested-oracle-provenance.json"
python3 port/oracle/test_iec60664_inputs.py "$oracle" "$rust"
python3 port/tools/validate_units.py
python3 -m unittest discover -s rust/kicad-difftest/tests -p test_policy.py -v
"$runner" check --root "$work/repo" --unit "$unit" --oracle "$oracle" --rust "$rust" --report "$work/live-check.json"
python3 - "$work" <<'PY'
import json,pathlib,sys
work=pathlib.Path(sys.argv[1]); p=json.loads((work/'tested-oracle-provenance.json').read_text())
assert p['overlay_revision']=='f777c57baaf1813f210e3ce022db553b301c5dd6'
assert p['overlay_dirty_at_export'] is False
assert p['reference_file_count']==824 and len(p['compile_commands'])==60
assert p['floating_point']=={'contract':'off','fast_math':False}
(work/'summary.json').write_text(json.dumps({'tested_source':'f777c57baaf1813f210e3ce022db553b301c5dd6','parser_dependency':'8f5f45729c733d8f1695dda9ee6d953f9bb4d400','cases':14190,'complete_refusals_per_adapter':106,'pinned_files':824,'compile_commands':60,'original_artifacts_unchanged':True},indent=2)+'\n')
PY
printf 'Request parser validation complete\n'
