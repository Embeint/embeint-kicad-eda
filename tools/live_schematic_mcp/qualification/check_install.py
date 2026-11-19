# SPDX-License-Identifier: GPL-3.0-or-later
"""Exercise the installed distribution in a clean runtime container over MCP stdio.

The driver runs outside the runtime. The application has no checkout, MCP virtual
environment, outside API socket or network access. Use a new Dockerfile.runtime
container or a separate installed Bubblewrap root; the driver creates its project.
"""

import argparse
import asyncio
from contextlib import asynccontextmanager
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
import uuid

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from qualification.persistence import canonical_items


class Runtime:
    """Run installed programs in Docker or a source-free Bubblewrap root."""

    def __init__(self, target, kind="docker"):
        self.target, self.kind = target, kind
        if kind == "bwrap":
            self.root = Path(target).resolve(strict=True)
            if self.root == Path("/") or not (self.root / "opt/embeint-kicad/distribution.json").is_file():
                raise ValueError("select a separate root containing the installed distribution")
            for name in ("home/qualification", "tmp", "run"):
                (self.root / name).mkdir(parents=True, exist_ok=True)
            self.temporary = self.root / "tmp" / ("embeint-check-" + uuid.uuid4().hex)
            self.temporary.mkdir(mode=0o700)

    def command(self, *args):
        if self.kind == "docker":
            return ["docker", "exec", "-i", self.target, *args]
        return ["bwrap", "--unshare-user", "--uid", "1000", "--gid", "1000",
                "--unshare-net", "--unshare-pid", "--die-with-parent", "--clearenv",
                "--ro-bind", str(self.root), "/", "--proc", "/proc", "--dev", "/dev",
                *[part for name in ("home/qualification", "run")
                  for part in ("--bind", str(self.root / name), "/" + name)],
                "--bind", str(self.temporary), "/tmp",
                "--setenv", "PATH", "/usr/bin:/bin", "--setenv", "HOME", "/home/qualification",
                "--setenv", "DISPLAY", ":99", "--setenv", "LIBGL_ALWAYS_SOFTWARE", "1",
                "--setenv", "LANG", "C.UTF-8", "--chdir", "/home/qualification", *args]

    def execute(self, *args, input=None):
        return subprocess.check_output(self.command(*args), input=input, text=True)

    def launch(self, *args):
        # Keep the namespace alive for this application. A separate invocation
        # cannot address its private PID namespace, so retain the owned host group.
        log = (self.root / "home/qualification/app.log").open("ab")
        try:
            return subprocess.Popen(self.command(*args), stdout=log, stderr=subprocess.STDOUT,
                                    start_new_session=True)
        finally:
            log.close()

    def stop(self, process):
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=5)

    def validate(self):
        if self.kind == "docker":
            configuration = json.loads(subprocess.check_output(["docker", "inspect", self.target], text=True))[0]
            assert configuration["HostConfig"]["NetworkMode"] == "none", "use --network none"
            assert not configuration["Mounts"], "runtime check must have no host mounts"
            assert configuration["Config"]["User"] == "qualification", "use the unprivileged runtime image"
        self.execute("sh", "-c", "test -z \"$(command -v python || command -v python3)\"")


def payload(reply):
    assert not reply.isError, reply
    return reply.structuredContent or json.loads(reply.content[0].text)


def disk_hashes(container):
    return container.execute("sh", "-c", "sha256sum /home/qualification/live-demo/*.kicad_sch").splitlines()


@asynccontextmanager
async def session(container):
    command = container.command("/usr/bin/embeint-kicad-mcp")
    params = StdioServerParameters(command=command[0], args=command[1:])
    async with stdio_client(params) as (reader, writer):
        async with ClientSession(reader, writer) as client:
            await client.initialize()
            yield client


async def start(container):
    if container.kind == "bwrap":
        pid = container.launch("/usr/bin/embeint-kicad", "/home/qualification/live-demo/live-demo.kicad_pro")
    else:
        pid = container.execute("sh", "-c",
                 "/usr/bin/embeint-kicad /home/qualification/live-demo/live-demo.kicad_pro "
                 ">/home/qualification/app.log 2>&1 & echo $!").strip()
        assert pid.isdecimal(), pid
    try:
        deadline = time.monotonic() + 90
        async with session(container) as client:
            while time.monotonic() < deadline:
                if container.kind == "bwrap" and pid.poll() is not None:
                    raise RuntimeError("installed application exited during startup; inspect app.log")
                # Version is a read; no edit/open request is replayed after a timeout.
                reply = await client.call_tool("live_schematic_documents", {})
                if not reply.isError:
                    payload(await client.call_tool("live_schematic_open", {}))
                    return pid
                await asyncio.sleep(0.5)
        raise RuntimeError("installed application did not become ready")
    except BaseException:
        stop(container, pid)
        raise


def stop(container, pid):
    if container.kind == "bwrap":
        container.stop(pid)
    else:
        container.execute("kill", "-TERM", pid)


def history_refs(container):
    text = container.execute("sh", "-c",
        'for ref in /home/qualification/live-demo/.history/.git/refs/heads/* '
        '/home/qualification/live-demo/.history/.git/refs/tags/Last_Save_sch; do '
        'if test -f "$ref"; then printf "%s " "$ref"; cat "$ref"; fi; done')
    return {line.split()[0].split("/refs/", 1)[1]: line.split()[1]
            for line in text.splitlines()}


async def wait_for_autosave(container, saved=None):
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        refs = history_refs(container)
        heads = [oid for name, oid in refs.items() if name.startswith("heads/")]
        if heads and (saved is None or heads[0] != saved):
            return refs
        await asyncio.sleep(0.5)
    raise AssertionError("no new incremental autosave observed before deadline")


async def check_cli(container, client):
    """kicad-cli of the saved design, from the MCP and from the package's shell wrapper, both in the
    launcher's environment: no library-configuration warnings."""
    schematic = "/home/qualification/live-demo/live-demo.kicad_sch"
    erc = payload(await client.call_tool("live_kicad_cli", {"args": [
        "sch", "erc", "--format", "json", "--severity-all", "-o", "/tmp/installed-erc.json", schematic]}))
    assert erc["returncode"] == 0 and erc["environment"]["source"] == "kicad", erc
    assert "KICAD_CONFIG_HOME" in erc["environment"]["variables"], erc
    report = json.loads(container.execute("cat", "/tmp/installed-erc.json"))
    violations = sorted(v["type"] for sheet in report["sheets"] for v in sheet["violations"])
    assert "lib_symbol_issues" not in violations, violations
    exported = payload(await client.call_tool("live_kicad_cli", {"args": [
        "sch", "export", "netlist", "-o", "/tmp/installed.net", schematic]}))
    assert exported["returncode"] == 0, exported
    text = container.execute("cat", "/tmp/installed.net")
    assert '(ref "R1")' in text and '(ref "R2")' in text, text[-2000:]
    version = container.execute("/usr/bin/embeint-kicad-cli", "version").strip()
    assert version, "embeint-kicad-cli printed no version"
    return {"erc": erc, "erc_violation_types": violations, "netlist_export": exported,
            "wrapper_version": version}


async def check(container, evidence=None, recovery_history=False):
    container.validate()
    container.execute("mkdir", "/home/qualification/live-demo")
    container.execute("sh", "-c", "cat > /home/qualification/live-demo/live-demo.kicad_pro",
           input='{"meta":{"version":3}}\n')
    container.execute("sh", "-c", "cat > /home/qualification/live-demo/live-demo.kicad_sch",
           input='(kicad_sch (version 20250114) (generator "eeschema")\n'
                 '(uuid "6976536f-f7a2-4323-874c-c6aa91f39999") (paper "A4")\n'
                 '(lib_symbols) (sheet_instances (path "/" (page "1"))))\n')
    xvfb = None
    if container.kind == "bwrap":
        xvfb = container.launch("Xvfb", ":99", "-screen", "0", "1600x1200x24")
    else:
        subprocess.run(["docker", "exec", "-d", container.target, "Xvfb", ":99", "-screen", "0", "1600x1200x24"], check=True)
    pid = None
    observations = {}
    try:
        await asyncio.sleep(1)
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
            assert len(before["items"]) >= 3, "placement/wiring returned an empty fixture"
            payload(await client.call_tool("live_schematic_undo", {"document_spec": document}))
            payload(await client.call_tool("live_schematic_undo", {"document_spec": document, "redo": True}))
            restored = payload(await client.call_tool("live_schematic_items", {"document_spec": document}))
            assert canonical_items(before) == canonical_items(restored), "undo/redo changed the design"
            if recovery_history:
                observations["before_save_history"] = await wait_for_autosave(container)
                assert "tags/Last_Save_sch" not in observations["before_save_history"]
            payload(await client.call_tool("live_schematic_save", {"document_spec": document}))
            if recovery_history:
                refs = history_refs(container)
                assert refs["tags/Last_Save_sch"] in [
                    oid for name, oid in refs.items() if name.startswith("heads/")], refs
                observations["after_save_history"] = refs
                # Saving unchanged content must still anchor recovery at the saved HEAD.
                payload(await client.call_tool("live_schematic_save", {"document_spec": document}))
                assert history_refs(container) == refs
            saved = payload(await client.call_tool("live_schematic_items", {"document_spec": document}))
            observations["saved_items"] = saved
            netlist = payload(await client.call_tool("live_schematic_netlist", {"document_spec": document}))
            observations["saved_netlist"] = netlist
            observations["saved_disk_hashes"] = disk_hashes(container)
            assert any({"R1.2", "R2.2"} <= set(net["pins"]) for net in netlist.values()), netlist
            observations["saved_cli"] = await check_cli(container, client)
        stop(container, pid)
        pid = None
        # Wait only for the process this check launched, without closing other applications.
        await asyncio.sleep(2)
        pid = await start(container)
        async with session(container) as client:
            documents = payload(await client.call_tool("live_schematic_documents", {}))
            document = documents["documents"][0]
            reopened = payload(await client.call_tool("live_schematic_items", {"document_spec": document}))
            observations["reopened_items"] = reopened
            assert canonical_items(saved, persisted=True) == canonical_items(reopened, persisted=True), "installed save/reopen changed items"
            assert payload(await client.call_tool("live_schematic_netlist", {"document_spec": document})) == netlist
            observations["reopened_disk_hashes"] = disk_hashes(container)
            assert observations["saved_disk_hashes"] == observations["reopened_disk_hashes"], "reopen changed saved files"
            if recovery_history:
                payload(await client.call_tool("live_schematic_place_symbol", {
                    "document_spec": document, "lib_id": "Device:R", "x_mm": 100,
                    "y_mm": 40, "reference": "R3"}))
                saved = observations["after_save_history"]["tags/Last_Save_sch"]
                refs = await wait_for_autosave(container, saved)
                assert refs["tags/Last_Save_sch"] == saved, refs
                observations["later_unsaved_history"] = refs
        return {"result": "passed", "schema_sha256": inventory["schema_sha256"],
                "version": documents["version"], "checks": ["install", "first_launch", "stdio",
                "place_symbols", "connect_pins", "undo_redo", "save_restart_reopen", "connectivity",
                "cli_erc", "cli_netlist", "cli_wrapper"],
                "network": "none", "runtime": container.kind, "host_source_mounted": False,
                "system_python_present": False, "recovery_history_checked": recovery_history}
    finally:
        if pid is not None:
            stop(container, pid)
        if xvfb is not None:
            container.stop(xvfb)
        if evidence is not None:
            evidence.write_text(json.dumps(observations, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("container")
    parser.add_argument("--runtime", choices=("docker", "bwrap"), default="docker",
                        help="Docker container name, or a separate installed root directory for bwrap")
    parser.add_argument("output", type=Path)
    parser.add_argument("--recovery-history", action="store_true",
                        help="require real incremental autosave, saved recovery marker and later unsaved history")
    args = parser.parse_args()
    result = asyncio.run(asyncio.wait_for(check(Runtime(args.container, args.runtime),
                                              args.output.with_suffix(".snapshots.json"),
                                              args.recovery_history), timeout=300))
    args.output.write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
