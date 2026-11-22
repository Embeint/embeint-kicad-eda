#!/usr/bin/env bash
# Copy the retained proof records of one run into the repository evidence directory.
set -euo pipefail
rev=${1:?revision}
work="$HOME/kicad-port/phase0-final-$rev"
win=/mnt/c/Users/KarlVonRichter/.t3/phase0-final
dest=/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-6446b4e5-27051171012f2ac7/port/findings/phase0-final-integration-$rev
test -f "$work/done-all"
mkdir -p "$dest/reports" "$dest/scripts"
sed 's/\x1b\[[0-9;]*m//g' "$work/run.log" > "$dest/run.log"
cp "$work/summary.json" "$work/oracle-provenance.json" "$work/source-validation.json" \
   "$work/adapters.sha256" "$work/registered-binaries.sha256" "$dest/"
cp "$work/oracle-src/reference/source-manifest.json" "$dest/source-manifest.json"
cp "$work/reports/"*.json "$dest/reports/"
cp "$win/export-$rev/head-identity.json" "$win/export-$rev/pin-identity.json" "$dest/"
cp "$win/identity-$rev.json" "$dest/layer-identity.json"
cp "$win/iec-corpus-check-$rev.log" "$dest/iec-corpus-check.log"
cp "$win/guard-mutations-$rev.log" "$dest/guard-mutations.log"
for f in export.py run.sh launch.sh watch.sh collect.sh identity_proof.py check_iec_corpus.py mutate.py; do
    cp "$win/$f" "$dest/scripts/$f"
done
(cd "$dest" && find . -type f ! -name files.sha256 -print0 | sort -z | xargs -0 sha256sum > files.sha256)
ls -la "$dest" "$dest/reports"
