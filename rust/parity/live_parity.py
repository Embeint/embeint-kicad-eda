# SPDX-License-Identifier: GPL-3.0-or-later
"""The --live part of mcp_parity.py: both MCP servers against a running KiCad.

Only the disposable test project is touched (e2e_guard.test_document refuses any other).  Every
edit is undone afterwards and the schematic is never saved; the schematic file must be unchanged
at the end.  The board is saved once, unchanged, only if it has no file yet (kicad-cli reads it).

  reads        live_schematic_documents, live_pcb_documents, live_schematic_hierarchy,
               live_schematic_items, live_document_revision and live_operation_status return
               identical results from both servers, and reading leaves the revision unchanged.
  transaction  For each server: create_items with an expected revision and an operation id
               commits one undo step (status OPS_COMMITTED, revision_after = the result's
               revision); a batch with an item KiCad rejects is rolled back (nothing changes,
               no commit stays open, OPS_ROLLED_BACK); a stale expected revision is refused
               (OPS_REJECTED, nothing changes).
  lost replies A forwarding proxy between the server and KiCad discards one real reply: the
               EndCommit reply (KiCad committed; the server must reconcile it as committed) and
               the BeginCommit reply (KiCad opened a commit; the server must drop it and report
               that nothing changed).  KiCad's own operation history decides the outcome.
  workflows    live_workflows.py: the schematic, library, board and shared tools through both
               servers, call by call, with KiCad's undo checked on every edit
               (LIVE_PARITY_AREAS=schematic,board limits the areas).
"""

import asyncio
import copy
import json
import os
from pathlib import Path
import shutil
import tempfile
import threading
import uuid

import pynng
from google.protobuf import json_format

from client import LiveSchematic, envelope, kiid, parse_items, point, sch, types



def as_json(message):
    return json_format.MessageToDict(message, preserving_proto_field_name=True)


def text_item(label, x_mm, item_id=None):
    item = sch.SchematicText(id=kiid(item_id), text=types.Text(text=label, position=point(x_mm, 190)))
    return json.loads(json.dumps({"@type": "type.googleapis.com/kiapi.schematic.types.SchematicText",
                                  **as_json(item)}))


def line_item(x_mm):
    item = sch.SchematicLine(id=kiid(), start=point(x_mm, 195), end=point(x_mm + 5.08, 195),
                             type=sch.SLT_WIRE)
    return {"@type": "type.googleapis.com/kiapi.schematic.types.SchematicLine", **as_json(item)}


def nm(mm):
    return str(round(mm * 1_000_000))


def vector(x, y):
    return {"x_nm": nm(x), "y_nm": nm(y)}


SCHEMATIC = "type.googleapis.com/kiapi.schematic.types."

# Items of many kinds, added as one undo step so the read comparison covers real content: every
# field KiCad fills in (strokes, fonts, angles, fields, enums, 64-bit coordinates) is printed by
# both servers from the same bytes, then parsed back by both.
POPULATE = [
    {"@type": SCHEMATIC + "SchematicLine", "start": vector(101.6, 101.6), "end": vector(127, 101.6), "type": "SLT_WIRE"},
    {"@type": SCHEMATIC + "SchematicLine", "start": vector(101.6, 120.65), "end": vector(127, 120.65), "type": "SLT_BUS"},
    {"@type": SCHEMATIC + "SchematicLine", "start": vector(101.6, 130), "end": vector(110, 135),
     "type": "SLT_GRAPHIC", "stroke": {"width": {"value_nm": "254000"}, "style": "SLS_DASH",
                                       "color": {"r": 0.25, "g": 0.5, "b": 0.75, "a": 1.0}}},
    {"@type": SCHEMATIC + "Junction", "position": vector(114.3, 101.6)},
    {"@type": SCHEMATIC + "NoConnectMarker", "position": vector(127, 110.49)},
    {"@type": SCHEMATIC + "LocalLabel", "position": vector(127, 101.6),
     "text": {"text": "NET_µA", "position": vector(127, 101.6)}, "spin_style": "SLSS_RIGHT"},
    {"@type": SCHEMATIC + "GlobalLabel", "position": vector(101.6, 101.6),
     "text": {"text": "VBUS", "position": vector(101.6, 101.6)}, "spin_style": "SLSS_LEFT", "shape": "SLSH_INPUT"},
    {"@type": SCHEMATIC + "HierarchicalLabel", "position": vector(127, 120.65),
     "text": {"text": "DATA[0..7]", "position": vector(127, 120.65)}, "shape": "SLSH_BIDI"},
    {"@type": SCHEMATIC + "SchematicText",
     "text": {"text": "Parity ± 0.1 % note", "position": vector(101.6, 140),
              "attributes": {"angle": {"value_degrees": 90.0}, "bold": True, "italic": True,
                             "size": {"x_nm": "1524000", "y_nm": "1524000"}}}},
]


def populate(checker, doc):
    """Adds POPULATE and two library symbols; returns how many undo steps remove them again."""
    steps = 0
    checker.mutate(doc, parse_items(POPULATE))
    steps += 1
    for lib_id, x in (("Device:R", 140), ("Device:C", 150)):
        try:
            checker.place_symbol(doc, lib_id, x, 101.6)
            steps += 1
        except Exception as error:  # noqa: BLE001 - the symbol libraries may not be configured
            print(f"  (no {lib_id}: {error})", flush=True)
    return steps


class DroppingProxy:
    """REP0 endpoint forwarding each request to KiCad; discards the reply to chosen requests."""

    def __init__(self, target, drop_types):
        self.target, self.drop = target, list(drop_types)
        base = Path(tempfile.gettempdir()) / f"embeint-live-proxy-{os.getpid()}-{uuid.uuid4().hex[:6]}"
        if os.name != "nt":
            base.mkdir()  # Windows pipes need no directory
        self.base = base
        self.url = "ipc://" + str(base / "api.sock")
        self.log, self.dropped, self.errors = [], [], []
        self.stop_event = threading.Event()
        self.socket = pynng.Rep0(listen=self.url, recv_timeout=50, send_timeout=2000)
        self.thread = threading.Thread(target=self.serve, daemon=True)
        self.thread.start()

    def serve(self):
        while not self.stop_event.is_set():
            try:
                raw = self.socket.recv()
            except pynng.Timeout:
                continue
            except pynng.exceptions.Closed:
                return
            kind = envelope.ApiRequest.FromString(raw).message.TypeName().rsplit(".", 1)[-1]
            try:
                with pynng.Req0(dial=self.target, send_timeout=10000, recv_timeout=10000,
                                resend_time=-1) as up:
                    up.send(raw)
                    answer = up.recv()
            except pynng.NNGException as error:
                # Recorded and checked, so a KiCad that stopped answering is reported as such
                self.errors.append(f"{kind}: {error}")
                self.log.append(kind)
                continue
            self.log.append(kind)
            if self.drop and self.drop[0] == kind:
                self.drop.pop(0)
                self.dropped.append(kind)
                continue  # KiCad ran it; the MCP server never hears back
            self.socket.send(answer)

    def stop(self):
        self.stop_event.set()
        self.socket.close()
        self.thread.join(timeout=5)
        shutil.rmtree(self.base, ignore_errors=True)


def run(binary, failures, servers, run_session, server_env, test_document):
    from mcp_parity import build_evidence

    checker = LiveSchematic()
    doc = test_document(checker)
    spec = as_json(doc)
    report = {"project": spec.get("project", {}).get("name"), "checks": [],
              "build": build_evidence(binary), "application_version": checker.documents()["version"]}
    endpoint = checker.socket_url

    def check(label, ok, detail=""):
        report["checks"].append({"check": label, "ok": bool(ok), **({"detail": detail} if detail else {})})
        print(f"  {'ok  ' if ok else 'FAIL'} {label}" + (f": {detail}" if detail and not ok else ""), flush=True)
        if not ok:
            failures.append(f"live {label}: {detail}")

    def revision():
        return checker.document_revision(doc)

    def item_ids():
        return sorted(item.get("id", {}).get("value", "") for item in as_json(checker.items(doc)).get("items", []))

    sch_file = Path(spec["project"]["path"]) / f"{spec['project']['name']}.kicad_sch"
    stamp = sch_file.stat().st_mtime_ns if sch_file.exists() else None

    def session(label, steps, socket=None):
        env = {"KICAD_API_SOCKET": socket or endpoint}
        if socket:
            env["KICAD_API_TIMEOUT_MS"] = "3000"
        make = dict(servers)[label]
        return asyncio.run(run_session(make(server_env(env)), steps))[2]

    def payload(result):
        assert not result.get("isError"), result
        return json.loads(result["content"][0]["text"])

    report["schema_sha256"] = payload(session("rust", [("live_mcp_capabilities", {})])[0])["schema_sha256"]

    original_ids = item_ids()
    undo_steps = populate(checker, doc)
    try:
        compare_reads(spec, servers, session, payload, check, revision, report, binary)
    finally:
        for _ in range(undo_steps):
            checker.undo(doc)
    check("the populated items are removed again", item_ids() == original_ids)

    # Base item for the rollback case (an existing id makes KiCad reject a created item)
    base = text_item("PARITY base", 20)
    checker.mutate(doc, parse_items([base]))
    base_id = base["id"]["value"]
    try:
        for index, (label, _) in enumerate(servers):
            transaction(label, index, spec, doc, checker, session, payload, check, revision, item_ids, base_id)
            lost_replies(label, index, spec, doc, checker, session, payload, check, revision, item_ids, endpoint)
            item_edits(label, spec, doc, checker, session, payload, check, revision, base_id, endpoint)
    finally:
        undo_created(checker, doc, item_ids, [base_id], original_ids)
    check("the base item is removed again", item_ids() == original_ids)
    manufacturing(checker, spec, session, payload, check)
    # The workflows of every tool area (live_workflows.py), both servers in turn
    import live_workflows
    from mcp_parity import DEVNULL
    areas = [a for a in os.environ.get("LIVE_PARITY_AREAS", "").split(",") if a] or None
    live_workflows.run_workflows(checker, spec, doc, servers, server_env, endpoint, DEVNULL, check, report, areas)
    check("open commits are all closed", revision()["open_commits"] == 0)
    if stamp is not None:
        check("the schematic file was not written", sch_file.stat().st_mtime_ns == stamp)
    return report


def manufacturing(checker, spec, session, payload, check):
    """The export tools and the fab package through both servers on the test board.  KiCad stamps
    Gerber and drill files with the time, so file contents are compared only for the placement CSV
    and the BOM; everything else is compared through the tool results (files written, verification,
    warnings, DRC summary).  Outputs go to scratch folders; the board file is never written."""
    from e2e_guard import test_board
    board = as_json(test_board(checker))
    board_file = Path(board["project"]["path"]) / board["board_filename"]
    stamp = board_file.stat().st_mtime_ns if board_file.exists() else None
    position = {"format": "PF_CSV", "single_file": True, "naked_filename": True, "units": "U_MM"}
    package = {"board_document": board, "schematic_document": spec, "allow_unsaved": True,
               "outputs": ["gerbers", "drill", "position", "step", "assembly_pdf", "bom", "schematic_pdf"]}
    results, contents = {}, {}
    with tempfile.TemporaryDirectory(prefix="parity-fab-") as scratch:
        for label in ("python", "rust"):
            root = Path(scratch) / label
            root.mkdir()
            steps = [("live_pcb_export", {"board_document": board, "kind": "position",
                                          "output_path": str(root / "single" / "board-pos.csv"), "options": position}),
                     ("live_schematic_export", {"document_spec": spec, "kind": "bom",
                                                "output_path": str(root / "single" / "bom.csv")}),
                     ("live_pcb_fab_package", dict(package, output_dir=str(root / "checked"))),
                     ("live_pcb_fab_package", dict(package, output_dir=str(root / "package"), checks=False))]
            replies = session(label, steps)
            text = json.dumps([r["content"][0]["text"] for r in replies])
            results[label] = json.loads(text.replace(json.dumps(str(root))[1:-1].replace("\\", "\\\\"), "<out>")
                                        .replace(json.dumps(str(root))[1:-1], "<out>"))
            contents[label] = {name: (root / "single" / name).read_bytes() if (root / "single" / name).exists()
                               else None for name in ("board-pos.csv", "bom.csv")}
    python, rust = results["python"], results["rust"]
    check("manufacturing: export results are identical", python[:2] == rust[:2], json.dumps(results)[:600])
    check("manufacturing: placement CSV and BOM contents are identical",
          contents["python"] == contents["rust"] and all(contents["python"].values()))
    check("manufacturing: the checked package (DRC with parity) gives the same result", python[2] == rust[2],
          json.dumps([python[2][:400], rust[2][:400]]))
    package_python = json.loads(python[3]) if python[3].startswith("{") else {}
    check("manufacturing: the package exports and verifies identically",
          python[3] == rust[3] and package_python.get("exported") is True, json.dumps([python[3][:400], rust[3][:400]]))
    if stamp is not None:
        check("manufacturing: the board file was not written", board_file.stat().st_mtime_ns == stamp)


def undo_created(checker, doc, item_ids, created, before):
    """Clean up a test step only while its items are the sole additions to its baseline.

    The disposable editor must be used exclusively during the run. Refuse cleanup if an earlier
    failed test left other items behind, rather than undoing that step in place of this one.
    """
    current = set(item_ids())
    present = set(created) & current
    if not present:
        return False
    if current != set(before) | present or checker.document_revision(doc)["open_commits"]:
        raise RuntimeError("document differs from the test cleanup baseline; inspect live-demo before undoing")
    checker.undo(doc)
    remaining = present & set(item_ids())
    if remaining:
        raise RuntimeError(f"undo did not remove the test items {sorted(remaining)}; inspect live-demo")
    return True


def compare_reads(spec, servers, session, payload, check, revision, report, binary):
    """Reads through both servers are identical, leave the revision alone, and parse back alike."""
    import mcp_parity
    before = revision()
    reads = [("live_schematic_documents", {}), ("live_pcb_documents", {}),
             ("live_schematic_hierarchy", {"document_spec": spec}), ("live_schematic_items", {"document_spec": spec}),
             ("live_document_revision", {"document_spec": spec}),
             ("live_operation_status", {"operation_id": f"parity-never-sent-{uuid.uuid4()}"})]
    results = {label: session(label, reads) for label, _ in servers}
    for (tool, _), python, rust in zip(reads, results["python"], results["rust"]):
        check(f"read {tool} identical", python == rust, f"{python} != {rust}"[:600])
    check("reads leave the revision unchanged", revision()["revision"] == before["revision"])
    items = payload(results["python"][3]).get("items", [])
    kinds = sorted({item["@type"].rsplit(".", 1)[-1] for item in items})
    report["items_compared"], report["item_types"] = len(items), kinds
    # KiCad's connectivity cleanup removes a junction that joins nothing, so it may not be read back
    check(f"{len(items)} live items of {len(kinds)} types read, library symbols included",
          len(kinds) >= 6 and "SchematicSymbolInstance" in kinds, ", ".join(kinds))
    # The same items parsed back (create/update input) by both: parse_items and protobuf JSON
    rust = mcp_parity.ParityProcess(binary)
    try:
        native = rust.ask({"op": "items", "values": items})
        check("live items parse back identically", native == mcp_parity.py_items(items),
              json.dumps(native)[:300])
        same = all(rust.ask({"op": "parse", "type": "google.protobuf.Any", "value": item})
                   == mcp_parity.py_parse("google.protobuf.Any", item) for item in items)
        check("live items round-trip identically through protobuf JSON", same)
    finally:
        rust.close()


def transaction(label, index, spec, doc, checker, session, payload, check, revision, item_ids, base_id):
    ids_before, rev0 = item_ids(), revision()["revision"]
    op = f"parity-{label}-{uuid.uuid4()}"
    created = [text_item(f"PARITY {label}", 40 + 30 * index), line_item(40 + 30 * index)]
    rejected_op, stale_op = f"{op}-rejected", f"{op}-stale"
    created_ids = [i["id"]["value"] for i in created]
    try:
        results = session(label, [
            ("live_schematic_create_items", {"document_spec": spec, "items": created,
                                             "expected_revision": rev0, "operation_id": op}),
            ("live_operation_status", {"operation_id": op}),
        ])
        result = payload(results[0])
        status = payload(results[1])
        ops = result.get("operations", [])
        check(f"{label}: create_items commits one operation",
              len(ops) == 1 and ops[0]["state"] == "committed" and ops[0]["operation_id"] == op, json.dumps(ops))
        check(f"{label}: KiCad records it committed", status["state"] == "OPS_COMMITTED"
              and status.get("revision_after") == result["revision"], json.dumps(status))
        check(f"{label}: the items exist", sorted(set(item_ids()) - set(ids_before)) == sorted(created_ids))
    finally:
        undo_created(checker, doc, item_ids, created_ids, ids_before)
    check(f"{label}: one undo removes them", item_ids() == ids_before)

    # A rejected item rolls the whole batch back
    rev1 = revision()["revision"]
    duplicate = text_item(f"PARITY {label} duplicate", 45 + 30 * index, base_id)
    results = session(label, [
        ("live_schematic_create_items", {"document_spec": spec, "items": [line_item(50 + 30 * index), duplicate],
                                         "operation_id": rejected_op}),
        ("live_operation_status", {"operation_id": rejected_op}),
        ("live_schematic_create_items", {"document_spec": spec, "items": [line_item(60 + 30 * index)],
                                         "expected_revision": "stale-0", "operation_id": stale_op}),
        ("live_operation_status", {"operation_id": stale_op}),
    ])
    check(f"{label}: a rejected item fails the call", results[0].get("isError"), json.dumps(results[0])[:300])
    check(f"{label}: the batch is rolled back", payload(results[1])["state"] == "OPS_ROLLED_BACK",
          json.dumps(results[1])[:300])
    check(f"{label}: a stale revision is refused", results[2].get("isError")
          and "AS_REVISION_MISMATCH" in results[2]["content"][0]["text"], json.dumps(results[2])[:300])
    check(f"{label}: KiCad records the refusal", payload(results[3])["state"] == "OPS_REJECTED")
    after = revision()
    check(f"{label}: nothing changed and no commit is open", item_ids() == ids_before and after["open_commits"] == 0,
          f"rev {rev1} -> {after}")


def item_edits(label, spec, doc, checker, session, payload, check, revision, base_id, endpoint):
    """Update/delete/batch change exactly the intended item; undo restores every field.

    Refuse cleanup when the document has changed beyond the expected edit. These checks
    deliberately need exclusive access to the guarded disposable project.
    """
    def snapshot():
        return {item["id"]["value"]: item for item in as_json(checker.items(doc)).get("items", [])}

    baseline = snapshot()
    updated = copy.deepcopy(baseline[base_id])
    updated["text"]["text"] = f"PARITY {label} replacement"
    missing = copy.deepcopy(updated)
    missing["id"]["value"] = str(uuid.uuid4())

    for verb in ("update", "delete", "batch"):
        tool = "live_schematic_batch" if verb == "batch" else f"live_schematic_{verb}_items"
        if verb == "delete":
            args, rejected = {"ids": [base_id]}, {"ids": [base_id, missing["id"]["value"]]}
            expected = {key: value for key, value in baseline.items() if key != base_id}
        elif verb == "update":
            args, rejected = {"items": [updated]}, {"items": [updated, missing]}
            expected = {**baseline, base_id: updated}
        else:
            step = {"command": "UpdateItems", "payload": {"items": [updated]}}
            args = {"steps": [step]}
            rejected = {"steps": [step, {"command": "CreateItems", "payload": {"items": [baseline[base_id]]}}]}
            expected = {**baseline, base_id: updated}

        for lost in (False, True):
            op = f"parity-{label}-{verb}-{uuid.uuid4()}"
            proxy = DroppingProxy(endpoint, ["EndCommit"]) if lost else None
            try:
                results = session(label, [(tool, {"document_spec": spec, **args, "operation_id": op,
                                                 "expected_revision": revision()["revision"]})],
                                  proxy.url if proxy else None)
            finally:
                if proxy:
                    proxy.stop()
            title = f"{label}: {verb}" + (" lost reply" if lost else "")
            try:
                result = {} if results[0].get("isError") else payload(results[0])
                status = checker.operation_status(op)
                check(title + " commits exactly the expected content", snapshot() == expected, json.dumps(results[0])[:300])
                check(title + " records its revision", status["state"] == "OPS_COMMITTED"
                      and status.get("revision_after") == result.get("revision"))
                if proxy:
                    check(title + " reconciles without resend", proxy.dropped == ["EndCommit"]
                          and proxy.log.count("BeginCommit") == 1 and proxy.log.count("EndCommit") == 1
                          and proxy.log[-1] == "GetOperationStatus" and not proxy.errors
                          and result.get("operations", [{}])[0].get("reconciled") is True)
            finally:
                current = snapshot()
                if current != baseline:
                    if current != expected or revision()["open_commits"]:
                        raise RuntimeError("unexpected live edit state; refusing to undo unrelated content")
                    undo = session(label, [("live_schematic_execute", {"command": "UndoLastChange", "payload": {"document": spec}})])
                    payload(undo[0])
            check(title + " is one undo step", snapshot() == baseline)

        op = f"parity-{label}-{verb}-rejected-{uuid.uuid4()}"
        rejected_result = session(label, [(tool, {"document_spec": spec, **rejected, "operation_id": op})])[0]
        check(f"{label}: {verb} partial failure rolls back every field", rejected_result.get("isError")
              and checker.operation_status(op)["state"] == "OPS_ROLLED_BACK" and snapshot() == baseline
              and revision()["open_commits"] == 0, json.dumps(rejected_result)[:300])
        if snapshot() != baseline:
            raise RuntimeError("failed batch changed the live baseline; inspect test project")
        op = f"parity-{label}-{verb}-stale-{uuid.uuid4()}"
        stale = session(label, [(tool, {"document_spec": spec, **args, "operation_id": op, "expected_revision": "stale-0"})])[0]
        check(f"{label}: {verb} refuses stale revisions", stale.get("isError")
              and checker.operation_status(op)["state"] == "OPS_REJECTED" and snapshot() == baseline)


def lost_replies(label, index, spec, doc, checker, session, payload, check, revision, item_ids, endpoint):
    ids_before = item_ids()
    # EndCommit's reply lost: KiCad committed; the server must find that out, not resend
    proxy = DroppingProxy(endpoint, ["EndCommit"])
    op = f"parity-{label}-lost-end-{uuid.uuid4()}"
    item = text_item(f"PARITY {label} lost reply", 40 + 30 * index)
    try:
        results = session(label, [("live_schematic_create_items",
                                   {"document_spec": spec, "items": [item], "operation_id": op})], proxy.url)
    finally:
        proxy.stop()
    try:
        result = payload(results[0]) if not results[0].get("isError") else {}
        ops = result.get("operations", [])
        check(f"{label}: lost EndCommit reply is reconciled as committed",
              proxy.dropped == ["EndCommit"] and len(ops) == 1 and ops[0].get("reconciled") is True
              and ops[0]["state"] == "committed", json.dumps(results[0])[:400])
        check(f"{label}: nothing was resent", proxy.log == ["BeginCommit", "CreateItems", "EndCommit",
                                                             "GetOperationStatus"] and not proxy.errors,
              json.dumps([proxy.log, proxy.errors]))
        check(f"{label}: the item was committed once",
              sorted(set(item_ids()) - set(ids_before)) == [item["id"]["value"]])
    finally:
        undo_created(checker, doc, item_ids, [item["id"]["value"]], ids_before)

    # BeginCommit's reply lost: KiCad opened a commit; the server must drop it
    proxy = DroppingProxy(endpoint, ["BeginCommit"])
    op = f"parity-{label}-lost-begin-{uuid.uuid4()}"
    never = text_item("PARITY never", 40)
    try:
        results = session(label, [("live_schematic_create_items",
                                   {"document_spec": spec, "items": [never],
                                    "operation_id": op})], proxy.url)
    finally:
        proxy.stop()
    text = results[0]["content"][0]["text"]
    check(f"{label}: lost BeginCommit reply drops the open commit",
          results[0].get("isError") and "it was dropped and nothing was changed" in text, text[:400])
    check(f"{label}: no edit was sent after it",
          proxy.log == ["BeginCommit", "GetOperationStatus", "EndCommit"] and not proxy.errors,
          json.dumps([proxy.log, proxy.errors]))
    check(f"{label}: KiCad has it rolled back", checker.operation_status(op)["state"] == "OPS_ROLLED_BACK")
    check(f"{label}: nothing changed and no commit is open",
          item_ids() == ids_before and revision()["open_commits"] == 0)
    undo_created(checker, doc, item_ids, [never["id"]["value"]], ids_before)  # only if wrongly applied
