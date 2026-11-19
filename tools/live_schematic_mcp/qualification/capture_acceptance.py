# SPDX-License-Identifier: GPL-3.0-or-later
"""Owned Linux/WSL schematic capture acceptance using a retained native install.

Run with the existing development venv, not a customer's MCP session. A new output
directory and private Xvfb/config/project/IPC namespace are mandatory. This is a
scripted fixture; it does not establish installed or fresh-agent skill usability.
"""

import argparse
import asyncio
import importlib.util
from hashlib import sha256
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile
from datetime import datetime, timezone

MCP = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(MCP))

from client import as_dict, base, kiid, point, sch, types
from qualification import run
from qualification.capture_fixture import (INTENT, build, documents, erc_summary,
                                           negative_flow_through, prepare_library, symbol)
from qualification.connectivity_intent import (observe_pins, require_connectivity,
                                               validate_board_parity, validate_connectivity)
from qualification.persistence import canonical_items
from qualification.capture_persistence import snapshot


def native_identity(install, hashes, source_record):
    """Validate retained binary identity before launching; never relabel old tests."""
    expected = {}
    for line in hashes.read_text().splitlines():
        digest, filename = line.split(maxsplit=1)
        expected[Path(filename.strip()).name] = digest
    required = {"kicad", "kicad-cli", "_eeschema.kiface", "_pcbnew.kiface"}
    if not required <= expected.keys():
        raise ValueError("native hash record must identify manager, CLI and both editor modules")
    actual = {name: sha256((install / "bin" / name).read_bytes()).hexdigest() for name in sorted(required)}
    if any(actual[name] != expected[name] for name in required):
        raise ValueError("retained native hash record does not match selected install")
    source = json.loads(source_record.read_text())
    return {"install": str(install), "files_sha256": actual,
            "retained_hash_record_sha256": sha256(hashes.read_bytes()).hexdigest(),
            "retained_source_record_sha256": sha256(source_record.read_bytes()).hexdigest(),
            "source_revision": source["native_base_revision"],
            "source_patch_blobs": source["native_patch_blobs"],
            "version_stamp_note": source["version_stamp_note"],
            "scope": "retained binary/source identity only; all acceptance checks below are new"}


def source_identity(git="git"):
    root = MCP.parents[1]
    revision = subprocess.check_output([git, "rev-parse", "HEAD"], cwd=root, text=True).strip()
    changed = subprocess.check_output([git, "status", "--porcelain", "--untracked-files=all"],
                                      cwd=root, text=True)
    paths = list(MCP.glob("*.py")) + list((MCP / "qualification").glob("*.py"))
    paths += list((root / "api/proto").rglob("*.proto"))
    return {"revision": revision, "dirty": bool(changed), "status": changed.splitlines(),
            "files_sha256": {str(path.relative_to(root)): sha256(path.read_bytes()).hexdigest()
                             for path in sorted(paths)}}


def native_views(client, docs, board, output):
    pages = {}
    for name, doc in [*docs.items(), ("PCB", board)]:
        client.set_page(doc, size="A4", title="Synthetic capture - " + name,
                        date="2026-11-19", revision="acceptance", company="Embeint Pty Ltd",
                        comments=["Disposable synthetic acceptance fixture"])
        pages[name] = client.page_layout(doc)
        assert pages[name]["fits"], pages[name]
        framed = client.board_render(board, dpi=100, frame=True) if name == "PCB" else client.render(doc, frame=True)
        bare = client.board_render(board, dpi=100, frame=False) if name == "PCB" else client.render(doc, frame=False)
        assert framed.startswith(b"\x89PNG") and framed != bare
        (output / (name.lower() + "-framed.png")).write_bytes(framed)
    return pages


def exact_items(client, docs):
    return {name: canonical_items({"items": [as_dict(item) for item in client.items(doc).items]})
            for name, doc in docs.items()}


def part_properties(client, docs):
    from qualification.capture_fixture import PARTS

    properties = []
    for name, doc in docs.items():
        for packed in client.items(doc).items:
            item = sch.SchematicSymbolInstance()
            if not packed.Unpack(item):
                continue
            if item.definition.type in (sch.SST_GLOBAL_POWER, sch.SST_LOCAL_POWER):
                continue
            identity = item.lib_id if item.HasField("lib_id") else item.definition.id
            assert identity.library_nickname == "Acceptance", as_dict(item)
            fields = {field.name: field for field in item.user_fields}
            assert fields["Manufacturer"].text.text == "Synthetic acceptance fixture"
            assert fields["MPN"].text.text == "FIXTURE-" + identity.entry_name
            assert not fields["Manufacturer"].visible and not fields["MPN"].visible
            assert item.footprint_field.text.text == PARTS[identity.entry_name][1]
            properties.append({"sheet": name, "reference": item.reference_field.text.text,
                               "unit": item.unit.unit, "lib_id": as_dict(identity),
                               "footprint": item.footprint_field.text.text,
                               "fields": {key: as_dict(value) for key, value in fields.items()}})
    assert len(properties) == 8, "seven physical parts, including both buffer units, required"
    return sorted(properties, key=lambda entry: (entry["reference"], entry["unit"]))


def edit_invariants(client, docs, project):
    root = docs["Root"]
    before_items, before_pins = exact_items(client, docs), observe_pins(client, docs.values())
    before_disk = run.disk_hashes(project)
    good = as_dict(sch.SchematicText(id=kiid(), text=types.Text(text="rollback probe", position=point(30, 140))))
    good["@type"] = "type.googleapis.com/kiapi.schematic.types.SchematicText"
    missing = dict(good, id={"value": kiid().value})
    rejected = None
    try:
        client.batch(root, [
            {"command": "CreateItems", "payload": {"header": {"document": as_dict(root)}, "items": [good]}},
            {"command": "UpdateItems", "payload": {"header": {"document": as_dict(root)}, "items": [missing]}},
        ])
    except RuntimeError as error:
        rejected = str(error)
    assert rejected and "rolled back" in rejected, rejected
    assert exact_items(client, docs) == before_items
    assert observe_pins(client, docs.values()) == before_pins
    good["id"] = {"value": kiid().value}
    accepted = client.batch(root, [{"command": "CreateItems", "payload": {
        "header": {"document": as_dict(root)}, "items": [good]}}])
    assert accepted["committed"]
    edited = exact_items(client, docs)
    assert edited != before_items
    assert client.undo(root).get("performed")
    assert exact_items(client, docs) == before_items
    assert client.undo(root, redo=True).get("performed")
    assert exact_items(client, docs) == edited
    assert client.undo(root).get("performed")
    assert exact_items(client, docs) == before_items
    assert observe_pins(client, docs.values()) == before_pins
    assert run.disk_hashes(project) == before_disk
    return {"failed_batch": "exact items and pins restored", "undo_redo": "exact items restored",
            "implicit_save": False, "rejection": rejected}


def update_regressions(client, docs):
    """Guarded probes of historical reports, retaining observations even on reproduction.

    Each UpdateItems request is isolated by undo. A reproduced historical defect
    is recorded as a finding, never silently repaired or called a passing probe.
    """
    doc = docs["MCU"]
    before = observe_pins(client, docs.values())
    baseline_items = exact_items(client, docs)
    item = symbol(client, doc, "U2")
    item.value_field.text.text = "Dual probe"
    client.mutate(doc, [item], update=True)
    after = observe_pins(client, docs.values())
    duplicates = [finding for finding in validate_connectivity(INTENT, after)
                  if finding["kind"] in ("duplicate_pin", "duplicate_identity")]
    library_drops = [{"pin": entry["pin"], "lib_id": entry["lib_id"]}
                     for entry in after if not entry["lib_id"].startswith("Acceptance:")]
    assert client.undo(doc).get("performed")
    assert exact_items(client, docs) == baseline_items, "UpdateItems probe undo changed serialized items"
    assert observe_pins(client, docs.values()) == before, "UpdateItems probe undo changed pin identities"
    return {
        "multi_unit_update_duplicate_pin_ids": {
            "status": "reproduced" if duplicates else "historical_not_reproduced",
            "findings": duplicates, "before": before, "after": after},
        "update_dropped_library_nickname": {
            "status": "reproduced" if library_drops else "historical_not_reproduced",
            "findings": library_drops},
    }


def board_transfer(client, root, observations):
    from client import board_document
    board = board_document(client.open_board())
    assert not client.board_footprints(board)
    transfer = client.board_update_from_schematic(board, root)
    assert transfer["errors"] == 0 and transfer["new_footprints"] == 7, transfer
    refs = sorted({entry["pin"].split(".")[0] for entry in observations})
    client.board_move_footprints(board, [{"reference": ref, "x_mm": 40 + i * 20, "y_mm": 60}
                                        for i, ref in enumerate(refs)])
    client.board_outline(board, 145, 30, x=25, y=45, replace=True)
    footprints = client.board_footprints(board, with_pads=True)
    findings = validate_board_parity(observations, footprints)
    assert not findings, findings
    return board, {"transfer": transfer, "parity": "all numbered pads match all schematic pins",
                   "footprints": footprints, "drc": client.board_drc(board),
                   "drc_scope": "unrouted fixture findings retained; not manufacturing sign-off"}


async def bundled_entrypoints(env):
    from mcp import ClientSession, StdioServerParameters
    from mcp.client.stdio import stdio_client
    import skills

    def payload(result):
        assert not result.isError
        return result.structuredContent or json.loads(result.content[0].text)

    async with stdio_client(StdioServerParameters(command=sys.executable, args=[str(MCP / "server.py")], env=env)) as streams:
        async with ClientSession(*streams) as session:
            initialized = await session.initialize()
            assert "live_skill_reference" in initialized.instructions
            result = await session.call_tool("live_skill_reference", {})
            assert not result.isError
            listing = payload(result)
            assert listing["default"] == skills.DEFAULT_SKILL
            manifest = await session.read_resource("kicad-skill://manifest")
            assert json.loads(manifest.contents[0].text) == skills.skill_manifest()
            for entry in listing["skills"]:
                for name in entry["files"]:
                    expected = skills.read_skill(entry["skill"], name)
                    tool = await session.call_tool("live_skill_reference", {"skill": entry["skill"], "file": name})
                    assert payload(tool) == expected
                    resource = await session.read_resource(expected["uri"])
                    assert resource.contents[0].text == expected["text"]
    return {"runtime": "source stdio; scripted integration, not independent agent evaluation",
            "manifest": skills.skill_manifest(), "checks": ["initialize instructions", "tool", "resources", "exact all-file content"]}


def helper_probe(path, client, root, git):
    """Optional coordinated probe, serially on the still-empty owned schematic."""
    folder = path.parent
    identity = {"revision": subprocess.check_output([git, "rev-parse", "HEAD"], cwd=folder, text=True).strip(),
                "files_sha256": {name: sha256((folder / name).read_bytes()).hexdigest()
                                 for name in ("block_helpers.py", "block_example.py")}}
    sys.path.insert(0, str(folder))
    try:
        spec = importlib.util.spec_from_file_location("coordinated_block_probe", path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return {"source": identity, "result": module.run_probe(client, root)}
    finally:
        sys.path.remove(str(folder))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--install", type=Path, required=True)
    parser.add_argument("--native-hashes", type=Path, required=True)
    parser.add_argument("--native-source-record", type=Path, required=True)
    parser.add_argument("--library-root", type=Path, default=Path("/usr/share/kicad"))
    parser.add_argument("--git-executable", default="git", help="use git.exe for a Windows-owned T3 worktree")
    parser.add_argument("--helper-probe", type=Path, help="optional block_example.py from the coordinated helper checkout")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if sys.platform != "linux":
        parser.error("run inside Linux/WSL with the retained native build")
    identity = native_identity(args.install, args.native_hashes, args.native_source_record)
    args.output.mkdir(parents=True, exist_ok=False)
    report = {"result": "failed", "started_utc": datetime.now(timezone.utc).isoformat(),
              "kind": "scripted_source_fixture", "fresh_agent_evaluation": "pending separate evaluator",
              "installed_distribution_evaluation": "not run by this driver",
              "adapter_source": source_identity(args.git_executable), "native": identity,
              "native_dependency_search_path": os.environ.get("LD_LIBRARY_PATH", ""),
              "platform": platform.platform(), "checks": {}}
    # A short new Linux path is required by Unix socket limits. Retain it as raw
    # evidence, together with the project/config; never reuse another run's state.
    folder = Path(tempfile.mkdtemp(prefix="emcap-"))
    report["owned_runtime"] = str(folder)
    project = run.prepare(folder)
    prepare_library(project)
    # Only the explicitly selected stock footprint libraries are needed.
    config = folder / "config/10.99"
    (config / "sym-lib-table").write_text(
        '(sym_lib_table (version 7) (lib (name "Device") (type "KiCad") '
        f'(uri "{args.library_root}/symbols/Device.kicad_sym") (options "") (descr "Probe dependency")) '
        f'(lib (name "power") (type "KiCad") (uri "{args.library_root}/symbols/power.kicad_sym") '
        '(options "") (descr "Native rail markers")))\n')
    libraries = ("Resistor_SMD", "Package_SO", "Package_TO_SOT_SMD", "Connector_PinHeader_2.54mm")
    (config / "fp-lib-table").write_text('(fp_lib_table (version 7)\n' + "\n".join(
        f'(lib (name "{name}") (type "KiCad") (uri "{args.library_root}/footprints/{name}.pretty") '
        '(options "") (descr "Acceptance stock footprint"))' for name in libraries) + ")\n")
    (folder / "tmp").mkdir()
    (folder / "runtime").mkdir(mode=0o700)
    read_fd, write_fd = os.pipe()
    xlog = (args.output / "xvfb.log").open("w")
    display = subprocess.Popen(["Xvfb", "-displayfd", str(write_fd), "-screen", "0", "1600x1200x24", "-nolisten", "tcp"],
                               pass_fds=(write_fd,), stdout=xlog, stderr=subprocess.STDOUT, start_new_session=True)
    os.close(write_fd)
    with os.fdopen(read_fd) as ready:
        display_number = ready.readline().strip()
    if not display_number.isdigit():
        run.stop(display, xlog)
        raise RuntimeError("owned Xvfb did not allocate a display")
    env = {**os.environ, "DISPLAY": ":" + display_number, "TMPDIR": str(folder / "tmp"),
           "XDG_RUNTIME_DIR": str(folder / "runtime"), "KICAD_CONFIG_HOME": str(folder / "config"),
           "KICAD_API_SOCKET": "ipc://" + str(folder / "tmp/kicad/api.sock"), "KICAD_API_TOKEN": "",
           "KICAD_LIVE_TEST_PROJECT": "live-demo", "KICAD_LIVE_ROOT": str(folder),
           "KICAD10_FOOTPRINT_DIR": str(args.library_root / "footprints"),
           "LIBGL_ALWAYS_SOFTWARE": "1"}
    run.INSTALL = args.install
    run.MCP = MCP
    proc = log = None
    try:
        proc, log, client = run.start(project, folder, args.output, env, "seed")
        report["native"]["measured_application_version"] = as_dict(client.send(base.GetVersion(), base.GetVersionResponse))
        import server
        report["schema_sha256"] = asyncio.run(server.live_mcp_capabilities())["schema_sha256"]
        from e2e_guard import test_document
        os.environ["KICAD_LIVE_TEST_PROJECT"] = "live-demo"
        root = test_document(client)
        before_disk = run.disk_hashes(project)
        if args.helper_probe:
            report["checks"]["coordinated_helper_probe"] = helper_probe(args.helper_probe, client, root, args.git_executable)
            assert not client.items(root).items
            assert run.disk_hashes(project) == before_disk
            print("coordinated helper probe passed", flush=True)
        docs, bridges = build(client, root)
        print("hierarchical capture and expected pin membership passed", flush=True)
        run.write_json(args.output / "intent.json", INTENT)
        report["checks"]["erc"] = erc_summary(client.erc(root))
        assert report["checks"]["erc"]["violation_count"] == 0, report["checks"]["erc"]
        report["checks"]["flow_through_negative"] = negative_flow_through(client, docs, bridges)
        print("ERC-zero split ESD regression reproduced and undone", flush=True)
        report["checks"]["edit_invariants"] = edit_invariants(client, docs, project)
        report["checks"].update(update_regressions(client, docs))
        print("rollback, undo/redo and historical UpdateItems probes completed", flush=True)
        observations = observe_pins(client, docs.values())
        require_connectivity(INTENT, observations)
        properties = part_properties(client, docs)
        report["checks"]["project_part_properties"] = properties
        run.write_json(args.output / "pin-observations.json", observations)
        board, report["checks"]["board"] = board_transfer(client, root, observations)
        report["checks"]["pages"] = native_views(client, docs, board, args.output)
        report["checks"]["bundled_entrypoints"] = asyncio.run(bundled_entrypoints(env))
        report["checks"]["source_mcp_edit_undo"] = asyncio.run(run.stdio_edit_check(client, env))
        print("PCB parity, framed views and source MCP/skill integration passed", flush=True)
        assert run.disk_hashes(project) == before_disk, "capture implicitly saved schematic/board"
        for value in client.documents()["documents"] + client.boards():
            client.execute("SaveDocument", {"document": value})
        saved = run.disk_hashes(project)
        assert saved != before_disk, "explicit save did not write capture"
        expected = snapshot(client)
        run.write_json(args.output / "before-restart.json", expected)
        expected_pins = observe_pins(client, docs.values())
        report["owned_manager_pids"] = [proc.pid]
        modules = {line.split()[-1] for line in Path(f"/proc/{proc.pid}/maps").read_text().splitlines()
                   if str(args.install) in line}
        report["native"]["loaded_modules_sha256"] = {path: sha256(Path(path).read_bytes()).hexdigest()
                                                       for path in sorted(modules)}
        run.stop(proc, log)
        proc = log = None
        proc, log, client = run.start(project, folder, args.output, env, "reopened")
        report["owned_manager_pids"].append(proc.pid)
        board = __import__("client").board_document(client.open_board())
        root = test_document(client)
        docs = documents(client, root)
        actual = snapshot(client)
        run.write_json(args.output / "after-restart.json", actual)
        assert actual == expected, "normalized items/hierarchy/net membership changed on reopen"
        observed = observe_pins(client, docs.values())
        require_connectivity(INTENT, observed)
        assert part_properties(client, docs) == properties
        assert observed == expected_pins, "native pin identities/connectivity changed on reopen"
        assert not validate_board_parity(observed, client.board_footprints(board, with_pads=True))
        for doc in [*docs.values(), board]:
            assert client.page_layout(doc)["fits"]
        assert run.disk_hashes(project) == saved, "reopening wrote schematic/board files"
        report["checks"]["explicit_save_owned_stop_restart_reopen"] = "passed"
        report["saved_file_sha256"] = saved
        assert native_identity(args.install, args.native_hashes, args.native_source_record)["files_sha256"] == identity["files_sha256"]
        report["result"] = "passed"
    except BaseException as error:
        report["error"] = f"{type(error).__name__}: {error}"
        raise
    finally:
        if proc is not None:
            run.stop(proc, log)
        run.stop(display, xlog)
        report["finished_utc"] = datetime.now(timezone.utc).isoformat()
        run.write_json(args.output / "report.json", report)
        shutil.copytree(project, args.output / "project")
        shutil.copytree(folder / "config", args.output / "config")
    print(json.dumps({"result": report["result"], "evidence": str(args.output)}, indent=2))


if __name__ == "__main__":
    main()
