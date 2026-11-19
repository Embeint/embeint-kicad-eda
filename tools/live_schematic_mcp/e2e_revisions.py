# SPDX-License-Identifier: GPL-3.0-or-later
"""Live qualification of document revisions, operation reconciliation and abandoned commits.

Against a running development KiCad with the live-demo test project (e2e_guard), checks that:
reads leave the revision alone; every committed change (API commits, another client's edits,
the editor's undo/redo, staged and dropped changes) advances it; a stale expected revision is
refused before anything changes; a commit opened with an expected revision is rolled back when
another client changes the document; duplicate operation ids are refused; lost replies are
reconciled with GetOperationStatus instead of being resent; a client killed mid-batch has its
open commit rolled back when its lease expires; failed requests are recorded as failed or rejected.
With the PCB editor open it also checks that leases are opt-in (a client that asks for none keeps
its commit past the old 60 s default while a short opt-in lease expires) and that no lease
expires while an API task (a zone refill) runs.  Never saves; deletes the markers and zones it
creates.

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

from board_client import board_tasks, bt, make_zone, tasks
from client import (DuplicateOperation, LiveSchematic, ReplyTimeout, RevisionMismatch, editor, envelope, kiid,
                    point, sch, types, as_dict)
from commands import message_class
from e2e_guard import test_board, test_document

HERE = Path(__file__).resolve().parent
LEASE_S = 3
# The lease every commit had before leases became opt-in; a client that asks for none must outlive it
OLD_DEFAULT_LEASE_S = 60
# Lease held across the zone refill task, and the shortest task that proves it was not counted
TASK_LEASE_S = 2
TASK_MIN_S = 4.0
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


def drop(client, doc, commit_id=None):
    """EndCommit(DROP) for a commit opened with a raw BeginCommit."""
    request = editor.EndCommit(header=types.ItemHeader(document=doc), action=editor.CMA_DROP)
    if commit_id is not None:
        request.id.CopyFrom(commit_id)
    return client.send(request, editor.EndCommitResponse)


class BoardCommits:
    """Two empty commits on the test board, opened early so the slow lease checks overlap the rest.

    `upstream` asks for no lease and sends no operation id or expected revision, like an upstream
    kipy plugin: it must keep its commit as long as it likes.  `leased` opts in with LEASE_S.
    """

    def __init__(self, a, board):
        self.board = board
        header = types.ItemHeader(document=board)
        self.upstream, self.leased = connect(like=a), connect(like=a)
        self.upstream_id = self.upstream._send_with(editor.BeginCommit(header=header),
                                                    editor.BeginCommitResponse).id
        self.upstream_open = True
        self.leased_op = f"e2e-{uuid.uuid4()}"
        self.leased._send_with(editor.BeginCommit(header=header, lease_seconds=LEASE_S),
                               editor.BeginCommitResponse, None, self.leased_op)
        self.opened_at = time.monotonic()

    def close(self):
        if self.upstream_open:
            self.upstream_open = False
            drop(self.upstream, self.board, self.upstream_id)


def hatched_zone(points, step_mm):
    """A no-net zone with a fine hatch: slow to fill, so its refill task runs for seconds."""
    zone = make_zone(points, ["F.Cu"], "", name="REV lease task", min_width=min(0.25, step_mm))
    settings = zone.copper_settings
    settings.fill_mode = bt.ZFM_HATCHED
    settings.hatch_settings.thickness.value_nm = int(step_mm * 1e6)
    settings.hatch_settings.gap.value_nm = int(step_mm * 1e6)
    settings.hatch_settings.border_mode = bt.ZHFBM_USE_MIN_ZONE_THICKNESS
    return zone


def refill(client, board, zone_id, during=None):
    """Run a zone refill task to its end; returns (seconds, final TaskStatusResponse)."""
    started = client.send(board_tasks.StartRefillZonesTask(board=board, zones=[types.KIID(value=zone_id)]),
                          tasks.StartTaskResponse)
    began = time.monotonic()
    while True:
        if during:
            during()
        status = client.send(tasks.GetTaskStatus(token=started.token), tasks.TaskStatusResponse)
        if status.state not in (tasks.TS_RUNNING, tasks.TS_UNKNOWN):
            return time.monotonic() - began, status
        if time.monotonic() - began > 300:
            raise TimeoutError("zone refill task did not finish in 300 s")
        time.sleep(0.1)


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
    # The board's lease checks need more than the old 60 s default lease, so their commits are
    # opened first and checked after the schematic checks
    try:
        board = test_board(a)
    except (RuntimeError, SystemExit) as error:
        board, skipped = None, str(error)
    commits = BoardCommits(a, board) if board is not None else None
    try:
        run_schematic(a, b, root, markers, rev, out)
        if board is None:
            RESULTS.append({"check": "board revision and lease contract", "passed": None,
                            "detail": f"skipped: {skipped}"})
            print("SKIP board revision and lease contract:", skipped)
            return
        run_board(a, b, board, commits)
    finally:
        if commits is not None:
            try:
                commits.close()
            except Exception as error:  # noqa: BLE001 - do not hide the original failure
                print("cleanup: could not drop the upstream-style board commit:", error)


def run_schematic(a, b, root, markers, rev, out):
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

    # Requests that fail inside an open commit are not bound to it: a refused BeginCommit is
    # rejected and a failed step is failed, whatever the commit's outcome
    failing = connect(like=a)
    header = types.ItemHeader(document=root)
    op_open, op_again, op_undo = (f"e2e-{uuid.uuid4()}" for _ in range(3))
    opened = failing._send_with(editor.BeginCommit(header=header), editor.BeginCommitResponse, None, op_open)
    try:
        failing._send_with(editor.BeginCommit(header=header), editor.BeginCommitResponse, None, op_again)
        check("second BeginCommit is refused", False, "accepted")
    except RuntimeError as error:
        check("second BeginCommit is refused", "already has a commit" in str(error), str(error))
    try:
        failing._send_with(editor.UndoLastChange(document=root), editor.UndoRedoResponse, None, op_undo)
        check("undo inside the client's own commit fails", False, "undo ran")
    except RuntimeError as error:
        check("undo inside the client's own commit fails", "commit is open" in str(error), str(error))
    drop(failing, root, opened.id)
    states = {op: a.operation_status(op)["state"] for op in (op_open, op_again, op_undo)}
    check("refused BeginCommit is recorded as rejected", states[op_again] == "OPS_REJECTED", json.dumps(states))
    check("failed step is recorded as failed, not bound to the commit", states[op_undo] == "OPS_FAILED"
          and states[op_open] == "OPS_ROLLED_BACK", json.dumps(states))

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


def run_board(a, b, board, commits):
    """The board editor (project manager mode only): the same contract, opt-in leases, tasks."""
    board_rev = a.document_revision(board)["revision"]
    try:
        with a.mutation_options("stale-0"):
            a.undo(board)
        check("board refuses a stale revision", False, "undo ran")
    except RevisionMismatch as error:
        check("board refuses a stale revision", a.document_revision(board)["revision"] == board_rev, str(error))

    zones = []
    try:
        check_no_expiry_during_task(a, b, board, zones)
    finally:
        if zones:
            b.delete(board, zones)

    # Leases are opt-in: the short opt-in lease has long expired, the commit that asked for none
    # is still open after the old default lease
    wait = OLD_DEFAULT_LEASE_S + 2 - (time.monotonic() - commits.opened_at)
    if wait > 0:
        print(f"waiting {wait:.0f} s for the old default lease to pass", flush=True)
        time.sleep(wait)
    held = time.monotonic() - commits.opened_at
    status = b.operation_status(commits.leased_op)
    check("an opt-in lease expires", status["state"] == "OPS_ROLLED_BACK" and "lease" in status.get("message", ""),
          json.dumps(status))
    state = b.document_revision(board)
    check("a commit without a lease stays open past the old 60 s default", state["open_commits"] == 1,
          f"after {held:.0f} s: {json.dumps(state)}")
    try:
        commits.close()
        check("the client that asked for no lease ends its commit normally", True, f"after {held:.0f} s")
    except RuntimeError as error:
        check("the client that asked for no lease ends its commit normally", False, str(error))
    drop(commits.leased, board)
    check("expired opt-in client confirms its rollback", b.document_revision(board)["open_commits"] == 0)


def check_no_expiry_during_task(a, b, board, zones):
    """A lease shorter than a zone refill task survives it: the lease clock stops during tasks."""
    box = a.board_box(board, "edges")
    x1, y1, x2, y2 = (box[0] + 1, box[1] + 1, box[2] - 1, box[3] - 1) if box else (20, 20, 120, 120)
    points = [(x1, y1), (x2, y1), (x2, y2), (x1, y2)]

    # Find a hatch fine enough that the refill outlasts the lease with a margin
    step, duration, zone_id = 0.5, 0.0, None
    for _ in range(6):
        zone = hatched_zone(points, step)
        b.mutate(board, [zone])
        zone_id = zone.id.value
        zones.append(zone_id)
        duration, status = refill(b, board, zone_id)
        if status.state != tasks.TS_COMPLETED:
            raise RuntimeError(f"zone refill task {tasks.TaskState.Name(status.state)}: {status.error_message}")
        if duration >= TASK_MIN_S or step <= 0.05:
            break
        b.delete(board, [zones.pop()])
        step = max(0.05, step * max(0.3, min(0.8, (duration / (TASK_MIN_S * 1.5)) ** 0.5)))
    if duration < TASK_MIN_S:
        RESULTS.append({"check": "no lease expiry while a task runs", "passed": None,
                        "detail": f"skipped: the slowest refill took {duration:.1f} s (hatch {step} mm)"})
        print(f"SKIP no lease expiry while a task runs: refill took {duration:.1f} s")
        return

    # Requests allowed during the task (GetDocumentRevision, GetTaskStatus) keep arriving; none
    # of them may roll the commit back, and the task's time is not counted against the client
    held = connect(like=a)
    op = f"e2e-{uuid.uuid4()}"
    opened = held._send_with(editor.BeginCommit(header=types.ItemHeader(document=board), lease_seconds=TASK_LEASE_S),
                             editor.BeginCommitResponse, None, op)
    duration, _ = refill(b, board, zone_id, during=lambda: b.document_revision(board))
    status = b.operation_status(op)
    check("no lease expiry while a task runs", status["state"] == "OPS_IN_PROGRESS",
          f"{TASK_LEASE_S} s lease, {duration:.1f} s task; {json.dumps(status)}")
    drop(held, board, opened.id)
    status = b.operation_status(op)
    check("the commit held across the task ends normally", status["state"] == "OPS_ROLLED_BACK"
          and "lease" not in status.get("message", ""), json.dumps(status))


if __name__ == "__main__":
    main()
