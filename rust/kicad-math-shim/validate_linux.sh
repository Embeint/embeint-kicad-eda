#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Disposable Ubuntu 24.04 linux/amd64 validation of libs.kimath.include.math.application.pure.p001.
# Run inside a throwaway container; the checkout is read-only, results go to /out:
#
#   python3 port/oracle/prepare.py --output-dir STAGE        # on the host, clean committed tree
#   docker run --rm --platform linux/amd64 -v "$PWD":/src:ro -v STAGE:/stage:ro -v OUT:/out \
#       ubuntu:24.04 bash /src/rust/kicad-math-shim/validate_linux.sh [bless]
#
# Without "bless" it builds/audits the oracle and runs all checks plus the live compare (and
# check, when a golden exists). With "bless" it also runs propose and bless against the live
# oracle and writes the golden and evidence into /out for review and commit.
set -euo pipefail
mode=${1:-check}
unit=libs.kimath.include.math.application.pure.p001
work=/work
repo=$work/repo
export DEBIAN_FRONTEND=noninteractive CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4}
log() { printf '\n== %s\n' "$*"; }

log "environment"
uname -a
apt-get update -qq
apt-get install -y -qq --no-install-recommends g++ gcc cmake ninja-build python3 rsync git \
    libboost-dev libwxgtk3.2-dev curl ca-certificates > /dev/null
ldd --version | head -1
g++ --version | head -1
if [[ ! -x "$HOME/.cargo/bin/cargo" ]]; then
    curl -sSf https://sh.rustup.rs | sh -s -- -y -q --profile minimal --default-toolchain 1.99.1 \
        --component clippy,rustfmt --target wasm32-unknown-unknown
fi
source "$HOME/.cargo/env"
rustc --version
cat > "$work-cpu.c" <<'EOF'
#include <stdio.h>
int main( void )
{
    __builtin_cpu_init();
    printf( "cpu fma=%d avx2=%d\n", __builtin_cpu_supports( "fma" ),
            __builtin_cpu_supports( "avx2" ) );
    return 0;
}
EOF
gcc -O2 "$work-cpu.c" -o "$work-cpu" && "$work-cpu"

log "copy checkout"
mkdir -p "$repo" /out
rsync -a --delete --exclude=target/ --exclude=.git --exclude=__pycache__ /src/ "$repo/"
cd "$repo"

log "oracle build, source/flag audit and ctest"
ORACLE_JOBS=${ORACLE_JOBS:-4} bash port/oracle/build.sh /stage "$repo/port/oracle" "$work/kicad-port"
oracle=$work/kicad-port/oracle-build/kicad-oracle
"$oracle" --provenance > /out/tested-oracle-provenance.json
cp "$work/kicad-port/oracle-build/binary.sha256" /out/binary.sha256
"$oracle" --list > /out/oracle-list.json

log "rust format, clippy, tests and wasm"
cargo fmt --manifest-path rust/Cargo.toml --all --check
cargo clippy --locked --manifest-path rust/Cargo.toml -p kicad-math -p kicad-math-shim \
    -p kicad-difftest --all-targets -- -D warnings
cargo test --locked --manifest-path rust/Cargo.toml -p kicad-math -p kicad-math-shim
cargo build --locked --manifest-path rust/Cargo.toml -p kicad-math --target wasm32-unknown-unknown
cargo build --locked --manifest-path rust/Cargo.toml -p kicad-difftest -p kicad-math-shim
rust=$repo/rust/target/debug/kicad-math-shim
runner=$repo/rust/target/debug/kicad-difftest
sha256sum "$rust" "$runner" > /out/rust-binaries.sha256

log "unit records"
python3 port/tools/validate_units.py

log "refusal parity"
python3 rust/kicad-math-shim/test_refusals.py "$oracle" "$rust" | tee /out/refusals.json

log "raw outputs and libm variant probe"
cases=port/cases/$unit.jsonl
"$oracle" < "$cases" > /out/oracle-replies.jsonl
"$rust" < "$cases" > /out/rust-replies.jsonl
GLIBC_TUNABLES=glibc.cpu.hwcaps=-AVX2,-FMA,-FMA4,-AVX "$oracle" < "$cases" > /out/oracle-replies-sse2.jsonl
sha256sum "$cases" /out/oracle-replies.jsonl /out/rust-replies.jsonl /out/oracle-replies-sse2.jsonl \
    | tee /out/replies.sha256
if cmp -s /out/oracle-replies.jsonl /out/oracle-replies-sse2.jsonl; then
    echo "libm variant probe: oracle replies identical with FMA/AVX ifuncs disabled"
else
    echo "libm variant probe: oracle replies DIFFER with FMA/AVX ifuncs disabled"
fi

log "live exact compare"
"$runner" compare --root "$repo" --unit "$unit" --oracle "$oracle" --rust "$rust" \
    --report /out/live-compare.json

if [[ "$mode" == bless ]]; then
    log "propose and bless"
    mkdir -p port/goldens/evidence
    "$runner" propose --root "$repo" --unit "$unit" --oracle "$oracle" --rust "$rust" \
        --provenance /out/tested-oracle-provenance.json --approval /out/proposal.json
    python3 - /out/proposal.json <<'EOF'
import json, sys
path = sys.argv[1]
approval = json.load(open(path, encoding="utf-8"))
approval["reason"] = ("First baseline for libs.kimath.include.math.application.pure.p001 after exact "
                      "live agreement of the Rust shim and the pinned C++ oracle on every case, "
                      "on Ubuntu 24.04 linux/amd64 (glibc 2.39).")
open(path, "w", encoding="utf-8").write(json.dumps(approval, indent=2) + "\n")
EOF
    "$runner" bless --root "$repo" --unit "$unit" --oracle "$oracle" --rust "$rust" \
        --provenance /out/tested-oracle-provenance.json --approval /out/proposal.json \
        --evidence "port/goldens/evidence/$unit-baseline.json"
    cp "port/goldens/$unit.jsonl" /out/
    cp "port/goldens/evidence/$unit-baseline.json" /out/
fi

if [[ -f "port/goldens/$unit.jsonl" ]]; then
    log "live check against the golden"
    "$runner" check --root "$repo" --unit "$unit" --oracle "$oracle" --rust "$rust" \
        --report /out/live-check.json
fi
log "validation complete ($mode)"
