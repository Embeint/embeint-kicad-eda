#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Compare the complete advertised MCP surfaces, without contacting a KiCad editor.

Missing entries are reported during migration. --require-complete makes them failures.
Matching advertisements do not establish native implementation or behavioral parity;
protocol fixtures and live workflow qualification remain separate requirements.
"""

import argparse
import asyncio
import inspect
import json
import os
from pathlib import Path
import sys

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client


ROOT = Path(__file__).resolve().parents[2]
SURFACES = {"tools": "name", "resources": "uri", "resource_templates": "uriTemplate",
            "prompts": "name", "commands": "command"}


def entries_by_key(entries, key):
    result = {}
    for entry in entries:
        entry = dict(entry)
        if isinstance(entry.get("description"), str):
            entry["description"] = inspect.cleandoc(entry["description"])
        name = entry[key]
        if name in result:
            raise ValueError(f"Duplicate advertised {key}: {name}")
        result[name] = entry
    return result


def compare(python, rust):
    """Compare every reference entry, without a curated list of ported tools."""
    surfaces = {}
    for surface, key in SURFACES.items():
        reference = entries_by_key(python[surface], key)
        native = entries_by_key(rust[surface], key)
        shared = reference.keys() & native.keys()
        surfaces[surface] = {
            "python_count": len(reference), "rust_count": len(native),
            "missing": sorted(reference.keys() - native.keys()),
            "extra": sorted(native.keys() - reference.keys()),
            "different": sorted(name for name in shared if reference[name] != native[name]),
        }
    compatible = all(not entry["different"] for entry in surfaces.values())
    complete = compatible and all(not entry["missing"] for entry in surfaces.values())
    return {
        "report_version": 1,
        "evidence_kind": "mcp_advertisements",
        "editor_connection_attempted": False,
        "behavioral_parity_verified": False,
        "native_implementation_verified": False,
        "shared_definitions_match": compatible,
        "complete_reference_surface": complete,
        "surfaces": surfaces,
    }


def exit_status(report, require_complete=False):
    return int(not report["shared_definitions_match"]
               or (require_complete and not report["complete_reference_surface"]))


async def listed(method, field):
    entries, cursors = [], set()
    cursor = None
    while True:
        reply = await method(cursor=cursor)
        entries.extend(item.model_dump(mode="json", exclude_none=True)
                       for item in getattr(reply, field))
        cursor = reply.nextCursor
        if cursor is None:
            return entries
        if cursor in cursors:
            raise ValueError(f"Repeated pagination cursor while listing {field}")
        cursors.add(cursor)


async def inventory(parameters):
    async with stdio_client(parameters) as (read, write):
        async with ClientSession(read, write) as session:
            initialization = await session.initialize()
            result = {
                "identity": initialization.serverInfo.model_dump(mode="json", exclude_none=True),
                "tools": await listed(session.list_tools, "tools"),
                "resources": await listed(session.list_resources, "resources"),
                "resource_templates": await listed(session.list_resource_templates, "resourceTemplates"),
                "prompts": await listed(session.list_prompts, "prompts"),
            }
            # This resource is explicitly offline in both implementations. Do not enumerate
            # editor documents or execute generic commands while measuring the surface.
            resource = await session.read_resource("embeint://mcp/capabilities")
            manifest = json.loads("".join(item.text for item in resource.contents))
            result["commands"] = manifest["commands"]
            result["schema_sha256"] = manifest["schema_sha256"]
            return result


async def collect(binary):
    env = {key: value for key, value in os.environ.items()
           if key not in {"KICAD_API_SOCKET", "KICAD_API_TOKEN", "KICAD_API_TIMEOUT_MS"}}
    python = await inventory(StdioServerParameters(
        command=sys.executable, args=[str(ROOT / "tools/live_schematic_mcp/server.py")], env=env))
    rust = await inventory(StdioServerParameters(command=str(binary.resolve()), env=env))
    report = compare(python, rust)
    report["servers"] = {name: {key: source[key] for key in ("identity", "schema_sha256")}
                         for name, source in (("python", python), ("rust", rust))}
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "rust/target/release" /
                        ("embeint-mcp.exe" if sys.platform == "win32" else "embeint-mcp"))
    parser.add_argument("--report", type=Path)
    parser.add_argument("--require-complete", action="store_true",
                        help="fail if any Python entry is absent or differs")
    args = parser.parse_args()
    report = asyncio.run(collect(args.binary))
    if args.report:
        args.report.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    for name, entry in report["surfaces"].items():
        print(f"{name}: {len(entry['missing'])} missing, {len(entry['different'])} different, "
              f"{len(entry['extra'])} Rust additions")
    print("Complete reference surface: " + str(report["complete_reference_surface"]))
    print("Advertisement coverage only; native implementation and behavior require separate evidence.")
    return exit_status(report, args.require_complete)


if __name__ == "__main__":
    sys.exit(main())
