# SPDX-License-Identifier: GPL-3.0-or-later
# CANDIDATE Linux x86-64 reference environment for the Rust port. Build it only
# through qualify.sh, which stages verified inputs and builds with --network=none.
# The base reference must equal candidate.toml and inputs.lock.json (tested).
FROM docker.io/library/ubuntu:noble-20261110.1@sha256:9a04f13b3f80ef056361e30868593b1cfd2279fe19e4081cb93a90bf56819d6a

ARG LOCK_SHA256
ENV DEBIAN_FRONTEND=noninteractive \
    LANG=C.UTF-8 \
    LC_ALL=C.UTF-8 \
    TZ=UTC \
    PYTHONDONTWRITEBYTECODE=1

# One step: verify the signature/hash chain of every staged input, install exactly
# the locked local packages, then the locked Rust components. The image has no
# package index, so apt cannot fetch, choose or upgrade anything else; an
# incomplete or inconsistent lock fails here instead of silently resolving.
RUN --mount=type=bind,target=/opt/refenv-context,readonly \
    set -eu; \
    test -n "$LOCK_SHA256"; \
    echo "$LOCK_SHA256  /opt/refenv-context/inputs.lock.json" | sha256sum -c --quiet -; \
    sh /opt/refenv-context/image/verify_chain.sh /opt/refenv-context/inputs /opt/refenv-context/inputs.lock.json "$LOCK_SHA256"; \
    apt-get install -y --no-install-recommends --no-download /opt/refenv-context/inputs/debs/*.deb; \
    rust=$(mktemp -d); \
    while read -r archive component; do \
        tar -xJf "/opt/refenv-context/inputs/rust/$archive" -C "$rust"; \
        "$rust/$component/install.sh" --prefix=/usr/local --disable-ldconfig; \
    done < /opt/refenv-context/inputs/rust-components.list; \
    rm -rf "$rust" /var/lib/apt/lists/* /var/cache/apt/*.bin /var/log/apt /var/log/dpkg.log \
           /var/log/alternatives.log /var/cache/ldconfig/aux-cache

COPY inputs.lock.json candidate.toml /opt/refenv/
COPY image/ /opt/refenv/image/
COPY probe/ /opt/refenv/probe/
COPY tools/source_binding.py /opt/refenv/source_binding.py

# The libm dispatch contract (see candidate.toml [dispatch]): baseline SSE2 code
# for the explicitly checked libm function subset; other paths are uncertified.
ENV GLIBC_TUNABLES=glibc.cpu.hwcaps=-AVX,-AVX2,-FMA,-FMA4,-AVX512F

RUN python3 /opt/refenv/image/verify_image.py --lock /opt/refenv/inputs.lock.json \
        --candidate /opt/refenv/candidate.toml --record /opt/refenv/image-identity.json

LABEL org.opencontainers.image.title="rust-port-reference" \
      org.opencontainers.image.description="CANDIDATE, unqualified: Embeint KiCad Rust port reference environment" \
      org.opencontainers.image.licenses="GPL-3.0-or-later AND distribution package licences" \
      org.opencontainers.image.base.name="docker.io/library/ubuntu:noble-20261110.1" \
      org.opencontainers.image.base.digest="sha256:9a04f13b3f80ef056361e30868593b1cfd2279fe19e4081cb93a90bf56819d6a" \
      com.embeint.refenv.lock-sha256="${LOCK_SHA256}" \
      com.embeint.refenv.status="candidate-unqualified"
