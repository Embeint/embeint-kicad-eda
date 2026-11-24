# SPDX-License-Identifier: GPL-3.0-or-later
FROM ubuntu:24.04
ARG RUNNER_VERSION=2.338.2
ARG RUNNER_SHA256=baebf204307bac6193480852732f0aba2ae97b6c5597c244f4d5558390147c40
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl git tar gzip xz-utils unzip build-essential pkg-config \
    python3 python3-venv libicu74 libssl3t64 zlib1g libkrb5-3 liblttng-ust1t64 \
    && rm -rf /var/lib/apt/lists/*
RUN useradd --create-home --uid 10001 runner && mkdir /runner \
    && curl --fail --location --retry 3 -o /tmp/runner.tar.gz \
       "https://github.com/actions/runner/releases/download/v${RUNNER_VERSION}/actions-runner-linux-x64-${RUNNER_VERSION}.tar.gz" \
    && echo "${RUNNER_SHA256}  /tmp/runner.tar.gz" | sha256sum --check - \
    && tar -xzf /tmp/runner.tar.gz -C /runner && rm /tmp/runner.tar.gz \
    && chown -R runner:runner /runner
COPY job-started.sh /opt/kicad-ci/job-started.sh
RUN chmod 755 /opt/kicad-ci/job-started.sh
ENV HOME=/home/runner \
    PATH=/home/runner/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    CARGO_BUILD_JOBS=2 \
    RUNNER_TOOL_CACHE=/home/runner/toolcache \
    ACTIONS_RUNNER_HOOK_JOB_STARTED=/opt/kicad-ci/job-started.sh
USER runner
# Tool/download cache only. Numerical certification does not use this runner.
RUN curl --proto '=https' --tlsv1.2 --fail --silent --show-error https://sh.rustup.rs \
    -o /tmp/rustup.sh && sh /tmp/rustup.sh -y --profile minimal --default-toolchain none \
    && rm /tmp/rustup.sh \
    && rustup toolchain install 1.99.1 --profile minimal --component clippy,rustfmt --target wasm32-unknown-unknown \
    && rustup toolchain install 1.85.0 --profile minimal --target wasm32-unknown-unknown \
    && rustup default 1.99.1
WORKDIR /runner
ENTRYPOINT ["/runner/run.sh"]
