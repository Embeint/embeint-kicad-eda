# SPDX-License-Identifier: GPL-3.0-or-later
"""End-to-end check of the SolidCraft handoff: datasheet numbers -> SolidCraft STEP -> embedded model.

Run against a development KiCad with a schematic open, with SOLIDCRAFT_CLI pointing at
solidcraft-cli.  Plays the agent: generates an SOIC-8 footprint, asks for the model plan, runs it on
SolidCraft's MCP (batch, measure, screenshot, export), attaches the STEP, saves, and checks the
embedded bytes, the model link and KiCad's own 3D render and STEP export.  Cleans up its libraries.

    python e2e_model_handoff.py [output_dir] [library_dir]
"""

from pathlib import Path
import hashlib
import shutil
import sys
import uuid

from client import LiveSchematic, document, footprint_document
from e2e_guard import park_footprint_editor, test_document
from model_handoff import kicad_path
from solidcraft_stdio import SolidCraftMcp

SOIC8 = dict(pins=8, pitch=1.27, body_width=3.9, body_length=4.9, pad_length=1.95, pad_width=0.6, row_pitch=4.95)
# JEDEC MS-012 (SOIC-8 narrow) nominal: A 1.75, A1 0.1, b 0.41, c 0.2, E 6.0, L 0.835
DRAWING = dict(body_height=1.75, standoff=0.1, lead_thickness=0.2, lead_width=0.41, lead_span=6.0, foot_length=0.835)


def check(condition, message):
    if not condition:
        raise SystemExit("FAILED: " + message)
    print("ok  ", message)


def main():
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "live-smoke-output")
    lib_dir = Path(sys.argv[2] if len(sys.argv) > 2 else Path.home() / "kicad-live" / "libs")
    out.mkdir(parents=True, exist_ok=True)
    lib_dir.mkdir(parents=True, exist_ok=True)
    client = LiveSchematic(timeout_ms=60000)
    test_document(client)

    tag = uuid.uuid4().hex[:6]
    fp_lib, fp_path = f"mcp_3d_{tag}", lib_dir / f"mcp_3d_{tag}.pretty"
    name = f"SOIC-8_3D_{tag}"
    client.add_library(fp_lib, str(fp_path), scope="global", description="MCP 3D handoff test", kind="footprint")
    solidcraft = None
    try:
        doc = footprint_document(client.new_footprint(fp_lib, name))
        client.footprint_generate(doc, "soic", SOIC8)
        client.footprint_set_properties(doc, description="SOIC-8 with a SolidCraft model (MCP test)",
                                        datasheet="https://www.ti.com/lit/ds/symlink/lm358.pdf")

        # 1. Plan: what the agent hands SolidCraft
        plan = client.footprint_model_plan("soic", SOIC8, **DRAWING)
        export = plan["export"]["arguments"]
        check(len(plan["bodies"]) == 9 and export["path"].endswith(".step"),
              f"plan: {len(plan['solidcraft']['arguments']['commands'])} SolidCraft steps, 9 bodies, "
              f"export to {export['path']}")

        # 2. SolidCraft: build, inspect, export (as the agent would through SolidCraft's MCP)
        model_dir = str(Path(export["path"]).parent) if ":" not in export["path"] else export["path"].rsplit("\\", 1)[0]
        solidcraft = SolidCraftMcp(allow_paths=[model_dir])
        built, _ = solidcraft.call("batch", plan["solidcraft"]["arguments"])
        check(built.get("ok") and built.get("completed") == len(plan["solidcraft"]["arguments"]["commands"]),
              f"SolidCraft built the model ({built.get('completed')} steps)")
        measured, _ = solidcraft.call("measure", {})
        boxes = [b["bbox"] for b in measured["bodies"]]
        lo = [min(b["min"][i] for b in boxes) for i in range(3)]
        hi = [max(b["max"][i] for b in boxes) for i in range(3)]
        check(abs(hi[0] - lo[0] - 6.0) < 0.05 and abs(hi[2] - 1.75) < 0.01 and abs(lo[2]) < 0.01,
              f"model spans E = {hi[0] - lo[0]:.2f} mm, seated height {hi[2]:.2f} mm, bottom on the board")
        _, shots = solidcraft.call("screenshot", {"view": "iso"})
        if shots:
            (out / "solidcraft_model.png").write_bytes(shots[0])
        solidcraft.call(plan["export"]["tool"], export)
        step_file = Path(kicad_path(export["path"]))
        check(step_file.exists() and step_file.read_bytes()[:13] == b"ISO-10303-21;",
              f"SolidCraft exported {step_file.stat().st_size} bytes of STEP")

        # 3. Back in KiCad: embed, save, verify
        attached = client.footprint_attach_step(doc, export["path"])  # the Windows path, as SolidCraft reports it
        client.footprint_save(doc)
        lib_id = f"{fp_lib}:{name}"
        report = client.footprint_check_model(lib_id, source_path=export["path"])
        embedded = report["embedded"][0] if report["embedded"] else {}
        check(embedded.get("is_step") and embedded.get("matches_source") and embedded.get("linked"),
              f"embedded STEP is byte-identical to SolidCraft's file and is the footprint's model ({attached['model']})")
        check(report["kicad_export_solids"] >= 9,
              f"KiCad loads the embedded model in a separate process: {report['kicad_export_solids']} solids exported")
        png = client.footprint_view_3d(lib_id)
        (out / "model_3d.png").write_bytes(png)
        check(png[:4] == bytes([0x89]) + b"PNG", f"KiCad rendered the footprint in 3D ({len(png)} bytes)")
        print("sha256", hashlib.sha256(step_file.read_bytes()).hexdigest()[:16], "->", embedded["sha256"][:16])
    finally:
        if solidcraft:
            solidcraft.close()
        park_footprint_editor(client)  # before deleting the library folder it watches
        try:
            client.remove_library(fp_lib, scope="global", kind="footprint")
        except Exception as error:  # noqa: BLE001
            print("cleanup:", error)
        shutil.rmtree(fp_path, ignore_errors=True)
    print("OK: SolidCraft model handed off, embedded, and verified in KiCad")


if __name__ == "__main__":
    main()
