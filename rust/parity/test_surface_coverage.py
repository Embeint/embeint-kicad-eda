# SPDX-License-Identifier: GPL-3.0-or-later
"""Coverage must reveal missing operations even when the ported subset matches."""

import asyncio
import copy
from types import SimpleNamespace

import pytest

from surface_coverage import SURFACES, compare, entries_by_key, exit_status, listed


def inventory():
    return {surface: [{key: f"example-{surface}", "description": "A definition"}]
            for surface, key in SURFACES.items()}


@pytest.mark.parametrize("surface", SURFACES)
def test_new_reference_entry_blocks_complete_surface(surface):
    python, rust = inventory(), inventory()
    python[surface].append({SURFACES[surface]: "new-operation"})
    report = compare(python, rust)
    assert report["surfaces"][surface]["missing"] == ["new-operation"]
    assert report["shared_definitions_match"] is True
    assert exit_status(report) == 0
    assert exit_status(report, require_complete=True) == 1


@pytest.mark.parametrize("surface", SURFACES)
def test_changed_shared_definition_always_fails(surface):
    python, rust = inventory(), inventory()
    rust[surface][0]["inputSchema"] = {"type": "string"}
    report = compare(python, rust)
    assert exit_status(report) == 1
    assert exit_status(report, require_complete=True) == 1


def test_additions_are_allowed_but_do_not_offset_missing_tools():
    python, rust = inventory(), inventory()
    rust["tools"] = [{"name": "new-rust-tool"}]
    report = compare(python, rust)
    assert report["complete_reference_surface"] is False
    rust["tools"].extend(copy.deepcopy(python["tools"]))
    report = compare(python, rust)
    assert exit_status(report, require_complete=True) == 0
    assert report["behavioral_parity_verified"] is False
    assert report["native_implementation_verified"] is False


def test_duplicate_names_cannot_inflate_coverage():
    with pytest.raises(ValueError, match="Duplicate advertised name"):
        entries_by_key([{"name": "same"}, {"name": "same"}], "name")


def test_python_docstring_indentation_is_normalized():
    python, rust = inventory(), inventory()
    python["tools"][0]["description"] = "Summary\n    Details"
    rust["tools"][0]["description"] = "Summary\nDetails"
    assert compare(python, rust)["complete_reference_surface"] is True


def test_inventory_follows_all_pages():
    from mcp.types import Tool

    async def page(cursor=None):
        name = "first" if cursor is None else "second"
        return SimpleNamespace(tools=[Tool(name=name, inputSchema={"type": "object"})],
                               nextCursor="next" if cursor is None else None)

    assert [entry["name"] for entry in asyncio.run(listed(page, "tools"))] == ["first", "second"]


def test_repeated_page_cursor_is_an_error():
    async def page(cursor=None):
        return SimpleNamespace(tools=[], nextCursor="same")

    with pytest.raises(ValueError, match="Repeated pagination cursor"):
        asyncio.run(listed(page, "tools"))
