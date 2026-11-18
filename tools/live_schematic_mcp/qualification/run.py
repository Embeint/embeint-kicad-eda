# SPDX-License-Identifier: GPL-3.0-or-later
"""Run the desktop qualification in the disposable Linux build container.

Invoke with xvfb-run and the build's venv. Only processes launched here are stopped;
the temporary configuration, projects and IPC directory are never shared with a host.
"""

import asyncio
from hashlib import sha256
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import uuid

ROOT = Path("/qualification")
MCP = ROOT / "src/tools/live_schematic_mcp"
INSTALL = Path("/opt/embeint-kicad")
sys.path.insert(0, str(MCP))


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def disk_hashes(folder):
    return {str(path.relative_to(folder)): sha256(path.read_bytes()).hexdigest()
            for suffix in ("*.kicad_sch", "*.kicad_pcb") for path in folder.glob(suffix)}


def prepare(folder):
    project = folder / "projects/live-demo"
    project.mkdir(parents=True)
    write_json(project / "live-demo.kicad_pro", {"meta": {"version": 3}})
    # Keep the fixture independent of the source file format version; the native
    # parser upgrades this empty schematic, then the explicit save exercises its writer.
    (project / "live-demo.kicad_sch").write_text(
        '(kicad_sch (version 20250114) (generator "eeschema")\n'
        ' (uuid "7855b1ef-9f07-4f35-b2e8-40002cbaae99") (paper "A4")\n'
        ' (lib_symbols) (sheet_instances (path "/" (page "1"))))\n')
    config = folder / "config/10.99"
    config.mkdir(parents=True)
    write_json(config / "kicad_common.json", {
        "api": {"enable_server": True},
        "do_not_show_again": {"data_collection_prompt": True, "update_check_prompt": True},
    })
    for filename, kind, directory, names, suffix in (
        ("sym-lib-table", "sym", "symbols", ["Device", "Regulator_Linear"], ".kicad_sym"),
        ("fp-lib-table", "fp", "footprints", ["Resistor_SMD", "Package_SO"], ".pretty"),
    ):
        entries = [f' (lib (name "{name}") (type "KiCad") '
                   f'(uri "/usr/share/kicad/{directory}/{name}{suffix}") '
                   '(options "") (descr "Qualification stock fixture"))' for name in names]
        (config / filename).write_text(f'({kind}_lib_table (version 7)\n' + "\n".join(entries) + ")\n")
    (config / "design-block-lib-table").write_text("(design_block_lib_table (version 7))\n")
    return project


def start(project, folder, output, env, label, open_editor=True):
    from client import LiveSchematic, base

    log = (output / f"{label}.app.log").open("w")
    proc = subprocess.Popen([str(INSTALL / "bin/kicad"), str(project / "live-demo.kicad_pro")],
                            env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    url = "ipc://" + str(folder / "tmp/kicad/api.sock")
    deadline = time.monotonic() + 60
    opening = False
    last_error = "no reply"
    try:
        while time.monotonic() < deadline:
            if proc.poll() is not None:
                raise RuntimeError(f"application exited: {proc.returncode}")
            try:
                probe = LiveSchematic(socket_url=url, timeout_ms=500)
                probe.send(base.GetVersion(), base.GetVersionResponse)
                client = LiveSchematic(socket_url=url, timeout_ms=60000)
                # A mutation is attempted once, after read-only readiness polling.
                if open_editor:
                    opening = True
                    client.open_schematic()
                return proc, log, client
            except Exception as error:
                # Never replay an OpenDocument request after an unknown outcome.
                if opening:
                    raise
                last_error = str(error)
            time.sleep(0.25)
        raise RuntimeError(f"application readiness timeout: {last_error}")
    except BaseException:
        stop(proc, log)
        raise


def stop(proc, log):
    if proc.poll() is None:
        os.killpg(proc.pid, signal.SIGTERM)
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGKILL)
            proc.wait(timeout=10)
    log.close()


def snapshot(client):
    from client import as_dict, document, commands
    from board_client import board_document, ITEM_TYPES

    result = {"schematics": {}, "boards": {}}
    root = document(client.documents()["documents"][0])
    hierarchy = client.hierarchy(root)

    def visit(nodes):
        for node in nodes:
            spec = type(root)()
            spec.CopyFrom(root)
            spec.sheet_path.CopyFrom(node.path)
            key = "/".join(i.value for i in node.path.path)
            result["schematics"][key] = sorted(
                (as_dict(item) for item in client.items(spec).items),
                key=lambda item: json.dumps(item, sort_keys=True))
            visit(node.children)

    visit(hierarchy.top_level_sheets)
    result["hierarchy"] = as_dict(hierarchy)
    netlist = client.send(commands.GetSchematicNetlist(document=root), commands.SchematicNetlistResponse)
    result["netlist"] = sorted([
        {"name": net.name, "sheets": sorted([
            {"path": [part.value for part in sheet.path.path],
             "items": sorted(item.value for item in sheet.items)} for sheet in net.sheets
        ], key=lambda sheet: sheet["path"])} for net in netlist.nets
    ], key=lambda net: net["name"])
    for value in client.boards():
        spec = board_document(value)
        result["boards"][spec.board_filename] = sorted(
            (as_dict(item) for item in client.board_items(spec, list(ITEM_TYPES))),
            key=lambda item: json.dumps(item, sort_keys=True))
    return result


def roundtrip_fixture(client):
    from client import document, board_document, kiid, point, sch, types
    from e2e_smoke import find_child

    root = document(client.documents()["documents"][0])
    sheet = sch.SheetSymbol(id=kiid(), position=point(100, 50), size=point(40, 30),
                            page_number="2", fields_autoplaced=True)
    sheet.name_field.text.text = "Power"
    sheet.name_field.visible = True
    sheet.filename_field.text.text = "power.kicad_sch"
    sheet.filename_field.visible = True
    sheet.pins.add(id=kiid(), position=point(100, 60), text=types.Text(text="VIN"),
                   side=sch.SHS_LEFT, shape=sch.SLSH_INPUT)
    client.mutate(root, [sheet])
    node = find_child(client.hierarchy(root).top_level_sheets, "Power")
    assert node is not None
    child = type(root)()
    child.CopyFrom(root)
    child.sheet_path.CopyFrom(node.path)
    for spec, reference, x, y in ((root, "R1", 40, 40), (root, "R2", 70, 40), (child, "R3", 50, 60)):
        client.place_symbol(spec, "Device:R", x, y, reference=reference)
        for packed in client.items(spec).items:
            symbol = sch.SchematicSymbolInstance()
            if packed.Unpack(symbol) and symbol.reference_field.text.text == reference:
                symbol.footprint_field.text.text = "Resistor_SMD:R_0603_1608Metric"
                client.mutate(spec, [symbol], update=True)
    client.connect_pins(root, "R1", "2", "R2", "2")
    parent_pin = next(p for p in client.symbol_pins(root, with_nets=False)
                      if p["reference"] == "R2")["pins"]
    start_pin = next(p for p in parent_pin if p["number"] == "1")["position_mm"]
    client.mutate(root, [
        sch.SchematicLine(id=kiid(), start=point(*start_pin), end=point(95, start_pin[1]), type=sch.SLT_WIRE),
        sch.SchematicLine(id=kiid(), start=point(95, start_pin[1]), end=point(95, 60), type=sch.SLT_WIRE),
        sch.SchematicLine(id=kiid(), start=point(95, 60), end=point(100, 60), type=sch.SLT_WIRE),
    ])
    pins = client.symbol_pins(child, with_nets=False)[0]["pins"]
    pin = next(p for p in pins if p["number"] == "1")
    client.mutate(child, [sch.HierarchicalLabel(id=kiid(), position=point(*pin["position_mm"]),
                                              text=types.Text(text="VIN"), shape=sch.SLSH_INPUT)])
    for spec, reference, number in ((root, "R1", "1"), (child, "R3", "2")):
        pin = next(p for symbol in client.symbol_pins(spec, with_nets=False)
                   if symbol["reference"] == reference for p in symbol["pins"] if p["number"] == number)
        client.mutate(spec, [sch.NoConnectMarker(id=kiid(), position=point(*pin["position_mm"]))])
    client.netlist(root)
    root_net = next(p for s in client.symbol_pins(root) if s["reference"] == "R2"
                    for p in s["pins"] if p["number"] == "1")["net"]
    child_net = next(p for p in client.symbol_pins(child)[0]["pins"] if p["number"] == "1")["net"]
    assert root_net and root_net == child_net, "hierarchical VIN connectivity is incorrect"
    board = board_document(client.open_board())
    imported = client.board_update_from_schematic(board, root)
    assert imported["errors"] == 0 and imported["new_footprints"] == 3, imported
    client.board_move_footprints(board, [{"reference": ref, "x_mm": x, "y_mm": 40}
                                        for ref, x in (("R1", 40), ("R2", 60), ("R3", 80))])
    for name, spec in (("Root", root), ("Power", child), ("PCB", board)):
        client.set_page(spec, size="A4", title=f"live-demo - {name}", revision="qualification",
                        company="Embeint Pty Ltd", comments=["Disposable native round-trip fixture"])
        assert client.page_layout(spec)["fits"], f"{name} does not fit its page"
    return {"hierarchical_net": root_net, "schematic_to_board": imported,
            "erc": client.erc(root), "drc": client.board_drc(board)}


async def stdio_edit_check(client, env):
    from client import as_dict, document
    from mcp import ClientSession, StdioServerParameters
    from mcp.client.stdio import stdio_client

    root = document(client.documents()["documents"][0])
    before = sorted(item.SerializeToString(deterministic=True) for item in client.items(root).items)
    executable = env.get("KICAD_MCP_EXECUTABLE")
    params = StdioServerParameters(command=executable or sys.executable,
                                   args=[] if executable else [str(MCP / "server.py")], env=env)
    async with stdio_client(params) as (reader, writer):
        async with ClientSession(reader, writer) as session:
            await session.initialize()
            listed = await session.call_tool("live_schematic_documents", {})
            assert not listed.isError, listed
            spec = as_dict(root)
            item_id = str(uuid.uuid4())
            response = await session.call_tool("live_schematic_create_items", {
                "document_spec": spec, "items": [{
                    "@type": "type.googleapis.com/kiapi.schematic.types.SchematicText",
                    "id": {"value": item_id},
                    "text": {"text": "Packaged MCP qualification", "position": {"x_nm": "20000000", "y_nm": "180000000"}},
                }],
            })
            assert not response.isError, response
            assert len(client.items(root).items) == len(before) + 1
            undone = await session.call_tool("live_schematic_undo", {"document_spec": spec})
            assert not undone.isError, undone
            after = sorted(item.SerializeToString(deterministic=True) for item in client.items(root).items)
            assert after == before, "MCP undo did not restore the exact prior item set"
    return {"result": "passed", "runtime": "packaged" if executable else "source",
            "checks": ["initialize", "documents", "create_text", "undo", "exact_item_restore"]}


def main():
    if not Path("/.dockerenv").exists() or not (MCP / "server.py").is_file():
        raise SystemExit("Run inside the qualification container with its source snapshot mounted")
    from client import LiveSchematic, as_dict, base
    import server

    output = ROOT / "evidence/live"
    output.mkdir(parents=True, exist_ok=False)
    report = {"source_revision": os.environ["KICAD_SOURCE_REVISION"], "checks": {}, "result": "failed"}
    with tempfile.TemporaryDirectory(prefix="emk-", dir="/tmp") as temp:
        folder = Path(temp)
        project = prepare(folder)
        (folder / "tmp").mkdir()
        env = {**os.environ, "TMPDIR": str(folder / "tmp"),
               "KICAD_CONFIG_HOME": str(folder / "config"),
               "KICAD_LIVE_ROOT": str(folder), "KICAD_LIVE_TEST_PROJECT": "live-demo",
               "KICAD_API_SOCKET": "ipc://" + str(folder / "tmp/kicad/api.sock"),
               "KICAD_API_TOKEN": "", "PYTHONUNBUFFERED": "1"}
        proc = log = None
        try:
            proc, log, client = start(project, folder, output, env, "initial")
            report["application_version"] = as_dict(client.send(base.GetVersion(), base.GetVersionResponse))
            report["schema_sha256"] = asyncio.run(server.live_mcp_capabilities())["schema_sha256"]
            for test in ("e2e_symbols", "e2e_footprints", "e2e_smoke", "e2e_features", "e2e_pcb", "e2e_design"):
                if test in ("e2e_pcb", "e2e_design"):
                    # Routing failures can leave fixture copper/footprints behind.
                    # Each PCB suite gets a fresh app and board even after a failure.
                    stop(proc, log)
                    proc = log = None
                    test_folder = folder / test
                    project = prepare(test_folder)
                    (test_folder / "tmp").mkdir()
                    env.update(TMPDIR=str(test_folder / "tmp"),
                               KICAD_CONFIG_HOME=str(test_folder / "config"),
                               KICAD_LIVE_ROOT=str(test_folder),
                               KICAD_API_SOCKET="ipc://" + str(test_folder / "tmp/kicad/api.sock"))
                    proc, log, client = start(project, test_folder, output, env, test)
                args = [sys.executable, str(MCP / f"{test}.py"), str(output), str(folder / "libs")]
                if test == "e2e_footprints":
                    args.append(str(ROOT / "src/qa/data/pcbnew/step_model_colors/TO-252-2.step"))
                with (output / f"{test}.log").open("w") as trace:
                    completed = subprocess.run(args, env=env, stdout=trace, stderr=subprocess.STDOUT, timeout=600)
                report["checks"][test] = {"exit_code": completed.returncode}
                print(test, completed.returncode, flush=True)
            if any(value["exit_code"] for value in report["checks"].values()):
                raise RuntimeError("live workflow failures; inspect individual logs")

            stop(proc, log)
            proc = log = None
            folder = folder / "roundtrip"
            project = prepare(folder)
            (folder / "tmp").mkdir()
            env.update(TMPDIR=str(folder / "tmp"), KICAD_CONFIG_HOME=str(folder / "config"),
                       KICAD_LIVE_ROOT=str(folder),
                       KICAD_API_SOCKET="ipc://" + str(folder / "tmp/kicad/api.sock"))
            proc, log, client = start(project, folder, output, env, "roundtrip")
            report["checks"]["hierarchical_roundtrip_fixture"] = roundtrip_fixture(client)
            before = disk_hashes(project)
            report["checks"]["mcp_stdio_edit_undo"] = asyncio.run(
                asyncio.wait_for(stdio_edit_check(client, env), timeout=90))
            assert disk_hashes(project) == before, "MCP edit/undo changed saved files"
            for value in client.documents()["documents"] + client.boards():
                client.execute("SaveDocument", {"document": value})
            after = disk_hashes(project)
            assert before != after, "explicit save did not persist live edits"
            expected = snapshot(client)
            write_json(output / "before-restart.json", expected)
            stop(proc, log)
            proc = log = None
            proc, log, client = start(project, folder, output, env, "reopened")
            client.open_board()
            actual = snapshot(client)
            write_json(output / "after-restart.json", actual)
            assert actual == expected, "hierarchy/items/connectivity changed across save/reopen"
            assert disk_hashes(project) == after, "reopening changed saved schematic/board contents"
            report["checks"]["explicit_save_restart_reopen"] = "passed"
            report["saved_file_sha256"] = after
            report["result"] = "passed"
        except BaseException as error:
            report["error"] = f"{type(error).__name__}: {error}"
            raise
        finally:
            if proc is not None:
                stop(proc, log)
            write_json(output / "report.json", report)


if __name__ == "__main__":
    main()
