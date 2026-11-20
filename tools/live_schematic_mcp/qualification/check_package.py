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


NOTICES = Path(__file__).resolve().with_name("notices")


def check_notices(package, build_info):
    """The licence notices are present, and the notices kept in notices/ were written for the
    versions this package bundles (pynng's nng and Mbed TLS, the interpreter's OpenSSL)."""
    manifest = json.loads((NOTICES / "manifest.json").read_text(encoding="utf-8"))
    licenses = package / "licenses"
    entries = [*manifest["distributions"].values(), *manifest["runtime"].values()]
    required = ["KiCad-GPL.txt", "Python-LICENSE.txt",
                *(f"third-party/{name}" for entry in entries for name in entry["notices"])]
    for name in required:
        assert (licenses / name).is_file(), f"missing licence notice {name}"
    dependencies = build_info.get("dependencies", {})
    for name, entry in manifest["distributions"].items():
        assert dependencies.get(name) == entry["version"], (
            f"the package bundles {name} {dependencies.get(name)}, but notices/ covers {name} "
            f"{entry['version']} ({entry['components']}); update notices/")
        assert (licenses / name).is_dir(), f"missing licence files of {name}"
    openssl = manifest["runtime"]["openssl"]
    assert str(build_info.get("openssl_version", "")).startswith(f"OpenSSL {openssl['major_version']}."), (
        f"the package bundles {build_info.get('openssl_version')}, but notices/ covers OpenSSL "
        f"{openssl['major_version']}.x")


def tool_definition(tool):
    """A tool as advertised, with its description as inspect.cleandoc gives it: Python 3.13 and
    later strip docstring indentation, so only that form is comparable across interpreters."""
    import inspect
    entry = {"name": tool.name, "description": inspect.cleandoc(tool.description or ""),
             "inputSchema": tool.inputSchema}
    if tool.outputSchema is not None:
        entry["outputSchema"] = tool.outputSchema
    return entry


def check_rust_files(package, rust):
    """The bundled Rust executables are the ones recorded, and their notices are present."""
    for name, digest in rust["binaries"].items():
        assert sha256((package / name).read_bytes()).hexdigest() == digest, f"{name} is not the recorded build"
    notices = package / "licenses" / "rust"
    assert (notices / "README.txt").is_file(), "missing licenses/rust/README.txt"
    assert (notices / "rust-standard-library" / "COPYRIGHT-library.html").is_file(), (
        "missing the Rust standard library notice")
    for crate in rust["crates"]:
        for name in crate["files"]:
            assert (notices / f"{crate['name']}-{crate['version']}" / name).is_file(), (
                f"missing licence file {name} of {crate['name']} {crate['version']}")


async def check_rust(package, rust, folder, environment, python_tools, layout):
    """The native server starts from the package alone and offers the Python server's tools, with
    the same schemas; the layout runner answers and the frozen server finds it."""
    windows = sys.platform == "win32"
    native = package / ("embeint-mcp.exe" if windows else "embeint-mcp")
    runner = package / ("embeint-layout.exe" if windows else "embeint-layout")
    version = subprocess.run([str(native), "--version"], cwd=folder, env=environment, capture_output=True,
                             text=True, check=True, timeout=20).stdout.split()
    assert version == ["embeint-mcp", rust["mcp"]["server_version"]], version
    request = json.dumps({"pins": [{"number": "1", "name": "VIN", "type": "power_in"},
                                   {"number": "2", "name": "GND", "type": "power_in"},
                                   {"number": "3", "name": "VOUT", "type": "power_out"}]})
    laid_out = json.loads(subprocess.run([str(runner)], input=request + "\n", cwd=folder, env=environment,
                                         capture_output=True, text=True, check=True, timeout=20).stdout)
    assert len(laid_out.get("pins", [])) == 3 and "body" in laid_out, laid_out
    assert layout.get("runner") and Path(layout["runner"]).resolve() == runner.resolve(), (
        f"the frozen server does not find the bundled layout runner: {layout}")
    params = StdioServerParameters(command=str(native), cwd=folder, env=environment)
    async with stdio_client(params) as (reader, writer):
        async with ClientSession(reader, writer) as session:
            initialized = await session.initialize()
            assert initialized.serverInfo.version == rust["mcp"]["server_version"], initialized.serverInfo
            tools = (await session.list_tools()).tools
            names = sorted(tool.name for tool in tools)
            assert names == sorted(rust["mcp"]["tools"]), names
            for tool in tools:
                assert tool_definition(tool) == python_tools.get(tool.name), (
                    f"{tool.name} differs from the bundled Python server's tool")
            reply = await session.call_tool("live_mcp_capabilities", {})
            assert not reply.isError, reply
            inventory = json.loads(reply.content[0].text)
            assert inventory["schema_sha256"] == rust["mcp"]["schema_sha256"], inventory["schema_sha256"]
            assert inventory["tool_count"] == len(tools) == rust["mcp"]["tool_count"]
            invalid = await session.call_tool("live_operation_status", {"operation_id": []})
            assert invalid.isError, invalid
            unknown = await session.call_tool("unknown_embeint_tool", {})
            assert unknown.isError, unknown
    eof = subprocess.run([str(native)], input="", cwd=folder, env=environment, capture_output=True,
                         text=True, check=True, timeout=20)
    assert not eof.stdout, "the native server wrote unsolicited protocol output on EOF"
    return {"server_version": rust["mcp"]["server_version"], "tools": names,
            "schema_sha256": rust["mcp"]["schema_sha256"], "crates": len(rust["crates"]),
            "checks": ["binaries", "notices", "version", "layout_runner", "runner_found_by_python",
                       "initialize", "tools_match_python", "capabilities", "invalid_arguments",
                       "unknown_tool", "clean_eof"]}


async def check(executable, expected_skill_manifest=None):
    expected = json.loads((executable.parent / "build-info.json").read_text())
    if expected_skill_manifest is not None:
        assert expected["skills"] == expected_skill_manifest, "source and build skill manifests differ"
    check_notices(executable.parent, expected)
    if "rust" in expected:
        check_rust_files(executable.parent, expected["rust"])
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
        native = None
        if "rust" in expected:
            native = await check_rust(executable.parent, expected["rust"], folder, environment,
                                      {tool.name: tool_definition(tool) for tool in tools},
                                      report["runtime"].get("layout", {}))
        return {"result": "passed", "schema_sha256": inventory["schema_sha256"],
                "server_version": version["server_version"],
                "application_version": version["application_version"],
                "skills": skill_manifest,
                "source_skill_manifest_compared": expected_skill_manifest is not None,
                "tool_count": len(tools), "command_count": inventory["command_count"],
                "checks": ["offline_diagnostics", "version", "clean_eof", "initialize", "tools", "capabilities",
                           "resource", "describe", "skill_discovery", "skill_tools_and_resources_all_files",
                           "skill_hashes", "missing_and_traversal_errors", "invalid_arguments", "unknown_tool"],
                "native_rust": native, "editor_contacted": False}


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
