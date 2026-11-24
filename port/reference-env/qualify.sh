#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Build and probe the CANDIDATE reference image on a native Linux x86-64 Docker
# host (WSL Ubuntu with a native engine, or a hosted ubuntu-24.04 runner).
# It never pushes an image, never changes goldens and never qualifies the
# candidate by itself: it produces evidence for independent review.
#
# usage: qualify.sh --revision SHA --work DIR [--sweep-count N] [--stage DIR] [--with-oracle]
#                   [--with-difftest REPO] [--rebuild-check]
#                   [--source-bundle DIR --source-sha256 SHA256]
set -euo pipefail
export PYTHONDONTWRITEBYTECODE=1

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$here/../.." && pwd)
work=
sweep=200000
stage=
with_oracle=0
difftest_repo=
rebuild_check=0
revision=
source_bundle=
source_sha=

usage() {
    sed -n '/^# usage:/,/^set /p' "$0" | sed '$d; s/^# \{0,1\}//' >&2
    exit 2
}
while [[ $# -gt 0 ]]; do
    case "$1" in
        --revision) revision=${2:?}; shift 2 ;;
        --source-bundle) source_bundle=${2:?}; shift 2 ;;
        --source-sha256) source_sha=${2:?}; shift 2 ;;
        --work) work=${2:?}; shift 2 ;;
        --sweep-count) sweep=${2:?}; shift 2 ;;
        --stage) stage=${2:?}; shift 2 ;;
        --with-oracle) with_oracle=1; shift ;;
        --with-difftest) difftest_repo=${2:?}; with_oracle=1; shift 2 ;;
        --rebuild-check) rebuild_check=1; shift ;;
        *) usage ;;
    esac
done
[[ -n "$work" && "$revision" =~ ^[0-9a-f]{40}$ ]] || usage
[[ "$sweep" =~ ^[0-9]+$ ]] && (( sweep >= 1 && sweep <= 2000000 )) || { echo 'sweep 1..2000000' >&2; exit 2; }

# Native x86-64 Linux engine only. Emulated amd64 (for example on an arm64 VM)
# is refused: it neither measures the real CPU dispatch nor builds reliably.
[[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]] || { echo 'Run on native Linux x86-64' >&2; exit 2; }
engine_arch=$(docker info --format '{{.Architecture}}')
[[ "$engine_arch" == x86_64 ]] || { echo "Docker engine architecture is $engine_arch" >&2; exit 2; }
python3 - <<'EOF' || { echo 'Python 3.11 or newer is required' >&2; exit 2; }
import sys
sys.exit(sys.version_info < (3, 11))
EOF
work=$(realpath -m "$work")
case "$work/" in "$(realpath -m "$HOME/kicad-live")/"*|/mnt/*) echo 'Use a dedicated Linux work directory' >&2; exit 2 ;; esac
[[ "$work" != / ]] || { echo 'Use a dedicated Linux work directory' >&2; exit 2; }
[[ "$work" != "$HOME" ]] || { echo 'Use a dedicated Linux work directory' >&2; exit 2; }
mkdir -p "$work"

# Serialize with other heavy port builds on the shared host (port/AGENTS.md).
lock="$HOME/kicad-port.build-lock"
if ! mkdir "$lock"; then
    echo "Build lock busy: $lock; retry after its owner finishes" >&2
    exit 3
fi
trap 'rmdir "$lock"' EXIT

run_id=$(date -u +%Y%m%dT%H%M%SZ)
out="$work/runs/$run_id"
mkdir -p "$work/runs"
mkdir "$out"
# All Git operations occur before building, in a native Linux checkout. A
# Windows-prepared bundle bypasses Git entirely; its externally recorded digest
# and intended head are mandatory. Mutable checkout paths are never built.
source_tool="$here/tools/source_binding.py"
if [[ -n "$difftest_repo" ]]; then
    python3 "$source_tool" checkout --repo "$difftest_repo" --revision "$revision"
fi
if [[ -z "$source_bundle" ]]; then
    source_bundle="$out/source-bundle"
    export_args=(--repo "$repo" --revision "$revision" --out "$source_bundle")
    if [[ -n "$stage" ]]; then export_args+=(--stage "$(realpath "$stage")"); fi
    python3 "$source_tool" export "${export_args[@]}" > "$out/source-export.log"
    source_sha=$(sha256sum "$source_bundle/source.json" | cut -d' ' -f1)
else
    [[ -z "$stage" && "$source_sha" =~ ^[0-9a-f]{64}$ ]] || { echo 'Bundle requires its digest and forbids a stage override' >&2; exit 2; }
fi
snapshot="$out/snapshot"
python3 "$source_tool" materialize --bundle "$source_bundle" --revision "$revision" \
    --source-sha256 "$source_sha" --out "$snapshot"
# From here onward only the verified canonical source snapshot is consumed.
repo="$snapshot/source"
here="$repo/port/reference-env"
source_tool="$here/tools/source_binding.py"
source_check() {
    python3 "$source_tool" check --bundle "$snapshot" --revision "$revision" --source-sha256 "$source_sha"
}
stage="$snapshot/stage"
lock_sha=$(sha256sum "$here/inputs.lock.json" | cut -d' ' -f1)
tag="$(python3 -c 'import sys,tomllib; c=tomllib.load(open(sys.argv[1],"rb")); print(c["image"]["reference"]+":"+c["image"]["tag"])' "$here/candidate.toml")"
# Pull the base by digest alone; the Dockerfile's tag@digest names the same object.
base="$(sed -n 's/^FROM \([^:@ ]*\)[^@ ]*\(@sha256:[0-9a-f]*\).*/\1\2/p' "$here/Dockerfile")"
[[ "$base" =~ ^docker\.io/library/ubuntu@sha256:[0-9a-f]{64}$ ]] || { echo "Unexpected base $base" >&2; exit 2; }

python3 "$here/tools/check_candidate.py"
python3 "$here/tools/fetch_inputs.py" --lock "$here/inputs.lock.json" --cache "$work/cache" \
    --dest "$work/context/inputs"
# Fresh context: keep only the verified staged inputs and exact snapshot files.
for directory in image probe tools; do rm -rf "$work/context/$directory"; done
python3 "$source_tool" context --repo "$repo" --bundle "$snapshot" --revision "$revision" \
    --source-sha256 "$source_sha" --out "$work/context"

# Keep engine evidence even if the first pull/build fails. Image-bound host
# provenance is written only after an image has been built successfully.
docker version --format '{{json .}}' > "$out/docker-version.json"
docker info --format '{{json .}}' > "$out/docker-info.json"
docker pull --platform linux/amd64 "$base"
build() {
    source_check
    python3 "$source_tool" context --bundle "$snapshot" --revision "$revision" \
        --source-sha256 "$source_sha" --out "$work/context"
    docker build --network=none --platform linux/amd64 --pull=false "$@" \
        --build-arg "LOCK_SHA256=$lock_sha" "$work/context"
}
build --tag "$tag" --iidfile "$out/image.iid"
image=$(cat "$out/image.iid")
docker image inspect "$image" > "$out/image-inspect.json"
docker run --rm --network=none "$image" cat /opt/refenv/image-identity.json > "$out/image-identity.json"

if (( rebuild_check )); then
    # An independent no-cache rebuild must reproduce the identity record (package
    # set, identity file hashes, compiler and Rust facts). The image ID is
    # recorded too; it is not expected to match because layers carry timestamps.
    build --no-cache --iidfile "$out/rebuild.iid"
    docker run --rm --network=none "$(cat "$out/rebuild.iid")" cat /opt/refenv/image-identity.json \
        > "$out/rebuild-identity.json"
    if cmp -s "$out/image-identity.json" "$out/rebuild-identity.json"; then
        echo 'rebuild identity: identical' | tee "$out/rebuild-check.txt"
    else
        echo 'rebuild identity: DIFFERENT' | tee "$out/rebuild-check.txt"
        diff "$out/image-identity.json" "$out/rebuild-identity.json" >> "$out/rebuild-check.txt" || true
        exit 1
    fi
fi

python3 "$here/tools/host_provenance.py" --run-id "$run_id" --image "$image" --tag "$tag" \
    --lock-sha256 "$lock_sha" --repo "$repo" --docker-version "$out/docker-version.json" \
    --docker-info "$out/docker-info.json" --source-manifest "$snapshot/source.json" > "$out/host-provenance.json"

user=(--user "$(id -u):$(id -g)" -e HOME=/work/home
      -e "REFENV_REVISION=$revision" -e "REFENV_SOURCE_SHA=$source_sha" -v "$snapshot:/snapshot:ro")
mkdir -p "$work/oracle-work/home"
probe_args=()
if (( with_oracle )); then
    # A fixed in-container path keeps absolute paths in the audited provenance
    # (and the embedded binary) the same on every host.
    rm -rf "$work/oracle-work/kicad-port"
    docker run --rm --network=none "${user[@]}" -e ORACLE_JOBS="${ORACLE_JOBS:-8}" \
        -v "$stage:/stage:ro" -v "$repo/port/oracle:/src/oracle:ro" -v "$work/oracle-work:/work" \
        "$image" bash -c 'set -euo pipefail
            python3 /opt/refenv/image/verify_image.py --lock /opt/refenv/inputs.lock.json \
                --candidate /opt/refenv/candidate.toml --expect /opt/refenv/image-identity.json
            python3 /opt/refenv/source_binding.py check --bundle /snapshot --revision "$REFENV_REVISION" --source-sha256 "$REFENV_SOURCE_SHA"
            bash /src/oracle/build.sh /stage /src/oracle /work/kicad-port
            python3 /opt/refenv/source_binding.py check --bundle /snapshot --revision "$REFENV_REVISION" --source-sha256 "$REFENV_SOURCE_SHA"
            /work/kicad-port/oracle-build/kicad-oracle --provenance > /work/oracle-provenance.json' \
        2>&1 | tee "$out/oracle-build.log"
    mkdir -p "$out/oracle"
    cp "$work/oracle-work/oracle-provenance.json" "$work/oracle-work/kicad-port/oracle-build/binary.sha256" \
        "$work/oracle-work/kicad-port/oracle-build/compile_commands.json" "$out/oracle/"
    cp "$work/oracle-work/kicad-port/oracle-src/reference/source-manifest.json" "$out/oracle/"
    probe_args=(--oracle-build /work/kicad-port/oracle-build)
fi

mkdir -p "$out/probe"
set +e
docker run --rm --network=none "${user[@]}" -v "$out/probe:/out" -v "$work/oracle-work:/work:ro" \
    "$image" python3 /opt/refenv/probe/run_probe.py --out /out --sweep-count "$sweep" "${probe_args[@]}" \
    2>&1 | tee "$out/probe.log"
probe_status=${PIPESTATUS[0]}
set -e

difftest_status=skipped
if [[ -n "$difftest_repo" ]]; then
    # Hosted-CI-equivalent difftest inside the image. Needs network for west's
    # PyPI packages and crates.io (both pinned by version/Cargo.lock checksums).
    # The repository must be a Linux clone at the commit under test.
    difftest_repo="$repo" # build the sealed exact source, not the supplied mutable checkout
    mkdir -p "$out/difftest"
    set +e
    docker run --rm "${user[@]}" -e CARGO_HOME=/work/home/cargo -e CARGO_BUILD_JOBS=8 \
        -v "$difftest_repo:/repo" -v "$work/oracle-work:/work" -v "$out/difftest:/out" -w /repo \
        "$image" bash -c 'set -euo pipefail
            python3 /opt/refenv/image/verify_image.py --lock /opt/refenv/inputs.lock.json \
                --candidate /opt/refenv/candidate.toml --expect /opt/refenv/image-identity.json
            python3 /opt/refenv/source_binding.py check --bundle /snapshot --revision "$REFENV_REVISION" --source-sha256 "$REFENV_SOURCE_SHA"
            python3 port/tools/bootstrap_west.py
            python3 /opt/refenv/source_binding.py check --bundle /snapshot --revision "$REFENV_REVISION" --source-sha256 "$REFENV_SOURCE_SHA"
            python3 port/tools/bootstrap_west.py --check
            python3 /opt/refenv/source_binding.py check --bundle /snapshot --revision "$REFENV_REVISION" --source-sha256 "$REFENV_SOURCE_SHA"
            cargo build --locked --manifest-path rust/Cargo.toml -p kicad-difftest -j8
            python3 /opt/refenv/source_binding.py check --bundle /snapshot --revision "$REFENV_REVISION" --source-sha256 "$REFENV_SOURCE_SHA"
            printf "%s\n" "$REFENV_REVISION" > /out/revision.txt
            python3 port/tools/check_goldens_policy.py --ci --runner "$PWD/rust/target/debug/kicad-difftest" \
                --oracle /work/kicad-port/oracle-build/kicad-oracle --reports /out/reports --require-phase0
            python3 /opt/refenv/source_binding.py check --bundle /snapshot --revision "$REFENV_REVISION" --source-sha256 "$REFENV_SOURCE_SHA"' \
        2>&1 | tee "$out/difftest.log"
    difftest_status=${PIPESTATUS[0]}
    set -e
fi

source_check
echo "Run directory: $out"
echo "Probe exit status: $probe_status (1 = contract failed, 4 = C++/Rust mismatch)"
echo "Difftest exit status: $difftest_status"
echo "Image $image was built locally and not pushed. Qualification is not asserted by this script."
[[ "$probe_status" == 0 && ( "$difftest_status" == 0 || "$difftest_status" == skipped ) ]]
