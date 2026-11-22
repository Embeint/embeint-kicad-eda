#!/usr/bin/env bash
# Validate clean IEC60664 runtime source 7903c45d in a private Linux tree.
set -euo pipefail
source "$HOME/.cargo/env"
source_sha=7903c45d1cfdad45439608f244f8cfbe2a5a2d70
work="$HOME/kicad-port/iec-nesting-bound"
retained="$HOME/kicad-port/iec-request-parser"
windows=/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-d800db45-ee30d0388e64ea12
unit=pcb_calculator.calculator_panels.application.pure.p001
export ORACLE_JOBS=8 CARGO_BUILD_JOBS=8
mkdir -p "$work/repo" "$work/stage"
# The retained tree supplies west dependencies and caches; it is read, never changed.
if [[ ! -f "$work/copied-retained-tree" ]]; then
    cp -a --reflink=auto "$retained/repo/." "$work/repo/"
    touch "$work/copied-retained-tree"
fi
# Exact committed blobs: the Windows checkout converts some files to CRLF.
archive="$windows/.port-venv/iec60664/source-7903c45d.tar"
rm -rf "$work/export" && mkdir -p "$work/export"
tar -xf "$archive" -C "$work/export"
sha256sum "$archive" > "$work/source-archive.sha256"
rsync -a --checksum --delete --exclude=target/ "$work/export/rust/" "$work/repo/rust/"
rsync -a --checksum --delete --exclude=__pycache__/ "$work/export/port/" "$work/repo/port/"
rsync -a --delete "$windows/.port-venv/iec60664/nesting-stage/" "$work/stage/"
cd "$work/repo"
echo "Source $source_sha; waiting for the real shared build lock"
while true; do
    result=0
    bash port/oracle/build.sh "$work/stage" "$work/repo/port/oracle" "$work" || result=$?
    if [[ "$result" == 0 ]]; then break; fi
    if [[ "$result" != 3 ]]; then exit "$result"; fi
    sleep 15
done
until mkdir "$HOME/kicad-port.build-lock" 2>/dev/null; do sleep 15; done
trap 'rmdir "$HOME/kicad-port.build-lock"' EXIT
echo '== fmt'
cargo fmt --manifest-path rust/Cargo.toml --all --check
echo '== clippy'
cargo clippy --locked --manifest-path rust/Cargo.toml -p kicad-difftest -p kicad-iec60664 -p kicad-iec60664-shim --all-targets -j8 -- -D warnings
echo '== tests'
cargo test --locked --manifest-path rust/Cargo.toml -p kicad-difftest -p kicad-iec60664 -p kicad-iec60664-shim -j8
echo '== build'
cargo build --locked --manifest-path rust/Cargo.toml -p kicad-difftest -p kicad-iec60664-shim -j8
echo '== portable wasm'
cargo build --locked --manifest-path rust/Cargo.toml -p kicad-iec60664 --target wasm32-unknown-unknown -j8
python3 port/tools/check_goldens_policy.py --wasm
oracle="$work/oracle-build/kicad-oracle"
rust="$work/repo/rust/target/debug/kicad-iec60664-shim"
runner="$work/repo/rust/target/debug/kicad-difftest"
"$oracle" --provenance > "$work/tested-oracle-provenance.json"
echo '== full-envelope probes'
python3 port/oracle/test_iec60664_inputs.py "$oracle" "$rust"
echo '== unit records'
python3 port/tools/validate_units.py
echo '== policy tests'
python3 -m unittest discover -s rust/kicad-difftest/tests -p test_policy.py -v
echo '== registered live golden check'
python3 port/tools/check_goldens_policy.py --ci --runner "$runner" --oracle "$oracle" --reports "$work/difftest-reports"
sha256sum "$oracle" "$rust" "$runner" > "$work/binary.sha256"
python3 - "$work" "$source_sha" <<'PY'
import json, pathlib, sys
work = pathlib.Path(sys.argv[1]); source = sys.argv[2]
p = json.loads((work / 'tested-oracle-provenance.json').read_text())
assert p['overlay_revision'] == source, p['overlay_revision']
assert p['overlay_dirty_at_export'] is False
assert p['reference_file_count'] == 824 and len(p['compile_commands']) == 60
assert p['floating_point'] == {'contract': 'off', 'fast_math': False}
report = json.loads((work / 'difftest-reports' / 'pcb_calculator.calculator_panels.application.pure.p001.json').read_text())
phase0 = json.loads((work / 'difftest-reports' / 'phase0.json').read_text())
(work / 'summary.json').write_text(json.dumps({
    'tested_source': source, 'cases': report['cases'], 'deferred': report['deferred'],
    'status': report['status'], 'oracle_sha256': report['oracle_sha256'], 'rust_sha256': report['rust_sha256'],
    'phase0': phase0['phase0'], 'validated_unit_records': phase0['validated_unit_records'],
    'pinned_files': p['reference_file_count'], 'compile_commands': len(p['compile_commands']),
}, indent=2) + '\n')
PY
echo 'IEC nesting-bound validation complete'
