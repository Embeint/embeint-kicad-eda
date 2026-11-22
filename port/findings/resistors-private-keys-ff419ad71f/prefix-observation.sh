#!/usr/bin/env bash
# Observe the authored wires against the prior (read-only) actual oracle and pre-fix shim.
set -euo pipefail
prior="$HOME/kicad-port/pilot-resistors"
out="$HOME/kicad-port/pilot-resistors-private-keys/observe"
mkdir -p "$out"
cp /mnt/c/Users/KarlVonRichter/.t3/resistor-privkeys/requests.jsonl "$out/requests.jsonl"
sha256sum "$prior/oracle-build/kicad-oracle" "$prior/src/rust/target/debug/kicad-resistors-shim" \
    "$prior/src/rust/target/release/kicad-resistors-shim" "$out/requests.jsonl" > "$out/inputs.sha256"
"$prior/oracle-build/kicad-oracle" < "$out/requests.jsonl" > "$out/cpp.jsonl" 2> "$out/cpp.stderr"
"$prior/src/rust/target/debug/kicad-resistors-shim" < "$out/requests.jsonl" > "$out/prefix-debug.jsonl" 2> "$out/prefix-debug.stderr" || echo "debug exit $?"
"$prior/src/rust/target/release/kicad-resistors-shim" < "$out/requests.jsonl" > "$out/prefix-release.jsonl" 2> "$out/prefix-release.stderr" || echo "release exit $?"
wc -l "$out"/*.jsonl
wc -c "$out"/*.stderr
cat "$out/inputs.sha256"
