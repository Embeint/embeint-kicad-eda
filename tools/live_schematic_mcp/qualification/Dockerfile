# SPDX-License-Identifier: GPL-3.0-or-later
# Native Linux build and isolated Xvfb qualification environment.
FROM ubuntu:24.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential cmake ninja-build git ca-certificates gettext swig \
    python3-dev python3-venv libwxgtk3.2-dev libwxgtk-webview3.2-dev \
    libglm-dev libglew-dev libcairo2-dev libboost-all-dev unixodbc-dev \
    libharfbuzz-dev libfontconfig-dev libwayland-dev libgtk-3-dev libssl-dev \
    libpixman-1-dev libxml2-dev libsqlite3-dev libeigen3-dev libgit2-dev \
    libsecret-1-dev libzstd-dev protobuf-compiler libprotobuf-dev libabsl-dev \
    libcurl4-openssl-dev libspnav-dev libngspice0-dev libocct-data-exchange-dev \
    libocct-visualization-dev libpoppler-glib-dev libnng-dev libatomic1 \
    xvfb xauth xdotool rsync kicad-symbols kicad-footprints \
    && rm -rf /var/lib/apt/lists/*
ENV LIBGL_ALWAYS_SOFTWARE=1
RUN apt-get update && apt-get install -y --no-install-recommends \
    glslang-tools spirv-tools libvulkan-dev lld \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /qualification
