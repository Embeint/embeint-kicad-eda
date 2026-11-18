# SPDX-License-Identifier: GPL-3.0-or-later
"""Exercise the installed distribution in a clean runtime container over MCP stdio.

The driver runs outside the container. The application container has no checkout,
MCP virtual environment, exposed API socket or network access. Use only a newly
started Dockerfile.runtime container: this driver creates its disposable project.
"""

import argparse
import asyncio
from contextlib import asynccontextmanager
import json
from pathlib import Path
import subprocess
import time

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client


def docker(container, *args, input=None):
    return subprocess.check_output(["docker", "exec", "-i", container, *args], input=input, text=True)


def payload(reply):
    assert not reply.isError, reply
    return reply.structuredContent or json.loads(reply.content[0].text)


@asynccontextmanager
async def session(container):
    params = StdioServerParameters(command="docker", args=["exec", "-i", container, "/usr/bin/embeint-kicad-mcp"])
    async with stdio_client(params) as (reader, writer):
        async with ClientSession(reader, writer) as client:
            await client.initialize()
            yield client


async def start(container):
    pid = docker(container, "sh", "-c",
                 "/usr/bin/embeint-kicad /home/qualification/live-demo/live-demo.kicad_pro "
                 ">/home/qualification/app.log 2>&1 & echo $!").strip()
    assert pid.isdecimal(), pid
    deadline = time.monotonic() + 90
    async with session(container) as client:
        while time.monotonic() < deadline:
            # Version is a read; no edit/open request is replayed after a timeout.
            reply = await client.call_tool("live_schematic_documents", {})
            if not reply.isError:
                payload(await client.call_tool("live_schematic_open", {}))
                return pid
            await asyncio.sleep(0.5)
    raise RuntimeError("installed application did not become ready")


def stop(container, pid):
    docker(container, "kill", "-TERM", pid)


async def check(container):
    configuration = json.loads(subprocess.check_output(["docker", "inspect", container], text=True))[0]
    assert configuration["HostConfig"]["NetworkMode"] == "none", "use --network none"
    assert not configuration["Mounts"], "runtime check must have no host mounts"
    assert configuration["Config"]["User"] == "qualification", "use the unprivileged runtime image"
    docker(container, "mkdir", "/home/qualification/live-demo")
    docker(container, "sh", "-c", "cat > /home/qualification/live-demo/live-demo.kicad_pro",
           input='{"meta":{"version":3}}\n')
    docker(container, "sh", "-c", "cat > /home/qualification/live-demo/live-demo.kicad_sch",
           input='(kicad_sch (version 20250114) (generator "eeschema")\n'
                 '(uuid "6976536f-f7a2-4323-874c-c6aa91f39999") (paper "A4")\n'
                 '(lib_symbols) (sheet_instances (path "/" (page "1"))))\n')
    subprocess.run(["docker", "exec", "-d", container, "Xvfb", ":99", "-screen", "0", "1600x1200x24"], check=True)
    await asyncio.sleep(1)
    pid = None
    try:
        pid = await start(container)
        async with session(container) as client:
            inventory = payload(await client.call_tool("live_mcp_capabilities", {}))
            documents = payload(await client.call_tool("live_schematic_documents", {}))
            document = documents["documents"][0]
            for reference, x in (("R1", 40), ("R2", 70)):
                payload(await client.call_tool("live_schematic_place_symbol", {
                    "document_spec": document, "lib_id": "Device:R", "x_mm": x, "y_mm": 40,
                    "reference": reference}))
            payload(await client.call_tool("live_schematic_connect_pins", {
                "document_spec": document, "from_reference": "R1", "from_pin": "2",
                "to_reference": "R2", "to_pin": "2"}))
            before = payload(await client.call_tool("live_schematic_items", {"document_spec": document}))
            payload(await client.call_tool("live_schematic_undo", {"document_spec": document}))
            payload(await client.call_tool("live_schematic_undo", {"document_spec": document, "redo": True}))
            restored = payload(await client.call_tool("live_schematic_items", {"document_spec": document}))
            canonical = lambda value: sorted(json.dumps(i, sort_keys=True) for i in value.get("items", []))
            assert canonical(before) == canonical(restored), "undo/redo changed the design"
            payload(await client.call_tool("live_schematic_save", {"document_spec": document}))
            saved = payload(await client.call_tool("live_schematic_items", {"document_spec": document}))
            netlist = payload(await client.call_tool("live_schematic_netlist", {"document_spec": document}))
        stop(container, pid)
        pid = None
        # Wait only for the process this check launched, without closing other applications.
        await asyncio.sleep(2)
        pid = await start(container)
        async with session(container) as client:
            documents = payload(await client.call_tool("live_schematic_documents", {}))
            document = documents["documents"][0]
            reopened = payload(await client.call_tool("live_schematic_items", {"document_spec": document}))
            assert canonical(saved) == canonical(reopened), "installed save/reopen changed items"
            assert payload(await client.call_tool("live_schematic_netlist", {"document_spec": document})) == netlist
        return {"result": "passed", "schema_sha256": inventory["schema_sha256"],
                "version": documents["version"], "checks": ["install", "first_launch", "stdio",
                "place_symbols", "connect_pins", "undo_redo", "save_restart_reopen", "connectivity"],
                "network": "none", "host_mounts": False}
    finally:
        if pid is not None:
            stop(container, pid)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("container")
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    result = asyncio.run(asyncio.wait_for(check(args.container), timeout=300))
    args.output.write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
