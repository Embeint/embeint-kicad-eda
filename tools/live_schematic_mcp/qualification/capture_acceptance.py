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
from qualification.capture_fixture import (INTENT, build, documents, erc_summary, exact_items,
                                           negative_flow_through, prepare_library, symbols)
from qualification.capture_runtime import (cleanup_steps, dependency_identity, loaded_identity,
                                           native_identity, owned_display)
from qualification.connectivity_intent import (observe_pins, require_connectivity,
                                               validate_board_parity, validate_connectivity)
from qualification.capture_persistence import snapshot

# Source MCP stdio checks start server.py subprocesses while owned native
# processes run; a hung tool call must fail the run, not leave them running.
SKILL_DELIVERY_TIMEOUT = 180
MCP_EDIT_TIMEOUT = 90
PROBE_VALUE = "Dual probe"


def source_identity(git="git"):
    root = MCP.parents[1]
    revision = subprocess.check_output([git, "rev-parse", "HEAD"], cwd=root, text=True).strip()
    changed = subprocess.check_output([git, "status", "--porcelain", "--untracked-files=all"],
                                      cwd=root, text=True)
    paths = list(MCP.glob("*.py")) + list((MCP / "qualification").glob("*.py"))
    paths += list((root / "api/proto").rglob("*.proto"))
    paths += [path for path in (MCP / "skills").rglob("*") if path.is_file()]
    staged = subprocess.check_output([git, "ls-files", "-s", "--", "tools/live_schematic_mcp", "api/proto"],
                                     cwd=root, text=True)
    return {"revision": revision, "dirty": bool(changed), "status": changed.splitlines(),
            "files_sha256": {str(path.relative_to(root).as_posix()): sha256(path.read_bytes()).hexdigest()
                             for path in sorted(paths)},
            "files_sha256_scope": "executed working-tree bytes; line endings follow this checkout",
            # Blob ids are independent of core.autocrlf, so an LF checkout can reproduce them.
            "git_blobs": {line.split("\t", 1)[1]: line.split()[1] for line in staged.splitlines()}}


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


def update_probe(client, docs, item, record):
    """Update one placed unit's value through UpdateItems, observe, then undo.

    Observations are written into ``record`` before any restore assertion, so a
    reproduced defect or a failed restore keeps its evidence in the report.
    """
    doc = docs["MCU"]
    baseline_items = exact_items(client, docs)
    record["before"] = observe_pins(client, docs.values())
    item.value_field.text.text = PROBE_VALUE
    try:
        client.mutate(doc, [item], update=True)
    except RuntimeError as error:
        # The client drops a rejected commit; an undo here would revert an earlier edit.
        record.update(applied=False, rejected=str(error))
        assert exact_items(client, docs) == baseline_items, "rejected UpdateItems changed serialized items"
        record["restored"] = "rejected commit dropped; exact items unchanged"
        return
    try:
        record["after"] = after = observe_pins(client, docs.values())
        values = []
        for packed in client.items(doc).items:
            placed = sch.SchematicSymbolInstance()
            if packed.Unpack(placed) and placed.id.value == item.id.value:
                values.append(placed.value_field.text.text)
        record["value_after"] = values
        record["items_changed"] = exact_items(client, docs) != baseline_items
        # A no-op update cannot be evidence that a historical defect is absent.
        record["applied"] = values == [PROBE_VALUE] and record["items_changed"]
        record["duplicates"] = [finding for finding in validate_connectivity(INTENT, after)
                                if finding["kind"] in ("duplicate_pin", "duplicate_identity")]
        record["library_drops"] = [{"pin": entry["pin"], "lib_id": entry["lib_id"]}
                                   for entry in after if not entry["lib_id"].startswith("Acceptance:")]
    finally:
        record["undo"] = client.undo(doc)
        assert record["undo"].get("performed"), record["undo"]
        assert exact_items(client, docs) == baseline_items, "UpdateItems probe undo changed serialized items"
        assert observe_pins(client, docs.values()) == record["before"], \
            "UpdateItems probe undo changed pin identities"
        record["restored"] = "exact items and native pins/nets"


def update_regressions(client, docs, result):
    """Guarded probes of historical reports, one isolated update per placed unit.

    ``result`` is filled in place so partial evidence survives a failed probe. A
    defect is ``reproduced`` when seen; ``historical_not_reproduced`` requires
    every unit's update to have demonstrably taken effect; otherwise the probe
    is ``inconclusive``. Nothing is repaired or retried.
    """
    units = symbols(client, docs["MCU"], "U2")
    result["request"] = ("UpdateItems with the complete read-back SchematicSymbolInstance; "
                         "only the value field changed; one placed unit per request")
    result["units"] = []
    assert sorted(unit.unit.unit for unit in units) == [1, 2], "both buffer units must be placed"
    for unit in units:
        record = {"symbol_id": unit.id.value, "unit": unit.unit.unit}
        result["units"].append(record)
        update_probe(client, docs, unit, record)
    applied = all(record.get("applied") for record in result["units"])
    for key, kind in (("multi_unit_update_duplicate_pin_ids", "duplicates"),
                      ("update_dropped_library_nickname", "library_drops")):
        findings = [dict(finding, probed_unit=record["unit"])
                    for record in result["units"] for finding in record.get(kind, [])]
        status = "reproduced" if findings else "historical_not_reproduced" if applied else "inconclusive"
        result[key] = {"status": status, "findings": findings}
    return result


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


def bounded(coroutine, timeout):
    return asyncio.run(asyncio.wait_for(coroutine, timeout=timeout))


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


def capture(args, report, project, folder, env, owned, dependencies):
    """The owned native session. ``owned`` tracks the live manager for cleanup."""
    native = report["native"]
    owned["proc"], owned["log"], client = run.start(project, folder, args.output, env, "seed")
    report["owned_manager_pids"] = [owned["proc"].pid]
    native["measured_application_version"] = as_dict(client.send(base.GetVersion(), base.GetVersionResponse))
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
    update_regressions(client, docs, report["checks"].setdefault("update_probes", {}))
    print("rollback, undo/redo and per-unit UpdateItems probes completed", flush=True)
    observations = observe_pins(client, docs.values())
    require_connectivity(INTENT, observations)
    properties = part_properties(client, docs)
    report["checks"]["project_part_properties"] = properties
    run.write_json(args.output / "pin-observations.json", observations)
    board, report["checks"]["board"] = board_transfer(client, root, observations)
    report["checks"]["pages"] = native_views(client, docs, board, args.output)
    report["checks"]["bundled_entrypoints"] = bounded(bundled_entrypoints(env), SKILL_DELIVERY_TIMEOUT)
    report["checks"]["source_mcp_edit_undo"] = bounded(run.stdio_edit_check(client, env), MCP_EDIT_TIMEOUT)
    print("PCB parity, framed views and source MCP/skill integration passed", flush=True)
    assert run.disk_hashes(project) == before_disk, "capture implicitly saved schematic/board"
    for value in client.documents()["documents"] + client.boards():
        client.execute("SaveDocument", {"document": value})
    saved = run.disk_hashes(project)
    assert saved != before_disk, "explicit save did not write capture"
    expected = snapshot(client)
    run.write_json(args.output / "before-restart.json", expected)
    expected_pins = observe_pins(client, docs.values())
    # Every module the seed manager loaded, after all editors and checks ran.
    native["loaded"] = {"seed": loaded_identity(owned["proc"].pid, args.install, native, dependencies)}
    run.stop(owned["proc"], owned["log"])
    owned["proc"] = owned["log"] = None
    owned["proc"], owned["log"], client = run.start(project, folder, args.output, env, "reopened")
    report["owned_manager_pids"].append(owned["proc"].pid)
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
    native["loaded"]["reopened"] = loaded_identity(owned["proc"].pid, args.install, native, dependencies)
    report["checks"]["explicit_save_owned_stop_restart_reopen"] = "passed"
    report["saved_file_sha256"] = saved


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--install", type=Path, required=True,
                        help="native prefix whose every file the provenance manifest lists")
    parser.add_argument("--native-provenance", type=Path, required=True,
                        help="retained native-provenance.json, beside its source archive")
    parser.add_argument("--library-root", type=Path, default=Path("/usr/share/kicad"))
    parser.add_argument("--git-executable", default="git", help="use git.exe for a Windows-owned T3 worktree")
    parser.add_argument("--helper-probe", type=Path, help="optional block_example.py from the coordinated helper checkout")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if sys.platform != "linux":
        parser.error("run inside Linux/WSL with the retained native build")
    identity = native_identity(args.install, args.native_provenance)
    dependency_path = os.environ.get("LD_LIBRARY_PATH", "")
    dependencies = dependency_identity(dependency_path)
    args.output.mkdir(parents=True, exist_ok=False)
    source = source_identity(args.git_executable)
    report = {"result": "failed", "started_utc": datetime.now(timezone.utc).isoformat(),
              "kind": "scripted_source_fixture", "fresh_agent_evaluation": "pending separate evaluator",
              "installed_distribution_evaluation": "not run by this driver",
              "adapter_source": source, "native": identity,
              "native_dependency_search_path": dependency_path,
              "native_private_dependencies_sha256": dependencies,
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
    run.INSTALL = args.install
    run.MCP = MCP
    owned = {"proc": None, "log": None}
    try:
        with owned_display(args.output, report) as display_number:
            env = {**os.environ, "DISPLAY": ":" + display_number, "TMPDIR": str(folder / "tmp"),
                   "XDG_RUNTIME_DIR": str(folder / "runtime"), "KICAD_CONFIG_HOME": str(folder / "config"),
                   "KICAD_API_SOCKET": "ipc://" + str(folder / "tmp/kicad/api.sock"), "KICAD_API_TOKEN": "",
                   "KICAD_LIVE_TEST_PROJECT": "live-demo", "KICAD_LIVE_ROOT": str(folder),
                   "KICAD10_FOOTPRINT_DIR": str(args.library_root / "footprints"),
                   "LIBGL_ALWAYS_SOFTWARE": "1"}
            try:
                capture(args, report, project, folder, env, owned, dependencies)
            finally:
                # The manager stops before its display; each step is independent.
                if owned["proc"] is not None:
                    cleanup_steps(report, [("KiCad manager", lambda: run.stop(owned["proc"], owned["log"]))])
        # Identities after all owned processes stopped: nothing may have changed.
        assert native_identity(args.install, args.native_provenance)["files_sha256"] == identity["files_sha256"]
        assert dependency_identity(dependency_path) == dependencies, "private dependencies changed during run"
        final_source = source_identity(args.git_executable)
        assert final_source == source, "adapter source or worktree status changed during run"
        report["adapter_source_rechecked_at_end"] = True
        if "cleanup_errors" not in report:
            report["result"] = "passed"
    except BaseException as error:
        report["result"] = "failed"
        report["error"] = f"{type(error).__name__}: {error}"
        raise
    finally:
        report["finished_utc"] = datetime.now(timezone.utc).isoformat()
        cleanup_steps(report, [("project copy", lambda: shutil.copytree(project, args.output / "project")),
                               ("config copy", lambda: shutil.copytree(folder / "config", args.output / "config"))])
        run.write_json(args.output / "report.json", report)
    print(json.dumps({"result": report["result"], "evidence": str(args.output)}, indent=2))
    if report["result"] != "passed":
        sys.exit(1)


if __name__ == "__main__":
    main()
