# SPDX-License-Identifier: GPL-3.0-or-later
"""Rerunnable KiCad 10.0.x compatibility checks for the development (10.99) fork.

Subcommands (each writes a JSON report into a NEW --out folder and never edits its input):

  versions DIR...          format version headers declared by every KiCad file in each folder
  reference                run the reference kicad-cli (ERC, DRC, every export, library reads) on a
                           COPY of a project and classify each result; no development KiCad needed
  reference-mixed          fork-written rules and project files beside reference-authored designs
  reference-fixture        build a reference-authored project: copies of reference demos rewritten by
                           the reference kicad-cli and its Python pcbnew module (pro/prl)
  fork-exports             LIVE: run every live_schematic_export / live_pcb_export kind twice on the
                           open compat-fixture project, normalize, and compare with the golden hashes
  fork-roundtrip save|verify
                           LIVE: snapshot and explicitly save a reference-authored project in the fork,
                           then (after a restart) check the reopened content is unchanged

The live subcommands talk to the development KiCad over IPC and refuse projects other than the one
named by KICAD_LIVE_TEST_PROJECT.  See KICAD_10_COMPATIBILITY.md for the procedure and the matrix.
"""

import argparse
from hashlib import sha256
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

from compat.formats import normalized_tree, project_versions, tree_hash  # noqa: E402

GOLDEN = HERE / "golden" / "mcp-exports.json"
FIXTURE = HERE / "fixture" / "compat-fixture"
SCH_EXPORTS = ("svg", "pdf", "dxf", "ps", "netlist", "bom")
PCB_EXPORTS = ("gerbers", "drill", "position", "step", "pdf", "svg", "dxf", "ipc2581", "ipcd356", "odb",
               "stats", "dsn", "gencad", "idf")
FUTURE = re.compile(r"newer version|future|too recent|more recent version", re.I)


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True, default=str) + "\n", encoding="utf-8")


def new_folder(path):
    path = Path(path)
    path.mkdir(parents=True, exist_ok=False)
    return path


def file_hashes(folder):
    """Hashes of the project's files; KiCad's local-history repository (.history), lock files and
    backups are not project content."""
    result = {}
    for path in sorted(Path(folder).rglob("*")):
        relative = path.relative_to(folder).as_posix()
        if (path.is_file() and not relative.startswith(".history/") and not path.name.startswith("~")
                and not path.name.endswith(".lck") and "-backups/" not in relative):
            result[relative] = sha256(path.read_bytes()).hexdigest()
    return result


# ---- Reference kicad-cli ---------------------------------------------------------------------------

def reference_jobs(project, out):
    """(name, kicad-cli arguments, expected output path) for every check against a project copy."""
    pro = next(project.glob("*.kicad_pro"))
    sch, pcb = pro.with_suffix(".kicad_sch"), pro.with_suffix(".kicad_pcb")
    o = out / "outputs"
    jobs = [("version", ["version", "--format", "about"], None),
            ("sch erc", ["sch", "erc", "--format", "json", "--severity-all", "-o", str(o / "erc.json"), str(sch)],
             o / "erc.json"),
            ("pcb drc", ["pcb", "drc", "--format", "json", "--schematic-parity", "--severity-all",
                         "-o", str(o / "drc.json"), str(pcb)], o / "drc.json")]
    sch_args = {"svg": ["svg", "-o", str(o / "sch-svg")], "pdf": ["pdf", "-o", str(o / "sch.pdf")],
                "dxf": ["dxf", "-o", str(o / "sch-dxf")], "ps": ["ps", "-o", str(o / "sch-ps")],
                "netlist": ["netlist", "--format", "kicadsexpr", "-o", str(o / "sch.net")],
                "bom": ["bom", "--fields", "Reference,Value,Footprint,Datasheet", "-o", str(o / "bom.csv")]}
    for kind in SCH_EXPORTS:
        args = sch_args[kind]
        jobs.append((f"sch export {kind}", ["sch", "export", *args, str(sch)], Path(args[-1])))
    layers = "F.Cu,B.Cu,F.Silkscreen,Edge.Cuts"
    pcb_args = {"gerbers": ["gerbers", "--board-plot-params", "-o", str(o / "gerbers") + os.sep],
                "drill": ["drill", "-o", str(o / "drill") + os.sep],
                "position": ["pos", "-o", str(o / "board.pos")],
                "step": ["step", "--subst-models", "--force", "-o", str(o / "board.step")],
                "pdf": ["pdf", "--layers", layers, "-o", str(o / "board.pdf")],
                "svg": ["svg", "--layers", layers, "--mode-single", "-o", str(o / "board.svg")],
                "dxf": ["dxf", "--layers", layers, "--mode-single", "-o", str(o / "board.dxf")],
                "ipc2581": ["ipc2581", "-o", str(o / "board.xml")],
                "ipcd356": ["ipcd356", "-o", str(o / "board.d356")],
                "odb": ["odb", "-o", str(o / "board-odb.zip")],
                "stats": ["stats", "-o", str(o / "board-stats.txt")],
                "gencad": ["gencad", "-o", str(o / "board.cad")]}
    for kind in PCB_EXPORTS:
        if kind not in pcb_args:
            jobs.append((f"pcb export {kind}", None, None))  # the reference CLI has no such export
            continue
        args = pcb_args[kind]
        jobs.append((f"pcb export {kind}", ["pcb", "export", *args, str(pcb)], Path(args[-1].rstrip(os.sep))))
    for library in sorted(project.glob("*.kicad_sym")):
        target = o / "libs" / library.name
        jobs.append((f"sym read {library.name}", ["sym", "upgrade", "--force", "-o", str(target), str(library)],
                     target))
    for library in sorted(project.glob("*.pretty")):
        target = o / "libs" / library.name
        jobs.append((f"fp read {library.name}", ["fp", "upgrade", "--force", "-o", str(target), str(library)],
                     target))
    return jobs


def classify(code, text, produced):
    if FUTURE.search(text):
        return "refused-newer-format"
    if code == 0 and produced:
        return "ok"
    if code == 0:
        return "no-output"
    if code == 5 and produced:  # ERC/DRC exit 5 when violations are found with --exit-code-violations
        return "ok-violations"
    return "failed"


def summarize_report(path):
    """Violation counts by type from an ERC/DRC JSON report."""
    data = json.loads(Path(path).read_text(encoding="utf-8"))
    counts = {}
    for section in ("violations", "unconnected_items", "schematic_parity"):
        for entry in data.get(section, []):
            key = f"{section}:{entry.get('type', '?')}:{entry.get('severity', '?')}"
            counts[key] = counts.get(key, 0) + 1
    for sheet in data.get("sheets", []):
        for entry in sheet.get("violations", []):
            key = f"violations:{entry.get('type', '?')}:{entry.get('severity', '?')}"
            counts[key] = counts.get(key, 0) + 1
    return dict(sorted(counts.items()))


def run_reference(args):
    out = new_folder(args.out)
    project = out / "project"
    shutil.copytree(args.project, project)
    before = file_hashes(project)
    report = {"cli": str(args.cli), "source": str(Path(args.project).resolve()), "label": args.label,
              "versions": project_versions(project), "checks": {}}
    (out / "outputs").mkdir()
    (out / "outputs" / "libs").mkdir()
    for name, cli_args, produced_path in reference_jobs(project, out):
        if cli_args is None:
            report["checks"][name] = {"result": "not-available-in-reference-cli"}
            print(f"{name:28} not-available-in-reference-cli", flush=True)
            continue
        for folder_arg in cli_args:
            if folder_arg.endswith(os.sep):
                Path(folder_arg).mkdir(parents=True, exist_ok=True)
        completed = subprocess.run([str(args.cli), *cli_args], capture_output=True, text=True,
                                   encoding="utf-8", errors="replace", timeout=600)
        text = completed.stdout + completed.stderr
        produced = bool(produced_path and produced_path.exists()
                        and (produced_path.is_file() or any(produced_path.iterdir())))
        entry = {"exit_code": completed.returncode, "result": classify(completed.returncode, text, produced),
                 "output_tail": text.strip().splitlines()[-6:]}
        if name == "version":
            entry["result"] = "ok" if completed.returncode == 0 else "failed"
            report["reference_version"] = next((line.split(":", 1)[1].strip() for line in text.splitlines()
                                                if line.strip().startswith("Version:")), text.strip())
        if produced and name in ("sch erc", "pcb drc"):
            entry["findings"] = summarize_report(produced_path)
        if produced and produced_path.is_dir() and name.startswith(("sym read", "fp read")):
            entry["files"] = len([p for p in produced_path.rglob("*") if p.is_file()])
        report["checks"][name] = entry
        print(f"{name:28} {entry['result']:24} rc={completed.returncode}", flush=True)
    after = file_hashes(project)
    report["input_unchanged"] = after == before
    # The reference may rewrite settings files even when it refuses the design (a downgrade)
    report["changed_inputs"] = {
        name: {"before": report["versions"].get(name), "after": project_versions(project).get(name)}
        for name in sorted(set(before) | set(after)) if before.get(name) != after.get(name)}
    for name in report["changed_inputs"]:
        print(f"input rewritten by the reference: {name}", flush=True)
    write_json(out / "report.json", report)
    print("report:", out / "report.json")
    return compare_expected(report, args.expect)


def compare_expected(report, expect_path):
    """Exit status against a committed expectation: {"checks": {name: result}, "changed_inputs": [...]}."""
    if not expect_path:
        return 0
    expected = json.loads(Path(expect_path).read_text(encoding="utf-8"))
    mismatches = [f"{name}: expected {result}, got {report['checks'].get(name, {}).get('result')}"
                  for name, result in expected["checks"].items()
                  if report["checks"].get(name, {}).get("result") != result]
    if sorted(expected.get("changed_inputs", [])) != sorted(report["changed_inputs"]):
        mismatches.append(f"changed inputs: expected {expected.get('changed_inputs', [])}, "
                          f"got {sorted(report['changed_inputs'])}")
    for line in mismatches:
        print("MISMATCH", line)
    print("matches expectation" if not mismatches else f"{len(mismatches)} mismatch(es)")
    return 1 if mismatches else 0


def run_reference_mixed(args):
    """Fork-written project-level files beside reference-authored designs: the rules file and the
    project file, each placed alone in a copy of the reference project."""
    out = new_folder(args.out)
    reference, fork = Path(args.reference_project), Path(args.fork_project)
    stem = next(reference.glob("*.kicad_pro")).stem
    report = {"cli": str(args.cli), "cases": {}}
    for case, source in (("fork rules", next(fork.glob("*.kicad_dru"))),
                         ("fork project file", next(fork.glob("*.kicad_pro")))):
        project = out / case.replace(" ", "-")
        shutil.copytree(reference, project)
        target = project / f"{stem}{source.suffix}"
        shutil.copy(source, target)
        before = sha256(target.read_bytes()).hexdigest()
        entry = {"file": source.name, "declares": file_versions_safe(target), "checks": {}}
        for name, cli_args, produced in (
                ("sch erc", ["sch", "erc", "--format", "json", "-o", str(project / "erc.json"),
                             str(project / f"{stem}.kicad_sch")], project / "erc.json"),
                ("pcb drc", ["pcb", "drc", "--format", "json", "-o", str(project / "drc.json"),
                             str(project / f"{stem}.kicad_pcb")], project / "drc.json"),
                ("pcb export gerbers", ["pcb", "export", "gerbers", "-o", str(project / "gerbers") + os.sep,
                                        str(project / f"{stem}.kicad_pcb")], project / "gerbers")):
            completed = subprocess.run([str(args.cli), *cli_args], capture_output=True, text=True,
                                       encoding="utf-8", errors="replace", timeout=600)
            text = completed.stdout + completed.stderr
            entry["checks"][name] = {"exit_code": completed.returncode,
                                     "result": classify(completed.returncode, text, produced.exists()),
                                     "rule_errors": bool(re.search(r"rule|\.kicad_dru", text, re.I)
                                                         and completed.returncode),
                                     "output_tail": text.strip().splitlines()[-4:]}
        entry["file_unchanged"] = sha256(target.read_bytes()).hexdigest() == before
        report["cases"][case] = entry
        print(case, {k: v["result"] for k, v in entry["checks"].items()}, "unchanged:", entry["file_unchanged"])
    write_json(out / "report.json", report)
    ok = all(c["file_unchanged"] and all(v["result"] == "ok" for v in c["checks"].values())
             for c in report["cases"].values())
    return 0 if ok else 1


def file_versions_safe(path):
    from compat.formats import file_versions
    try:
        return file_versions(path)
    except ValueError as error:
        return {"error": str(error)}


# ---- Reference-authored fixture --------------------------------------------------------------------

def run_reference_fixture(args):
    """complex_hierarchy (hierarchy, project libraries and tables) plus a footprint with an embedded
    model and a rules file from vme-wren, every KiCad file rewritten by the reference writer."""
    share = Path(args.share)
    out = new_folder(args.out)
    project = out / "ref-fixture"
    shutil.copytree(share / "demos/complex_hierarchy", project)
    for path in project.rglob("*"):
        path.chmod(0o666 if path.is_file() else 0o777)
    shutil.copy(share / "demos/vme-wren/wren.pretty/SODFL1608X65N.kicad_mod",
                project / "complex_hierarchy.pretty/SODFL1608X65N.kicad_mod")
    shutil.copy(share / "demos/vme-wren/vme-wren.kicad_dru", project / "complex_hierarchy.kicad_dru")
    log = {}

    def cli(name, *cli_args):
        completed = subprocess.run([str(args.cli), *cli_args], capture_output=True, text=True,
                                   encoding="utf-8", errors="replace", timeout=600)
        log[name] = {"exit_code": completed.returncode,
                     "output_tail": (completed.stdout + completed.stderr).strip().splitlines()[-4:]}
        if completed.returncode:
            raise SystemExit(f"reference {name} failed: {log[name]}")

    for sheet in sorted(project.glob("*.kicad_sch")):
        cli(f"sch upgrade {sheet.name}", "sch", "upgrade", "--force", str(sheet))
    cli("pcb upgrade", "pcb", "upgrade", "--force", str(project / "complex_hierarchy.kicad_pcb"))
    cli("sym upgrade", "sym", "upgrade", "--force", str(project / "complex_hierarchy.kicad_sym"))
    cli("fp upgrade", "fp", "upgrade", "--force", str(project / "complex_hierarchy.pretty"))
    # kicad-cli never writes the project file; the reference pcbnew module saves board, pro and prl
    script = ("import pcbnew, sys; b = pcbnew.LoadBoard(sys.argv[1]); "
              "assert pcbnew.SaveBoard(sys.argv[1], b); print(pcbnew.Version())")
    completed = subprocess.run([str(args.python), "-I", "-c", script, str(project / "complex_hierarchy.kicad_pcb")],
                               capture_output=True, text=True, timeout=600)
    log["pcbnew SaveBoard"] = {"exit_code": completed.returncode,
                               "output_tail": (completed.stdout + completed.stderr).strip().splitlines()[-4:]}
    if completed.returncode:
        raise SystemExit(f"reference pcbnew save failed: {log['pcbnew SaveBoard']}")
    report = {"steps": log, "versions": project_versions(project), "sha256": file_hashes(project)}
    write_json(out / "report.json", report)
    print(json.dumps(report["versions"], indent=1))
    return 0


# ---- Live checks against the development KiCad -------------------------------------------------------

def live_client():
    from client import LiveSchematic
    from e2e_guard import test_board, test_document
    client = LiveSchematic(timeout_ms=300000)
    root = test_document(client)
    return client, root, test_board(client)


def export_once(client, root, board, folder):
    """Every MCP export kind into its own subfolder; returns {kind: error or None}."""
    errors = {}
    names = {"svg": "", "dxf": "", "ps": "", "pdf": "schematic.pdf", "netlist": "schematic.net",
             "bom": "bom.csv"}
    for kind in SCH_EXPORTS:
        target = folder / f"sch-{kind}"
        target.mkdir(parents=True)
        try:
            client.export(root, kind, str(target / names[kind]) if names[kind] else str(target))
            errors[f"sch/{kind}"] = None
        except Exception as error:  # noqa: BLE001 - every kind is reported
            errors[f"sch/{kind}"] = str(error)
    files = {"position": "board.pos", "step": "board.step", "pdf": "board.pdf", "svg": "board.svg",
             "dxf": "board.dxf", "ipc2581": "board.xml", "ipcd356": "board.d356", "odb": "board-odb.zip",
             "stats": "board-stats.txt", "dsn": "board.dsn", "gencad": "board.cad", "idf": "board.emn"}
    for kind in PCB_EXPORTS:
        target = folder / f"pcb-{kind}"
        target.mkdir(parents=True)
        try:
            client.board_export(board, kind, str(target / files[kind]) if kind in files else str(target) + "/")
            errors[f"pcb/{kind}"] = None
        except Exception as error:  # noqa: BLE001
            errors[f"pcb/{kind}"] = str(error)
    return errors


def run_fork_exports(args):
    from client import as_dict, base
    out = new_folder(args.out)
    client, root, board = live_client()
    project_dir = Path(root.project.path)
    report = {"application_version": as_dict(client.send(base.GetVersion(), base.GetVersionResponse)),
              "project": str(project_dir), "before": file_hashes(project_dir), "exports": {}}
    runs = []
    for run in ("run1", "run2"):
        errors = export_once(client, root, board, out / run)
        runs.append(errors)
    golden = json.loads(Path(args.golden).read_text()) if Path(args.golden).exists() else {"exports": {}}
    failures = []
    for key in runs[0]:
        folder = key.replace("/", "-")
        entry = {"error": runs[0][key] or runs[1][key]}
        if not entry["error"]:
            trees = [normalized_tree(out / run / folder, (project_dir,)) for run in ("run1", "run2")]
            entry.update(files=trees[0], tree_sha256=tree_hash(trees[0]), deterministic=trees[0] == trees[1],
                         raw_identical=file_hashes(out / "run1" / folder) == file_hashes(out / "run2" / folder))
            if not trees[0]:
                entry["error"] = "export produced no files"
        expected = golden["exports"].get(key, {})
        entry["golden"] = ("match" if expected.get("tree_sha256") == entry.get("tree_sha256")
                           else "missing" if not expected else "mismatch")
        if entry["error"] or not entry.get("deterministic"):
            failures.append(key)
        elif entry["golden"] != "match" and not args.update:
            failures.append(key)
        report["exports"][key] = entry
        print(f"{key:14} {'ERROR ' + entry['error'] if entry['error'] else ''}"
              f"{'' if entry['error'] else ('deterministic' if entry['deterministic'] else 'NONDETERMINISTIC')}"
              f" golden={entry['golden']}", flush=True)
    report["project_unchanged"] = file_hashes(project_dir) == report.pop("before")
    if not report["project_unchanged"]:
        failures.append("project files changed")
    if args.update:
        Path(args.golden).parent.mkdir(parents=True, exist_ok=True)
        write_json(Path(args.golden), {
            "fixture": "compat/fixture/compat-fixture",
            "note": "sha256 of normalized export trees (compat/formats.py); regenerate with fork-exports --update",
            "application_version": report["application_version"].get("version", {}).get("full_version"),
            "exports": {k: {"tree_sha256": v["tree_sha256"], "files": v["files"]}
                        for k, v in sorted(report["exports"].items()) if v.get("tree_sha256")}})
    report["failures"] = failures
    write_json(out / "report.json", report)
    print("failures:", failures or "none")
    return 1 if failures else 0


def snapshot(client, root, board):
    from board_client import ITEM_TYPES
    from client import as_dict, commands
    from qualification.persistence import persisted_board_items, persisted_schematic_item
    result = {"schematics": {}}
    hierarchy = client.hierarchy(root)

    def visit(nodes):
        for node in nodes:
            spec = type(root)()
            spec.CopyFrom(root)
            spec.sheet_path.CopyFrom(node.path)
            key = "/".join(i.value for i in node.path.path)
            result["schematics"][key] = sorted(
                (json.dumps(persisted_schematic_item(as_dict(item)), sort_keys=True)
                 for item in client.items(spec).items))
            visit(node.children)

    visit(hierarchy.top_level_sheets)
    netlist = client.send(commands.GetSchematicNetlist(document=root), commands.SchematicNetlistResponse)
    result["nets"] = sorted(json.dumps({"name": net.name, "sheets": sorted(
        [sorted(i.value for i in sheet.items) for sheet in net.sheets])}) for net in netlist.nets)
    result["board"] = [json.dumps(item, sort_keys=True) for item in persisted_board_items(
        (as_dict(item) for item in client.board_items(board, list(ITEM_TYPES))), board.board_filename)]
    return result


PIN_TYPE = "type.googleapis.com/kiapi.schematic.types.SchematicPin"


def canonical(snapshot_value):
    """Drop identities neither KiCad 10.0.6 nor the fork stores in the files: library-definition pin
    UUIDs (generated on load, like the graphic UUIDs persisted_schematic_item already drops) and the
    board root UUID that parents board-level items (also generated on load)."""
    result = {"nets": snapshot_value["nets"], "schematics": {}}
    for sheet, items in snapshot_value["schematics"].items():
        cleaned = []
        for text in items:
            item = json.loads(text)
            for child in item.get("definition", {}).get("items", []):
                if child.get("item", {}).get("@type") == PIN_TYPE:
                    child["item"].pop("id", None)
            cleaned.append(json.dumps(item, sort_keys=True))
        result["schematics"][sheet] = sorted(cleaned)
    board = [json.loads(text) for text in snapshot_value["board"]]
    known = {item.get("id", {}).get("value") for item in board}
    for item in board:
        parent = item.get("parent", {}).get("value")
        if parent and parent not in known:  # the board itself, not an item in the file
            item["parent"] = {"value": "board"}
    # Teardrop zones are generated copper: neither version stores their UUIDs, and the fork writes
    # their invisible border as "hatch none", which reads back as a solid border.  Compare their
    # geometry and settings separately from the rest of the board.
    teardrops = [item for item in board if item.get("type") == "ZT_TEARDROP"]
    for item in teardrops:
        item.pop("id", None)
        item.get("border", {}).pop("style", None)
    result["board"] = sorted(json.dumps(item, sort_keys=True) for item in board if item.get("type") != "ZT_TEARDROP")
    result["teardrops"] = sorted(json.dumps(item, sort_keys=True) for item in teardrops)
    return result


def count_teardrops(board_file):
    """Teardrop zones stored in a board file."""
    text = Path(board_file).read_text(encoding="utf-8")
    zones = re.findall(r"\n\t\(zone\n.*?\n\t\)", text, re.S)
    return sum(1 for zone in zones if "(teardrop" in zone)


def compare_snapshots(expected, actual):
    expected, actual = canonical(expected), canonical(actual)
    differences = {}
    for key in ("nets", "board", "teardrops"):
        missing, extra = set(expected[key]) - set(actual[key]), set(actual[key]) - set(expected[key])
        if missing or extra:
            differences[key] = {"missing": len(missing), "extra": len(extra),
                                "example_missing": sorted(missing)[:2], "example_extra": sorted(extra)[:2]}
    if set(expected["schematics"]) != set(actual["schematics"]):
        differences["sheets"] = {"expected": sorted(expected["schematics"]), "actual": sorted(actual["schematics"])}
    for sheet, items in expected["schematics"].items():
        other = actual["schematics"].get(sheet, [])
        missing, extra = set(items) - set(other), set(other) - set(items)
        if missing or extra:
            differences[f"sheet {sheet}"] = {"missing": len(missing), "extra": len(extra),
                                             "example_missing": sorted(missing)[:1], "example_extra": sorted(extra)[:1]}
    return differences


def run_fork_roundtrip(args):
    from client import as_dict, base
    state = Path(args.state)
    client, root, board = live_client()
    project_dir = Path(root.project.path)
    current = snapshot(client, root, board)
    if args.phase == "save":
        board_file = project_dir / board.board_filename
        before = {"versions": project_versions(project_dir), "sha256": file_hashes(project_dir),
                  "teardrop_zones_in_file": count_teardrops(board_file),
                  "teardrop_zones_after_open": len(canonical(current)["teardrops"])}
        client.execute("SaveDocument", {"document": as_dict(root)})
        client.board_save(board)
        after = snapshot(client, root, board)
        write_json(state, {"application_version": as_dict(client.send(base.GetVersion(), base.GetVersionResponse)),
                           "before": before, "snapshot": current,
                           "unchanged_by_save": not compare_snapshots(current, after),
                           "after": {"versions": project_versions(project_dir),
                                     "sha256": file_hashes(project_dir),
                                     "teardrop_zones_in_file": count_teardrops(board_file)}})
        print("saved; versions after save:", json.dumps(project_versions(project_dir), indent=1))
        return 0
    saved = json.loads(state.read_text())
    differences = compare_snapshots(saved["snapshot"], current)
    on_disk = file_hashes(project_dir) == saved["after"]["sha256"]
    write_json(state.with_name(state.stem + "-verify.json"),
               {"differences": differences, "reopen_left_files_unchanged": on_disk,
                "unchanged_by_save": saved["unchanged_by_save"],
                "teardrop_zones": {"reference_file": saved["before"]["teardrop_zones_in_file"],
                                   "after_fork_open": saved["before"]["teardrop_zones_after_open"],
                                   "fork_file": saved["after"]["teardrop_zones_in_file"],
                                   "after_reopen": len(canonical(current)["teardrops"])}})
    print("differences:", json.dumps(differences, indent=1) if differences else "none",
          "| files unchanged by reopen:", on_disk)
    return 1 if differences or not saved["unchanged_by_save"] else 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    versions = sub.add_parser("versions")
    versions.add_argument("folders", nargs="+")
    reference = sub.add_parser("reference")
    reference.add_argument("--cli", required=True, help="reference kicad-cli executable")
    reference.add_argument("--project", default=str(FIXTURE), help="project folder (copied, never edited)")
    reference.add_argument("--label", default="")
    reference.add_argument("--out", required=True)
    reference.add_argument("--expect", help="JSON of expected results; exit 1 on any difference")
    mixed = sub.add_parser("reference-mixed")
    mixed.add_argument("--cli", required=True)
    mixed.add_argument("--reference-project", required=True, help="reference-authored project folder")
    mixed.add_argument("--fork-project", default=str(FIXTURE), help="project with fork-written rules/pro")
    mixed.add_argument("--out", required=True)
    ref_fixture = sub.add_parser("reference-fixture")
    ref_fixture.add_argument("--cli", required=True)
    ref_fixture.add_argument("--python", required=True, help="reference KiCad's python (pcbnew module)")
    ref_fixture.add_argument("--share", required=True, help="reference share/kicad folder (demos)")
    ref_fixture.add_argument("--out", required=True)
    exports = sub.add_parser("fork-exports")
    exports.add_argument("--out", required=True)
    exports.add_argument("--golden", default=str(GOLDEN))
    exports.add_argument("--update", action="store_true", help="rewrite the golden hashes")
    roundtrip = sub.add_parser("fork-roundtrip")
    roundtrip.add_argument("phase", choices=("save", "verify"))
    roundtrip.add_argument("--state", required=True, help="JSON written by save and read by verify")
    args = parser.parse_args(argv)
    if args.command == "versions":
        print(json.dumps({folder: project_versions(folder) for folder in args.folders}, indent=1, sort_keys=True))
        return 0
    return {"reference": run_reference, "reference-mixed": run_reference_mixed,
            "reference-fixture": run_reference_fixture,
            "fork-exports": run_fork_exports, "fork-roundtrip": run_fork_roundtrip}[args.command](args)


if __name__ == "__main__":
    sys.exit(main())
