# SPDX-License-Identifier: GPL-3.0-or-later
"""Live schematic IPC. No schematic-file reads, writes, or SWIG fallback."""

from contextlib import contextmanager
import math
import os
from pathlib import Path
import sys
import tempfile
import threading
import uuid

import pynng
from google.protobuf import json_format

from board_client import JOB_OK, BoardTools, board_document  # noqa: F401
from board_design import BoardDesignTools
from fab_package import FabPackageTools
from footprint_client import FootprintTools, footprint_document, library_kind  # noqa: F401
from instance_discovery import (DiscoveryError, default_socket_url, local_socket_path,
                                select_instance, socket_reachability)

sys.path.insert(0, str(Path(__file__).resolve().parent / "_generated"))
try:
    from common import envelope_pb2 as envelope
    from common import tasks_pb2 as tasks
    from common.commands import base_commands_pb2 as base
    from common.commands import cross_probe_commands_pb2 as cross_probe
    from common.commands import editor_commands_pb2 as editor
    from common.commands import library_commands_pb2 as library
    from common.commands import project_commands_pb2 as project
    from common.commands import variant_commands_pb2  # noqa: F401
    from common.types import base_types_pb2 as types
    from common.types import enums_pb2 as enums
    from common.types import jobs_pb2 as jobs
    from common.types import library_types_pb2 as lib_types
    from common.types import project_settings_pb2  # noqa: F401
    from schematic import schematic_commands_pb2 as commands
    from schematic import schematic_jobs_pb2 as sch_jobs
    from schematic import schematic_rules_pb2 as rules
    from schematic import schematic_tasks_pb2 as sch_tasks
    from schematic import schematic_types_pb2 as sch
except ImportError as exc:
    raise RuntimeError("Run generate_proto.py with this interpreter before starting the MCP") from exc


def as_dict(message):
    return json_format.MessageToDict(message, preserving_proto_field_name=True)


class RevisionMismatch(RuntimeError):
    """KiCad refused a request because the document is not at the expected revision (or changed
    while a commit opened with one was open).  Nothing was changed; read the document again."""


class DuplicateOperation(RuntimeError):
    """KiCad has already received this operation id, so it did not run the request again."""


class ReplyTimeout(pynng.Timeout, TimeoutError):
    """No reply arrived in time.  KiCad may still apply the request: never resend it blindly.

    Still a pynng.Timeout, so callers that wait out a slow read keep working."""

    def __init__(self, message, errno=5):
        super().__init__(message, errno)
        self.message = message

    def __str__(self):
        return self.message


class OutcomeUnknown(RuntimeError):
    """A mutation's outcome could not be confirmed.  Do not resend it; query its operation id."""

    def __init__(self, message, operation_id, status=None):
        super().__init__(message)
        self.operation_id = operation_id
        self.status = status


# Errors whose status code means the request did not run; see api/proto/common/envelope.proto
STATUS_ERRORS = {"AS_REVISION_MISMATCH": RevisionMismatch, "AS_DUPLICATE_OPERATION": DuplicateOperation}

# Commit lease (seconds) the MCP asks for on BeginCommit.  Leases are opt-in: the editor gives a
# commit none unless its client asks, so a killed MCP's open commit would otherwise stay open.
DEFAULT_LEASE_S = 60

# Reply timeout (ms) for each request, unless KICAD_API_TIMEOUT_MS sets another (the native Rust
# MCP reads the same variable; the parity fixtures shorten it to exercise lost replies quickly).
DEFAULT_TIMEOUT_MS = 15000


def api_timeout_ms():
    """The reply timeout: KICAD_API_TIMEOUT_MS when set (ASCII digits, 1 to 600000 ms; anything else
    raises ValueError), else DEFAULT_TIMEOUT_MS."""
    text = os.environ.get("KICAD_API_TIMEOUT_MS", "").strip()
    if not text:
        return DEFAULT_TIMEOUT_MS
    if not (text.isascii() and text.isdigit()) or not 1 <= int(text) <= 600_000:
        raise ValueError("KICAD_API_TIMEOUT_MS must be a whole number of milliseconds from 1 to 600000")
    return int(text)


def new_operation_id():
    return f"mcp-{uuid.uuid4()}"


def operation_state(status):
    """OperationStatusResponse as a dict that always names its state."""
    result = as_dict(status)
    result["state"] = editor.OperationState.Name(status.state)
    return result


def document(value):
    result = json_format.ParseDict(value, types.DocumentSpecifier())
    if result.type != types.DOCTYPE_SCHEMATIC:
        raise ValueError("Use a schematic document returned by live_schematic_documents")
    if not result.project.name or not result.sheet_path.path:
        raise ValueError("An explicit project and UUID sheet_path are required")
    return result


def point(x, y):
    if not all(math.isfinite(v) and abs(v) <= 10000 for v in (x, y)):
        raise ValueError("Coordinates must be finite millimeters within +/-10000")
    return types.Vector2(x_nm=round(x * 1_000_000), y_nm=round(y * 1_000_000))


def mm(vector):
    return [vector.x_nm / 1e6, vector.y_nm / 1e6]


def kiid(value=None):
    return types.KIID(value=str(uuid.UUID(value)) if value else str(uuid.uuid4()))


def lib_identifier(lib_id):
    library, separator, entry = lib_id.partition(":")
    if not separator or not library or not entry:
        raise ValueError("lib_id must be Library:Symbol")
    return types.LibraryIdentifier(library_nickname=library, entry_name=entry)


def sheet_key(path):
    return tuple(k.value for k in path.path)


def symbol_document(value):
    """A DOCTYPE_SYMBOL document as returned by new_symbol/edit_symbol."""
    result = json_format.ParseDict(value, types.DocumentSpecifier())
    if result.type != types.DOCTYPE_SYMBOL or not result.lib_id.entry_name:
        raise ValueError("Use the symbol document returned by live_symbol_new or live_symbol_edit")
    return result


def lib_id_text(identifier):
    return f"{identifier.library_nickname}:{identifier.entry_name}"


def item_id(packed):
    """The UUID of a packed library item, or None."""
    for message in (sch.SchematicPin(), sch.SchematicGraphicShape(), sch.SchematicText()):
        if packed.Unpack(message):
            return message.id.value or None
    return None


MANUFACTURER_FIELD, MPN_FIELD = "Manufacturer", "MPN"

SCOPES = {"global": lib_types.LTS_GLOBAL, "project": lib_types.LTS_PROJECT, "both": lib_types.LTS_BOTH}
SCOPE_NAMES = {lib_types.LTS_GLOBAL: "global", lib_types.LTS_PROJECT: "project"}


def check_library_status(status, action):
    if status.code != lib_types.LibraryCommandStatus.LCS_OK:
        code = lib_types.LibraryCommandStatus.Code.Name(status.code)
        raise RuntimeError(f"Could not {action}: {code} {status.error_message}".strip())
    return {"success": True}


def summarize_symbol(symbol):
    """The parts of a library symbol an agent needs, without its graphics."""
    pins, user_fields = [], {}
    for child in symbol.items:
        pin, field = sch.SchematicPin(), sch.SchematicField()
        if child.item.Unpack(field) and field.name:
            user_fields[field.name] = field.text.text
            continue
        if child.item.Unpack(pin):
            entry = {"number": pin.number, "name": pin.name, "type": types.ElectricalPinType.Name(pin.electrical_type),
                     "position_mm": mm(pin.position), "orientation": sch.SchematicPinOrientation.Name(pin.orientation)}
            if not pin.visible:
                entry["hidden"] = True
            if child.HasField("unit"):
                entry["unit"] = child.unit.unit
            pins.append(entry)
    fields = {name: getattr(symbol, f"{name}_field").text.text
              for name in ("reference", "value", "footprint", "datasheet", "description")}
    placement = {}
    for name in ("reference", "value"):
        text = getattr(symbol, f"{name}_field").text
        placement[name] = {"position_mm": mm(text.position),
                           "align": as_dict(text).get("attributes", {}).get("horizontal_alignment")}
    return {"lib_id": lib_id_text(symbol.id), **fields, "fields": user_fields, "field_placement": placement,
            "keywords": symbol.keywords,
            "footprint_filters": list(symbol.footprint_filters), "units": symbol.unit_count or 1,
            "type": sch.SchematicSymbolType.Name(symbol.type),
            "pins": sorted(pins, key=lambda p: (len(p["number"]), p["number"]))}


# A pin's drawn orientation is the direction from its connection point toward the body, so wires
# and labels leave it the opposite way.  Label spin names the side the text extends to.
OUTWARD = {sch.SPO_RIGHT: (-1, 0), sch.SPO_LEFT: (1, 0), sch.SPO_UP: (0, 1), sch.SPO_DOWN: (0, -1)}
SPIN_FOR = {(-1, 0): sch.SLSS_LEFT, (1, 0): sch.SLSS_RIGHT, (0, 1): sch.SLSS_BOTTOM, (0, -1): sch.SLSS_UP}


class LiveSchematic(FootprintTools, BoardTools, BoardDesignTools, FabPackageTools):
    def __init__(self, socket_url=None, timeout_ms=None):
        self.socket_url = socket_url or os.environ.get("KICAD_API_SOCKET")
        explicit_socket = bool(self.socket_url)
        # Linux/WSL (Unix sockets) and Windows (named pipes) discover the instance lazily;
        # other hosts keep the default endpoint.
        self._socket_selected = explicit_socket or sys.platform not in ("linux", "win32")
        if not self.socket_url:
            self.socket_url = default_socket_url()
        # Offline MCP startup never validates or dials the default endpoint; an
        # explicit one must be a local IPC URL from the start.
        if explicit_socket:
            local_socket_path(self.socket_url)
        self.token = os.environ.get("KICAD_API_TOKEN", "") if explicit_socket else ""
        self.client_name = "kicad.live-schematic-mcp-" + str(uuid.uuid4())
        self.timeout_ms = api_timeout_ms() if timeout_ms is None else timeout_ms
        self._received_reply = False
        self.lock = threading.RLock()
        # (expected_revision, operation_id) for the next send(); set only by _send_with
        self._pending_header = None
        # Revision KiCad reported in the last reply (ApiResponseHeader.document_revision), or None
        self.last_revision = None
        # Expected revision/operation id for the transactions of one tool call; see mutation_options
        self._options = None
        # Lease every BeginCommit asks for (tests shorten it); None or 0 asks for none (no lease)
        self.lease_seconds = DEFAULT_LEASE_S
        # {"operation_id", "state", "revision"} of the last mutation this client finished
        self.last_operation = None

    def send(self, command, response_type):
        """Never retry mutations: a timeout may follow a successfully applied edit.

        Raises ReplyTimeout when no reply arrives, RevisionMismatch/DuplicateOperation when KiCad
        refused the request under the revision contract, and RuntimeError for other errors.
        """
        with self.lock:
            if not self._socket_selected:
                # With no endpoint at all this fails at once (no_editor) and the next request looks
                # again, so a KiCad started later is still found.  Pin only an instance that was
                # found, never the default endpoint a KiCad would take.
                self.socket_url, selection = select_instance(check_liveness=True, require_listener=True)
                self._socket_selected = selection != "default_missing"
            local_socket_path(self.socket_url)
            if self._received_reply and socket_reachability(self.socket_url) == "not_listening":
                raise DiscoveryError("instance_unavailable", "Selected KiCad instance is unavailable; "
                                     "no command was sent. Run diagnose.py --probe and restart MCP "
                                     "with the intended KICAD_API_SOCKET. Instance identity remains pinned.")
            request = envelope.ApiRequest()
            request.header.client_name = self.client_name
            request.header.kicad_token = self.token
            if self._pending_header:
                expected_revision, operation_id = self._pending_header
                request.header.expected_revision = expected_revision
                request.header.operation_id = operation_id
            request.message.Pack(command)
            # NNG_DURATION_INFINITE disables both timer and disconnect-triggered resends.
            try:
                with pynng.Req0(send_timeout=self.timeout_ms, recv_timeout=self.timeout_ms,
                                resend_time=-1) as conn:
                    # Synchronous NNG negotiation has its own timeout, which can exceed
                    # the request deadline when a peer is stopped before its handshake.
                    conn.dial(self.socket_url, block=False)
                    conn.send(request.SerializeToString())
                    raw = conn.recv()
            except pynng.Timeout as error:
                raise ReplyTimeout(f"No reply from KiCad to {command.DESCRIPTOR.name} within "
                                   f"{self.timeout_ms} ms; KiCad may still apply it",
                                   getattr(error, "errno", 5)) from error
            except pynng.ConnectionReset as error:
                # KiCad closed the connection (it exited or crashed) after the request may have
                # reached it: a lost reply, reconciled like a timeout, never a resend
                raise ReplyTimeout(f"KiCad closed the connection before replying to "
                                   f"{command.DESCRIPTOR.name}; KiCad may still apply it",
                                   getattr(error, "errno", 5)) from error
            reply = envelope.ApiResponse.FromString(raw)
            self.last_revision = reply.header.document_revision or None
            if reply.status.status != envelope.AS_OK:
                name = envelope.ApiStatusCode.Name(reply.status.status)
                raise STATUS_ERRORS.get(name, RuntimeError)(f"{name}: {reply.status.error_message}")
            response = response_type()
            if not reply.message.Unpack(response):
                raise RuntimeError(f"Unexpected reply to {command.DESCRIPTOR.full_name}")
            self._received_reply = True
            if not self.token:
                self.token = reply.header.kicad_token
            return response

    def documents(self):
        version = self.send(base.GetVersion(), base.GetVersionResponse)
        # These handlers first shipped on the 10.99 development branch for KiCad 11.
        if (version.version.major, version.version.minor) < (10, 99):
            raise RuntimeError("Live schematic editing requires this KiCad development build (10.99/11+)")
        try:
            opened = self.send(editor.GetOpenDocuments(type=types.DOCTYPE_SCHEMATIC),
                               editor.GetOpenDocumentsResponse)
        except RuntimeError as error:
            # The project manager answers with no editor open; live_schematic_open opens one
            if "AS_UNHANDLED" not in str(error):
                raise
            opened = editor.GetOpenDocumentsResponse()
        return {"version": as_dict(version.version), "documents": as_dict(opened).get("documents", [])}

    def open_schematic(self, path=None):
        """Open the project's schematic in the running KiCad (project manager mode)."""
        request = project.OpenDocument(type=types.DOCTYPE_SCHEMATIC, path=str(path or ""))
        self.send(request, project.OpenDocumentResponse)
        return self.documents()

    def hierarchy(self, doc):
        return self.send(commands.GetSchematicHierarchy(document=doc), commands.SchematicHierarchyResponse)

    def items(self, doc):
        response = self.send(editor.GetItems(header=types.ItemHeader(document=doc)), editor.GetItemsResponse)
        if response.status != types.IRS_OK:
            raise RuntimeError(f"GetItems failed: {types.ItemRequestStatus.Name(response.status)}")
        return response

    # ---- Revisions and operation reconciliation (REVISIONS.md) --------------------------------

    def _send_with(self, command, response_type, expected_revision=None, operation_id=None):
        """send() with ApiRequestHeader.expected_revision/operation_id set for this request only."""
        with self.lock:
            self._pending_header = (expected_revision or "", operation_id or "")
            try:
                return self.send(command, response_type)
            finally:
                self._pending_header = None

    @contextmanager
    def mutation_options(self, expected_revision=None, operation_id=None):
        """Apply an expected revision and an operation id to the mutations of one tool call.

        The first mutation uses them as given.  Each later one expects the revision the previous
        one left (so nobody else may change the document in between) and uses operation_id.2,
        .3, and so on.  Yields a dict whose "operations" lists what was done.
        """
        with self.lock:
            saved = self._options
            self._options = {"expected_revision": expected_revision or None,
                             "operation_id": operation_id or None, "operations": []}
            try:
                yield self._options
            finally:
                self._options = saved

    def _next_mutation(self):
        """(expected_revision, operation_id) for the next mutation of the current tool call."""
        options = self._options
        if options is None:
            return None, new_operation_id()
        base = options["operation_id"]
        count = len(options["operations"])
        operation_id = (base if count == 0 else f"{base}.{count + 1}") if base else new_operation_id()
        return options["expected_revision"], operation_id

    def _mutation_done(self, operation_id, state, reconciled=False):
        entry = {"operation_id": operation_id, "state": state, "revision": self.last_revision}
        if reconciled:
            entry["reconciled"] = True
        self.last_operation = entry
        options = self._options
        if options is not None:
            options["operations"].append(entry)
            if options["expected_revision"] and state == "committed":
                options["expected_revision"] = self.last_revision

    def document_revision(self, doc):
        """The live revision of an open document, its open API commits and unsaved state."""
        response = self.send(editor.GetDocumentRevision(document=doc), editor.DocumentRevisionResponse)
        return {"revision": response.revision, "open_commits": response.open_commits,
                "modified": response.modified}

    def operation_status(self, operation_id):
        """What happened to a mutation sent with this operation id (never resends anything)."""
        if not operation_id:
            raise ValueError("operation_id is required")
        return operation_state(self.send(editor.GetOperationStatus(operation_id=operation_id),
                                         editor.OperationStatusResponse))

    def reconcile(self, operation_id, error):
        """Ask KiCad once what became of a mutation whose reply was lost; never resend it."""
        try:
            return self.operation_status(operation_id)
        except Exception as query_error:
            raise OutcomeUnknown(f"{error}. The outcome of operation {operation_id} is unknown and could "
                                 f"not be queried ({query_error}). Do not resend it; query "
                                 "live_operation_status later.", operation_id) from error

    def send_mutation(self, command, response_type):
        """One mutation outside a commit (undo, page settings), with the call's revision options."""
        with self.lock:
            expected, operation_id = self._next_mutation()
            try:
                response = self._send_with(command, response_type, expected, operation_id)
            except ReplyTimeout as error:
                status = self.reconcile(operation_id, error)
                if status["state"] != "OPS_COMMITTED":
                    raise OutcomeUnknown(f"{error}. Operation {operation_id} is {status['state']}; "
                                         "do not resend it. Query live_operation_status again.",
                                         operation_id, status) from error
                self._mutation_done(operation_id, "committed", reconciled=True)
                return response_type()
            self._mutation_done(operation_id, "committed")
            return response

    @contextmanager
    def transaction(self, doc, description):
        """One undo step, including rollback if KiCad rejects any item in a batch.

        Every transaction carries an operation id; a commit whose reply is lost is reconciled
        with GetOperationStatus instead of being resent.  With mutation_options, BeginCommit
        also carries the expected revision, and KiCad rolls the commit back if anything else
        changes the document before it ends.
        """
        with self.lock:
            header = types.ItemHeader(document=doc)
            expected, operation_id = self._next_mutation()
            begin = editor.BeginCommit(header=header)
            if self.lease_seconds:
                begin.lease_seconds = self.lease_seconds
            try:
                started = self._send_with(begin, editor.BeginCommitResponse, expected, operation_id)
            except ReplyTimeout as error:
                self._abandon_lost_begin(header, operation_id, error)
            try:
                yield
            except Exception as original:
                try:
                    self.send(editor.EndCommit(header=header, id=started.id, action=editor.CMA_DROP),
                              editor.EndCommitResponse)
                except Exception as rollback:
                    raise RuntimeError(f"Edit failed ({original}); rollback unconfirmed ({rollback}). "
                                       f"Operation {operation_id}: query live_operation_status and inspect "
                                       "the live editor before continuing.") from original
                self._mutation_done(operation_id, "rolled_back")
                raise
            else:
                try:
                    self.send(editor.EndCommit(header=header, id=started.id, action=editor.CMA_COMMIT,
                                               message=description), editor.EndCommitResponse)
                except RevisionMismatch:
                    # KiCad rolled the commit back: the document changed while it was open
                    self._mutation_done(operation_id, "rolled_back")
                    raise
                except ReplyTimeout as error:
                    status = self.reconcile(operation_id, error)
                    if status["state"] == "OPS_IN_PROGRESS":
                        status = self._settle_open_commit(header, operation_id, error)
                    if status["state"] == "OPS_COMMITTED":
                        self.last_revision = status.get("revision_after") or None
                        self._mutation_done(operation_id, "committed", reconciled=True)
                        return
                    if status["state"] == "OPS_ROLLED_BACK":
                        self._mutation_done(operation_id, "rolled_back", reconciled=True)
                        raise RuntimeError(f"Operation {operation_id} was not committed: "
                                           f"{status.get('message', 'rolled back')}") from error
                    raise OutcomeUnknown(f"{error}. Operation {operation_id} is {status['state']}: it may "
                                         "still commit, or be rolled back when its lease expires. Do not "
                                         "resend it; query live_operation_status again.",
                                         operation_id, status) from error
                self._mutation_done(operation_id, "committed")

    def _settle_open_commit(self, header, operation_id, error):
        """EndCommit's reply was lost and the commit still looks open: settle it with a drop.

        KiCad answers requests in order, so a delivered EndCommit runs before this drop (which
        then finds no commit and changes nothing); an undelivered one leaves the commit open and
        the drop rolls it back.  A drop never applies changes, so this is not a resend.
        """
        try:
            self.send(editor.EndCommit(header=header, action=editor.CMA_DROP), editor.EndCommitResponse)
        except ReplyTimeout:
            return {"state": "OPS_IN_PROGRESS"}
        except RuntimeError:
            pass  # no commit left to drop: the delivered EndCommit settled it
        return self.reconcile(operation_id, error)

    def _abandon_lost_begin(self, header, operation_id, error):
        """BeginCommit's reply was lost.  No edit was sent; close the commit if it opened."""
        status = self.reconcile(operation_id, error)
        if status["state"] == "OPS_IN_PROGRESS":
            # Drop needs no id ("a safety net in case the id was lost on the client side")
            self.send(editor.EndCommit(header=header, action=editor.CMA_DROP), editor.EndCommitResponse)
            raise RuntimeError(f"{error}. The commit opened but its reply was lost; it was dropped and "
                               "nothing was changed.") from error
        if status["state"] == "OPS_REJECTED":
            raise RevisionMismatch(f"{error}. KiCad refused operation {operation_id}: "
                                   f"{status.get('message', '')}") from error
        raise OutcomeUnknown(f"{error}. KiCad has not seen operation {operation_id} ({status['state']}). "
                             "No edit was sent, so nothing was changed; a late commit is rolled back "
                             "when its lease expires.", operation_id, status) from error

    def mutate(self, doc, items, update=False, description=None):
        if not items or len(items) > 200:
            raise ValueError("Supply between 1 and 200 items")
        request = (editor.UpdateItems if update else editor.CreateItems)(header=types.ItemHeader(document=doc))
        for item in items:
            request.items.add().Pack(item)
        response_type = editor.UpdateItemsResponse if update else editor.CreateItemsResponse
        description = description or ("MCP: update items" if update else "MCP: create items")
        with self.transaction(doc, description):
            response = self.send(request, response_type)
            results = response.updated_items if update else response.created_items
            if response.status != types.IRS_OK or len(results) != len(items):
                raise RuntimeError("KiCad did not acknowledge the entire item batch")
            errors = [r.status.error_message or editor.ItemStatusCode.Name(r.status.code)
                      for r in results if r.status.code != editor.ISC_OK]
            if errors:
                raise RuntimeError("; ".join(errors))
        return as_dict(response)

    def delete(self, doc, ids):
        if not ids or len(ids) > 200:
            raise ValueError("Supply between 1 and 200 UUIDs")
        request = editor.DeleteItems(header=types.ItemHeader(document=doc))
        for value in ids:
            request.item_ids.add(value=str(uuid.UUID(value)))
        with self.transaction(doc, "MCP: delete items"):
            response = self.send(request, editor.DeleteItemsResponse)
            if (response.status != types.IRS_OK or len(response.deleted_items) != len(ids)
                    or any(r.status != editor.IDS_OK for r in response.deleted_items)):
                raise RuntimeError(f"Deletion rejected: {as_dict(response)}")
        return as_dict(response)

    def run_edit(self, doc, request, response_type, description):
        """Run one editing command as its own undo step."""
        with self.transaction(doc, description):
            return self.send(request, response_type)

    def annotate(self, doc, scope="all", symbols=(), order="x", numbering="first_free",
                 reset_existing=False, recursive=True, start_number=0):
        scopes = {"all": commands.ANS_ALL, "sheet": commands.ANS_SHEET, "symbols": commands.ANS_SYMBOLS}
        orders = {"x": commands.AO_BY_X_POSITION, "y": commands.AO_BY_Y_POSITION,
                  "unsorted": commands.AO_UNSORTED}
        numberings = {"first_free": commands.AN_FIRST_FREE, "sheet_x100": commands.AN_SHEET_X_100,
                      "sheet_x1000": commands.AN_SHEET_X_1000}
        if scope not in scopes or order not in orders or numbering not in numberings:
            raise ValueError(f"scope must be one of {list(scopes)}, order one of {list(orders)}, "
                             f"numbering one of {list(numberings)}")
        if scope == "symbols" and not symbols:
            raise ValueError("scope 'symbols' needs symbol UUIDs")
        request = commands.AnnotateSchematic(
            document=doc, scope=scopes[scope], order=orders[order], numbering=numberings[numbering],
            reset_existing=reset_existing, recursive=recursive, start_number=start_number)
        request.symbols.extend(kiid(v) for v in symbols)
        response = self.run_edit(doc, request, commands.AnnotateSchematicResponse, "MCP: annotate")
        return {"changes": [{"id": c.symbol.value, "old": c.old_reference, "new": c.new_reference}
                            for c in response.changes]}

    def update_from_library(self, doc, symbols=(), new_lib_id=None, reset_field_text=False,
                            reset_field_visibility=False, reset_field_positions=False,
                            remove_extra_fields=False):
        request = commands.UpdateSymbolsFromLibrary(
            document=doc, reset_field_text=reset_field_text,
            reset_field_visibility=reset_field_visibility,
            reset_field_positions=reset_field_positions, remove_extra_fields=remove_extra_fields)
        request.symbols.extend(kiid(v) for v in symbols)
        if new_lib_id:
            request.new_lib_id.CopyFrom(lib_identifier(new_lib_id))
        response = self.run_edit(doc, request, commands.UpdateSymbolsFromLibraryResponse,
                                 "MCP: update symbols from library")
        return {"symbols": [{"id": s.symbol.value,
                             "lib_id": f"{s.lib_id.library_nickname}:{s.lib_id.entry_name}",
                             "updated": s.updated, **({"error": s.error_message} if s.error_message else {})}
                            for s in response.symbols]}

    def autoplace(self, doc, ids=()):
        request = commands.AutoplaceFields(document=doc)
        request.items.extend(kiid(v) for v in ids)
        from google.protobuf import empty_pb2
        self.run_edit(doc, request, empty_pb2.Empty, "MCP: autoplace fields")
        return {"success": True}

    def transform(self, doc, ids, dx=0.0, dy=0.0, rotation=0, mirror_horizontally=False,
                  mirror_vertically=False, drag_wires=True):
        rotations = {0: commands.IR_NONE, 90: commands.IR_CCW_90, 180: commands.IR_180,
                     270: commands.IR_CW_90, -90: commands.IR_CW_90}
        if rotation not in rotations:
            raise ValueError("rotation must be 0, 90, 180 or 270 degrees counter-clockwise")
        if not ids or len(ids) > 200:
            raise ValueError("Supply between 1 and 200 UUIDs")
        request = commands.TransformItems(
            document=doc, translation=point(dx, dy), rotation=rotations[rotation],
            mirror_horizontally=mirror_horizontally, mirror_vertically=mirror_vertically,
            drag_wires=drag_wires)
        request.items.extend(kiid(v) for v in ids)
        response = self.run_edit(doc, request, commands.TransformItemsResponse, "MCP: move items")
        return {"moved": len(ids), "dragged": [i.value for i in response.dragged_items]}

    def undo(self, doc, redo=False):
        """Undo/redo are refused while any commit is open, so they never run in a transaction."""
        request = (editor.RedoLastChange if redo else editor.UndoLastChange)(document=doc)
        return as_dict(self.send_mutation(request, editor.UndoRedoResponse))

    def netlist(self, doc):
        """Nets as {name: [{sheet, id}]} plus a pin lookup {(sheet, pin id): net}."""
        response = self.send(commands.GetSchematicNetlist(document=doc), commands.SchematicNetlistResponse)
        pin_nets = {}
        nets = {}
        for net in response.nets:
            members = nets.setdefault(net.name, [])
            for sheet in net.sheets:
                key = sheet_key(sheet.path)
                for item in sheet.items:
                    members.append((key, item.value))
                    pin_nets[(key, item.value)] = net.name
        return nets, pin_nets

    def sheet_instances(self, doc):
        """Every sheet instance of the schematic, depth first: (document for that sheet, its path)."""
        result = []

        def visit(nodes):
            for node in nodes:
                spec = types.DocumentSpecifier()
                spec.CopyFrom(doc)
                spec.sheet_path.CopyFrom(node.path)
                result.append((spec, node.path.path_human_readable or "/"))
                visit(node.children)

        visit(self.hierarchy(doc).top_level_sheets)
        return result

    def netlist_pins(self, doc, scope="hierarchy"):
        """Nets with their pins as REF.PIN and the number of connected items.

        scope="hierarchy" labels pins on every sheet instance, so a net that crosses sheets lists all of
        its pins, and adds "sheets": each sheet path with the pins on it.  A reused sheet's pins carry the
        reference of each instance.  scope="sheet" labels only pins on the requested sheet (the reply
        before hierarchy support); "items" counts every sheet in both cases.
        """
        if scope not in ("hierarchy", "sheet"):
            raise ValueError('scope must be "hierarchy" or "sheet"')
        nets = self.netlist(doc)[0]
        if scope == "sheet":
            labels = {}
            for symbol in self.symbol_pins(doc, with_nets=False):
                for pin in symbol["pins"]:
                    labels[pin["id"]] = f'{symbol["reference"]}.{pin["number"]}'
            return {name: {"pins": sorted({labels[i] for _, i in members if i in labels}), "items": len(members)}
                    for name, members in sorted(nets.items())}
        labels, paths = {}, {}
        for spec, path in self.sheet_instances(doc):
            key = sheet_key(spec.sheet_path)
            paths[key] = path
            for symbol in self.symbol_pins(spec, with_nets=False):
                for pin in symbol["pins"]:
                    labels[(key, pin["id"])] = f'{symbol["reference"]}.{pin["number"]}'
        result = {}
        for name, members in sorted(nets.items()):
            sheets = {}
            for key, item in members:
                if (key, item) in labels:
                    sheets.setdefault(paths.get(key, "/".join(key)), set()).add(labels[(key, item)])
            result[name] = {"pins": sorted(set().union(*sheets.values())), "items": len(members),
                            "sheets": [{"path": path, "pins": sorted(pins)} for path, pins in sorted(sheets.items())]}
        return result

    def symbol_pins(self, doc, symbols=(), with_nets=True):
        request = commands.GetSymbolPins(document=doc)
        request.symbols.extend(kiid(v) for v in symbols)
        response = self.send(request, commands.SymbolPinsResponse)
        pin_nets = self.netlist(doc)[1] if with_nets else {}
        key = sheet_key(doc.sheet_path)
        result = []
        for symbol in response.symbols:
            pins = []
            for pin in symbol.pins:
                entry = {"number": pin.number, "name": pin.name, "position_mm": mm(pin.position),
                         "orientation": sch.SchematicPinOrientation.Name(pin.orientation),
                         "type": types.ElectricalPinType.Name(pin.electrical_type), "id": pin.id.value}
                if with_nets:
                    entry["net"] = pin_nets.get((key, pin.id.value))
                pins.append(entry)
            result.append({"id": symbol.symbol.value, "reference": symbol.reference,
                           "lib_id": f"{symbol.lib_id.library_nickname}:{symbol.lib_id.entry_name}",
                           "unit": symbol.unit, "pins": pins})
        return result

    def find_pin(self, doc, reference, pin):
        """Locate one placed pin by reference designator and pin number or name."""
        for symbol in self.symbol_pins(doc, with_nets=False):
            if symbol["reference"] != reference:
                continue
            for candidate in symbol["pins"]:
                if pin in (candidate["number"], candidate["name"]):
                    return symbol, candidate
        raise ValueError(f"No pin {reference}.{pin} on this sheet")

    def connect_pins(self, doc, from_ref, from_pin, to_ref, to_pin):
        """Draw an orthogonal wire (one or two segments) between two pins on one sheet."""
        _, a = self.find_pin(doc, from_ref, from_pin)
        _, b = self.find_pin(doc, to_ref, to_pin)
        (x1, y1), (x2, y2) = a["position_mm"], b["position_mm"]
        if (x1, y1) == (x2, y2):
            raise ValueError("Pins already share a connection point")
        # Leave the first pin along its own axis, then turn toward the second
        horizontal = OUTWARD[sch.SchematicPinOrientation.Value(a["orientation"])][1] == 0
        corner = (x2, y1) if horizontal else (x1, y2)
        points = [(x1, y1)]
        for p in (corner, (x2, y2)):
            if p != points[-1]:
                points.append(p)
        wires = [sch.SchematicLine(id=kiid(), start=point(*s), end=point(*e), type=sch.SLT_WIRE)
                 for s, e in zip(points, points[1:])]
        self.mutate(doc, wires)
        return {"wires": [w.id.value for w in wires], "path_mm": [list(p) for p in points]}

    def connect_pin_to_net(self, doc, reference, pin, net, label="local", stub_mm=2.54):
        """Add a short wire leaving the pin and a net label at its end, as one undo step."""
        if label not in ("local", "global", "hierarchical"):
            raise ValueError("label must be local, global or hierarchical")
        if not 0 < stub_mm <= 50:
            raise ValueError("stub_mm must be between 0 and 50")
        _, found = self.find_pin(doc, reference, pin)
        x, y = found["position_mm"]
        direction = OUTWARD[sch.SchematicPinOrientation.Value(found["orientation"])]
        end = (x + direction[0] * stub_mm, y + direction[1] * stub_mm)
        wire = sch.SchematicLine(id=kiid(), start=point(x, y), end=point(*end), type=sch.SLT_WIRE)
        kind = {"local": sch.LocalLabel, "global": sch.GlobalLabel, "hierarchical": sch.HierarchicalLabel}[label]
        text = types.Text(text=net)
        tag = kind(id=kiid(), position=point(*end), text=text, spin_style=SPIN_FOR[direction])
        if label != "local":
            tag.shape = sch.SLSH_BIDI if label == "hierarchical" else sch.SLSH_PASSIVE
        self.mutate(doc, [wire, tag])
        return {"wire": wire.id.value, "label": tag.id.value, "label_position_mm": list(end)}

    def erc(self, doc, timeout_s=60.0):
        import time
        started = self.send(sch_tasks.StartErcTask(document=doc), tasks.StartTaskResponse)
        deadline = time.monotonic() + timeout_s
        while True:
            status = self.send(tasks.GetTaskStatus(token=started.token), tasks.TaskStatusResponse)
            if status.state == tasks.TS_COMPLETED:
                break
            if status.state in (tasks.TS_FAILED, tasks.TS_CANCELLED):
                raise RuntimeError(f"ERC {tasks.TaskState.Name(status.state)}: {status.error_message}")
            if time.monotonic() > deadline:
                raise TimeoutError("ERC did not finish in time; it may still be running in KiCad")
            time.sleep(0.25)
        result = self.send(sch_tasks.GetErcTaskResult(token=started.token), sch_tasks.ErcTaskResult)
        violations = []
        for marker in result.violations:
            entry = as_dict(marker)
            entry["position_mm"] = mm(marker.position)
            entry.pop("position", None)
            violations.append(entry)
        return {"violation_count": len(violations), "ignored_checks": result.ignored_count,
                "excluded": result.excluded_count, "violations": violations}

    def select(self, doc, ids, add=False):
        if not add:
            self.send(editor.ClearSelection(header=types.ItemHeader(document=doc)), empty())
        if not ids:
            return {"selected": []}
        request = editor.AddToSelection(header=types.ItemHeader(document=doc))
        request.items.extend(kiid(v) for v in ids)
        response = self.send(request, editor.SelectionResponse)
        return {"selected": [i.TypeName().rsplit(".", 1)[-1] for i in response.items]}

    def highlight_nets(self, doc, nets):
        response = self.send(cross_probe.HighlightNets(net_name=list(nets)), cross_probe.HighlightNetsResponse)
        return as_dict(response)

    def export(self, doc, kind, output_path, options=None):
        """Export the unsaved design (svg, pdf, dxf, ps, netlist, bom) to a file KiCad can write.
        options (protobuf JSON of the kind's RunSchematicJobExport* message) are merged over the
        defaults; the BOM default is orderable (fab_package.BOM_DEFAULT)."""
        from fab_package import SCHEMATIC_EXPORT_DEFAULTS, SCHEMATIC_EXPORTS, export_request
        request = export_request(SCHEMATIC_EXPORTS, SCHEMATIC_EXPORT_DEFAULTS, kind, options)
        request.job_settings.document.CopyFrom(doc)
        request.job_settings.output_path = str(output_path)
        response = self.send(request, jobs.RunJobResponse)
        if response.status not in JOB_OK:
            raise RuntimeError(f"Export failed: {response.message}")
        # Warnings KiCad logged while exporting (JS_WARNING)
        return {"outputs": list(response.output_path), **({"message": response.message} if response.message else {})}

    def execute(self, name, payload=None):
        from commands import PROJECT_EDITS, build_request
        full, request, response_type, kind = build_request(name, payload)
        # Document edits carry an operation id (and any expected revision); project settings
        # edits are not handled by a document editor, which keeps those
        if kind == "edit" and full not in PROJECT_EDITS:
            response = self.send_mutation(request, response_type)
        else:
            response = self.send(request, response_type)
        return {"command": full, "kind": kind, "result": as_dict(response)}

    def batch(self, doc, steps, description="MCP: batch"):
        """Run transactional edits on one document. UI selection/zoom are not rolled back."""
        from commands import BATCHABLE_KINDS, NOT_BATCHABLE, TRANSACTIONAL_COMMANDS, build_request
        if not steps or len(steps) > 100:
            raise ValueError("Supply between 1 and 100 steps")
        prepared = []
        for index, step in enumerate(steps):
            full, request, response_type, kind = build_request(step["command"], step.get("payload"))
            if (kind not in BATCHABLE_KINDS or full in NOT_BATCHABLE
                    or (kind == "edit" and full not in TRANSACTIONAL_COMMANDS)):
                raise ValueError(f"step {index}: {full} ({kind}) cannot run inside a batch")
            if not bind_batch_document(request, doc) and kind == "edit":
                raise ValueError(f"step {index}: {full} has no transactional document target")
            prepared.append((full, request, response_type))
        results = []
        with self.transaction(doc, description):
            for index, (full, request, response_type) in enumerate(prepared):
                try:
                    response = self.send(request, response_type)
                except Exception as error:
                    raise RuntimeError(f"step {index} ({full}) failed: {error}") from error
                check_item_statuses(index, full, response, request)
                results.append({"command": full, "result": as_dict(response)})
        return {"committed": True, "steps": results}

    # ---- Symbol libraries -------------------------------------------------------------------

    def library_table(self, scope="both", kind="symbol"):
        if kind == "footprint":
            self.open_footprint_editor()
        request = library.GetLibraryTable(type=library_kind(kind), scope=SCOPES[scope], substituted=True)
        response = self.send(request, library.LibraryTableResponse)
        return [{"nickname": r.nickname, "path": r.uri, "scope": SCOPE_NAMES.get(r.scope, "?"),
                 "description": r.description, "writable": r.writable,
                 **({"disabled": True} if r.disabled else {})} for r in response.rows]

    def add_library(self, nickname, path, scope="global", description="", create=True, kind="symbol"):
        if scope not in ("global", "project"):
            raise ValueError("scope must be global or project")
        suffix = ".kicad_sym" if kind == "symbol" else ".pretty"
        if create and not str(path).rstrip("/").endswith(suffix):
            raise ValueError(f"A new {kind} library must end in {suffix}")
        if kind == "footprint":
            self.open_footprint_editor()
        entry = lib_types.LibraryTableEntry(nickname=nickname, uri=str(path).rstrip("/"), type=library_kind(kind),
                                            description=description)
        status = self.send(library.AddLibraryTableEntry(entry=entry, scope=SCOPES[scope], create_library=create),
                           lib_types.LibraryCommandStatus)
        return check_library_status(status, f"add library {nickname}")

    def remove_library(self, nickname, scope="global", kind="symbol"):
        """Remove a library from a table; its file is left on disk."""
        if kind == "footprint":
            self.open_footprint_editor()
        request = library.DeleteLibraryTableEntry(type=library_kind(kind), nickname=nickname, scope=SCOPES[scope])
        return check_library_status(self.send(request, lib_types.LibraryCommandStatus), f"remove {nickname}")

    def reload_library(self, nickname, kind="symbol"):
        for row in self.library_table(kind=kind):
            if row["nickname"] == nickname:
                request = library.ReloadLibrary(type=library_kind(kind), scope=SCOPES[row["scope"]],
                                                nickname=[nickname])
                return check_library_status(self.send(request, lib_types.LibraryCommandStatus), "reload")
        raise ValueError(f"Library {nickname} is not in the {kind} library tables")

    def library_symbols(self, lib_ids):
        request = library.GetItemsFromLibrary(type=lib_types.LT_SYMBOL)
        request.item_ids.extend(lib_identifier(i) for i in lib_ids)
        response = self.send(request, editor.GetItemsResponse)
        result = []
        for packed in response.items:
            symbol = sch.SchematicSymbol()
            if packed.Unpack(symbol):
                result.append(symbol)
        return result

    def search_symbols(self, query, limit=25, details=True):
        response = self.send(library.SearchLibraries(type=lib_types.LT_SYMBOL, query=query),
                             library.SearchLibrariesResponse)
        ids = [f"{i.library_nickname}:{i.entry_name}" for i in response.items]
        top = ids[:max(1, min(limit, 200))]
        loading = {"libraries_still_loading": response.libraries_loading} if response.libraries_loading else {}
        if not details or not top:
            return {"total": len(ids), **loading, "results": [{"lib_id": i} for i in top]}
        found = {lib_id_text(s.id): s for s in self.library_symbols(top)}
        results = []
        for lib_id in top:
            symbol = found.get(lib_id)
            entry = {"lib_id": lib_id}
            if symbol:
                entry.update(description=symbol.description_field.text.text, keywords=symbol.keywords,
                             pins=sum(1 for c in symbol.items if c.item.TypeName().endswith(".SchematicPin")),
                             units=symbol.unit_count or 1)
                if symbol.footprint_field.text.text:
                    entry["footprint"] = symbol.footprint_field.text.text
            results.append(entry)
        return {"total": len(ids), **loading, "results": results}

    def library_symbol(self, lib_id):
        symbols = self.library_symbols([lib_id])
        if not symbols:
            raise ValueError(f"{lib_id} not found")
        return summarize_symbol(symbols[0])

    # ---- Symbol Editor (live, unsaved until symbol_save) ------------------------------------

    def new_symbol(self, library_nickname, name, reference_prefix="U", unit_count=1, inherit_from="",
                   power_symbol=False, pin_names_outside=False, hide_pin_numbers=False, hide_pin_names=False,
                   exclude_from_bom=False, exclude_from_board=False, replace_existing=False,
                   manufacturer=None, mpn=None):
        request = commands.NewLibrarySymbol(
            library_nickname=library_nickname, name=name, reference_prefix=reference_prefix,
            unit_count=unit_count, inherit_from=inherit_from, power_symbol=power_symbol,
            pin_names_outside=pin_names_outside, hide_pin_numbers=hide_pin_numbers,
            hide_pin_names=hide_pin_names, exclude_from_bom=exclude_from_bom,
            exclude_from_board=exclude_from_board, replace_existing=replace_existing)
        document_spec = self.send(request, commands.LibrarySymbolResponse).document
        # Every new symbol carries sourcing fields, hidden, ready to fill in
        self.symbol_set_properties(document_spec, manufacturer=manufacturer or "", mpn=mpn or "")
        return as_dict(document_spec)

    def edit_symbol(self, lib_id):
        request = commands.EditLibrarySymbol(id=lib_identifier(lib_id))
        return as_dict(self.send(request, commands.LibrarySymbolResponse).document)

    def symbol_items(self, doc):
        response = self.send(editor.GetItems(header=types.ItemHeader(document=doc)), editor.GetItemsResponse)
        return response

    def symbol_add_pins(self, doc, pins, draw_body=True, replace=False, arrange="auto", group_gap=2):
        """Add pins (and a body rectangle) laid out around a box, as one undo step."""
        # Python unless EMBEINT_LAYOUT_BACKEND opts in to the Rust port (see layout_backend.py)
        import layout_backend
        layout, backend = layout_backend.layout(pins, arrange=arrange, group_gap=group_gap)
        items = []
        if replace:
            existing = [i for i in self.symbol_items(doc).items
                        if i.TypeName().endswith((".SchematicPin", ".SchematicGraphicShape"))]
            if existing:
                self.symbol_delete(doc, [item_id(i) for i in existing if item_id(i)])
        if draw_body:
            x1, y1, x2, y2 = layout["body"]
            body = sch.SchematicGraphicShape()
            body.shape.rectangle.top_left.CopyFrom(point(x1, y1))
            body.shape.rectangle.bottom_right.CopyFrom(point(x2, y2))
            # Stock KiCad symbol body: 0.254 mm default-style outline, filled with the body color
            body.shape.attributes.stroke.width.value_nm = 254000
            body.shape.attributes.stroke.style = enums.SLS_DEFAULT
            body.shape.attributes.fill.fill_type = types.GFT_FILLED_WITH_BACKGROUND_BODY_COLOR
            items.append(body)
        for pin in layout["pins"]:
            message = sch.SchematicPin(
                id=kiid(), name=str(pin.get("name", "")), number=pin["number"], position=point(*pin["position"]),
                orientation=sch.SchematicPinOrientation.Value(pin["orientation"]),
                electrical_type=types.ElectricalPinType.Value(pin["electrical_type"]),
                shape=sch.SchematicPinShape.Value(pin.get("shape", "SPS_LINE")), visible=not pin.get("hidden"))
            message.length.value_nm = round(pin["length"] * 1_000_000)
            # 1.27 mm (50 mil) is KiCad's standard pin text; zero would hide names and numbers
            message.name_text_size.value_nm = 1270000
            message.number_text_size.value_nm = 1270000
            items.append(message)
        request = editor.CreateItems(header=types.ItemHeader(document=doc))
        for item in items:
            request.items.add().Pack(item)
        with self.transaction(doc, "MCP: add symbol pins"):
            response = self.send(request, editor.CreateItemsResponse)
            check_item_statuses(0, "CreateItems", response)
            if draw_body:
                self.send(commands.SetLibrarySymbolProperties(document=doc, place_fields_around_body=True), empty())
        result = {"body_mm": list(layout["body"]),
                  "pins": [{"number": p["number"], "name": p.get("name", ""), "side": p["side"],
                            "position_mm": list(p["position"])} for p in layout["pins"]]}
        if backend:
            result["layout_backend"] = backend
        return result

    def symbol_delete(self, doc, ids):
        request = editor.DeleteItems(header=types.ItemHeader(document=doc))
        request.item_ids.extend(kiid(v) for v in ids)
        with self.transaction(doc, "MCP: delete symbol items"):
            response = self.send(request, editor.DeleteItemsResponse)
            check_item_statuses(0, "DeleteItems", response)
        return {"deleted": len(ids)}

    def symbol_set_properties(self, doc, value=None, footprint=None, datasheet=None, description=None,
                              keywords=None, reference_prefix=None, footprint_filters=None, user_fields=None,
                              remove_user_fields=None, show_pin_numbers=None, show_pin_names=None,
                              exclude_from_bom=None, exclude_from_board=None, exclude_from_sim=None,
                              power_symbol=None, place_fields_around_body=False, manufacturer=None, mpn=None):
        # Designator/value alignment depends on their text, so re-place them when it changes
        place = place_fields_around_body or value is not None or reference_prefix is not None
        request = commands.SetLibrarySymbolProperties(document=doc, place_fields_around_body=place)
        for name, setting in (("value", value), ("footprint", footprint), ("datasheet", datasheet),
                              ("description", description), ("keywords", keywords),
                              ("reference_prefix", reference_prefix), ("show_pin_numbers", show_pin_numbers),
                              ("show_pin_names", show_pin_names), ("exclude_from_bom", exclude_from_bom),
                              ("exclude_from_board", exclude_from_board), ("exclude_from_sim", exclude_from_sim),
                              ("power_symbol", power_symbol)):
            if setting is not None:
                setattr(request, name, setting)
        if footprint_filters is not None:
            request.set_footprint_filters = True
            request.footprint_filters.extend(footprint_filters)
        sourcing = [{"name": name, "value": setting, "visible": False}
                    for name, setting in ((MANUFACTURER_FIELD, manufacturer), (MPN_FIELD, mpn)) if setting is not None]
        for field in sourcing + list(user_fields or []):
            request.user_fields.add(name=field["name"], value=str(field.get("value", "")),
                                    visible=bool(field.get("visible", False)))
        request.remove_user_fields.extend(remove_user_fields or [])
        with self.transaction(doc, "MCP: symbol properties"):
            self.send(request, empty())
        return {"success": True}

    def symbol_render(self, doc, dpi=600, unit=0):
        if not 150 <= dpi <= 1200:
            raise ValueError("dpi must be between 150 and 1200")
        request = commands.RenderLibrarySymbol(document=doc, dpi=dpi, unit=unit)
        return self.send(request, commands.RenderLibrarySymbolResponse).image

    def symbol_summary(self, doc):
        pins, shapes = [], []
        for packed in self.symbol_items(doc).items:
            pin = sch.SchematicPin()
            if packed.Unpack(pin):
                pins.append({"number": pin.number, "name": pin.name, "position_mm": mm(pin.position),
                             "type": types.ElectricalPinType.Name(pin.electrical_type),
                             "orientation": sch.SchematicPinOrientation.Name(pin.orientation), "id": pin.id.value,
                             **({} if pin.visible else {"hidden": True})})
            elif packed.TypeName().endswith(".SchematicGraphicShape"):
                shapes.append(item_id(packed))
        return {"lib_id": lib_id_text(doc.lib_id), "pins": pins, "graphic_shape_ids": shapes}

    def symbol_save(self, doc):
        """Write the symbol into its library file and make the schematic see the new version."""
        self.send(project.SaveDocument(document=doc), empty())
        self.reload_library(doc.lib_id.library_nickname)
        return {"saved": f"{doc.lib_id.library_nickname}:{doc.lib_id.entry_name}"}

    # ---- Page, drawing sheet and title block -------------------------------------------------

    def page_layout(self, doc):
        """Page size, drawing area, title block box and fields, items outside the frame and overlapping
        text (mm).  `fits` covers the frame only; `overlap_free` and `overlaps` report overlapping text,
        fields, labels and bodies (schematic) or silkscreen/fab text (board) from KiCad's own extents,
        and are None/empty with `overlap_check` set when the editor cannot check."""
        layout = self.send(editor.GetPageLayout(document=doc), editor.PageLayoutResponse)
        settings = self.send(editor.GetPageSettings(document=doc), types.PageSettings)
        block = self.send(editor.GetTitleBlockInfo(document=doc), types.TitleBlockInfo)
        problems = [{"id": p.id.value, "type": p.type, "name": p.name, "box_mm": box_mm(p.box),
                     **({"outside_drawing_area": True} if p.outside_drawing_area else {}),
                     **({"overlaps_title_block": True} if p.overlaps_title_block else {})}
                    for p in layout.problems]

        def part(item):
            return {"id": item.id.value, "type": item.type, "name": item.name,
                    **({"part": item.part} if item.part else {}), "box_mm": box_mm(item.box)}

        overlaps = [{"first": part(o.first), "second": part(o.second), "overlap_mm": box_mm(o.overlap),
                     **({"layer": o.layer} if o.layer else {})} for o in layout.overlaps]
        overlap_report = {"overlap_free": None, "overlaps": [],
                          "overlap_check": "not available from this KiCad build"}
        if layout.overlaps_checked:
            overlap_report = {"overlap_free": layout.overlap_count == 0, "overlaps": overlaps}
            if layout.overlap_count > len(overlaps):
                overlap_report["overlaps_not_listed"] = layout.overlap_count - len(overlaps)
        return {
            "paper": types.PageSize.Name(settings.page_size).removeprefix("PS_"),
            "orientation": types.PageOrientation.Name(settings.orientation).removeprefix("PO_").lower(),
            "page_size_mm": [round(v, 1) for v in mm(layout.page_size)],
            "drawing_area_mm": box_mm(layout.drawing_area),
            "title_block_mm": box_mm(layout.title_block),
            "content_mm": box_mm(layout.content),
            **({"board_outline_mm": box_mm(layout.board_outline)} if box_mm(layout.board_outline) else {}),
            "items": layout.item_count,
            "fits": not problems,
            "problems": problems,
            **overlap_report,
            "title_block": title_block_dict(block),
            **({"drawing_sheet": settings.drawing_sheet} if settings.drawing_sheet else {}),
        }

    def set_page(self, doc, size=None, orientation=None, user_size_mm=None, title=None, date=None,
                 revision=None, company=None, comments=None):
        """Change the sheet's paper and/or title block; omitted settings are kept.  date="today" works."""
        if size is not None or orientation is not None or user_size_mm is not None:
            settings = self.send(editor.GetPageSettings(document=doc), types.PageSettings)
            if size is not None:
                key = size.upper().replace("-", "_").replace(" ", "_").removeprefix("US_")
                if key not in PAGE_SIZES:
                    raise ValueError(f"size must be one of {sorted(PAGE_SIZES)}")
                settings.page_size = PAGE_SIZES[key]
            if user_size_mm is not None:
                settings.page_size = types.PS_USER
                settings.user_page_size.CopyFrom(point(*user_size_mm))
            if orientation is not None:
                if orientation not in ORIENTATIONS:
                    raise ValueError("orientation must be landscape or portrait")
                settings.orientation = ORIENTATIONS[orientation]
            self.send_mutation(editor.SetPageSettings(document=doc, page_settings=settings), types.PageSettings)
        values = {"title": title, "date": date, "revision": revision, "company": company}
        if any(v is not None for v in values.values()) or comments is not None:
            block = self.send(editor.GetTitleBlockInfo(document=doc), types.TitleBlockInfo)
            if values["date"] == "today":
                import datetime
                values["date"] = datetime.date.today().isoformat()
            for name, value in values.items():
                if value is not None:
                    setattr(block, name, value)
            if comments is not None:
                if len(comments) > 9:
                    raise ValueError("a title block has at most 9 comments")
                for n in range(1, 10):
                    setattr(block, f"comment{n}", comments[n - 1] if n <= len(comments) else "")
            self.send_mutation(editor.SetTitleBlockInfo(document=doc, title_block=block), empty())
        return self.page_layout(doc)

    def place_symbol(self, doc, lib_id, x, y, reference=None, rotation=0):
        if rotation not in (0, 90, 180, 270):
            raise ValueError("rotation must be 0, 90, 180, or 270 degrees")
        request = commands.PlaceSymbolFromLibrary(
            header=types.ItemHeader(document=doc),
            lib_id=lib_identifier(lib_id),
            position=point(x, y), orientation=getattr(sch, f"SSO_{rotation}"),
        )
        if reference:
            request.reference = reference
        with self.transaction(doc, "MCP: place schematic symbol"):
            result = self.send(request, library.PlaceFromLibraryResponse)
        # The reply embeds the full library definition (graphics, pins); return only what an
        # agent needs. live_schematic_items has the complete instance when required.
        placed = sch.SchematicSymbolInstance()
        if not result.item.Unpack(placed):
            raise RuntimeError("KiCad did not return the placed symbol")
        return {
            "id": placed.id.value,
            "lib_id": lib_id,
            "reference": placed.reference_field.text.text,
            "value": placed.value_field.text.text,
            "position_mm": [placed.position.x_nm / 1e6, placed.position.y_nm / 1e6],
            "unit": placed.unit.unit,
        }

    def render(self, doc, dpi=100, frame=True):
        """The GUI job handler plots the live model; only the output PNG touches disk.

        The image is always the whole page.  frame=True draws the drawing sheet (border and title
        block) too, so content that runs past the border or into the title block is visible.
        """
        if not 72 <= dpi <= 200:
            raise ValueError("Preview DPI must be between 72 and 200")
        with self.lock, tempfile.TemporaryDirectory(prefix="kicad-live-preview-") as directory:
            target = Path(directory) / "preview.png"
            request = sch_jobs.RunSchematicJobExportPng(dpi=dpi, plot_background_color=True)
            request.job_settings.document.CopyFrom(doc)
            request.job_settings.output_path = str(target)
            request.plot_settings.sheet_mode = sch_jobs.SJSM_SINGLE_SHEET
            request.plot_settings.plot_drawing_sheet = frame
            response = self.send(request, jobs.RunJobResponse)
            if response.status not in JOB_OK or not target.is_file():
                raise RuntimeError(f"Live rendering failed: {as_dict(response)}")
            return target.read_bytes()


def empty():
    from google.protobuf import empty_pb2
    return empty_pb2.Empty


def bind_batch_document(request, doc):
    """Bind omitted targets and reject cross-document edits before opening a commit."""
    bound = False
    for field in request.DESCRIPTOR.fields:
        if not field.message_type or field.is_repeated:
            continue
        target = None
        if field.message_type.full_name == types.DocumentSpecifier.DESCRIPTOR.full_name:
            target = getattr(request, field.name)
        elif field.message_type.full_name == types.ItemHeader.DESCRIPTOR.full_name:
            target = getattr(request, field.name).document
        if target is not None:
            if target.ListFields() and target != doc:
                raise ValueError("Batch steps must target the batch document exactly")
            target.CopyFrom(doc)
            bound = True
    return bound


def check_item_statuses(index, command, response, request=None):
    """Raise for any per-item rejection, which KiCad reports inside an otherwise OK reply."""
    for field in ("created_items", "updated_items", "items"):
        for result in getattr(response, field, ()):
            # Only item-result messages have ItemStatus; GetItems contains protobuf Any items.
            if hasattr(result, "status") and hasattr(result.status, "code") \
                    and result.status.code != editor.ISC_OK:
                status = editor.ItemStatusCode.DESCRIPTOR.values_by_number.get(result.status.code)
                raise RuntimeError(f"step {index} ({command}) rejected an item, batch rolled back: "
                                   f"{result.status.error_message or (status.name if status else result.status.code)}")
    for result in getattr(response, "deleted_items", ()):
        if result.status != editor.IDS_OK:
            raise RuntimeError(f"step {index} ({command}) could not delete {result.id.value}, batch rolled back")
    for result in getattr(response, "symbols", ()):
        if hasattr(result, "updated") and not result.updated:
            raise RuntimeError(f"step {index} ({command}) rejected a symbol, batch rolled back: "
                               f"{result.error_message or 'symbol was not updated'}")
    # Check the response contract, not the command suffix: parsed creation also returns
    # CreateItemsResponse, despite its command ending in "String".
    status_field = response.DESCRIPTOR.fields_by_name.get("status")
    if status_field and status_field.enum_type == types.ItemRequestStatus.DESCRIPTOR \
            and response.status != types.IRS_OK:
        status = types.ItemRequestStatus.DESCRIPTOR.values_by_number.get(response.status)
        raise RuntimeError(f"step {index} ({command}) returned {status.name if status else response.status}")
    if request is not None:
        result_fields = {"CreateItems": ("items", "created_items"),
                         "UpdateItems": ("items", "updated_items"),
                         "DeleteItems": ("item_ids", "deleted_items"),
                         "TransformBoardItems": ("items", "items"),
                         "UpdateSymbolsFromLibrary": ("symbols", "symbols")}
        fields = result_fields.get(command.rsplit(".", 1)[-1])
        if fields:
            expected = len(getattr(request, fields[0]))
            # An empty symbol filter intentionally addresses every symbol on the sheet.
            if len(getattr(response, fields[1])) != expected \
                    and (expected or not command.endswith("UpdateSymbolsFromLibrary")):
                raise RuntimeError(f"step {index} ({command}) did not acknowledge every item, batch rolled back")


PAGE_SIZES = {"A5": types.PS_A5, "A4": types.PS_A4, "A3": types.PS_A3, "A2": types.PS_A2, "A1": types.PS_A1,
              "A0": types.PS_A0, "ANSI_A": types.PS_ANSI_A, "ANSI_B": types.PS_ANSI_B, "ANSI_C": types.PS_ANSI_C,
              "ANSI_D": types.PS_ANSI_D, "ANSI_E": types.PS_ANSI_E, "LETTER": types.PS_US_LETTER,
              "LEGAL": types.PS_US_LEGAL, "LEDGER": types.PS_US_LEDGER, "USER": types.PS_USER}
ORIENTATIONS = {"landscape": types.PO_LANDSCAPE, "portrait": types.PO_PORTRAIT}
TITLE_FIELDS = ("title", "date", "revision", "company")


def box_mm(box):
    """A Box2 as [x1, y1, x2, y2] in mm, or None for an empty box."""
    if not box.HasField("size") or (box.size.x_nm == 0 and box.size.y_nm == 0):
        return None
    x, y = box.position.x_nm / 1e6, box.position.y_nm / 1e6
    return [round(x, 3), round(y, 3), round(x + box.size.x_nm / 1e6, 3), round(y + box.size.y_nm / 1e6, 3)]


def title_block_dict(block):
    fields = {name: getattr(block, name) for name in TITLE_FIELDS}
    fields["comments"] = [getattr(block, f"comment{n}") for n in range(1, 10)]
    while fields["comments"] and not fields["comments"][-1]:
        fields["comments"].pop()
    return fields


def parse_items(values):
    """Accept protobuf-JSON Any objects returned by GetItems; reject other namespaces."""
    from google.protobuf import any_pb2
    result = []
    for value in values:
        packed = json_format.ParseDict(value, any_pb2.Any())
        name = packed.TypeName().removeprefix("kiapi.schematic.types.")
        descriptor = sch.DESCRIPTOR.message_types_by_name.get(name)
        if descriptor is None or packed.TypeName() != descriptor.full_name:
            raise ValueError("Only schematic item types are accepted")
        message = getattr(sch, name)()
        if not packed.Unpack(message):
            raise ValueError("Invalid schematic item")
        result.append(message)
    return result
