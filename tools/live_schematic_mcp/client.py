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
    from common import tasks_pb2 as tasks
    from common.commands import base_commands_pb2 as base
    from common.commands import cross_probe_commands_pb2 as cross_probe
    from common.commands import editor_commands_pb2 as editor
    from common.commands import library_commands_pb2  # noqa: F401  (registers descriptors)
    from common.commands import project_commands_pb2 as project
    from common.commands import variant_commands_pb2  # noqa: F401
    from common.types import base_types_pb2 as types
    from common.types import jobs_pb2 as jobs
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


# A pin's drawn orientation is the direction from its connection point toward the body, so wires
# and labels leave it the opposite way.  Label spin names the side the text extends to.
OUTWARD = {sch.SPO_RIGHT: (-1, 0), sch.SPO_LEFT: (1, 0), sch.SPO_UP: (0, 1), sch.SPO_DOWN: (0, -1)}
SPIN_FOR = {(-1, 0): sch.SLSS_LEFT, (1, 0): sch.SLSS_RIGHT, (0, 1): sch.SLSS_BOTTOM, (0, -1): sch.SLSS_UP}


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
        request = (commands.RedoLastChange if redo else commands.UndoLastChange)(document=doc)
        return as_dict(self.send(request, commands.UndoRedoResponse))

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

    def export(self, doc, kind, output_path):
        """Export the unsaved design (svg, pdf, dxf, ps, netlist, bom) to a file KiCad can write."""
        classes = {"svg": sch_jobs.RunSchematicJobExportSvg, "pdf": sch_jobs.RunSchematicJobExportPdf,
                   "dxf": sch_jobs.RunSchematicJobExportDxf, "ps": sch_jobs.RunSchematicJobExportPs,
                   "netlist": sch_jobs.RunSchematicJobExportNetlist,
                   "bom": sch_jobs.RunSchematicJobExportBOM}
        if kind not in classes:
            raise ValueError(f"kind must be one of {list(classes)}")
        request = classes[kind]()
        if kind == "netlist":
            request.format = sch_jobs.SNF_KICAD_SEXPR
        request.job_settings.document.CopyFrom(doc)
        request.job_settings.output_path = str(output_path)
        response = self.send(request, jobs.RunJobResponse)
        if response.status != jobs.JS_SUCCESS:
            raise RuntimeError(f"Export failed: {response.message}")
        return {"outputs": list(response.output_path)}

    def execute(self, name, payload=None):
        from commands import build_request
        full, request, response_type, kind = build_request(name, payload)
        return {"command": full, "kind": kind, "result": as_dict(self.send(request, response_type))}

    def batch(self, doc, steps, description="MCP: batch"):
        """Run steps in one commit: one undo step, all dropped if any step fails."""
        from commands import BATCHABLE_KINDS, NOT_BATCHABLE, build_request
        if not steps or len(steps) > 100:
            raise ValueError("Supply between 1 and 100 steps")
        prepared = []
        for index, step in enumerate(steps):
            full, request, response_type, kind = build_request(step["command"], step.get("payload"))
            if kind not in BATCHABLE_KINDS or full in NOT_BATCHABLE:
                raise ValueError(f"step {index}: {full} ({kind}) cannot run inside a batch")
            prepared.append((full, request, response_type))
        results = []
        with self.transaction(doc, description):
            for index, (full, request, response_type) in enumerate(prepared):
                try:
                    response = self.send(request, response_type)
                except Exception as error:
                    raise RuntimeError(f"step {index} ({full}) failed, batch rolled back: {error}") from error
                check_item_statuses(index, full, response)
                results.append({"command": full, "result": as_dict(response)})
        return {"committed": True, "steps": results}

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


def empty():
    from google.protobuf import empty_pb2
    return empty_pb2.Empty


def check_item_statuses(index, command, response):
    """Raise for any per-item rejection, which KiCad reports inside an otherwise OK reply."""
    for field in ("created_items", "updated_items"):
        for result in getattr(response, field, ()):
            if result.status.code != editor.ISC_OK:
                raise RuntimeError(f"step {index} ({command}) rejected an item, batch rolled back: "
                                   f"{result.status.error_message or editor.ItemStatusCode.Name(result.status.code)}")
    for result in getattr(response, "deleted_items", ()):
        if result.status != editor.IDS_OK:
            raise RuntimeError(f"step {index} ({command}) could not delete {result.id.value}, batch rolled back")
    if hasattr(response, "status") and isinstance(response.status, int) and command.endswith("Items") \
            and response.status != types.IRS_OK:
        raise RuntimeError(f"step {index} ({command}) returned {types.ItemRequestStatus.Name(response.status)}")


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
