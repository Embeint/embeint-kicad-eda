# SPDX-License-Identifier: GPL-3.0-or-later
"""Exercise WSL setup's fail-fast package report without invoking its build path."""

import os
from pathlib import Path
import shutil
import subprocess
import sys

import pytest


@pytest.mark.skipif(sys.platform != "linux", reason="Ubuntu/WSL setup dependency reporting")
def test_setup_reports_shader_tools_and_nng_before_touching_build_root(tmp_path):
    bash = shutil.which("bash")
    if bash is None:
        pytest.skip("bash unavailable")
    fake_bin = tmp_path / "bin"
    fake_bin.mkdir()
    dpkg = fake_bin / "dpkg"
    dpkg.write_text('#!/bin/sh\ncase "$2" in glslang-tools|spirv-tools|libnng-dev) exit 1;; esac\nexit 0\n')
    dpkg.chmod(0o755)
    forbidden = tmp_path / "unexpected-build-action"
    for name in ("cmake", "ninja", "rsync", "apt-get", "mkdir"):
        sentinel = fake_bin / name
        sentinel.write_text(f'#!/bin/sh\nprintf unexpected > "{forbidden}"\nexit 99\n')
        sentinel.chmod(0o755)
    build_root = tmp_path / "untouched-build"
    script = Path(__file__).parent / "scripts/setup_wsl.sh"
    result = subprocess.run([bash, str(script)], text=True, capture_output=True, timeout=5,
                            env={**os.environ, "PATH":str(fake_bin) + os.pathsep + os.environ["PATH"],
                                 "KICAD_LIVE_ROOT":str(build_root)})
    assert result.returncode == 1
    assert result.stderr.strip() == "Install these first:  sudo apt-get install glslang-tools spirv-tools libnng-dev"
    assert not result.stdout
    assert not build_root.exists()
    assert not forbidden.exists(), "setup invoked a build action despite missing dependencies"
