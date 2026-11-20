# SPDX-License-Identifier: GPL-3.0-or-later
"""The --live part of mcp_parity.py: both MCP servers against a running KiCad.

Only the disposable test project is touched (e2e_guard.test_document refuses any other).  Every
edit is undone afterwards and nothing is saved; the schematic file must be unchanged at the end.

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
"""

import asyncio
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
        self.log, self.dropped = [], []
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
            with pynng.Req0(dial=self.target, send_timeout=10000, recv_timeout=10000, resend_time=-1) as up:
                up.send(raw)
                answer = up.recv()
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
    checker = LiveSchematic()
    doc = test_document(checker)
    spec = as_json(doc)
    report = {"project": spec.get("project", {}).get("name"), "checks": []}
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
    finally:
        checker.undo(doc)  # the base item
    check("open commits are all closed", revision()["open_commits"] == 0)
    if stamp is not None:
        check("the schematic file was not written", sch_file.stat().st_mtime_ns == stamp)
    return report


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
    check(f"{label}: the items exist", sorted(set(item_ids()) - set(ids_before)) ==
          sorted(i["id"]["value"] for i in created))
    checker.undo(doc)
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
    result = payload(results[0]) if not results[0].get("isError") else {}
    ops = result.get("operations", [])
    check(f"{label}: lost EndCommit reply is reconciled as committed",
          proxy.dropped == ["EndCommit"] and len(ops) == 1 and ops[0].get("reconciled") is True
          and ops[0]["state"] == "committed", json.dumps(results[0])[:400])
    check(f"{label}: nothing was resent", proxy.log == ["BeginCommit", "CreateItems", "EndCommit",
                                                         "GetOperationStatus"], json.dumps(proxy.log))
    check(f"{label}: the item was committed once", sorted(set(item_ids()) - set(ids_before)) == [item["id"]["value"]])
    checker.undo(doc)

    # BeginCommit's reply lost: KiCad opened a commit; the server must drop it
    proxy = DroppingProxy(endpoint, ["BeginCommit"])
    op = f"parity-{label}-lost-begin-{uuid.uuid4()}"
    try:
        results = session(label, [("live_schematic_create_items",
                                   {"document_spec": spec, "items": [text_item("PARITY never", 40)],
                                    "operation_id": op})], proxy.url)
    finally:
        proxy.stop()
    text = results[0]["content"][0]["text"]
    check(f"{label}: lost BeginCommit reply drops the open commit",
          results[0].get("isError") and "it was dropped and nothing was changed" in text, text[:400])
    check(f"{label}: no edit was sent after it", proxy.log == ["BeginCommit", "GetOperationStatus", "EndCommit"],
          json.dumps(proxy.log))
    check(f"{label}: KiCad has it rolled back", checker.operation_status(op)["state"] == "OPS_ROLLED_BACK")
    check(f"{label}: nothing changed and no commit is open",
          item_ids() == ids_before and revision()["open_commits"] == 0)
