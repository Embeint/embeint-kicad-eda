# SPDX-License-Identifier: GPL-3.0-or-later
"""End-to-end check of searching, building and registering footprints live, with an embedded STEP.

Run against a development KiCad with a schematic open. Opens the Footprint Editor, generates an
SOIC-8 from its datasheet land pattern into a scratch GLOBAL footprint library, links the datasheet,
embeds a STEP model, saves, links the footprint from a new symbol, places that symbol (unsaved), and
removes the scratch libraries.

    python e2e_footprints.py [output_dir] [library_dir] [step_file]
"""

from pathlib import Path
import shutil
import sys
import tempfile
import time
import uuid

from client import LiveSchematic, document, footprint_document, symbol_document
from e2e_guard import test_document

STOCK = "Package_SO:SOIC-8_3.9x4.9mm_P1.27mm"
STEP = "/mnt/c/Program Files/KiCad/10.0/share/kicad/3dmodels/Package_SO.3dshapes/SOIC-8_3.9x4.9mm_P1.27mm.step"
DATASHEET = "https://www.ti.com/lit/ds/symlink/lm358.pdf"


def check(condition, message):
    if not condition:
        raise SystemExit("FAILED: " + message)
    print("ok  ", message)


def main():
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "live-smoke-output")
    lib_dir = Path(sys.argv[2] if len(sys.argv) > 2 else Path.home() / "kicad-live" / "libs")
    step_source = Path(sys.argv[3] if len(sys.argv) > 3 else STEP)
    out.mkdir(parents=True, exist_ok=True)
    lib_dir.mkdir(parents=True, exist_ok=True)
    client = LiveSchematic(timeout_ms=60000)
    root = test_document(client)

    for _ in range(120):  # footprint libraries load in the background once the editor opens
        found = client.footprint_search("SOIC-8_3.9x4.9mm_P1.27mm")
        if not found.get("libraries_still_loading"):
            break
        time.sleep(1)
    check(any(r["lib_id"] == STOCK for r in found["results"]), f"search found {STOCK} ({found['total']} hits)")
    stock = client.library_footprint(STOCK)
    check(len(stock["pads"]) == 8, f"read {STOCK}: 8 pads, {stock['mounting']}")

    tag = uuid.uuid4().hex[:6]
    fp_lib, fp_path = f"mcp_fp_{tag}", lib_dir / f"mcp_fp_{tag}.pretty"
    sym_lib, sym_path = f"mcp_sym_{tag}", lib_dir / f"mcp_sym_{tag}.kicad_sym"
    client.add_library(fp_lib, str(fp_path), scope="global", description="MCP footprint test", kind="footprint")
    check(fp_path.is_dir(), f"created footprint library {fp_path}")
    check(any(r["nickname"] == fp_lib and r["scope"] == "global" for r in client.library_table("global", "footprint")),
          "footprint library is in the global table")
    scratch = Path(tempfile.mkdtemp(prefix="mcp-step-"))
    try:
        doc = footprint_document(client.new_footprint(fp_lib, "SOIC-8_TEST", mounting="smd"))
        check(doc.lib_id.entry_name == "SOIC-8_TEST", "new footprint open in the Footprint Editor")

        generated = client.footprint_generate(doc, "soic", dict(
            pins=8, pitch=1.27, body_width=3.9, body_length=4.9, pad_length=1.95, pad_width=0.6, row_pitch=4.95))
        summary = client.footprint_summary(doc)
        ours = {p["number"]: (p["position_mm"], p["size_mm"]) for p in summary["pads"]}
        theirs = {p["number"]: (p["position_mm"], p["size_mm"]) for p in stock["pads"]}
        check(ours == theirs, f"generated pads match the stock SOIC-8 land pattern: {generated}")
        layers = summary["graphics_per_layer"]
        check(all(layers.get(name) for name in ("F_SilkS", "F_Fab", "F_CrtYd")),
              f"silkscreen, fabrication and courtyard drawn: {layers}")
        fab_refs = [t for t in summary["texts"] if t["text"] == "${REFERENCE}"]
        check(len(fab_refs) == 1 and fab_refs[0]["layer"] == "F_Fab", "exactly one ${REFERENCE} text, on F.Fab")

        client.footprint_set_properties(doc, description="SOIC, 8 pins, 3.9x4.9mm body, 1.27mm pitch (MCP test)",
                                        keywords="SOIC SO mcptest", datasheet=DATASHEET)

        step_copy = scratch / f"SOIC-8_TEST_{tag}.step"
        shutil.copyfile(step_source, step_copy)
        attached = client.footprint_attach_step(doc, str(step_copy))
        files = client.embedded_files(doc)
        check(any(f["name"] == step_copy.name and f["type"] == "EFT_MODEL" for f in files),
              f"STEP embedded in the footprint as a model: {attached}")

        png = client.footprint_render(doc)
        check(png[:4] == bytes([0x89]) + b"PNG", f"rendered the unsaved footprint ({len(png)} bytes)")
        (out / "footprint.png").write_bytes(png)

        client.footprint_save(doc)
        saved = (fp_path / "SOIC-8_TEST.kicad_mod").read_text()
        check("kicad-embed://" + step_copy.name in saved and "embedded_files" in saved,
              "saved footprint embeds the STEP and references it as its 3D model")
        check(DATASHEET in saved, "saved footprint links the datasheet")

        stored = client.library_footprint(f"{fp_lib}:SOIC-8_TEST")
        check(stored["datasheet"] == DATASHEET and stored["models"] == ["kicad-embed://" + step_copy.name]
              and stored["mounting"] == "FMS_SMD", f"library serves the footprint with its model: {stored['models']}")

        # A symbol that links the new footprint and the datasheet, placed in the schematic
        client.add_library(sym_lib, str(sym_path), scope="global", description="MCP footprint test")
        sym_doc = symbol_document(client.new_symbol(sym_lib, "TESTAMP", reference_prefix="U",
                                                    manufacturer="Test Semiconductor", mpn=f"TA-{tag}"))
        client.symbol_add_pins(sym_doc, [
            {"number": "8", "name": "V+", "type": "power_in"}, {"number": "4", "name": "V-", "type": "power_in"},
            {"number": "3", "name": "+IN", "type": "input"}, {"number": "2", "name": "-IN", "type": "input"},
            {"number": "1", "name": "OUT", "type": "output"}])
        client.symbol_set_properties(sym_doc, value="TESTAMP", footprint=f"{fp_lib}:SOIC-8_TEST",
                                     datasheet=DATASHEET, footprint_filters=["SOIC*3.9x4.9mm*P1.27mm*"])
        client.symbol_save(sym_doc)
        symbol = client.library_symbol(f"{sym_lib}:TESTAMP")
        check(symbol["footprint"] == f"{fp_lib}:SOIC-8_TEST" and symbol["datasheet"] == DATASHEET,
              "symbol links the new footprint and the datasheet")
        placed = client.place_symbol(root, f"{sym_lib}:TESTAMP", 150, 150)
        check(placed["reference"].startswith("U"), f"placed the symbol as {placed['reference']}")
    finally:
        for nickname, kind in ((fp_lib, "footprint"), (sym_lib, "symbol")):
            try:
                client.remove_library(nickname, scope="global", kind=kind)
            except Exception as error:  # noqa: BLE001 - cleanup must try every library
                print("cleanup:", error)
        shutil.rmtree(fp_path, ignore_errors=True)
        sym_path.unlink(missing_ok=True)
        shutil.rmtree(scratch, ignore_errors=True)
    print("OK: footprint search, generation, embedded STEP, datasheet link, global library and linking verified")


if __name__ == "__main__":
    main()
