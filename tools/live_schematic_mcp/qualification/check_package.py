# SPDX-License-Identifier: GPL-3.0-or-later
"""Check a frozen MCP executable through real stdio from an empty working directory."""

import argparse
import asyncio
import json
import os
from pathlib import Path
import tempfile
import subprocess

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client
from pydantic import AnyUrl


async def check(executable):
    expected = json.loads((executable.parent / "build-info.json").read_text())
    with tempfile.TemporaryDirectory(prefix="embeint-mcp-install-check-") as folder:
        environment = {"PATH": os.defpath, "HOME": folder,
                       "KICAD_API_SOCKET": "ipc:///nonexistent-embeint-package-check/api.sock"}
        diagnostic = subprocess.run([str(executable), "--diagnose"], cwd=folder, env=environment,
                                    capture_output=True, text=True, check=True, timeout=20)
        report = json.loads(diagnostic.stdout)
        assert report["runtime"]["protocols"]["state"] == "ready", report
        assert not report["connection_attempted"], report
        eof = subprocess.run([str(executable)], input="", cwd=folder, env=environment,
                             capture_output=True, text=True, check=True, timeout=20)
        assert not eof.stdout, "server wrote unsolicited protocol output on EOF"
        assert "Traceback" not in eof.stderr, eof.stderr
        params = StdioServerParameters(
            command=str(executable), cwd=folder,
            env=environment)
        async with stdio_client(params) as (reader, writer):
            async with ClientSession(reader, writer) as session:
                await session.initialize()
                tools = (await session.list_tools()).tools
                reply = await session.call_tool("live_mcp_capabilities", {})
                assert not reply.isError, reply
                inventory = reply.structuredContent or json.loads(reply.content[0].text)
                assert inventory["schema_sha256"] == expected["schema_sha256"]
                assert inventory["tool_count"] == len(tools) == expected["tool_count"]
                assert inventory["command_count"] == expected["command_count"]
                resource = await session.read_resource(AnyUrl("embeint://mcp/capabilities"))
                assert json.loads(resource.contents[0].text) == inventory
                described = await session.call_tool("live_schematic_describe_command", {"command": "GetItems"})
                assert not described.isError, described
                invalid = await session.call_tool("live_schematic_item_schema", {"type_name": []})
                assert invalid.isError
                unknown = await session.call_tool("unknown_embeint_tool", {})
                assert unknown.isError
                return {"result": "passed", "schema_sha256": inventory["schema_sha256"],
                        "tool_count": len(tools), "command_count": inventory["command_count"],
                        "checks": ["offline_diagnostics", "clean_eof", "initialize", "tools", "capabilities", "resource", "describe",
                                   "invalid_arguments", "unknown_tool"],
                        "editor_contacted": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    args = parser.parse_args()
    print(json.dumps(asyncio.run(asyncio.wait_for(check(args.executable.resolve()), 60)), indent=2))


if __name__ == "__main__":
    main()
