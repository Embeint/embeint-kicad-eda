# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline registration inventory; it deliberately makes no editor IPC requests."""

import hashlib
import json

from commands import COMMANDS, describe, message_class


CAPABILITIES_URI = "embeint://mcp/capabilities"


def local_capabilities(tools):
    """Describe loaded tool and protobuf schemas without claiming live editor support."""
    tool_schemas = []
    for tool in sorted(tools, key=lambda item: item.name):
        entry = {"name": tool.name, "input_schema": tool.inputSchema}
        if tool.outputSchema is not None:
            entry["output_schema"] = tool.outputSchema
        tool_schemas.append(entry)

    command_schemas = []
    for command, (response, _kind) in sorted(COMMANDS.items()):
        # Resolve both sides now so an inventory cannot silently advertise stale schemas.
        message_class(command)
        message_class(response)
        command_schemas.append(describe(command))

    inventory = {"tools": tool_schemas, "commands": command_schemas}
    fingerprint = hashlib.sha256(
        json.dumps(inventory, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()
    return {
        "manifest_version": 1,
        "server": "Embeint KiCad Live",
        "transport": "stdio",
        "registration_source": "loaded MCP tools and local protobuf command registry",
        "editor_connection_attempted": False,
        "live_support_verified": False,
        "limitations": [
            "Local registration is not capability negotiation with a running KiCad instance.",
            "Commands may be unavailable for an editor, document, or application version.",
            "Operation kind does not guarantee undo or rollback support.",
            "The registry is a curated adapter surface, not every declared protobuf message.",
        ],
        "tool_count": len(tool_schemas),
        "command_count": len(command_schemas),
        "schema_sha256": fingerprint,
        **inventory,
    }
