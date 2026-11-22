#!/usr/bin/env bash
set -euo pipefail
work="$HOME/kicad-port/pilot-md5"
lock="$HOME/kicad-port.build-lock"
if ! mkdir "$lock"; then
    echo 'Shared lock busy; blessing did not start' >&2
    exit 3
fi
trap 'rmdir "$lock"' EXIT
cd "$work/repo"
unit=libs.kimath.src.md5_hash.application.pure.p001
mkdir -p port/goldens/evidence
rust/target/debug/kicad-difftest bless --unit "$unit" \
    --oracle "$work/oracle-build/kicad-oracle" \
    --rust "$work/repo/rust/target/debug/kicad-md5-shim" \
    --provenance "$work/oracle-provenance.json" \
    --approval "$work/baseline-approval.json" \
    --evidence "port/goldens/evidence/$unit-baseline.json" \
    --report "$work/bless.json"
rust/target/debug/kicad-difftest check --unit "$unit" \
    --oracle "$work/oracle-build/kicad-oracle" \
    --rust "$work/repo/rust/target/debug/kicad-md5-shim" \
    --report "$work/check.json"
cmp "port/goldens/$unit.jsonl" "$work/oracle-actual.jsonl"
echo 'All 5972 baseline replies match real oracle output; default-budget live check passed'
