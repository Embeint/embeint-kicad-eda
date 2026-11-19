# SPDX-License-Identifier: GPL-3.0-or-later
"""MCP protocol and local registration contracts; no running KiCad is required."""

import asyncio
import json
from pathlib import Path
import sys

import pytest
from jsonschema.validators import validator_for
from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client
from pydantic import AnyUrl

from commands import COMMANDS, message_class
import server
import client
from support import CAPABILITIES_URI, local_capabilities


@pytest.mark.parametrize("url", [r"ipc://C:\Temp\kicad\api.sock", "ipc:///"])
def test_unsupported_default_path_allows_offline_startup_but_never_dials(monkeypatch, url):
    monkeypatch.delenv("KICAD_API_SOCKET", raising=False)
    monkeypatch.setattr(client.sys, "platform", "win32")
    monkeypatch.setattr(client, "default_socket_url", lambda: url)
    monkeypatch.setattr(client.pynng, "Req0", lambda **_: pytest.fail("unsupported IPC dialed"))
    live = client.LiveSchematic()
    with pytest.raises(client.DiscoveryError) as error:
        live.send(client.base.GetVersion(), client.base.GetVersionResponse)
    assert error.value.code == "invalid_socket"


def test_local_capabilities_never_contact_the_editor(monkeypatch):
    def reject_ipc(*_args, **_kwargs):
        raise AssertionError("offline discovery must not send editor requests")

    monkeypatch.setattr(server.client, "send", reject_ipc)
    manifest = asyncio.run(server.live_mcp_capabilities())
    resource = json.loads(asyncio.run(server.local_capability_resource()))
    assert manifest == resource
    assert manifest["editor_connection_attempted"] is False
    assert manifest["live_support_verified"] is False
    assert manifest["command_count"] == len(COMMANDS)
    assert manifest["tool_count"] == len(asyncio.run(server.mcp.list_tools()))
    assert "live_mcp_capabilities" in {tool["name"] for tool in manifest["tools"]}


def test_registered_tool_and_command_schemas_are_valid():
    tools = asyncio.run(server.mcp.list_tools())
    assert len({tool.name for tool in tools}) == len(tools)
    for tool in tools:
        assert tool.inputSchema["type"] == "object", tool.name
        validator_for(tool.inputSchema).check_schema(tool.inputSchema)
        if tool.outputSchema is not None:
            validator_for(tool.outputSchema).check_schema(tool.outputSchema)
    for command, (response, kind) in COMMANDS.items():
        assert message_class(command).DESCRIPTOR.full_name == command
        assert message_class(response).DESCRIPTOR.full_name == response
        assert kind in {"read", "edit", "view", "file", "job"}
    original = local_capabilities(tools)
    reordered = local_capabilities(list(reversed(tools)))
    assert original["schema_sha256"] == reordered["schema_sha256"]
    changed_tool = tools[0].model_copy(
        update={"inputSchema": {**tools[0].inputSchema, "additionalProperties": False}}
    )
    changed = local_capabilities([changed_tool, *tools[1:]])
    assert original["schema_sha256"] != changed["schema_sha256"]


@pytest.mark.parametrize("socket_url", [None, "ipc:///nonexistent-embeint-mcp-test/api.sock"])
def test_stdio_initialization_discovery_tools_and_resource_without_kicad(socket_url):
    async def exercise_protocol():
        parameters = StdioServerParameters(
            command=sys.executable,
            args=[str(Path(server.__file__).resolve())],
            # Offline calls work with default discovery or a missing explicit socket.
            env={} if socket_url is None else {"KICAD_API_SOCKET": socket_url},
        )
        async with stdio_client(parameters) as (reader, writer):
            async with ClientSession(reader, writer) as session:
                initialized = await session.initialize()
                assert "live_skill_reference" in (initialized.instructions or "")
                assert "kicad-house-style" in initialized.instructions
                assert initialized.capabilities.tools is not None
                assert initialized.capabilities.resources is not None
                tools = (await session.list_tools()).tools
                assert "live_mcp_capabilities" in {tool.name for tool in tools}
                result = await session.call_tool("live_mcp_capabilities", {})
                assert not result.isError
                manifest = result.structuredContent or json.loads(result.content[0].text)
                assert manifest["tool_count"] == len(tools)
                assert manifest["live_support_verified"] is False
                resources = (await session.list_resources()).resources
                assert CAPABILITIES_URI in {str(resource.uri) for resource in resources}
                content = await session.read_resource(AnyUrl(CAPABILITIES_URI))
                assert json.loads(content.contents[0].text) == manifest
                import skills
                templates = (await session.list_resource_templates()).resourceTemplates
                assert "kicad-skill://{skill}/{file}" in {entry.uriTemplate for entry in templates}
                for uri in (skills.INDEX_URI, skills.MANIFEST_URI):
                    assert uri in {str(resource.uri) for resource in resources}
                result = await session.call_tool("live_skill_reference", {})
                assert not result.isError
                listing = result.structuredContent or json.loads(result.content[0].text)
                index = await session.read_resource(AnyUrl(skills.INDEX_URI))
                assert listing["skills"] == json.loads(index.contents[0].text)
                for entry in listing["skills"]:
                    for name in entry["files"]:
                        reply = await session.call_tool("live_skill_reference", {"skill": entry["skill"], "file": name})
                        assert not reply.isError
                        data = reply.structuredContent or json.loads(reply.content[0].text)
                        resource = await session.read_resource(AnyUrl(data["uri"]))
                        assert resource.contents[0].text == data["text"] == skills.read_skill(entry["skill"], name)["text"]
                data_manifest = await session.read_resource(AnyUrl(skills.MANIFEST_URI))
                assert json.loads(data_manifest.contents[0].text) == skills.skill_manifest()
                for bad in ("missing.md", "../server.py", "..\\secret.md"):
                    reply = await session.call_tool("live_skill_reference", {"skill": skills.DEFAULT_SKILL, "file": bad})
                    assert reply.isError
                with pytest.raises(Exception, match="missing|unsafe|invalid"):
                    await session.read_resource(AnyUrl("kicad-skill://kicad-house-style/missing.md"))
                description = await session.call_tool(
                    "live_schematic_describe_command", {"command": "GetItems"}
                )
                assert not description.isError
                invalid = await session.call_tool(
                    "live_schematic_item_schema", {"type_name": []}
                )
                assert invalid.isError
                unknown = await session.call_tool("unknown_embeint_tool", {})
                assert unknown.isError

    asyncio.run(asyncio.wait_for(exercise_protocol(), timeout=30))
