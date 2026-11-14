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

sys.path.insert(0, str(Path(__file__).resolve().parent / "_generated"))
try:
    from common import envelope_pb2 as envelope
    from common.commands import base_commands_pb2 as base
    from common.commands import editor_commands_pb2 as editor
    from common.commands import project_commands_pb2 as project
    from common.types import base_types_pb2 as types
    from common.types import jobs_pb2 as jobs
    from schematic import schematic_commands_pb2 as commands
    from schematic import schematic_jobs_pb2 as sch_jobs
    from schematic import schematic_types_pb2 as sch
except ImportError as exc:
    raise RuntimeError("Run generate_proto.py with this interpreter before starting the MCP") from exc


def as_dict(message):
    return json_format.MessageToDict(message, preserving_proto_field_name=True)


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


class LiveSchematic:
    def __init__(self, socket_url=None, timeout_ms=15000):
        self.socket_url = socket_url or os.environ.get("KICAD_API_SOCKET")
        if not self.socket_url:
            self.socket_url = "ipc://" + str(Path(tempfile.gettempdir()) / "kicad" / "api.sock")
        if not self.socket_url.startswith("ipc://"):
            raise ValueError("KICAD_API_SOCKET must be a local ipc:// socket URL")
        self.token = os.environ.get("KICAD_API_TOKEN", "")
        self.client_name = "kicad.live-schematic-mcp-" + str(uuid.uuid4())
        self.timeout_ms = timeout_ms
        self.lock = threading.RLock()

    def send(self, command, response_type):
        """Never retry mutations: a timeout may follow a successfully applied edit."""
        with self.lock:
            request = envelope.ApiRequest()
            request.header.client_name = self.client_name
            request.header.kicad_token = self.token
            request.message.Pack(command)
            with pynng.Req0(send_timeout=self.timeout_ms, recv_timeout=self.timeout_ms) as conn:
                conn.dial(self.socket_url, block=False)
                conn.send(request.SerializeToString())
                raw = conn.recv()
            reply = envelope.ApiResponse.FromString(raw)
            if reply.status.status != envelope.AS_OK:
                raise RuntimeError(
                    f"{envelope.ApiStatusCode.Name(reply.status.status)}: "
                    f"{reply.status.error_message}"
                )
            response = response_type()
            if not reply.message.Unpack(response):
                raise RuntimeError(f"Unexpected reply to {command.DESCRIPTOR.full_name}")
            if not self.token:
                self.token = reply.header.kicad_token
            return response

    def documents(self):
        version = self.send(base.GetVersion(), base.GetVersionResponse)
        # These handlers first shipped on the 10.99 development branch for KiCad 11.
        if (version.version.major, version.version.minor) < (10, 99):
            raise RuntimeError("Live schematic editing requires this KiCad development build (10.99/11+)")
        opened = self.send(editor.GetOpenDocuments(type=types.DOCTYPE_SCHEMATIC),
                           editor.GetOpenDocumentsResponse)
        return {"version": as_dict(version.version), "documents": as_dict(opened).get("documents", [])}

    def hierarchy(self, doc):
        return self.send(commands.GetSchematicHierarchy(document=doc), commands.SchematicHierarchyResponse)

    def items(self, doc):
        response = self.send(editor.GetItems(header=types.ItemHeader(document=doc)), editor.GetItemsResponse)
        if response.status != types.IRS_OK:
            raise RuntimeError(f"GetItems failed: {types.ItemRequestStatus.Name(response.status)}")
        return response

    @contextmanager
    def transaction(self, doc, description):
        """One undo step, including rollback if KiCad rejects any item in a batch."""
        with self.lock:
            header = types.ItemHeader(document=doc)
            started = self.send(editor.BeginCommit(header=header), editor.BeginCommitResponse)
            try:
                yield
            except Exception as original:
                try:
                    self.send(editor.EndCommit(header=header, id=started.id, action=editor.CMA_DROP),
                              editor.EndCommitResponse)
                except Exception as rollback:
                    raise RuntimeError(f"Edit failed ({original}); rollback unconfirmed ({rollback}). "
                                       "Inspect the live editor before continuing.") from original
                raise
            else:
                self.send(editor.EndCommit(header=header, id=started.id, action=editor.CMA_COMMIT,
                                           message=description), editor.EndCommitResponse)

    def mutate(self, doc, items, update=False):
        if not items or len(items) > 200:
            raise ValueError("Supply between 1 and 200 schematic items")
        request = (editor.UpdateItems if update else editor.CreateItems)(header=types.ItemHeader(document=doc))
        for item in items:
            request.items.add().Pack(item)
        response_type = editor.UpdateItemsResponse if update else editor.CreateItemsResponse
        with self.transaction(doc, "MCP: update schematic items" if update else "MCP: create schematic items"):
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
        with self.transaction(doc, "MCP: delete schematic items"):
            response = self.send(request, editor.DeleteItemsResponse)
            if (response.status != types.IRS_OK or len(response.deleted_items) != len(ids)
                    or any(r.status != editor.IDS_OK for r in response.deleted_items)):
                raise RuntimeError(f"Deletion rejected: {as_dict(response)}")
        return as_dict(response)

    def place_symbol(self, doc, lib_id, x, y, reference=None, rotation=0):
        library, separator, entry = lib_id.partition(":")
        if not separator or not library or not entry:
            raise ValueError("lib_id must be Library:Symbol")
        if rotation not in (0, 90, 180, 270):
            raise ValueError("rotation must be 0, 90, 180, or 270 degrees")
        request = commands.PlaceSymbolFromLibrary(
            header=types.ItemHeader(document=doc),
            lib_id=types.LibraryIdentifier(library_nickname=library, entry_name=entry),
            position=point(x, y), orientation=getattr(sch, f"SSO_{rotation}"),
        )
        if reference:
            request.reference = reference
        from common.commands import library_commands_pb2
        with self.transaction(doc, "MCP: place schematic symbol"):
            result = self.send(request, library_commands_pb2.PlaceFromLibraryResponse)
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

    def render(self, doc, dpi=100):
        """The GUI job handler plots the live model; only the output PNG touches disk."""
        if not 72 <= dpi <= 200:
            raise ValueError("Preview DPI must be between 72 and 200")
        with self.lock, tempfile.TemporaryDirectory(prefix="kicad-live-preview-") as directory:
            target = Path(directory) / "preview.png"
            request = sch_jobs.RunSchematicJobExportPng(dpi=dpi, plot_background_color=True)
            request.job_settings.document.CopyFrom(doc)
            request.job_settings.output_path = str(target)
            request.plot_settings.sheet_mode = sch_jobs.SJSM_SINGLE_SHEET
            request.plot_settings.plot_drawing_sheet = False
            response = self.send(request, jobs.RunJobResponse)
            if response.status != jobs.JS_SUCCESS or not target.is_file():
                raise RuntimeError(f"Live rendering failed: {as_dict(response)}")
            return target.read_bytes()


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
