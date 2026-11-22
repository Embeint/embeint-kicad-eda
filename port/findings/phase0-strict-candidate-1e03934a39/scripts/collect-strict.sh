#!/usr/bin/env bash
# Copy the PROSPECTIVE strict-candidate records to a private Windows directory (not the repository).
set -euo pipefail
rev=${1:?revision}
work="$HOME/kicad-port/phase0-strict-candidate-$rev"
win=/mnt/c/Users/KarlVonRichter/.t3/phase0-final-cont
dest="$win/strict-$rev"
test -f "$work/done-all"
test ! -e "$dest"
mkdir -p "$dest/reports" "$dest/scripts"
sed 's/\x1b\[[0-9;]*m//g' "$work/run.log" > "$dest/run.log"
cp "$work/summary.json" "$work/reuse-identity.json" "$work/oracle-provenance.json" \
   "$work/source-validation.json" "$work/binaries.sha256" "$dest/"
cp "$work/reports/"*.json "$dest/reports/"
for f in run-strict.sh launch-strict.sh watch-strict.sh collect-strict.sh candidate_identity.py export.py; do
    tr -d '\r' < "$win/$f" > "$dest/scripts/$f"
done
tr -d '\r' < "$win/candidate-identity-$rev.json" > "$dest/candidate-identity.json"
tr -d '\r' < "$win/export-$rev/head-identity.json" > "$dest/head-identity.json"
tr -d '\r' < "$win/export-$rev/pin-identity.json" > "$dest/pin-identity.json"
(cd "$dest" && find . -type f ! -name files.sha256 -print0 | sort -z | xargs -0 sha256sum > files.sha256)
(cd "$dest" && sha256sum -c --quiet files.sha256 && echo "files.sha256: $(wc -l < files.sha256) entries verify")
