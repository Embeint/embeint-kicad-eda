#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Build and probe the CANDIDATE reference image on a native Linux x86-64 Docker
# host (WSL Ubuntu with a native engine, or a hosted ubuntu-24.04 runner).
# It never pushes an image, never changes goldens and never qualifies the
# candidate by itself: it produces evidence for independent review.
#
# usage: qualify.sh --work DIR [--sweep-count N] [--stage DIR] [--with-oracle]
#                   [--with-difftest REPO] [--rebuild-check]
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$here/../.." && pwd)
work=
sweep=200000
stage=
with_oracle=0
difftest_repo=
rebuild_check=0

usage() {
    sed -n '/^# usage:/,/^set /p' "$0" | sed '$d; s/^# \{0,1\}//' >&2
    exit 2
}
while [[ $# -gt 0 ]]; do
    case "$1" in
        --work) work=${2:?}; shift 2 ;;
        --sweep-count) sweep=${2:?}; shift 2 ;;
        --stage) stage=${2:?}; shift 2 ;;
        --with-oracle) with_oracle=1; shift ;;
        --with-difftest) difftest_repo=${2:?}; with_oracle=1; shift 2 ;;
        --rebuild-check) rebuild_check=1; shift ;;
        *) usage ;;
    esac
done
[[ -n "$work" ]] || usage
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
lock_sha=$(sha256sum "$here/inputs.lock.json" | cut -d' ' -f1)
tag="$(python3 -c 'import sys,tomllib; c=tomllib.load(open(sys.argv[1],"rb")); print(c["image"]["reference"]+":"+c["image"]["tag"])' "$here/candidate.toml")"
# Pull the base by digest alone; the Dockerfile's tag@digest names the same object.
base="$(sed -n 's/^FROM \([^:@ ]*\)[^@ ]*\(@sha256:[0-9a-f]*\).*/\1\2/p' "$here/Dockerfile")"
[[ "$base" =~ ^docker\.io/library/ubuntu@sha256:[0-9a-f]{64}$ ]] || { echo "Unexpected base $base" >&2; exit 2; }

python3 "$here/tools/check_candidate.py"
python3 "$here/tools/fetch_inputs.py" --lock "$here/inputs.lock.json" --cache "$work/cache" \
    --dest "$work/context/inputs"
cp "$here/Dockerfile" "$here/candidate.toml" "$here/inputs.lock.json" "$work/context/"
rm -rf "$work/context/image" "$work/context/probe"
cp -r "$here/image" "$here/probe" "$work/context/"

docker pull --platform linux/amd64 "$base"
build() {
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

docker version --format '{{json .}}' > "$out/docker-version.json"
docker info --format '{{json .}}' > "$work/docker-info.json"
python3 "$here/tools/host_provenance.py" --run-id "$run_id" --image "$image" --tag "$tag" \
    --lock-sha256 "$lock_sha" --repo "$repo" --docker-version "$out/docker-version.json" \
    --docker-info "$work/docker-info.json" > "$out/host-provenance.json"

user=(--user "$(id -u):$(id -g)" -e HOME=/work/home)
mkdir -p "$work/oracle-work/home"
probe_args=()
if (( with_oracle )); then
    if [[ -z "$stage" ]]; then
        # prepare.py needs the pinned commit in this checkout's Git objects. For a
        # Windows worktree, run prepare.py from Windows and pass --stage instead.
        stage="$work/stage"
        python3 "$repo/port/oracle/prepare.py" --output-dir "$stage"
    fi
    stage=$(realpath "$stage")
    # A fixed in-container path keeps absolute paths in the audited provenance
    # (and the embedded binary) the same on every host.
    rm -rf "$work/oracle-work/kicad-port"
    docker run --rm --network=none "${user[@]}" -e ORACLE_JOBS="${ORACLE_JOBS:-8}" \
        -v "$stage:/stage:ro" -v "$repo/port/oracle:/src/oracle:ro" -v "$work/oracle-work:/work" \
        "$image" bash -c 'set -euo pipefail
            python3 /opt/refenv/image/verify_image.py --lock /opt/refenv/inputs.lock.json \
                --candidate /opt/refenv/candidate.toml --expect /opt/refenv/image-identity.json
            bash /src/oracle/build.sh /stage /src/oracle /work/kicad-port
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
    difftest_repo=$(realpath "$difftest_repo")
    mkdir -p "$out/difftest"
    set +e
    docker run --rm "${user[@]}" -e CARGO_HOME=/work/home/cargo -e CARGO_BUILD_JOBS=8 \
        -v "$difftest_repo:/repo" -v "$work/oracle-work:/work" -v "$out/difftest:/out" -w /repo \
        "$image" bash -c 'set -euo pipefail
            python3 /opt/refenv/image/verify_image.py --lock /opt/refenv/inputs.lock.json \
                --candidate /opt/refenv/candidate.toml --expect /opt/refenv/image-identity.json
            git config --global --add safe.directory /repo
            python3 port/tools/bootstrap_west.py
            python3 port/tools/bootstrap_west.py --check
            cargo build --locked --manifest-path rust/Cargo.toml -p kicad-difftest -j8
            git rev-parse HEAD > /out/revision.txt
            python3 port/tools/check_goldens_policy.py --ci --runner "$PWD/rust/target/debug/kicad-difftest" \
                --oracle /work/kicad-port/oracle-build/kicad-oracle --reports /out/reports --require-phase0' \
        2>&1 | tee "$out/difftest.log"
    difftest_status=${PIPESTATUS[0]}
    set -e
fi

echo "Run directory: $out"
echo "Probe exit status: $probe_status (1 = contract failed, 4 = C++/Rust mismatch)"
echo "Difftest exit status: $difftest_status"
echo "Image $image was built locally and not pushed. Qualification is not asserted by this script."
[[ "$probe_status" == 0 && ( "$difftest_status" == 0 || "$difftest_status" == skipped ) ]]
