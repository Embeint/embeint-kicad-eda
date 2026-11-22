#!/usr/bin/env bash
# Copy the retained records of the MD5 follow-up proof into the repository
# evidence directory. Windows-written reports are normalized to LF before hashing
# so files.sha256 verifies the committed bytes; the tested source manifest (CRLF,
# as prepare.py wrote it on Windows and the oracle embeds) is kept as gzip.
set -euo pipefail
rev=${1:?revision}
work="$HOME/kicad-port/phase0-final-cont-$rev"
win=/mnt/c/Users/KarlVonRichter/.t3/phase0-final-cont
dest=/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-9faea567-62b02bc02571d34b/port/findings/phase0-final-integration-$rev
test -f "$work/done-all"
test ! -e "$dest"
mkdir -p "$dest/scripts"
sed 's/\x1b\[[0-9;]*m//g' "$work/run.log" > "$dest/run.log"
cp "$work/summary.json" "$work/oracle-provenance.json" "$work/md5-guard.json" \
   "$work/registered-md5.json" "$work/binaries.sha256" "$dest/"
gzip -n -9 -c "$work/oracle-src/reference/source-manifest.json" > "$dest/source-manifest.original.json.gz"
lf() { tr -d '\r' < "$1" > "$2"; }
lf "$win/export-$rev/head-identity.json" "$dest/head-identity.json"
lf "$win/export-$rev/pin-identity.json" "$dest/pin-identity.json"
lf "$win/identity-$rev.json" "$dest/layer-identity.json"
lf "$win/supplemental-$rev.json" "$dest/supplemental-identity.json"
for f in export.py run-md5.sh launch-md5.sh watch.sh collect-md5.sh identity_proof.py \
         supplemental_md5.py verify-committed.sh; do
    lf "$win/$f" "$dest/scripts/$f"
done
python3 - "$dest" "$work" <<'PY'
import gzip, hashlib, json, pathlib, sys
dest, work = map(pathlib.Path, sys.argv[1:])
original = (work / 'oracle-src/reference/source-manifest.json').read_bytes()
assert gzip.decompress((dest / 'source-manifest.original.json.gz').read_bytes()) == original
digest = hashlib.sha256(original).hexdigest()
for name, key in [('oracle-provenance.json', 'source_manifest_sha256'), ('summary.json', 'source_manifest_sha256'),
                  ('pin-identity.json', 'manifest_sha256')]:
    assert json.loads((dest / name).read_text())[key] == digest, name
for path in dest.rglob('*'):
    if path.is_file() and path.suffix != '.gz':
        assert b'\r' not in path.read_bytes(), path
print('original manifest', digest, 'CR bytes', original.count(b'\r'))
PY
(cd "$dest" && find . -type f ! -name files.sha256 -print0 | sort -z | xargs -0 sha256sum > files.sha256)
(cd "$dest" && sha256sum -c --quiet files.sha256 && echo "files.sha256: $(wc -l < files.sha256) entries verify")
ls -la "$dest"
