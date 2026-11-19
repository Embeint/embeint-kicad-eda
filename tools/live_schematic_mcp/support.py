# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline registration inventory; it deliberately makes no editor IPC requests."""

import hashlib
import json
from pathlib import Path
import sys

from commands import COMMANDS, describe, message_class


CAPABILITIES_URI = "embeint://mcp/capabilities"

# The adapter's own release version.  A frozen package appends its source revision
# (package_mcp.py records it in build-info.json); a checkout reports "+source".
ADAPTER_VERSION = "0.1.0"


def build_info():
    """build-info.json beside a frozen executable, or None when running from a checkout."""
    if not getattr(sys, "frozen", False):
        return None
    try:
        info = json.loads(Path(sys.executable).with_name("build-info.json").read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}
    return info if isinstance(info, dict) else {}


def version_identity():
    """This server's version and the application it was packaged with, without editor contact."""
    info = build_info()
    if info is None:
        return {"server_version": ADAPTER_VERSION + "+source", "distribution": "source checkout",
                "application_version": None, "source_revision": None}
    return {"server_version": info.get("mcp_version") or ADAPTER_VERSION + "+unknown",
            "distribution": "bundled", "application_version": info.get("application_version"),
            "source_revision": info.get("source_revision")}


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
        **version_identity(),
        "transport": "stdio",
        "registration_source": "loaded MCP tools and local protobuf command registry",
        "editor_connection_attempted": False,
        "live_support_verified": False,
        "limitations": [
            "Local registration is not capability negotiation with a running KiCad instance.",
            "application_version is the KiCad this package was built with; live_schematic_documents "
            "reports the running editor's version.",
            "Commands may be unavailable for an editor, document, or application version.",
            "Operation kind does not guarantee undo or rollback support.",
            "The registry is a curated adapter surface, not every declared protobuf message.",
        ],
        "tool_count": len(tool_schemas),
        "command_count": len(command_schemas),
        "schema_sha256": fingerprint,
        **inventory,
    }
