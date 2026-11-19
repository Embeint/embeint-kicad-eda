# SPDX-License-Identifier: GPL-3.0-or-later
"""Check a frozen MCP executable through real stdio from an empty working directory."""

import argparse
import asyncio
from hashlib import sha256
import json
import os
from pathlib import Path
import tempfile
import subprocess
import sys

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client
from pydantic import AnyUrl


async def check(executable, expected_skill_manifest=None):
    expected = json.loads((executable.parent / "build-info.json").read_text())
    if expected_skill_manifest is not None:
        assert expected["skills"] == expected_skill_manifest, "source and build skill manifests differ"
    licenses = executable.parent / "licenses"
    for required in ("KiCad-GPL.txt", "Python-LICENSE.txt", "third-party/nng-LICENSE.txt",
                     "third-party/mbedtls-LICENSE.txt"):
        assert (licenses / required).is_file(), f"missing licence notice {required}"
    with tempfile.TemporaryDirectory(prefix="embeint-mcp-install-check-") as folder:
        environment = {"PATH": folder, "HOME": folder,
                       "PYTHONPATH": str(Path(folder) / "absent-source"),
                       "PYTHONHOME": str(Path(folder) / "absent-python"),
                       "KICAD_API_SOCKET": "ipc:///nonexistent-embeint-package-check/api.sock"}
        if sys.platform == "win32":
            # Windows processes need SYSTEMROOT (e.g. for random numbers); the endpoint must be
            # a valid local pipe URL that nothing serves
            environment.update({name: os.environ[name] for name in ("SYSTEMROOT", "TEMP", "TMP")
                                if name in os.environ})
            environment["KICAD_API_SOCKET"] = "ipc://" + str(Path(folder) / "absent-kicad" / "api.sock")
        diagnostic = subprocess.run([str(executable), "--diagnose"], cwd=folder, env=environment,
                                    capture_output=True, text=True, check=True, timeout=20)
        report = json.loads(diagnostic.stdout)
        assert report["runtime"]["protocols"]["state"] == "ready", report
        assert not report["connection_attempted"], report
        version = json.loads(subprocess.run([str(executable), "--version"], cwd=folder, env=environment,
                                            capture_output=True, text=True, check=True,
                                            timeout=60).stdout)
        assert version["distribution"] == "bundled", version
        assert version["server_version"] == expected["mcp_version"], version
        assert version["schema_sha256"] == expected["schema_sha256"], version
        assert version["build_schema_sha256_matches"] is True, version
        assert version["application_version"] == expected.get("application_version"), version
        eof = subprocess.run([str(executable)], input="", cwd=folder, env=environment,
                             capture_output=True, text=True, check=True, timeout=20)
        assert not eof.stdout, "server wrote unsolicited protocol output on EOF"
        assert "Traceback" not in eof.stderr, eof.stderr
        params = StdioServerParameters(
            command=str(executable), cwd=folder,
            env=environment)
        async with stdio_client(params) as (reader, writer):
            async with ClientSession(reader, writer) as session:
                initialized = await session.initialize()
                assert "live_skill_reference" in (initialized.instructions or "")
                assert "kicad-house-style" in initialized.instructions
                assert initialized.serverInfo.version == expected["mcp_version"], initialized.serverInfo
                tools = (await session.list_tools()).tools
                reply = await session.call_tool("live_mcp_capabilities", {})
                assert not reply.isError, reply
                inventory = reply.structuredContent or json.loads(reply.content[0].text)
                assert inventory["schema_sha256"] == expected["schema_sha256"]
                assert inventory["tool_count"] == len(tools) == expected["tool_count"]
                assert inventory["command_count"] == expected["command_count"]
                resource = await session.read_resource(AnyUrl("embeint://mcp/capabilities"))
                assert json.loads(resource.contents[0].text) == inventory
                resources = {str(item.uri) for item in (await session.list_resources()).resources}
                assert {"kicad-skill://index", "kicad-skill://manifest"} <= resources
                templates = (await session.list_resource_templates()).resourceTemplates
                assert "kicad-skill://{skill}/{file}" in {item.uriTemplate for item in templates}
                manifest_reply = await session.read_resource(AnyUrl("kicad-skill://manifest"))
                skill_manifest = json.loads(manifest_reply.contents[0].text)
                assert skill_manifest == expected["skills"]
                index_reply = await session.read_resource(AnyUrl("kicad-skill://index"))
                index = json.loads(index_reply.contents[0].text)
                listing = await session.call_tool("live_skill_reference", {})
                assert not listing.isError
                assert (listing.structuredContent or json.loads(listing.content[0].text))["skills"] == index
                observed_hashes = {}
                for entry in index:
                    assert entry["version"] == skill_manifest["skill_versions"][entry["skill"]]
                    for name in entry["files"]:
                        uri = f"kicad-skill://{entry['skill']}/{name}"
                        resource = await session.read_resource(AnyUrl(uri))
                        reply = await session.call_tool("live_skill_reference", {"skill": entry["skill"], "file": name})
                        assert not reply.isError
                        content = reply.structuredContent or json.loads(reply.content[0].text)
                        assert content["text"] == resource.contents[0].text
                        digest = sha256(content["text"].encode("utf-8")).hexdigest()
                        assert digest == content["sha256"]
                        observed_hashes[f"{entry['skill']}/{name}"] = digest
                assert observed_hashes == skill_manifest["skill_file_sha256"]
                digest = sha256(json.dumps(observed_hashes, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
                assert digest == skill_manifest["skill_content_sha256"]
                for name in ("missing.md", "../server.py", "..\\server.py", "/SKILL.md"):
                    invalid = await session.call_tool("live_skill_reference", {"skill": "kicad-house-style", "file": name})
                    assert invalid.isError
                described = await session.call_tool("live_schematic_describe_command", {"command": "GetItems"})
                assert not described.isError, described
                invalid = await session.call_tool("live_schematic_item_schema", {"type_name": []})
                assert invalid.isError
                unknown = await session.call_tool("unknown_embeint_tool", {})
                assert unknown.isError
                return {"result": "passed", "schema_sha256": inventory["schema_sha256"],
                        "server_version": version["server_version"],
                        "application_version": version["application_version"],
                        "skills": skill_manifest,
                        "source_skill_manifest_compared": expected_skill_manifest is not None,
                        "tool_count": len(tools), "command_count": inventory["command_count"],
                        "checks": ["offline_diagnostics", "version", "clean_eof", "initialize", "tools", "capabilities", "resource", "describe",
                                   "skill_discovery", "skill_tools_and_resources_all_files", "skill_hashes",
                                   "missing_and_traversal_errors", "invalid_arguments", "unknown_tool"],
                        "editor_contacted": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    parser.add_argument("--expected-skill-manifest", type=Path,
                        help="source skill manifest captured independently before packaging")
    args = parser.parse_args()
    source = (json.loads(args.expected_skill_manifest.read_text(encoding="utf-8"))
              if args.expected_skill_manifest else None)
    print(json.dumps(asyncio.run(asyncio.wait_for(check(args.executable.resolve(), source), 60)), indent=2))


if __name__ == "__main__":
    main()
