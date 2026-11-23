#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Write the Docker host side of a qualification run's provenance (JSON on stdout).

A container cannot see its own image digest or the engine that runs it; this
records them from the host. The image is local and unpushed.
"""

import argparse
import json
import platform
import subprocess

DOCKER_INFO_FIELDS = ("Architecture", "OSType", "OperatingSystem", "KernelVersion", "ServerVersion", "Driver",
                      "NCPU", "MemTotal", "CgroupVersion", "SecurityOptions", "DefaultRuntime")


def git(repo, *args):
    result = subprocess.run(["git", "-C", repo, *args], capture_output=True, text=True)
    return result.stdout.strip() if result.returncode == 0 else None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("--run-id", "--image", "--tag", "--lock-sha256", "--repo", "--docker-version", "--docker-info"):
        parser.add_argument(name, required=True)
    args = parser.parse_args()
    with open(args.docker_info, encoding="utf-8") as stream:
        info = json.load(stream)
    with open(args.docker_version, encoding="utf-8") as stream:
        version = json.load(stream)
    record = {
        "schema": 1,
        "run_id": args.run_id,
        "image_id": args.image,
        "image_tag": args.tag,
        "pushed": False,
        "lock_sha256": args.lock_sha256,
        "checkout_revision": git(args.repo, "rev-parse", "HEAD"),
        "checkout_status": git(args.repo, "status", "--porcelain"),
        "host_uname": platform.uname()._asdict(),
        "docker_version": version,
        "docker_info": {key: info.get(key) for key in DOCKER_INFO_FIELDS},
        "qualification": "not asserted",
    }
    print(json.dumps(record, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
