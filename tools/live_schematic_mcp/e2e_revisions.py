# SPDX-License-Identifier: GPL-3.0-or-later
"""Live qualification of document revisions, operation reconciliation and abandoned commits.

Against a running development KiCad with the live-demo test project (e2e_guard), checks that:
reads leave the revision alone; every committed change (API commits, another client's edits,
the editor's undo/redo, staged and dropped changes) advances it; a stale expected revision is
refused before anything changes; a commit opened with an expected revision is rolled back when
another client changes the document; duplicate operation ids are refused; lost replies are
reconciled with GetOperationStatus instead of being resent; and a client killed mid-batch has its
open commit rolled back when its lease expires.  Never saves; deletes the markers it creates.

    python e2e_revisions.py [output_dir]
"""

import json
import os
from hashlib import sha256
from pathlib import Path
import subprocess
import sys
import time
import uuid

import pynng

from client import (DuplicateOperation, LiveSchematic, ReplyTimeout, RevisionMismatch, editor, envelope, kiid,
                    point, sch, types, as_dict)
from commands import message_class
from e2e_guard import test_board, test_document

HERE = Path(__file__).resolve().parent
LEASE_S = 3
RESULTS = []


def check(name, condition, detail=""):
    RESULTS.append({"check": name, "passed": bool(condition), "detail": detail})
    print(("PASS " if condition else "FAIL ") + name + (f": {detail}" if detail else ""), flush=True)
    assert condition, f"{name}: {detail}"


def item_ids(client, doc):
    ids = set()
    for packed in client.items(doc).items:
        message = message_class(packed.TypeName())()
        packed.Unpack(message)
        ids.add(message.id.value)
    return ids


class Markers:
    """Text items on a free strip of the test sheet, one per scenario, removed at the end."""

    def __init__(self):
        self.created = []

    def new(self, label):
        x = 20 + 9 * (len(self.created) % 28)
        item = sch.SchematicText(id=kiid(), text=types.Text(text=f"REV {label}", position=point(x, 185)))
        self.created.append(item.id.value)
        return item


def create_request(doc, item):
    request = editor.CreateItems(header=types.ItemHeader(document=doc))
    request.items.add().Pack(item)
    return request


def update_request(doc, item):
    request = editor.UpdateItems(header=types.ItemHeader(document=doc))
    request.items.add().Pack(item)
    return request


def live_text(client, doc, item_id):
    """A marker as the editor holds it now (staged updates included), or None."""
    for packed in client.items(doc).items:
        text = sch.SchematicText()
        if packed.Unpack(text) and text.id.value == item_id:
            return text
    return None


def text_of(client, doc, item_id):
    item = live_text(client, doc, item_id)
    return item.text.text if item else None


def edited(client, doc, item_id, label):
    """The complete live marker with new text, for UpdateItems."""
    item = live_text(client, doc, item_id)
    item.text.text = label
    return item


def deliver_and_drop_reply(client, command):
    """Deliver a request to KiCad and give up on its reply, like a client whose timeout fired."""
    request = envelope.ApiRequest()
    request.header.client_name = client.client_name
    request.header.kicad_token = client.token
    if client._pending_header:
        request.header.expected_revision, request.header.operation_id = client._pending_header
    request.message.Pack(command)
    with pynng.Req0(send_timeout=5000, recv_timeout=1, resend_time=-1) as conn:
        conn.dial(client.socket_url, block=True)
        conn.send(request.SerializeToString())
        try:
            conn.recv()
            return False
        except pynng.Timeout:
            return True


class LostReply(LiveSchematic):
    """EndCommit(COMMIT) reaches KiCad but its reply is lost."""

    def send(self, command, response_type):
        if isinstance(command, editor.EndCommit) and command.action == editor.CMA_COMMIT:
            with self.lock:
                self.reply_lost = deliver_and_drop_reply(self, command)
            raise ReplyTimeout("simulated: EndCommit delivered, reply lost")
        return super().send(command, response_type)


class LostRequest(LiveSchematic):
    """EndCommit(COMMIT) never reaches KiCad."""

    def send(self, command, response_type):
        if isinstance(command, editor.EndCommit) and command.action == editor.CMA_COMMIT:
            raise ReplyTimeout("simulated: EndCommit lost before delivery")
        return super().send(command, response_type)


class LostBeginReply(LiveSchematic):
    """BeginCommit reaches KiCad but its reply is lost."""

    def send(self, command, response_type):
        if isinstance(command, editor.BeginCommit):
            with self.lock:
                self.reply_lost = deliver_and_drop_reply(self, command)
            raise ReplyTimeout("simulated: BeginCommit delivered, reply lost")
        return super().send(command, response_type)


def connect(cls=LiveSchematic, like=None):
    client = cls()
    if like is not None:
        client.socket_url, client.token, client._socket_selected = like.socket_url, like.token, True
    return client


# A client that opens a commit, stages an edit of an existing marker and then hangs until killed
CHILD = r"""
import json, sys, time
sys.path.insert(0, sys.argv[1])
from google.protobuf import json_format
from client import LiveSchematic, editor, sch, types
client = LiveSchematic()
doc = json_format.ParseDict(json.loads(sys.argv[2]), types.DocumentSpecifier())
item = json_format.ParseDict(json.loads(sys.argv[5]), sch.SchematicText())
header = types.ItemHeader(document=doc)
client._send_with(editor.BeginCommit(header=header, lease_seconds=int(sys.argv[4])),
                  editor.BeginCommitResponse, None, sys.argv[3])
item.text.text = "REV killed edit"
request = editor.UpdateItems(header=header)
request.items.add().Pack(item)
client.send(request, editor.UpdateItemsResponse)
print(json.dumps({"client": client.client_name}), flush=True)
time.sleep(600)
"""


def main():
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "live-revisions-output")
    out.mkdir(parents=True, exist_ok=True)
    a = LiveSchematic()
    listing = a.documents()
    print("KiCad", listing["version"].get("full_version", listing["version"]))
    root = test_document(a)
    b = connect(like=a)
    project_dir = Path(root.project.path)
    on_disk = {p: sha256(p.read_bytes()).hexdigest() for p in project_dir.glob("*.kicad_sch")}
    markers = Markers()

    def rev(client=a):
        return client.document_revision(root)["revision"]

    try:
        run(a, b, root, markers, rev, out)
    finally:
        leftovers = [i for i in markers.created if i in item_ids(b, root)]
        if leftovers:
            b.delete(root, leftovers)
        (out / "e2e_revisions.json").write_text(json.dumps(RESULTS, indent=2))

    unchanged = {p: sha256(p.read_bytes()).hexdigest() for p in project_dir.glob("*.kicad_sch")}
    check("nothing saved", unchanged == on_disk)
    check("markers removed", not set(markers.created) & item_ids(b, root))
    print(f"OK: {len(RESULTS)} revision/reconciliation checks passed")


def run(a, b, root, markers, rev, out):
    # Reads never advance the revision, and replies carry it
    r0 = rev()
    a.items(root)
    check("item reads carry the revision", a.last_revision == r0, r0)
    a.hierarchy(root)
    a.page_layout(root)
    a.netlist(root)
    (out / "revisions-sheet.png").write_bytes(a.render(root, dpi=72))
    check("reads leave the revision unchanged", rev() == r0, f"{r0} -> {rev()}")

    # Contract refusals that need no document change
    for name, command, reply, header in (
            ("expected revision without a document is refused",
             editor.GetOpenDocuments(type=types.DOCTYPE_SCHEMATIC), editor.GetOpenDocumentsResponse, ("x-1", "")),
            ("operation id outside a document editor is refused",
             editor.GetOperationStatus(operation_id="x"), editor.OperationStatusResponse,
             ("", f"op-{uuid.uuid4()}")),
            ("malformed operation id is refused",
             editor.GetDocumentRevision(document=root), editor.DocumentRevisionResponse, ("", "has space"))):
        try:
            a._send_with(command, reply, *header)
            check(name, False, "accepted")
        except RuntimeError as error:
            check(name, "AS_BAD_REQUEST" in str(error), str(error))

    # Our own guarded commit advances the revision and is recorded as committed
    op_a = f"e2e-{uuid.uuid4()}"
    marker_a = markers.new("A")
    with a.mutation_options(r0, op_a) as done:
        a.mutate(root, [marker_a])
    r_a = done["operations"][0]["revision"]
    status = a.operation_status(op_a)
    check("guarded commit advances the revision", r_a and r_a != r0 and rev() == r_a, f"{r0} -> {r_a}")
    check("committed operation is reconcilable", status["state"] == "OPS_COMMITTED"
          and status["revision_before"] == r0 and status["revision_after"] == r_a, json.dumps(status))

    # A reused operation id is refused without running
    before = item_ids(a, root)
    try:
        with a.mutation_options(None, op_a):
            a.mutate(root, [markers.new("dup")])
        check("duplicate operation id is refused", False, "accepted")
    except DuplicateOperation as error:
        check("duplicate operation id is refused", item_ids(a, root) == before and rev() == r_a, str(error))

    # Another client's edit makes our expected revision stale
    b.mutate(root, [markers.new("B")])
    r_b = rev()
    op_c = f"e2e-{uuid.uuid4()}"
    marker_c = markers.new("C")
    try:
        with a.mutation_options(r_a, op_c):
            a.mutate(root, [marker_c])
        check("stale revision after another client's edit is refused", False, "accepted")
    except RevisionMismatch as error:
        check("stale revision after another client's edit is refused",
              marker_c.id.value not in item_ids(a, root) and rev() == r_b, str(error))
    check("refused operation is recorded as rejected", a.operation_status(op_c)["state"] == "OPS_REJECTED")

    # The editor's own undo/redo actions (the tools behind Ctrl+Z/Ctrl+Y) advance the revision
    b_id = markers.created[-2]
    undo = b.undo(root)
    r_u = rev()
    check("editor undo advances the revision", undo["performed"] and r_u != r_b
          and b_id not in item_ids(a, root), f"{r_b} -> {r_u}")
    try:
        with a.mutation_options(r_b):
            a.mutate(root, [markers.new("D")])
        check("stale revision after an undo is refused", False, "accepted")
    except RevisionMismatch as error:
        check("stale revision after an undo is refused", True, str(error))
    b.undo(root, redo=True)
    check("editor redo advances the revision", rev() not in (r_u, r_b) and b_id in item_ids(a, root))

    # Changes staged in another client's open commit are visible, so they advance the revision;
    # dropping them advances it again
    r_s = rev()
    try:
        with a.transaction(root, "staged then dropped"):
            a.send(update_request(root, edited(a, root, marker_a.id.value, "REV staged")),
                   editor.UpdateItemsResponse)
            seen = b.document_revision(root)
            check("staged change is visible and advances the revision", seen["revision"] != r_s
                  and seen["open_commits"] == 1 and text_of(b, root, marker_a.id.value) == "REV staged",
                  json.dumps(seen))
            raise RuntimeError("drop on purpose")
    except RuntimeError as error:
        assert "drop on purpose" in str(error)
    r_d = rev()
    check("dropped commit advances the revision and restores the item",
          r_d not in (r_s, seen["revision"]) and text_of(a, root, marker_a.id.value) == "REV A")

    # A commit opened with an expected revision fails its next step when another client commits
    op_e = f"e2e-{uuid.uuid4()}"
    staged, interloper, late = markers.new("E1"), markers.new("E2"), markers.new("E3")
    try:
        with a.mutation_options(r_d, op_e):
            with a.transaction(root, "strict batch"):
                a.send(create_request(root, staged), editor.CreateItemsResponse)
                b.mutate(root, [interloper])
                a.send(create_request(root, late), editor.CreateItemsResponse)
        check("concurrent edit fails the guarded batch", False, "committed")
    except RevisionMismatch as error:
        ids = item_ids(a, root)
        check("concurrent edit fails the guarded batch", staged.id.value not in ids and late.id.value not in ids
              and interloper.id.value in ids, str(error))
    check("conflicting batch is rolled back", a.operation_status(op_e)["state"] == "OPS_ROLLED_BACK")

    # ... and is rolled back at EndCommit when the other client commits after its last step
    op_f = f"e2e-{uuid.uuid4()}"
    staged, interloper = markers.new("F1"), markers.new("F2")
    try:
        with a.mutation_options(rev(), op_f):
            with a.transaction(root, "strict batch"):
                a.send(create_request(root, staged), editor.CreateItemsResponse)
                b.mutate(root, [interloper])
        check("concurrent edit fails the guarded commit", False, "committed")
    except RevisionMismatch as error:
        ids = item_ids(a, root)
        status = a.operation_status(op_f)
        check("concurrent edit fails the guarded commit", staged.id.value not in ids
              and interloper.id.value in ids and status["state"] == "OPS_ROLLED_BACK", str(error))

    # Reconciliation: EndCommit delivered but its reply lost -> committed, not resent
    lost = connect(LostReply, like=a)
    marker_r = markers.new("lost reply")
    lost.mutate(root, [marker_r])
    check("lost EndCommit reply is reconciled as committed", lost.last_operation["reconciled"]
          and lost.last_operation["state"] == "committed" and marker_r.id.value in item_ids(a, root)
          and lost.last_operation["revision"] == rev(),
          f"reply lost: {lost.reply_lost}; {json.dumps(lost.last_operation)}")

    # EndCommit never delivered -> the client settles the open commit with a drop: rolled back
    gone = connect(LostRequest, like=a)
    marker_g = markers.new("lost request")
    try:
        gone.mutate(root, [marker_g])
        check("undelivered EndCommit is reconciled as rolled back", False, "committed")
    except RuntimeError as error:
        status = a.operation_status(gone.last_operation["operation_id"])
        check("undelivered EndCommit is reconciled as rolled back", status["state"] == "OPS_ROLLED_BACK"
              and marker_g.id.value not in item_ids(a, root)
              and b.document_revision(root)["open_commits"] == 0, str(error))

    # BeginCommit delivered but its reply lost -> the opened commit is dropped, no edit is sent
    begin = connect(LostBeginReply, like=a)
    marker_h = markers.new("lost begin")
    try:
        begin.mutate(root, [marker_h])
        check("lost BeginCommit reply leaves nothing open", False, "committed")
    except RuntimeError as error:
        check("lost BeginCommit reply leaves nothing open", "nothing was changed" in str(error)
              and marker_h.id.value not in item_ids(a, root) and b.document_revision(root)["open_commits"] == 0,
              str(error))

    # A client that waits past its lease has its commit rolled back; its next step is refused and
    # dropping the commit confirms the rollback
    slow = connect(like=a)
    slow.lease_seconds = LEASE_S
    m1, m2 = markers.new("slow 1"), markers.new("slow 2")
    try:
        with slow.transaction(root, "slow batch"):
            slow.send(create_request(root, m1), editor.CreateItemsResponse)
            time.sleep(LEASE_S + 2.5)
            slow.send(create_request(root, m2), editor.CreateItemsResponse)
        check("step after lease expiry is refused", False, "committed")
    except RuntimeError as error:
        ids = item_ids(a, root)
        check("step after lease expiry is refused", "rolled back" in str(error) and "unconfirmed" not in str(error)
              and m1.id.value not in ids and m2.id.value not in ids, str(error))
    slow.mutate(root, [markers.new("slow again")])
    check("client works again after confirming the rollback", slow.last_operation["state"] == "committed")

    # A client killed mid-batch: its staged change is visible, undo is blocked, then the lease
    # timer rolls the commit back without any further request
    op_k = f"e2e-{uuid.uuid4()}"
    r_k = rev(b)
    env = {**os.environ, "KICAD_API_SOCKET": a.socket_url, "KICAD_API_TOKEN": a.token}
    child = subprocess.Popen([sys.executable, "-c", CHILD, str(HERE), json.dumps(as_dict(root)), op_k,
                              str(LEASE_S), json.dumps(as_dict(live_text(b, root, marker_a.id.value)))],
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env)
    try:
        line = child.stdout.readline()
        check("killed client staged a change", line.strip() != "", child.stderr.read() if not line else line.strip())
        state = b.document_revision(root)
        check("abandoned commit is open and its edit visible", state["open_commits"] == 1
              and state["revision"] != r_k and text_of(b, root, marker_a.id.value) == "REV killed edit",
              json.dumps(state))
        try:
            b.undo(root)
            check("undo is refused while the commit is open", False, "undo ran")
        except RuntimeError as error:
            check("undo is refused while the commit is open", "commit is open" in str(error), str(error))
    finally:
        child.kill()
        child.wait()
    killed_at = time.monotonic()
    time.sleep(LEASE_S + 2.5)
    # GetOperationStatus runs in the common handler, so this proves the editor's timer did it
    status = b.operation_status(op_k)
    check("lease timer rolls back the killed client's commit", status["state"] == "OPS_ROLLED_BACK"
          and "lease" in status.get("message", ""), f"{time.monotonic() - killed_at:.1f} s; {json.dumps(status)}")
    state = b.document_revision(root)
    check("document is consistent after the killed client", state["open_commits"] == 0
          and text_of(b, root, marker_a.id.value) == "REV A", json.dumps(state))
    before = item_ids(b, root)
    undone = b.undo(root)
    redone = b.undo(root, redo=True)
    check("undo works again after cleanup", undone["performed"] and redone["performed"]
          and item_ids(b, root) == before)

    # The board editor follows the same contract (project manager mode only)
    try:
        board = test_board(a)
    except (RuntimeError, SystemExit) as error:
        RESULTS.append({"check": "board revision contract", "passed": None, "detail": f"skipped: {error}"})
        print("SKIP board revision contract:", error)
        return
    board_rev = a.document_revision(board)["revision"]
    try:
        with a.mutation_options("stale-0"):
            a.undo(board)
        check("board refuses a stale revision", False, "undo ran")
    except RevisionMismatch as error:
        check("board refuses a stale revision", a.document_revision(board)["revision"] == board_rev, str(error))


if __name__ == "__main__":
    main()
