# SPDX-License-Identifier: GPL-3.0-or-later
"""End-to-end check of searching, building and registering library symbols live.

Run against a development KiCad with a schematic open. Creates a scratch library in the GLOBAL
table of the running KiCad's configuration, builds a symbol in the Symbol Editor, saves it, places
it in the open schematic (unsaved), then unregisters and deletes the scratch library.

    python e2e_symbols.py [output_dir] [library_dir]
"""

from pathlib import Path
import sys
import time
import uuid

from client import LiveSchematic, as_dict, document, sch, symbol_document


def check(condition, message):
    if not condition:
        raise SystemExit("FAILED: " + message)
    print("ok  ", message)


def main():
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "live-smoke-output")
    lib_dir = Path(sys.argv[2] if len(sys.argv) > 2 else Path.home() / "kicad-live" / "libs")
    out.mkdir(parents=True, exist_ok=True)
    lib_dir.mkdir(parents=True, exist_ok=True)
    client = LiveSchematic()
    root = document(client.documents()["documents"][0])

    for _ in range(120):  # libraries load in the background after startup
        found = client.search_symbols("regulator 3.3", limit=5)
        if not found.get("libraries_still_loading"):
            break
        time.sleep(1)
    check(found["total"] > 0 and found["results"][0].get("description"),
          f"search 'regulator 3.3' found {found['total']}: {found['results'][0]}")
    resistor = client.library_symbol("Device:R")
    check(len(resistor["pins"]) == 2 and resistor["reference"] == "R", "read Device:R with its two pins")
    stock_doc = symbol_document(client.edit_symbol("Regulator_Linear:AP2112K-3.3"))
    stock_png = client.symbol_render(stock_doc, dpi=600)
    (out / "stock.png").write_bytes(stock_png)
    check(stock_png[:4] == bytes([0x89]) + b"PNG", "opened and rendered a stock symbol in the Symbol Editor")

    tag = uuid.uuid4().hex[:6]
    nickname, path = f"mcp_test_{tag}", lib_dir / f"mcp_test_{tag}.kicad_sym"
    client.add_library(nickname, str(path), scope="global", description="MCP end-to-end test")
    check(path.exists(), f"created {path}")
    check(any(r["nickname"] == nickname and r["scope"] == "global" for r in client.library_table("global")),
          "library is in the global table")
    try:
        doc_dict = client.new_symbol(nickname, "TESTREG", reference_prefix="U", manufacturer="Test Semiconductor")
        doc = symbol_document(doc_dict)
        check(doc.lib_id.entry_name == "TESTREG", f"new symbol open in the Symbol Editor: {doc_dict['lib_id']}")
        try:
            client.new_symbol(nickname, "TESTREG")
            duplicate_refused = False
        except RuntimeError:
            duplicate_refused = True
        check(duplicate_refused, "a second symbol with the same name is refused")

        pins = [{"number": "1", "name": "GND", "type": "power_in"},
                {"number": "2", "name": "VOUT", "type": "power_out"},
                {"number": "3", "name": "VIN", "type": "power_in"},
                {"number": "4", "name": "EN", "type": "input"},
                {"number": "5", "name": "ADJ", "type": "passive"}]
        laid_out = client.symbol_add_pins(doc, pins)
        summary = client.symbol_summary(doc)
        check(sorted(p["number"] for p in summary["pins"]) == ["1", "2", "3", "4", "5"]
              and len(summary["graphic_shape_ids"]) == 1, f"5 pins and a body added: {laid_out['body_mm']}")
        at = {p["name"]: p["position_mm"] for p in summary["pins"]}
        check(at["VIN"][0] < 0 < at["VOUT"][0] and at["EN"][0] < 0 < at["ADJ"][0],
              "VIN/EN on the left, VOUT/ADJ on the right")
        check(round(at["EN"][1] - at["VIN"][1], 2) == 7.62 and round(at["ADJ"][1] - at["VOUT"][1], 2) == 7.62,
              "EN and ADJ sit a resistor's length (7.62 mm) below VIN and VOUT")

        client.symbol_set_properties(
            doc, value="TESTREG", footprint="Package_TO_SOT_SMD:SOT-223-3_TabPin2",
            datasheet="https://example.com/testreg.pdf", description="Test 3.3 V LDO regulator",
            keywords="ldo regulator mcptest", footprint_filters=["SOT?223*"], mpn=f"TR-{tag}")
        png = client.symbol_render(doc, dpi=600)
        check(png.startswith(b"\x89PNG"), f"rendered the unsaved symbol ({len(png)} bytes)")
        (out / "symbol.png").write_bytes(png)

        client.symbol_save(doc)
        text = path.read_text()
        check('"TESTREG"' in text and f"TR-{tag}" in text and "SOT-223-3_TabPin2" in text,
              "saved symbol, footprint and MPN are in the library file")

        lib_id = f"{nickname}:TESTREG"
        stored = client.library_symbol(lib_id)
        check(stored["description"] == "Test 3.3 V LDO regulator" and len(stored["pins"]) == 5,
              f"library now serves {lib_id} with its details")
        def body_style(symbol_id):
            for child in client.library_symbols([symbol_id])[0].items:
                shape = sch.SchematicGraphicShape()
                if child.item.Unpack(shape):
                    return as_dict(shape.shape.attributes)
        stock = body_style("Regulator_Linear:AP2112K-3.3")
        check(body_style(lib_id) == stock, f"body outline and fill match stock symbols: {stock}")
        check(stored["fields"].get("Manufacturer") == "Test Semiconductor" and stored["fields"].get("MPN") == f"TR-{tag}",
              f"Manufacturer and MPN stored: {stored['fields']}")
        check(any(r["lib_id"] == lib_id for r in client.search_symbols("mcptest", limit=50)["results"]),
              "keyword search finds the new symbol")

        placed = client.place_symbol(root, lib_id, 120, 120)
        check(placed["reference"].startswith("U"), f"placed {lib_id} in the schematic as {placed['reference']}")
        pins_live = client.symbol_pins(root, [placed["id"]], with_nets=False)[0]["pins"]
        check(len(pins_live) == 5, "placed symbol has all 5 pins in the schematic")
        (out / "symbol_placed.png").write_bytes(client.render(root, dpi=100))
    finally:
        client.remove_library(nickname, scope="global")
        path.unlink(missing_ok=True)
    check(not any(r["nickname"] == nickname for r in client.library_table("global")),
          "scratch library unregistered and deleted")
    print("OK: symbol search, live build, global library and placement verified")


if __name__ == "__main__":
    main()
