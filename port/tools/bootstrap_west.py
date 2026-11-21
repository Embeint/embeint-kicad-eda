#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Bootstrap the pinned west workspace, or verify it offline with --check."""

import argparse
import configparser
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib
import venv


def run(args, root, *, capture=False):
    environment = os.environ.copy()
    # Windows may have checked these sources out as CRLF. Normalize Git's input
    # on both hosts, keep new checkouts LF, and do not change user/local config.
    count = int(environment.get("GIT_CONFIG_COUNT", "0"))
    for key, value in (("core.autocrlf", "input"), ("core.filemode", "false")):
        environment[f"GIT_CONFIG_KEY_{count}"] = key
        environment[f"GIT_CONFIG_VALUE_{count}"] = value
        count += 1
    environment["GIT_CONFIG_COUNT"] = str(count)
    environment["GIT_OPTIONAL_LOCKS"] = "0"
    result = subprocess.run(
        [str(arg) for arg in args], cwd=root, check=True,
        text=True, encoding="utf-8", capture_output=capture, env=environment,
    )
    return result.stdout.strip() if capture else None


def load_pins(root):
    pins = tomllib.loads((root / "port/pins.toml").read_text(encoding="utf-8"))
    if pins["schema_version"] != 1:
        raise ValueError("unsupported port/pins.toml schema_version")
    for section, key in (
        ("kicad", "revision"), ("fork", "initial_rust_revision"),
        ("solidcore", "revision"), ("solidcore", "solidcraft_revision"),
    ):
        if not re.fullmatch(r"[0-9a-f]{40}", pins[section][key]):
            raise ValueError(f"{section}.{key} must be a full lowercase commit SHA")
    # Cargo's relative path is part of the interface; changing it requires a PR.
    if pins["solidcore"]["path"] != ".port-deps/solidcore":
        raise ValueError("solidcore.path must match the Cargo path .port-deps/solidcore")
    return pins


def check_manifest(root, python, pins):
    data = json.loads(run([
        python, "-c",
        "import json, pathlib, yaml; "
        "print(json.dumps(yaml.safe_load(pathlib.Path('west.yml').read_text(encoding='utf-8'))))",
    ], root, capture=True))
    solidcore = pins["solidcore"]
    expected = {"manifest": {
        "version": "1.2", "projects": [{
            "name": "solidcore", "url": solidcore["url"],
            "revision": solidcore["revision"], "path": solidcore["path"],
        }], "self": {"path": "."},
    }}
    if data != expected:
        raise ValueError("west.yml disagrees with port/pins.toml; update both in one PR")
    run([python, "-m", "west", "manifest", "--validate"], root)


def configure(root, check):
    path = root / ".west/config"
    if path.exists():
        config = configparser.ConfigParser()
        config.read(path, encoding="utf-8")
        if (config.get("manifest", "path", fallback=None) != "."
                or config.get("manifest", "file", fallback="west.yml") != "west.yml"):
            raise ValueError(f"{path} belongs to another manifest; refusing to overwrite it")
    elif check:
        raise ValueError("missing .west/config; run bootstrap_west.py first")
    else:
        # A local `west init -l .` would choose the parent as topdir. Use west's
        # standard configuration so worktrees each own an independent workspace.
        path.parent.mkdir(exist_ok=True)
        path.write_text("[manifest]\npath = .\nfile = west.yml\n", encoding="utf-8")


def check_checkout(root, pins, *, before_update=False):
    pin = pins["solidcore"]
    checkout = root / pin["path"]
    if checkout.resolve() != root.resolve() / pin["path"]:
        raise ValueError("solidcore checkout must not redirect outside .port-deps")
    if not checkout.exists() and before_update:
        return
    if not (checkout / ".git").is_dir():
        raise ValueError(f"{checkout} is not a west project checkout")
    origin = run(["git", "remote", "get-url", "origin"], checkout, capture=True)
    if origin != pin["url"]:
        raise ValueError(f"unexpected solidcore origin: {origin}")
    if run(["git", "status", "--porcelain", "--untracked-files=all"], checkout, capture=True):
        raise ValueError("solidcore checkout is dirty; preserve its edits before updating")
    if before_update:
        return
    if run(["git", "rev-parse", "HEAD"], checkout, capture=True) != pin["revision"]:
        raise ValueError("solidcore HEAD differs from pin; run west update")
    message = run(["git", "show", "-s", "--format=%B", "HEAD"], checkout, capture=True)
    trailer = f"solidcraft: {pin['solidcraft_revision']}"
    if trailer not in message.splitlines():
        raise ValueError("solidcore commit does not carry the pinned solidcraft trailer")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify offline without changes")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    pins = load_pins(root)
    host = "windows" if sys.platform == "win32" else sys.platform
    environment = root / ".port-venv" / host
    python = environment / ("Scripts/python.exe" if sys.platform == "win32" else "bin/python")
    requirements = root / "port/tools/requirements-west.txt"
    if not args.check:
        if not python.is_file():
            venv.EnvBuilder(with_pip=True).create(environment)
        run([python, "-m", "pip", "install", "--disable-pip-version-check", "-r", requirements], root)
    elif not python.is_file():
        raise ValueError("missing west venv; run bootstrap_west.py first")
    installed = json.loads(run([
        python, "-c",
        "import importlib.metadata as m, json; "
        "print(json.dumps({d.metadata['Name'].lower(): d.version for d in m.distributions()}))",
    ], root, capture=True))
    for line in requirements.read_text(encoding="utf-8").splitlines():
        if line and not line.startswith("#"):
            name, version = line.split("==")
            if installed.get(name.lower()) != version:
                raise ValueError(f"west venv dependency differs: {name}; rerun bootstrap")
    if installed["west"] != pins["tools"]["west_version"]:
        raise ValueError("requirements-west.txt disagrees with tools.west_version")
    configure(root, args.check)
    check_manifest(root, python, pins)
    check_checkout(root, pins, before_update=not args.check)
    if not args.check:
        run([python, "-m", "west", "update"], root)
        check_checkout(root, pins)
    print(f"Verified solidcore {pins['solidcore']['revision']}")
    print(f"west executable: {python.parent / ('west.exe' if sys.platform == 'win32' else 'west')}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        print(f"bootstrap: {error}", file=sys.stderr)
        sys.exit(1)
