# SPDX-License-Identifier: GPL-3.0-or-later
"""Check helper source identity, frozen inclusion and bundled reference delivery.

Run in the isolated package build environment (PyInstaller required). The child
server uses real stdio with no checkout or system Python through check_package.
This is scripted delivery evidence, not an independent agent or native workflow.
"""

import argparse
import asyncio
from hashlib import sha256
import json
from pathlib import Path
import sys

from PyInstaller.archive.readers import CArchiveReader

from check_package import check

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import skills


def require(condition, message):
    # Explicit failure rather than assert, so `python -O` cannot write a passing receipt.
    if not condition:
        raise SystemExit(f"check failed: {message}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    parser.add_argument("source_mcp", type=Path, help="exact archived tools/live_schematic_mcp used to build")
    parser.add_argument("output", type=Path, help="new JSON receipt (must not exist)")
    parser.add_argument("--source-archive", type=Path, help="retain the exact source archive checksum")
    parser.add_argument("--native-source-mcp", type=Path,
                        help="optional native-tested helper source; compare after newline normalization")
    args = parser.parse_args()
    executable = args.executable.resolve()
    source = args.source_mcp.resolve()
    if args.output.exists():
        parser.error("output already exists; choose a new receipt")
    info_path = executable.parent / "build-info.json"
    info = json.loads(info_path.read_text())
    helper_hashes = {}
    for name in ("block_helpers.py", "block_example.py"):
        helper_hashes[name] = sha256((source / name).read_bytes()).hexdigest()
        require(helper_hashes[name] == info["source_sha256"][name], f"build source differs: {name}")
    embedded = CArchiveReader(str(executable)).open_embedded_archive("PYZ.pyz")
    require("block_helpers" in embedded.toc, "helper module missing from frozen archive")
    skills.SKILLS_DIR = source / "skills"
    expected = skills.skill_manifest()
    require("kicad-house-style/block-helpers.md" in expected["skill_file_sha256"],
            "helper reference missing from the bundled skill manifest")
    result = asyncio.run(check(executable, expected))
    result.update({"kind": "scripted_fresh_stdio_and_frozen_delivery_check",
                   "source_revision": info["source_revision"], "revision_source": info["revision_source"],
                   "source_dirty": info["source_dirty"], "helper_source_sha256": helper_hashes,
                   "executable_sha256": sha256(executable.read_bytes()).hexdigest(),
                   "build_info_sha256": sha256(info_path.read_bytes()).hexdigest(),
                   "independent_agent_evidence": False, "native_editor_contacted": False,
                   "limitations": ["No helper execution MCP tool added",
                                   "No native editor mutation in this check",
                                   "Independent fresh-session workflow awaits controller evaluator"]})
    result["checks"].append("block_helpers_module_in_PYZ_archive")
    if args.source_archive:
        result["source_archive_sha256"] = sha256(args.source_archive.read_bytes()).hexdigest()
    if args.native_source_mcp:
        hashes = {}
        for name in helper_hashes:
            raw = (args.native_source_mcp / name).read_bytes()
            built = (source / name).read_bytes()
            require(raw.replace(b"\r\n", b"\n") == built.replace(b"\r\n", b"\n"),
                    f"native-tested source differs: {name}")
            hashes[name] = sha256(raw).hexdigest()
        result["native_source_sha256"] = hashes
        result["native_source_comparison"] = "Equal after CRLF/LF newline normalization"
    # Exclusive creation retains receipts from earlier runs instead of overwriting evidence.
    with args.output.open("x", encoding="utf-8") as stream:
        json.dump(result, stream, indent=2, sort_keys=True)
        stream.write("\n")
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
