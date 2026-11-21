# SPDX-License-Identifier: GPL-3.0-or-later
"""Parity scenarios for the PCB editor tools (tools/board.rs and board_client.rs against
board_client.py and server.py).

Fixture items have ids that are not version-4 UUIDs, so scenarios that build items
(generated_ids=True) compare the servers' own uuid4 ids by order of appearance and the fixture ids
exactly.  Renders and the netlist go through temporary directories: the peer writes the file the
job asks for, as KiCad would.
"""

import json
from pathlib import Path

U1 = "f0000000-0000-0000-0000-000000000001"
R1 = "f0000000-0000-0000-0000-000000000002"
H1 = "f0000000-0000-0000-0000-000000000003"
TRACK_GND = "a0000000-0000-0000-0000-000000000001"
TRACK_SIG = "a0000000-0000-0000-0000-000000000002"
VIA_GND = "a0000000-0000-0000-0000-000000000003"
ZONE_GND = "a0000000-0000-0000-0000-000000000004"
EDGE_1 = "e0000000-0000-0000-0000-000000000001"
EDGE_2 = "e0000000-0000-0000-0000-000000000002"
EDGE_FP = "e0000000-0000-0000-0000-000000000003"
PNG = b"\x89PNG\r\n\x1a\nboard fixture"

ANY = "type.googleapis.com/kiapi.board.types."


def scenarios(h):
    from board_client import bc, board_jobs, board_tasks, bt, project, tasks
    from common.commands import library_commands_pb2 as library
    from schematic import schematic_jobs_pb2 as sch_jobs

    editor, envelope, types = h.editor, h.envelope, h.types
    reply, parse, failure, S = h.reply, h.parse, h.failure, h.S
    B = h.BOARD_DOC
    BEGIN, END_OK, DROP_OK = h.BEGIN, h.END_OK, h.DROP_OK

    def text(value):
        return {"text": {"text": {"text": value}}}

    fp_u1 = {"@type": ANY + "FootprintInstance", "id": {"value": U1},
             "position": {"x_nm": "10000000", "y_nm": "20000000"}, "orientation": {"value_degrees": 90.0},
             "layer": "BL_F_Cu", "definition": {"id": {"library_nickname": "Package_SO", "entry_name": "SOIC-8"}},
             "reference_field": text("U1"), "value_field": text("NE555")}
    fp_r1 = {"@type": ANY + "FootprintInstance", "id": {"value": R1},
             "position": {"x_nm": "-1234567", "y_nm": "5000001"}, "orientation": {"value_degrees": 45.12345},
             "layer": "BL_B_Cu", "locked": "LS_LOCKED", "attributes": {"do_not_populate": True},
             "definition": {"id": {"library_nickname": "Resistor_SMD", "entry_name": "R_0603"}},
             "reference_field": text("R1"), "value_field": text("10k")}
    fp_h1 = {"@type": ANY + "FootprintInstance", "id": {"value": H1}, "layer": "BL_F_Cu",
             "definition": {"id": {"library_nickname": "MountingHole", "entry_name": "M3"}},
             "reference_field": text("H1")}

    def pad(parent, number, net, x, y, drill=None, shape="PSS_RECTANGLE", kind="PT_SMD"):
        stack = {"copper_layers": [{"layer": "BL_F_Cu", "shape": shape,
                                    "size": {"x_nm": "1500000", "y_nm": "600000"}}]}
        if drill:
            stack["drill"] = {"diameter": {"x_nm": str(drill), "y_nm": str(drill)}}
        return {"@type": ANY + "Pad", "id": {"value": f"b0000000-0000-0000-0000-{parent[-4:]}{number:0>8}"},
                "number": number, "net": {"name": net}, "type": kind, "parent": {"value": parent},
                "position": {"x_nm": str(x), "y_nm": str(y)}, "pad_stack": stack}

    pads = [pad(U1, "10", "GND", 11000000, 21000000), pad(U1, "2", "VCC", 9000000, 21000000),
            pad(U1, "1", "SIG", 9000000, 19000000), pad(R1, "1", "SIG", -2000000, 5000000, 800000, "PSS_CIRCLE",
                                                         "PT_PTH"), pad("ffff", "1", "", 0, 0)]
    track_gnd = {"@type": ANY + "Track", "id": {"value": TRACK_GND}, "layer": "BL_F_Cu", "net": {"name": "GND"},
                 "start": {"x_nm": "0", "y_nm": "0"}, "end": {"x_nm": "1000000", "y_nm": "0"}}
    track_sig = {"@type": ANY + "Track", "id": {"value": TRACK_SIG}, "layer": "BL_B_Cu", "net": {"name": "SIG"}}
    via_gnd = {"@type": ANY + "Via", "id": {"value": VIA_GND}, "net": {"name": "GND"}}
    zone_gnd = {"@type": ANY + "Zone", "id": {"value": ZONE_GND}, "layers": ["BL_F_Cu", "BL_B_Cu"],
                "copper_settings": {"net": {"name": "GND"}}}
    edge = lambda value, parent: {"@type": ANY + "BoardGraphicShape", "id": {"value": value},  # noqa: E731
                                  "layer": "BL_Edge_Cuts", "parent": {"value": parent}}
    silk = {"@type": ANY + "BoardGraphicShape", "id": {"value": "e0000000-0000-0000-0000-000000000009"},
            "layer": "BL_F_SilkS"}

    def items(*values, status="IRS_OK"):
        return lambda r: reply(parse(editor.GetItemsResponse, {"status": status, "items": list(values)}))

    footprints = ("GetItems", items(fp_u1, fp_r1, fp_h1))
    no_items = ("GetItems", items())

    def nets_reply(*names):
        return ("GetNets", lambda r: reply(parse(bc.NetsResponse, {"nets": [
            {"name": name, "code": {"value": index}} for index, name in enumerate(names)]})))

    def classes(**by_net):
        def respond(request):
            query = bc.GetNetClassForNets()
            request.message.Unpack(query)
            found = {net.name: by_net[net.name] for net in query.net if net.name in by_net}
            return reply(parse(bc.NetClassForNetsResponse, {"classes": found}))
        return ("GetNetClassForNets", respond)

    power = {"name": "Power", "board": {"track_width": {"value_nm": "500000"},
                                        "clearance": {"value_nm": "200000"},
                                        "via_stack": {"copper_layers": [{"size": {"x_nm": "900000"}}],
                                                      "drill": {"diameter": {"x_nm": "450000"}}}}}
    plain = {"name": "Default"}

    def box(x=0, y=0, w=0, hgt=0):
        return ("GetBoardBoundingBox", lambda r: reply(parse(bc.BoardBoundingBoxResponse, {"box": {
            "position": {"x_nm": str(x), "y_nm": str(y)}, "size": {"x_nm": str(w), "y_nm": str(hgt)}}})))

    from google.protobuf import empty_pb2

    def ok(kind):
        return (kind, lambda r: reply(empty_pb2.Empty()))

    def acknowledged(command, *codes, message="refused"):
        """UpdateItems/DeleteItems answered for every item (codes per item, default OK)."""
        def respond(request):
            edit = getattr(editor, command)()
            request.message.Unpack(edit)
            response = getattr(editor, command + "Response")(header=edit.header, status=types.IRS_OK)
            if command == "UpdateItems":
                for index, item in enumerate(edit.items):
                    code = codes[index] if index < len(codes) else editor.ISC_OK
                    entry = response.updated_items.add(item=item)
                    entry.status.code = code
                    if code != editor.ISC_OK:
                        entry.status.error_message = message
            else:
                for item_id in edit.item_ids:
                    response.deleted_items.add(id=item_id, status=editor.IDS_OK)
            return reply(response, revision="fx-2")
        return (command, respond)

    def created(*codes, message="refused"):
        return h.created(*codes, message=message)

    def all_created():
        """CreateItems with every item acknowledged."""
        def respond(request):
            create = editor.CreateItems()
            request.message.Unpack(create)
            result = editor.CreateItemsResponse(status=types.IRS_OK, header=create.header)
            for packed in create.items:
                entry = result.created_items.add(item=packed)
                entry.status.code = editor.ISC_OK
            return reply(result, revision="fx-2")
        return ("CreateItems", respond)

    commit = [BEGIN, all_created(), END_OK]

    def job_writes(name, status=1, message="", write=True, outputs=None):
        """A job reply after writing the file the request names (a render's PNG, the netlist)."""
        def respond(request):
            job = getattr(board_jobs, name, None) or getattr(sch_jobs, name)
            message_value = job()
            request.message.Unpack(message_value)
            path = message_value.job_settings.output_path
            if write:
                Path(path).write_bytes(PNG if name.startswith("RunBoard") else b"(export (version E))\n")
            result = h.registry.message_class("kiapi.common.types.RunJobResponse")(status=status, message=message)
            result.output_path.extend([path] if outputs is None else outputs)
            return reply(result)
        return (name, respond)

    stackup_2 = {"layers": [
        {"type": "BSLT_SILKSCREEN", "layer": "BL_F_SilkS", "color": {"r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0}},
        {"type": "BSLT_SOLDERMASK", "layer": "BL_F_Mask", "thickness": {"value_nm": "10000"},
         "soldermask": {"epsilon_r": 3.3, "material_name": "LPI"}},
        {"type": "BSLT_COPPER", "layer": "BL_F_Cu", "thickness": {"value_nm": "35000"}},
        {"type": "BSLT_DIELECTRIC", "thickness": {"value_nm": "1510000"},
         "dielectric": {"type": "BSDT_CORE", "layer": [{"thickness": {"value_nm": "1510000"}, "epsilon_r": 4.5,
                                                        "loss_tangent": 0.02, "material_name": "FR4"}]}},
        {"type": "BSLT_COPPER", "layer": "BL_B_Cu", "thickness": {"value_nm": "35000"}}],
        "finish": {"type_name": "ENIG"}}
    get_stackup = ("GetBoardStackup", lambda r: reply(parse(bc.BoardStackupResponse, {"stackup": stackup_2})))
    enabled = ("GetBoardEnabledLayers", lambda r: reply(parse(bc.BoardEnabledLayersResponse, {
        "copper_layer_count": 2, "layers": ["BL_F_Cu", "BL_B_Cu", "BL_F_SilkS", "BL_Edge_Cuts", "BL_User_1"]})))
    visible = ("GetVisibleLayers", lambda r: reply(parse(bc.BoardLayers, {"layers": ["BL_F_Cu", "BL_Edge_Cuts"]})))
    active = ("GetActiveLayer", lambda r: reply(parse(bc.BoardLayerResponse, {"layer": "BL_B_Cu"})))

    def origin(x, y):
        return ("GetBoardOrigin", lambda r: reply(types.Vector2(x_nm=x, y_nm=y)))

    def drc(violations=(), state="TS_COMPLETED", error=""):
        script = [("StartDrcTask", lambda r: reply(tasks.StartTaskResponse(token=tasks.TaskToken(value="drc-9")))),
                  ("GetTaskStatus", lambda r: reply(tasks.TaskStatusResponse(state=tasks.TS_RUNNING))),
                  ("GetTaskStatus", lambda r: reply(tasks.TaskStatusResponse(state=tasks.TaskState.Value(state),
                                                                             error_message=error)))]
        if state == "TS_COMPLETED":
            result = parse(board_tasks.DrcTaskResult, {"ignored_count": 3, "excluded_count": 1,
                                                       "violations": list(violations)})
            script.append(("GetDrcTaskResult", lambda r: reply(result)))
        return script

    placed = parse(library.PlaceFromLibraryResponse, {"item": dict(fp_h1, position={"x_nm": "5000000",
                                                                                     "y_nm": "5000000"})})
    place = {"board_document": B, "lib_id": "MountingHole:M3", "x_mm": 5, "y_mm": 5.0}
    status_committed = ("GetOperationStatus", h.status("OPS_COMMITTED", revision_after="fx-5"))
    track_item = {"@type": ANY + "Track", "start": {"x_nm": "0", "y_nm": "0"},
                  "end": {"x_nm": "2540000", "y_nm": "0"}, "width": {"value_nm": "250000"}, "layer": "BL_F_Cu"}
    renders = ("live_pcb_view", {"board_document": B})
    long_report = "".join(f"line {i}\n" for i in range(1200))

    result = [
        # ---- Documents
        S("board_open", [("live_pcb_open", {}), ("live_pcb_open", {"path": "C:/fixture/parity/other.kicad_pcb"}),
                         ("live_pcb_open", {"path": None}), ("live_pcb_open", {})],
          [("OpenDocument", lambda r: reply(parse(project.OpenDocumentResponse, {"document": B}))),
           ("OpenDocument", lambda r: reply(parse(project.OpenDocumentResponse, {"document": B}))),
           ("OpenDocument", lambda r: failure(envelope.AS_UNHANDLED, "no handler")),
           ("OpenDocument", lambda r: failure(envelope.AS_BAD_REQUEST, "no project open"))]),
        S("board_open_unimplemented", [("live_pcb_open", {})],
          [("OpenDocument", lambda r: failure(envelope.AS_UNIMPLEMENTED, "not here"))]),
        S("board_save", [("live_pcb_save", {"board_document": B}), ("live_pcb_save", {"board_document": B})],
          [ok("SaveDocument"), ("SaveDocument", lambda r: failure(envelope.AS_BUSY, "a dialog is open"))]),
        S("board_document_refused", [
            ("live_pcb_summary", {"board_document": h.SHEET_DOC}),
            ("live_pcb_save", {"board_document": dict(B, board_filename="")}),
            ("live_pcb_move", {"board_document": dict(B, nope=1), "moves": []}),
            ("live_pcb_update_from_schematic", {"board_document": B, "schematic_document": B}),
            ("live_pcb_view", {"board_document": json.dumps(h.SHEET_DOC)})], []),
        # ---- Reads
        S("board_summary", [("live_pcb_summary", {"board_document": B})],
          [footprints, ("GetItems", items(track_gnd, track_sig)), no_items, ("GetItems", items(via_gnd)),
           ("GetItems", items(zone_gnd)), ("GetItems", items(edge(EDGE_1, ""), silk)), no_items, enabled,
           nets_reply("", "GND", "SIG", "VCC"), classes(GND=plain, VCC=power), box(-1000000, 2500000, 50000000,
                                                                                   30123456),
           ("GetDocumentModifiedState", lambda r: reply(parse(editor.GetDocumentModifiedStateResponse,
                                                              {"state": "DMS_MODIFIED"}))), footprints]),
        S("board_summary_empty", [("live_pcb_summary", {"board_document": B})],
          [no_items] * 7 + [("GetBoardEnabledLayers", lambda r: reply(bc.BoardEnabledLayersResponse())),
                            nets_reply(), box(),
                            ("GetDocumentModifiedState", lambda r: reply(editor.GetDocumentModifiedStateResponse())),
                            no_items]),
        S("board_summary_items_refused", [("live_pcb_summary", {"board_document": B})],
          [("GetItems", items(status="IRS_DOCUMENT_NOT_FOUND"))]),
        S("board_footprints", [("live_pcb_footprints", {"board_document": B}),
                               ("live_pcb_footprints", {"board_document": B, "references": ["U1", "R1", "X9"],
                                                        "pads": True}),
                               ("live_pcb_footprints", {"board_document": B, "references": [], "pads": "yes"})],
          [footprints, footprints, ("GetItems", items(*pads)), footprints, ("GetItems", items(*pads))]),
        S("board_footprints_unknown_pad_type", [("live_pcb_footprints", {"board_document": B, "pads": True})],
          [footprints, ("GetItems", items(dict(pads[0], type=99)))]),
        S("board_nets", [("live_pcb_nets", {"board_document": B}), ("live_pcb_nets", {"board_document": B})],
          [nets_reply("GND", "", "VCC", "/sheet/SIG"), classes(VCC=power, **{"/sheet/SIG": plain}),
           nets_reply()]),
        S("board_items", [("live_pcb_items", {"board_document": B, "kinds": ["tracks", "vias", "footprints"]}),
                          ("live_pcb_items", {"board_document": B, "kinds": []}),
                          ("live_pcb_items", {"board_document": B, "kinds": ["tracks", "wires"]})],
          [("GetItems", items(track_gnd, via_gnd, fp_u1)), ("GetItems", items(silk))]),
        S("board_items_validation", [("live_pcb_items", {"board_document": B, "kinds": "tracks"}),
                                     ("live_pcb_items", {"board_document": B})], [], compare="error_prefix"),
        # ---- Item edits (the revision contract)
        S("board_create_items", [("live_pcb_create_items", {"board_document": B, "items": [track_item, via_gnd],
                                                             "expected_revision": "fx-1",
                                                             "operation_id": "op-board-1"})],
          [BEGIN, created(1, 1), END_OK]),
        S("board_create_items_rejected", [("live_pcb_create_items", {"board_document": B, "items": [track_item]})],
          [BEGIN, created(editor.ISC_INVALID_DATA, message="track outside the board"), DROP_OK]),
        S("board_create_items_commit_lost", [("live_pcb_create_items", {"board_document": B, "items": [track_item],
                                                                         "operation_id": "op-board-2"})],
          [BEGIN, created(1), ("EndCommit", lambda r: h.DROP), status_committed]),
        S("board_create_items_refused", [
            ("live_pcb_create_items", {"board_document": B, "items": [h.LINE]}),
            ("live_pcb_create_items", {"board_document": B, "items": []}),
            ("live_pcb_create_items", {"board_document": B, "items": [track_item] * 201}),
            ("live_pcb_create_items", {"board_document": B, "items": [{"@type": ANY + "Track", "bogus": 1}]}),
            ("live_pcb_update_items", {"board_document": B, "items": [{}]})], []),
        S("board_update_items", [("live_pcb_update_items", {"board_document": B, "items": [track_gnd],
                                                             "operation_id": "op-board-3"})],
          [BEGIN, acknowledged("UpdateItems"), END_OK]),
        S("board_update_items_rejected", [("live_pcb_update_items", {"board_document": B,
                                                                      "items": [track_gnd, via_gnd]})],
          [BEGIN, acknowledged("UpdateItems", 1, editor.ISC_NONEXISTENT, message=""), DROP_OK]),
        S("board_delete_items", [("live_pcb_delete_items", {"board_document": B, "ids": [TRACK_GND, VIA_GND.upper()],
                                                             "expected_revision": "fx-1"})],
          [BEGIN, acknowledged("DeleteItems"), END_OK]),
        S("board_delete_items_refused", [("live_pcb_delete_items", {"board_document": B, "ids": ["nope"]}),
                                         ("live_pcb_delete_items", {"board_document": B, "ids": []})], []),
        S("board_delete_items_begin_lost", [("live_pcb_delete_items", {"board_document": B, "ids": [TRACK_GND]})],
          [("BeginCommit", lambda r: h.DROP), ("GetOperationStatus", h.status("OPS_IN_PROGRESS")), DROP_OK]),
        S("board_create_items_validation", [("live_pcb_create_items", {"board_document": B, "items": 3}),
                                            ("live_pcb_delete_items", {"board_document": B, "ids": [1]})], [],
          compare="error_prefix"),
        # ---- Ripping up routing
        S("board_delete_routing_net", [("live_pcb_delete_routing", {"board_document": B, "net": "GND"})],
          [("GetItems", items(track_gnd, track_sig)), no_items, ("GetItems", items(via_gnd)),
           BEGIN, acknowledged("DeleteItems"), END_OK]),
        S("board_delete_routing_layer", [
            ("live_pcb_delete_routing", {"board_document": B, "layer": "B.Cu", "kinds": ["tracks", "vias", "zones"]}),
            ("live_pcb_delete_routing", {"board_document": B, "layer": "Nope", "kinds": ["arcs"]}),
            ("live_pcb_delete_routing", {"board_document": B, "net": "SIG", "kinds": []})],
          [("GetItems", items(track_gnd, track_sig)), ("GetItems", items(via_gnd)), ("GetItems", items(zone_gnd)),
           BEGIN, acknowledged("DeleteItems"), END_OK, no_items,
           ("GetItems", items(track_gnd, track_sig)), no_items, no_items, BEGIN, acknowledged("DeleteItems"),
           END_OK]),
        S("board_delete_routing_refusals", [
            ("live_pcb_delete_routing", {"board_document": B, "layer": "Nope", "kinds": ["tracks"]}),
            ("live_pcb_delete_routing", {"board_document": B, "kinds": ["textboxes"]}),
            ("live_pcb_delete_routing", {"board_document": B, "kinds": ["wires"]}),
            ("live_pcb_delete_routing", {"board_document": B, "layer": "F.Cu", "kinds": ["pads"]}),
            ("live_pcb_delete_routing", {"board_document": B, "net": "none", "kinds": ["shapes", "texts"]})],
          [("GetItems", items(track_gnd)), ("GetItems", items(*pads[:1])), ("GetItems", items(silk)),
           no_items]),
        # ---- Update PCB from Schematic
        S("board_update_from_schematic", [
            ("live_pcb_update_from_schematic", {"board_document": B, "schematic_document": h.SHEET_DOC}),
            ("live_pcb_update_from_schematic", {"board_document": B, "schematic_document": h.SHEET_DOC,
                                                "dry_run": True, "delete_extra_footprints": True,
                                                "update_footprints": False, "match": "reference"})],
          [job_writes("RunSchematicJobExportNetlist"),
           ("ImportNetlist", lambda r: reply(bc.ImportNetlistResponse(error_count=1, warning_count=2,
                                                                       new_footprint_count=3,
                                                                       report="Added U1\nWarning: R9"))),
           job_writes("RunSchematicJobExportNetlist", status=2, message="Missing footprint"),
           ("ImportNetlist", lambda r: reply(bc.ImportNetlistResponse(report=long_report + "\u00b5 end")))]),
        S("board_update_from_schematic_fails", [
            ("live_pcb_update_from_schematic", {"board_document": B, "schematic_document": h.SHEET_DOC}),
            ("live_pcb_update_from_schematic", {"board_document": B, "schematic_document": h.SHEET_DOC}),
            ("live_pcb_update_from_schematic", {"board_document": B, "schematic_document": h.SHEET_DOC})],
          [job_writes("RunSchematicJobExportNetlist", status=3, message="ERC failed"),
           job_writes("RunSchematicJobExportNetlist", write=False),
           job_writes("RunSchematicJobExportNetlist"),
           ("ImportNetlist", lambda r: failure(envelope.AS_BAD_REQUEST, "board is locked"))]),
        # ---- Placement
        S("board_place_footprint", [("live_pcb_place_footprint", dict(place, reference="H2", value="M3 hole",
                                                                       locked=True))],
          [BEGIN, ("PlaceFootprintFromLibrary", lambda r: reply(placed)), END_OK,
           BEGIN, acknowledged("UpdateItems"), END_OK]),
        S("board_place_footprint_plain", [
            ("live_pcb_place_footprint", place),
            ("live_pcb_place_footprint", dict(place, side="back", rotation=-90.5, reference="", locked="false"))],
          [BEGIN, ("PlaceFootprintFromLibrary", lambda r: reply(placed)), END_OK,
           BEGIN, ("PlaceFootprintFromLibrary", lambda r: reply(placed)), END_OK]),
        S("board_place_footprint_failures", [
            ("live_pcb_place_footprint", place), ("live_pcb_place_footprint", dict(place, value="x")),
            ("live_pcb_place_footprint", place)],
          [BEGIN, ("PlaceFootprintFromLibrary", lambda r: failure(envelope.AS_BAD_REQUEST, "no such footprint")),
           DROP_OK,
           BEGIN, ("PlaceFootprintFromLibrary", lambda r: reply(parse(library.PlaceFromLibraryResponse, {}))),
           END_OK,
           BEGIN, ("PlaceFootprintFromLibrary", lambda r: reply(placed)), ("EndCommit", lambda r: h.DROP),
           ("GetOperationStatus", h.status("OPS_ROLLED_BACK", message="lease expired"))]),
        S("board_place_footprint_refusals", [
            ("live_pcb_place_footprint", dict(place, lib_id="M3")),
            ("live_pcb_place_footprint", dict(place, lib_id=":M3")),
            ("live_pcb_place_footprint", dict(place, side="top")),
            ("live_pcb_place_footprint", dict(place, x_mm=10001))], []),
        S("board_move", [("live_pcb_move", {"board_document": B, "moves": [
            {"reference": "U1", "x_mm": 20, "y_mm": 15.5, "rotation": 270},
            {"reference": "R1", "x_mm": 1.5, "relative": True, "rotation": 15, "side": "front", "locked": 0},
            {"reference": "H1", "side": "front", "locked": True, "rotation": -180.0},
            {"reference": "U1", "y_mm": 3, "side": "back", "rotation": 539.5}]})],
          [footprints, BEGIN] + [("TransformBoardItems", lambda r: reply(parse(bc.TransformBoardItemsResponse, {
              "status": "IRS_OK", "items": [{"status": {"code": "ISC_OK"}}]})))] * 4 + [END_OK, footprints]),
        S("board_move_rejected", [("live_pcb_move", {"board_document": B, "moves": [
            {"reference": "U1", "x_mm": 1}, {"reference": "R1", "x_mm": 2}]})],
          [footprints, BEGIN,
           ("TransformBoardItems", lambda r: reply(parse(bc.TransformBoardItemsResponse, {
               "items": [{"status": {"code": "ISC_OK"}}]}))),
           ("TransformBoardItems", lambda r: reply(parse(bc.TransformBoardItemsResponse, {"items": [
               {"status": {"code": "ISC_IMMUTABLE"}}, {"status": {"code": "ISC_INVALID_DATA",
                                                                  "error_message": "locked"}}]}))),
           DROP_OK]),
        S("board_move_refusals", [
            ("live_pcb_move", {"board_document": B, "moves": []}),
            ("live_pcb_move", {"board_document": B, "moves": [{"reference": "Q9"}]}),
            ("live_pcb_move", {"board_document": B, "moves": [{"x_mm": 1}]}),
            ("live_pcb_move", {"board_document": B, "moves": [{"reference": "U1", "side": "top"}]}),
            ("live_pcb_move", {"board_document": B, "moves": [{"reference": "U1", "x_mm": "1"}]}),
            ("live_pcb_move", {"board_document": B, "moves": [{"reference": "U1", "rotation": "90"}]}),
            ("live_pcb_move", {"board_document": B, "moves": [{"reference": "U1", "rotation": None,
                                                               "relative": 1}]}),
            ("live_pcb_move", {"board_document": B, "moves": [{"reference": 7}]}),
            ("live_pcb_move", {"board_document": B, "moves": [{"reference": ["U1"]}]}),
            ("live_pcb_move", {"board_document": B, "moves": [{"reference": "U1", "x_mm": 1e9}]})],
          [footprints] * 9),
        S("board_move_commit_lost", [("live_pcb_move", {"board_document": B, "moves": [
            {"reference": "H1", "x_mm": 3, "y_mm": 4}]})],
          [footprints, BEGIN, ("TransformBoardItems", lambda r: reply(bc.TransformBoardItemsResponse())),
           ("EndCommit", lambda r: h.DROP), status_committed, footprints]),
        S("board_create_items_stale", [("live_pcb_create_items", {
            "board_document": json.dumps(B), "items": json.dumps([track_item]), "expected_revision": "fx-1",
            "operation_id": "op-board-9"})],
          [("BeginCommit", lambda r: failure(envelope.AS_REVISION_MISMATCH, "document is at fx-9", "fx-9"))]),
        S("board_move_validation", [("live_pcb_move", {"board_document": B, "moves": ["U1"]})], [],
          compare="error_prefix"),
        # ---- Vias, outline, zones, graphics (built items: generated ids)
        S("board_add_via", [
            ("live_pcb_add_via", {"board_document": B, "x_mm": 1, "y_mm": 2, "net": "VCC"}),
            ("live_pcb_add_via", {"board_document": B, "x_mm": 1, "y_mm": 2, "net": "GND", "size_mm": 0.5}),
            ("live_pcb_add_via", {"board_document": B, "x_mm": -3.25, "y_mm": 0, "size_mm": 0.7, "drill_mm": 0.35}),
            ("live_pcb_add_via", {"board_document": B, "x_mm": 1, "y_mm": 2, "size_mm": 0.3})],
          [classes(VCC=power)] + commit + [classes(GND=plain)] + commit + commit, generated_ids=True),
        S("board_add_via_refused", [
            ("live_pcb_add_via", {"board_document": B, "x_mm": 1, "y_mm": 2, "drill_mm": 0.6}),
            ("live_pcb_add_via", {"board_document": B, "x_mm": 1e5, "y_mm": 2})], [], generated_ids=True),
        S("board_outline", [
            ("live_pcb_outline", {"board_document": B, "width_mm": 50, "height_mm": 30.5, "x_mm": 10, "y_mm": 10}),
            ("live_pcb_outline", {"board_document": B, "width_mm": 40, "height_mm": 20, "corner_radius_mm": 2.5,
                                  "replace": False})],
          [BEGIN, footprints, ("GetItems", items(edge(EDGE_1, ""), edge(EDGE_2, B["board_filename"]),
                                                 edge(EDGE_FP, U1), silk)),
           ("DeleteItems", lambda r: reply(editor.DeleteItemsResponse())), all_created(), END_OK,
           box(10000000, 10000000, 50000000, 30500000),
           BEGIN, all_created(), END_OK, box()], generated_ids=True),
        S("board_outline_polygon", [
            ("live_pcb_outline", {"board_document": B, "points": [[0, 0], [30, 0], [30, 20.25], [0, 20]]}),
            ("live_pcb_outline", {"board_document": B, "points": [[0, 0], [30, 0], [30, 20]]})],
          [BEGIN, footprints, no_items, created(1, 1, 1, 1), END_OK, box(0, 0, 30000000, 20250000),
           BEGIN, footprints, no_items, created(1, editor.ISC_INVALID_DATA, 1, message="self-intersecting"),
           DROP_OK], generated_ids=True),
        S("board_outline_refusals", [
            ("live_pcb_outline", {"board_document": B}),
            ("live_pcb_outline", {"board_document": B, "width_mm": 10, "height_mm": 4, "corner_radius_mm": 2.5}),
            ("live_pcb_outline", {"board_document": B, "points": [[0, 0], [1, 1]]}),
            ("live_pcb_outline", {"board_document": B, "points": [[0, 0], [1, 1], [2]]}),
            ("live_pcb_outline", {"board_document": B, "points": [], "width_mm": -1, "height_mm": 2})], [],
          generated_ids=True),
        S("board_outline_steps_fail", [
            ("live_pcb_outline", {"board_document": B, "width_mm": 5, "height_mm": 5}),
            ("live_pcb_outline", {"board_document": B, "width_mm": 5, "height_mm": 5})],
          [BEGIN, ("GetItems", items(status="IRS_DOCUMENT_NOT_FOUND")), DROP_OK,
           BEGIN, footprints, ("GetItems", items(edge(EDGE_1, ""))),
           ("DeleteItems", lambda r: failure(envelope.AS_BAD_REQUEST, "locked outline")),
           ("EndCommit", lambda r: failure(envelope.AS_BAD_REQUEST, "no commit"))], generated_ids=True),
        S("board_outline_begin_lost", [("live_pcb_outline", {"board_document": B, "width_mm": 5, "height_mm": 5,
                                                             "replace": False})],
          [("BeginCommit", lambda r: h.DROP), ("GetOperationStatus", h.status("OPS_REJECTED", message="stale"))],
          generated_ids=True),
        S("board_add_via_commit_unknown", [("live_pcb_add_via", {"board_document": B, "x_mm": 1, "y_mm": 1})],
          [BEGIN, all_created(), ("EndCommit", lambda r: h.DROP), ("GetOperationStatus", lambda r: h.DROP)],
          generated_ids=True),
        S("board_add_zone", [
            ("live_pcb_add_zone", {"board_document": B, "net": "GND", "layers": ["F.Cu", "B.Cu"], "inset_mm": 0.5,
                                   "priority": 2, "name": "pour", "connection": "solid"}),
            ("live_pcb_add_zone", {"board_document": B, "net": "VCC", "points": [[0, 0], [5, 0], [5, 5]],
                                   "fill": False, "clearance_mm": 0.3, "min_width_mm": 0.2,
                                   "connection": "pth_thermal", "layers": []})],
          [box(1000000, 2000000, 40000000, 30000000)] + commit + [ok("RefillZones")] + commit,
          generated_ids=True),
        S("board_add_zone_refusals", [
            ("live_pcb_add_zone", {"board_document": B, "net": "GND"}),
            ("live_pcb_add_zone", {"board_document": B, "net": "GND", "points": [[0, 0], [1, 0], [1, 1]],
                                   "connection": "spokes"}),
            ("live_pcb_add_zone", {"board_document": B, "net": "GND", "points": [[0, 0], [1, 0], [1, 1]],
                                   "layers": ["F.Cu", "Top.Cu"]}),
            ("live_pcb_add_zone", {"board_document": B, "net": "GND", "points": [[0, 0]], "priority": -1}),
            ("live_pcb_add_zone", {"board_document": B, "net": "GND", "points": [], "fill": False})],
          [box(), BEGIN, ("CreateItems", lambda r: failure(envelope.AS_BAD_REQUEST, "zone needs 3 points")),
           DROP_OK], generated_ids=True),
        S("board_add_graphics", [("live_pcb_add_graphics", {"board_document": B, "shapes": [
            {"kind": "line", "start": [0, 0], "end": [10, 0]},
            {"kind": "arc", "start": [0, 0], "mid": [1, 1], "end": [2, 0], "layer": "Dwgs.User", "width": 0.2},
            {"kind": "rect", "box": [0, 0, 5, 3], "layer": "F.Fab", "filled": 1},
            {"kind": "circle", "center": [5, 5], "radius": 1.5, "layer": "User.Drawings"},
            {"kind": "polygon", "points": [[0, 0], [1, 0], [1, 1]], "layer": "B.SilkS", "filled": False},
            {"kind": "text", "text": "REV A", "position": [10, 10]},
            {"kind": "text", "text": "BOTTOM", "position": [10, 12], "layer": "B.Silkscreen", "size": 2,
             "rotation": 90, "thickness": 0.3},
            {"kind": "text", "text": "T", "position": [1, 1], "size": 1.1, "thickness": None, "layer": "B.Fab"}]})],
          commit, generated_ids=True),
        S("board_add_graphics_refusals", [("live_pcb_add_graphics", {"board_document": B, "shapes": shapes})
                                          for shapes in (
            [], [{"kind": "spline"}], [{"kind": "line", "start": [0, 0]}], [{"kind": "line", "layer": "Nope"}],
            [{"kind": "line", "layer": 3}], [{"kind": "line", "layer": None}],
            [{"kind": "rect", "box": [0, 0]}], [{"kind": "rect", "box": [0, 0, 1, 1, 2]}],
            [{"kind": "rect", "box": 4}], [{"kind": "circle", "center": [0, 0], "radius": "1"}],
            [{"kind": "circle", "center": [0], "radius": 1}], [{"kind": "polygon", "points": 3}],
            [{"kind": "polygon", "points": "ab"}], [{"kind": "text", "text": 5, "position": [0, 0]}],
            [{"kind": "text", "text": "x", "position": [0, 0], "rotation": "90"}],
            [{"kind": "line", "start": [0, 0], "end": [1, 1], "width": "thin"}],
            [{"kind": "line", "start": 5, "end": [1, 1]}],
            [{"kind": "line", "start": [0, 0], "end": [1, 1]}] * 201)], [], generated_ids=True),
        S("board_refill_zones", [("live_pcb_refill_zones", {"board_document": B}),
                                 ("live_pcb_refill_zones", {"board_document": B})],
          [ok("RefillZones"), ("RefillZones", lambda r: failure(envelope.AS_BUSY, "zone filler running"))]),
        # ---- DRC
        S("board_drc", [("live_pcb_drc", {"board_document": B})], drc([
            {"error_type": "DRCET_CLEARANCE", "severity": "RS_ERROR", "description": "Clearance violation",
             "position": {"x_nm": "1500000", "y_nm": "-250000"}, "layer": "BL_F_Cu",
             "items": [{"value": TRACK_GND}, {"value": VIA_GND}], "violating_rule": "netclass 'Default'"},
            {"error_type": "DRCET_UNCONNECTED_ITEMS", "severity": "RS_WARNING", "description": "Missing connection"},
            {"error_type": "DRCET_CLEARANCE", "severity": "RS_ERROR", "layer": "BL_User_1"},
            {"error_type": 53, "severity": "RS_WARNING", "description": "aliased type"}])),
        S("board_drc_without_parity", [("live_pcb_drc", {"board_document": B, "schematic_parity": False})],
          drc()),
        S("board_drc_failed", [("live_pcb_drc", {"board_document": B}), ("live_pcb_drc", {"board_document": B})],
          drc(state="TS_FAILED", error="Zones need refilling") + drc(state="TS_CANCELLED")),
        S("board_drc_many", [("live_pcb_drc", {"board_document": B})],
          drc([{"error_type": "DRCET_SILK_CLEARANCE", "severity": "RS_WARNING",
                "description": f"Silk {i}"} for i in range(203)])),
        S("board_drc_status_lost", [("live_pcb_drc", {"board_document": B})],
          drc()[:1] + [("GetTaskStatus", lambda r: h.DROP)] + drc()[1:]),
        # ---- Views
        S("board_view", [renders, ("live_pcb_view", {"board_document": B, "layers": ["F.Cu", "Edge"], "dpi": 300,
                                                     "frame": False}),
                         ("live_pcb_view", {"board_document": B, "layers": []})],
          [job_writes("RunBoardJobExportPng"), job_writes("RunBoardJobExportPng", status=2, message="font"),
           job_writes("RunBoardJobExportPng", outputs=[])]),
        S("board_view_fails", [renders, renders, ("live_pcb_view", {"board_document": B, "dpi": 49}),
                               ("live_pcb_view", {"board_document": B, "layers": ["F.Cu", "Nope"]}),
                               ("live_pcb_view", {"board_document": B, "dpi": 10 ** 30})],
          [job_writes("RunBoardJobExportPng", status=3, message="plot failed"),
           job_writes("RunBoardJobExportPng", write=False, outputs=["", "C:\\nowhere\\x.png"])]),
        S("board_view_3d", [("live_pcb_view_3d", {"board_document": B}),
                            ("live_pcb_view_3d", {"board_document": B, "side": "bottom", "zoom": 1.5,
                                                  "quality": "high", "width": 640, "height": "480"}),
                            ("live_pcb_view_3d", {"board_document": B, "side": "iso"}),
                            ("live_pcb_view_3d", {"board_document": B, "quality": "ultra"}),
                            ("live_pcb_view_3d", {"board_document": B, "width": 3000000000}),
                            ("live_pcb_view_3d", {"board_document": B})],
          [job_writes("RunBoardJobExportRender"), job_writes("RunBoardJobExportRender"),
           job_writes("RunBoardJobExportRender", write=False)]),
        S("board_view_validation", [("live_pcb_view", {"board_document": B, "dpi": "high"}),
                                    ("live_pcb_view_3d", {"board_document": B, "zoom": [1]})], [],
          compare="error_prefix"),
        # ---- Settings
        S("board_origins", [("live_pcb_origins", {"board_document": B}),
                            ("live_pcb_origins", {"board_document": B, "grid_origin_mm": [10, 20.5],
                                                  "drill_origin_mm": ["1", 2]}),
                            ("live_pcb_origins", {"board_document": B, "grid_origin_mm": [1, 2],
                                                  "drill_origin_mm": [1, 2, 3]}),
                            ("live_pcb_origins", {"board_document": B, "grid_origin_mm": []})],
          [origin(0, 0), origin(-1234567, 7654321), ok("SetBoardOrigin"), ok("SetBoardOrigin"),
           origin(10000000, 20500000), origin(1000000, 2000000), ok("SetBoardOrigin")]),
        S("board_layers", [("live_pcb_layers", {"board_document": B}),
                           ("live_pcb_layers", {"board_document": B, "copper_layers": 4,
                                                "visible": ["F.Cu", "Edge", "User.Drawings"], "active": "B.Cu"}),
                           ("live_pcb_layers", {"board_document": B, "copper_layers": 0})],
          [enabled, visible, active, get_stackup,
           ("UpdateBoardStackup", lambda r: reply(parse(bc.BoardStackupResponse, {"stackup": stackup_2}))),
           get_stackup, ok("SetVisibleLayers"), ok("SetActiveLayer"), enabled, visible, active,
           get_stackup, ("UpdateBoardStackup", lambda r: reply(parse(bc.BoardStackupResponse, {}))), get_stackup,
           enabled, visible, active]),
        S("board_layers_refusals", [("live_pcb_layers", {"board_document": B, "copper_layers": 3}),
                                    ("live_pcb_layers", {"board_document": B, "copper_layers": -2}),
                                    ("live_pcb_layers", {"board_document": B, "copper_layers": 34}),
                                    ("live_pcb_layers", {"board_document": B, "visible": ["Top", "Nope"]}),
                                    ("live_pcb_layers", {"board_document": B, "active": "Middle"}),
                                    ("live_pcb_layers", {"board_document": B, "copper_layers": 4}),
                                    ("live_pcb_layers", {"board_document": B})],
          [get_stackup, get_stackup, get_stackup, get_stackup,
           ("UpdateBoardStackup", lambda r: failure(envelope.AS_BAD_REQUEST, "In1.Cu still has items")),
           ("GetBoardEnabledLayers", lambda r: reply(parse(bc.BoardEnabledLayersResponse, {"layers": [999]}))),
           visible, active]),
        S("board_layers_validation", [("live_pcb_layers", {"board_document": B, "copper_layers": 2.5})], [],
          compare="error_prefix"),
        # ---- Selection
        S("board_select", [("live_pcb_select", {"board_document": B, "references": ["U1", "R1"],
                                                "ids": [TRACK_GND]}),
                           ("live_pcb_select", {"board_document": B, "ids": [VIA_GND], "add": True}),
                           ("live_pcb_select", {"board_document": B}),
                           ("live_pcb_select", {"board_document": B, "references": ["Q1"]})],
          [footprints, footprints, ok("ClearSelection"),
           ("AddToSelection", lambda r: reply(parse(editor.SelectionResponse, {"items": [track_gnd, fp_u1, fp_r1]}))),
           ("AddToSelection", lambda r: reply(editor.SelectionResponse())), ok("ClearSelection"), footprints]),
        S("board_focus", [("live_pcb_focus", {"board_document": B, "references": ["H1"], "ids": [ZONE_GND]}),
                          ("live_pcb_focus", {"board_document": B}),
                          ("live_pcb_focus", {"board_document": B, "ids": ["x"]})],
          [footprints, ok("FocusOnItems"), ok("FocusOnItems"),
           ("FocusOnItems", lambda r: failure(envelope.AS_BAD_REQUEST, "no such item"))]),
    ]
    return result
