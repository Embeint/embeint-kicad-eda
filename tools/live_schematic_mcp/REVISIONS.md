# Document revisions and operation reconciliation

This is the contract for concurrent writers, disconnects and timed-out mutations
in the Embeint IPC API (MCP support plan P1 item 5). It covers the editors derived
from `API_HANDLER_EDITOR`: schematic, PCB, symbol and footprint. Library tables,
project settings (net classes, text variables) and the project manager are outside
it. The [qualification evidence](#qualification) section lists what was run.

## Revision token

Each editor window owns a revision counter for its document
(`EDA_BASE_FRAME::GetDocumentRevision`). It never goes back. It advances on:

- `OnModify()`, which every committed change calls: manual edits, tool actions,
  API commits, title block and page settings edits;
- every push to the undo or redo list: commits, undo and redo;
- clearing the undo history, which happens when a document is loaded or reloaded;
- every request from a client that has an open API commit (its staged changes can
  already be visible to readers), and dropping or rolling back a commit.

It may advance with no visible change; it never stays the same across a change
that goes through these paths.

The API exposes it as an opaque token, `<window epoch>-<counter>`. Compare tokens
for equality only. The epoch is random per editor window, so a token from a closed
and reopened editor never matches. Tokens are returned:

- in `ApiResponseHeader.document_revision` of every reply, success or error, to a
  request that names the editor's document (a top-level `DocumentSpecifier` or
  `ItemHeader.document`), read or write. On a mutation reply it is the revision
  after the mutation; on `EndCommit` it is the revision after the push;
- by `GetDocumentRevision{document}`, which also reports the number of open API
  commits (from any client) and whether the document has unsaved changes.

Headless editors (no window) have no revision: replies carry none,
`GetDocumentRevision` returns `AS_UNIMPLEMENTED` and an expected revision is
refused.

## Expected revision

`ApiRequestHeader.expected_revision` is optional. When set:

1. The request must name a document; an editor of another document type passes it
   on to the editor that owns that type. A request that names no document, or that
   no document editor handles (common, library and project manager commands), is
   refused with `AS_BAD_REQUEST`.
2. If the document is not at that revision, the editor refuses the request with
   **`AS_REVISION_MISMATCH` (100)** before its handler runs, so nothing changes.
   The error message and reply header give the current revision.
3. On `BeginCommit` it makes the commit *strict*. If anything else changes the
   document while the commit is open (another client's commit or staged change,
   a manual edit, undo/redo, the rollback of someone else's commit), the client's
   next step is refused with `AS_REVISION_MISMATCH`. An `EndCommit(COMMIT)` is
   refused the same way, and the editor rolls the commit back. Steps inside a
   commit do not need their own expected revision: the commit carries it.

Without an expected revision, behaviour is unchanged from upstream: commits from
several clients may interleave.

## Operation ids

`ApiRequestHeader.operation_id` is an optional client-chosen name for the mutation
a request starts. Use at most 128 printable ASCII characters with no spaces. Each
id can be used once. When set:

- An editor refuses an id it has already seen with
  **`AS_DUPLICATE_OPERATION` (101)** without running the request. A resend
  under the same id after a timeout therefore cannot apply twice, as long as the
  original is still in the history.
- On `BeginCommit` or any step inside an open commit, the operation is
  `OPS_IN_PROGRESS` until the commit ends: `OPS_COMMITTED` on push, and
  `OPS_ROLLED_BACK` on drop, on a revision conflict or on lease expiry.
- On a request outside a commit, the operation is `OPS_COMMITTED` if the request
  succeeded and `OPS_FAILED` if it returned an error after its handler ran (compare
  `revision_before` and `revision_after`, then inspect the document).
- A request refused by the contract itself (revision mismatch, a step after
  expiry, an invalid document with an expected revision) is `OPS_REJECTED`. The
  document is unchanged.
- Like an expected revision, an operation id on a request that no document editor
  handles is refused with `AS_BAD_REQUEST`. Duplicate and malformed ids are not
  recorded.

`GetOperationStatus{operation_id}` returns the state, request type, document,
revisions before and after, client name, message and `history_capacity`. It is
served by the common handler, so it works while an editor is busy with an API
task and when no editor is open. It never changes anything.

**Bound:** one history per KiCad process, shared by all editors, holding the most
recent **1024** operations (`API_OPERATION_LOG::CAPACITY`). The history is kept in
memory and lost when KiCad exits. An operation that is not in it is `OPS_UNKNOWN`.
That means it has not been received yet (it may still be queued), it was never
delivered, or it was evicted. Clients must treat `OPS_UNKNOWN` as unknown, never
as "not applied".

## Commit leases (abandoned commits)

A client that is killed or disconnected mid-batch leaves its commit open. That
commit's staged changes stay in the document, and undo/redo and revert are
refused while it is open. The editor therefore gives every open commit (explicit
`BeginCommit`, or an implicit one a failed request left behind) a **lease**:

- The lease is `BeginCommit.lease_seconds`, or **60 s** by default, at most 3600 s.
  Each request from the commit's client renews it.
- A one-second timer in the editor checks the leases while any commit is open.
  Every request to the editor also checks them first. An expired commit is
  reverted (`COMMIT::Revert`), its operations become `OPS_ROLLED_BACK` with a
  "lease expired" message, and the revision advances. The rollback is deferred
  while a dialog or interactive tool is active (`CanAcceptApiCommands`).
- The client is then remembered (up to 64 clients). Its later requests to that
  editor are refused with `AS_BAD_REQUEST` until it sends `EndCommit`, so a late
  step cannot land outside the commit as a standalone change. `EndCommit(DROP)`
  then succeeds and confirms the rollback; `EndCommit(COMMIT)` fails and says
  nothing was committed.

## Client behaviour (Python client and MCP tools)

- Every transaction (`LiveSchematic.transaction`, used by all MCP edit tools)
  sends an operation id on `BeginCommit` (`mcp-<uuid>` unless the caller names
  one). Standalone edits (undo/redo, page and title block settings, `execute` of
  a document edit) carry one too.
- `mutation_options(expected_revision, operation_id)` applies an expected
  revision and id to one tool call. Its first mutation uses them as given; each
  later mutation expects the revision the previous one left, under
  `<operation_id>.2`, `.3` and so on.
- Mutations are never resent. On a lost reply (`ReplyTimeout`, still a
  `pynng.Timeout`) the client asks `GetOperationStatus` once:
  - `BeginCommit` lost: if the commit opened it is dropped (no edit was sent);
  - `EndCommit` lost: `OPS_COMMITTED` returns normally (`reconciled: true`).
    `OPS_ROLLED_BACK` raises an error. For `OPS_IN_PROGRESS` the client sends
    `EndCommit(DROP)`, which never applies changes, and asks again. KiCad answers
    requests in order, so a delivered commit settles first; otherwise the drop
    rolls it back. Anything else raises `OutcomeUnknown` with the operation id;
  - a lost step reply drops the commit, as before.
- `RevisionMismatch` and `DuplicateOperation` are distinct exception types.
- MCP tools: `live_document_revision` and `live_operation_status` are new.
  Several edit tools gain optional `expected_revision` and `operation_id`
  arguments: `live_schematic_create_items`, `_update_items`, `_delete_items`,
  `_batch`, `_execute` and `_undo`, and `live_pcb_create_items`,
  `_update_items`, `_delete_items` and `_undo`. Their results gain `revision` and
  `operations`. Existing names and argument shapes are unchanged.

## Limits

- One counter per editor window. In the symbol and footprint editors, changes to
  any open tab advance it.
- In the schematic editor, created items are applied when the commit is pushed;
  modified items are visible while staged. The revision covers both, because
  every request in an open commit advances it.
- A revision check proves that no change went through the paths above. A plugin
  or a future editor path that changes the document without `OnModify`, the undo
  lists or an API commit would not advance it.
- Commits without an expected revision still interleave as upstream allows. Use
  expected revisions when more than one writer may be active.
- After a lost `BeginCommit` reply with `OPS_UNKNOWN`, a late-arriving
  `BeginCommit` opens an empty commit. The lease rolls it back; until then that
  client's next `BeginCommit` is refused as "already in progress".
- Operation outcomes are not persisted across a KiCad restart; neither are open
  commits.
- Cross-editor operations (schematic plus board) are separate operations with
  separate revisions; there is no coordinated multi-document commit.

## Qualification

`e2e_revisions.py` runs against the live-demo test project (`run_tests.sh --live`).
It checks that reads do not advance the revision and that replies carry it. It
checks the contract refusals; that a guarded commit is reconcilable as committed;
and that a duplicate id is refused. It checks that another client's commit and
the editor's undo/redo make an expected revision stale. It checks that a staged
update is visible and that a drop restores it. It checks that a strict batch fails
at its next step, and at `EndCommit`, when another client commits. It checks that
a lost `EndCommit` reply reconciles to committed, an undelivered `EndCommit` to
rolled back, and a lost `BeginCommit` reply leaves nothing open. It checks that a
step after lease expiry is refused and that the client recovers. Finally, a client
process killed mid-batch with a visible staged edit, with undo blocked, has its
commit rolled back by the editor's timer within the lease. The edit is reverted
and undo works again. When the PCB editor is open it also checks that the board
refuses a stale revision. Unit tests in `test_client.py` cover the client side
without KiCad.
