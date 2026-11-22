#!/usr/bin/env bash
# Verify committed integration records from an exact LF Git archive of port/findings:
# each named record's files.sha256, its original manifest identity, and the
# unchanged checksum lists of the earlier integration records.
set -euo pipefail
tarball=${1:?archive}
shift
d=$(mktemp -d)
tar -xf "$tarball" -C "$d"
for rev in "$@"; do
    (cd "$d/port/findings/phase0-final-integration-$rev"
     sha256sum -c --quiet files.sha256 && echo "$rev files.sha256: $(wc -l < files.sha256) entries verify from the LF archive"
     m=$(gzip -dc source-manifest.original.json.gz | sha256sum | cut -d' ' -f1)
     p=$(python3 -c 'import json;print(json.load(open("oracle-provenance.json"))["source_manifest_sha256"])')
     test "$m" = "$p" && echo "$rev original manifest $m equals the embedded provenance identity")
done
for p in phase0-final-integration-3731499b2b phase0-final-integration-d0cf06863c; do
    (cd "$d/port/findings/$p" && sha256sum -c --quiet files.committed.sha256 && echo "$p: historical committed list still verifies")
done
rm -rf "$d"
