# SPDX-License-Identifier: GPL-3.0-or-later
"""Parity scenarios for the schematic tools and the page and undo tools both editors share
(embeint-mcp/src/tools/schematic.rs against server.py and client.py)."""

import datetime
import json
import re

UUID_RE = re.compile(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
# Python's TemporaryDirectory and Rust's tempfile name the preview directory differently
PREVIEW_RE = re.compile(r"kicad-live-preview-[A-Za-z0-9_]+")

R1, R1P1, R1P2 = ("a1000000-0000-4000-8000-000000000001", "a1000000-0000-4000-8000-000000000011",
                  "a1000000-0000-4000-8000-000000000012")
U1, U1P3, U1P4 = ("b2000000-0000-4000-8000-000000000002", "b2000000-0000-4000-8000-000000000023",
                  "b2000000-0000-4000-8000-000000000024")
SUB_PIN, WIRE_ID = "c3000000-0000-4000-8000-000000000031", "d4000000-0000-4000-8000-000000000041"
POWER_PATH = "44444444-4444-4444-8444-4444"  # the sub-sheet of the harness hierarchy reply


def pin(pin_id, number, name, x_nm, y_nm, orientation, kind="EPT_PASSIVE"):
    return {"id": {"value": pin_id}, "number": number, "name": name,
            "position": {"x_nm": str(x_nm), "y_nm": str(y_nm)}, "orientation": orientation,
            "electrical_type": kind, "visible": True}


# R1 stands vertically (pin 1 above, pin 2 below); U1's pin 3 leaves to the left, pin 4 to the
# right.  The coordinates make float sums that are not exact in binary (101.6 - 1.27 and so on).
PINS = {"symbols": [
    {"symbol": {"value": R1}, "reference": "R1", "lib_id": {"library_nickname": "Device", "entry_name": "R"},
     "unit": 1, "pins": [pin(R1P1, "1", "~", 101600000, 50800000, "SPO_DOWN"),
                         pin(R1P2, "2", "~", 101600000, 60960000, "SPO_UP")]},
    {"symbol": {"value": U1}, "reference": "U1", "lib_id": {"library_nickname": "MCU", "entry_name": "STM32:X"},
     "unit": 2, "pins": [pin(U1P3, "3", "PA0", 127000000, 55880000, "SPO_RIGHT", "EPT_BIDIRECTIONAL"),
                         pin(U1P4, "4", "GND", 152400000, 55880000, "SPO_LEFT", "EPT_POWER_INPUT")]},
    {"symbol": {"value": "e5000000-0000-4000-8000-000000000051"}, "reference": "U1",
     "lib_id": {"library_nickname": "MCU", "entry_name": "STM32:X"}, "unit": 1,
     "pins": [pin("e5000000-0000-4000-8000-000000000052", "5", "PA1", 12700000, 2540000, "SPO_LEFT")]}]}

SUB_PINS = {"symbols": [
    {"symbol": {"value": "f6000000-0000-4000-8000-000000000061"}, "reference": "U2",
     "lib_id": {"library_nickname": "Power", "entry_name": "LDO"}, "unit": 1,
     "pins": [pin(SUB_PIN, "1", "VIN", 25400000, 25400000, "SPO_RIGHT", "EPT_POWER_INPUT")]}]}


def sheet(*path):
    return {"path": [{"value": value} for value in path]}


NETLIST = {"nets": [
    {"name": "GND", "sheets": [{"path": sheet("00000000-0000-4000-8000-000000000001"),
                                "items": [{"value": U1P4}, {"value": WIRE_ID}]}]},
    {"name": "/SIG", "sheets": [
        {"path": sheet("00000000-0000-4000-8000-000000000001"), "items": [{"value": R1P2}, {"value": U1P3}]},
        {"path": sheet("00000000-0000-4000-8000-000000000001", POWER_PATH), "items": [{"value": SUB_PIN}]}]},
    {"name": "Net-(R1-Pad1)", "sheets": [
        {"path": sheet("00000000-0000-4000-8000-000000000001"), "items": [{"value": R1P1}]},
        # A sheet the hierarchy does not list: its key is joined with "/"
        {"path": sheet("99999999-9999-4999-8999-999999999999"), "items": [{"value": SUB_PIN}]}]},
    # The same name twice: Python's setdefault merges the members, and the later net wins a pin
    {"name": "GND", "sheets": [{"path": sheet("00000000-0000-4000-8000-000000000001"),
                                "items": [{"value": R1P1}]}]},
    {"name": "\u00b5V \u00e9t\u00e9", "sheets": []}]}

FIXED = set(UUID_RE.findall(json.dumps([PINS, SUB_PINS, NETLIST]))) | {"00000000-0000-4000-8000-000000000001"}


def generated(requests, results):
    """Names the UUIDs the tools create, the preview directory and today's date alike."""
    names = {}
    # Only the local date of the run: a server that used another date (UTC, say) must fail.  A run
    # that crosses midnight between the two servers fails visibly; run it again.
    dates = {datetime.date.today().isoformat()}

    def text(value):
        value = UUID_RE.sub(lambda m: m.group(0) if m.group(0) in FIXED
                            else names.setdefault(m.group(0), f"<uuid{len(names) + 1}>"), value)
        value = PREVIEW_RE.sub("kicad-live-preview-<random>", value)
        for date in dates:
            value = value.replace(f'"date": "{date}"', '"date": "<today>"')
        return value
    return json.loads(text(json.dumps(requests))), json.loads(text(json.dumps(results)))


def scenarios(h):
    S, reply, parse, failure = h.S, h.reply, h.parse, h.failure
    pc, editor, types, envelope = h.pyclient, h.editor, h.types, h.envelope
    sc = pc.commands
    DOC, BOARD = h.SHEET_DOC, h.BOARD_DOC
    BEGIN, END_OK, DROP_OK = h.BEGIN, h.END_OK, h.DROP_OK
    status = h.status
    out = []

    def add(name, steps, script, **options):
        out.append(S("schematic_" + name, steps, script, **options))

    def answer(cls, value=None, revision=""):
        return lambda r: reply(parse(cls, value or {}), revision=revision)

    empty = answer(pc.empty())
    pins = ("GetSymbolPins", answer(sc.SymbolPinsResponse, PINS))
    netlist = ("GetSchematicNetlist", answer(sc.SchematicNetlistResponse, NETLIST))
    lost_end = [("EndCommit", lambda r: h.DROP),
                ("GetOperationStatus", status("OPS_COMMITTED", revision_after="fx-3"))]
    bad = lambda message: lambda r: failure(envelope.AS_BAD_REQUEST, message)  # noqa: E731

    # ---- Open, focus, save ------------------------------------------------------------------
    opened = ("OpenDocument", answer(pc.project.OpenDocumentResponse, {"document": DOC}))
    add("open", [("live_schematic_open", {}), ("live_schematic_open", {"path": "C:/fixture/parity/x.kicad_sch"})],
        [opened, ("GetVersion", lambda r: h.version()), ("GetOpenDocuments", lambda r: h.documents(DOC)),
         opened, ("GetVersion", lambda r: h.version()), ("GetOpenDocuments", lambda r: h.documents(DOC))])
    add("open_refused", [("live_schematic_open", {"path": ""})],
        [("OpenDocument", lambda r: failure(envelope.AS_UNHANDLED, "no project manager"))])
    add("open_project_manager_only", [("live_schematic_open", {"path": None})],
        [opened, ("GetVersion", lambda r: h.version()),
         ("GetOpenDocuments", lambda r: failure(envelope.AS_UNHANDLED, "no editor"))])
    add("focus", [("live_schematic_focus", {"document_spec": DOC, "ids": [R1, "not-a-uuid", ""]}),
                  ("live_schematic_focus", {"document_spec": DOC, "ids": []})],
        [("FocusOnItems", empty), ("FocusOnItems", bad("items must be on one sheet"))])
    add("save", [("live_schematic_save", {"document_spec": DOC}), ("live_schematic_save", {"document_spec": DOC})],
        [("SaveDocument", empty), ("SaveDocument", lambda r: failure(envelope.AS_BUSY, "a dialog is open"))])
    add("save_lost_reply", [("live_schematic_save", {"document_spec": DOC})], [("SaveDocument", lambda r: h.DROP)])

    # ---- Place symbol -----------------------------------------------------------------------
    instance = {"@type": "type.googleapis.com/kiapi.schematic.types.SchematicSymbolInstance",
                "id": {"value": R1}, "position": {"x_nm": "101600000", "y_nm": "-50800000"},
                "reference_field": {"text": {"text": "R7"}}, "value_field": {"text": {"text": "10k"}},
                "unit": {"unit": 2}}
    placed = ("PlaceSymbolFromLibrary", answer(pc.library.PlaceFromLibraryResponse,
                                               {"header": {"document": DOC}, "item": instance}, "fx-2"))
    place = {"document_spec": DOC, "lib_id": "Device:R", "x_mm": 101.6, "y_mm": -50.8}
    add("place_symbol", [("live_schematic_place_symbol", dict(place, reference="R7", rotation=90)),
                         ("live_schematic_place_symbol", dict(place, lib_id="Lib:Sub:Part", x_mm="0.0000005",
                                                              reference="", rotation="270"))],
        [BEGIN, placed, END_OK, BEGIN, placed, END_OK])
    add("place_symbol_refusals", [("live_schematic_place_symbol", dict(place, rotation=45)),
                                  ("live_schematic_place_symbol", dict(place, rotation=2 ** 70)),
                                  ("live_schematic_place_symbol", dict(place, lib_id="Device")),
                                  ("live_schematic_place_symbol", dict(place, lib_id=":R")),
                                  ("live_schematic_place_symbol", dict(place, lib_id="Device:", x_mm=1e5)),
                                  ("live_schematic_place_symbol", dict(place, x_mm=10000.01)),
                                  ("live_schematic_place_symbol", dict(place, document_spec=BOARD))], [])
    add("place_symbol_no_instance", [("live_schematic_place_symbol", place)],
        [BEGIN, ("PlaceSymbolFromLibrary", answer(pc.library.PlaceFromLibraryResponse, {
            "item": {"@type": "type.googleapis.com/kiapi.schematic.types.SchematicLine"}})), END_OK])
    add("place_symbol_refused_step", [("live_schematic_place_symbol", place)],
        [BEGIN, ("PlaceSymbolFromLibrary", bad("no such symbol")), DROP_OK])
    add("place_symbol_lost_step", [("live_schematic_place_symbol", place)],
        [BEGIN, ("PlaceSymbolFromLibrary", lambda r: h.DROP), DROP_OK])
    add("place_symbol_lost_end", [("live_schematic_place_symbol", place)], [BEGIN, placed] + lost_end)
    add("place_symbol_lost_end_rolled_back", [("live_schematic_place_symbol", place)],
        [BEGIN, placed, ("EndCommit", lambda r: h.DROP),
         ("GetOperationStatus", status("OPS_ROLLED_BACK", message="lease expired"))])
    add("place_symbol_lost_begin", [("live_schematic_place_symbol", place)],
        [("BeginCommit", lambda r: h.DROP), ("GetOperationStatus", status("OPS_IN_PROGRESS")), DROP_OK])
    add("place_symbol_conflict_at_commit", [("live_schematic_place_symbol", place)],
        [BEGIN, placed, ("EndCommit", lambda r: failure(envelope.AS_REVISION_MISMATCH, "changed"))])

    # ---- View -------------------------------------------------------------------------------
    png = b"\x89PNG\r\n\x1a\n" + bytes(range(256)) * 3

    def rendered(status=1, write=True, message=""):
        def respond(request):
            job = pc.sch_jobs.RunSchematicJobExportPng()
            request.message.Unpack(job)
            if write:
                with open(job.job_settings.output_path, "wb") as handle:
                    handle.write(png)
            result = pc.jobs.RunJobResponse(status=status, message=message)
            result.output_path.append(job.job_settings.output_path)
            return reply(result)
        return ("RunSchematicJobExportPng", respond)

    add("view", [("live_schematic_view", {"document_spec": DOC}),
                 ("live_schematic_view", {"document_spec": DOC, "dpi": 72, "frame": False}),
                 ("live_schematic_view", {"document_spec": DOC, "dpi": "200", "frame": "no"})],
        [rendered(), rendered(status=2, message="warned"), rendered()], normalize=generated)
    add("view_failures", [("live_schematic_view", {"document_spec": DOC, "dpi": 150})] * 3,
        [rendered(status=3, write=False, message="plot 'failed'"), rendered(write=False),
         ("RunSchematicJobExportPng", lambda r: failure(envelope.AS_BUSY, "busy"))], normalize=generated)
    add("view_dpi_refused", [("live_schematic_view", {"document_spec": DOC, "dpi": dpi}) for dpi in (71, 201, 2 ** 64)]
        + [("live_schematic_view", {"document_spec": {"type": "DOCTYPE_SCHEMATIC"}})], [])

    # ---- Page and title block (schematic and board) -----------------------------------------
    def box(x, y, w, h_):
        return {"position": {"x_nm": str(x), "y_nm": str(y)}, "size": {"x_nm": str(w), "y_nm": str(h_)}}

    def item(name, part="", **flags):
        return dict({"id": {"value": R1}, "type": "Symbol", "name": name, "box": box(1234567, -2500, 333333, 1),
                     "part": part}, **flags)

    layout = {"page_size": {"x_nm": "296999950", "y_nm": "209999999"},
              "drawing_area": box(10000000, 10000000, 276999999, 189999999),
              "title_block": box(176000000, 168000000, 110000000, 32000000),
              "content": box(-5000, 0, 0, 1000), "item_count": 7,
              "problems": [item("R1", outside_drawing_area=True), item("U1", overlaps_title_block=True),
                           item("Wire")],
              "overlaps_checked": True, "overlap_count": 3,
              "overlaps": [{"first": item("R1", part="Reference"), "second": item("R2"),
                            "overlap": box(1, 2, 3, 4)},
                           {"first": item("T"), "second": item("U", part="Text: x"), "overlap": {}, "layer": "F.SilkS"}]}
    settings = {"page_size": "PS_A4", "orientation": "PO_LANDSCAPE"}
    block = {"title": "parity - Root", "date": "2025-06-30", "revision": "B", "company": "Embeint Pty Ltd",
             "comment1": "First", "comment3": "Third", "comment5": ""}

    def page_script(layout_value=layout, settings_value=settings, block_value=block):
        return [("GetPageLayout", answer(editor.PageLayoutResponse, layout_value)),
                ("GetPageSettings", answer(types.PageSettings, settings_value)),
                ("GetTitleBlockInfo", answer(types.TitleBlockInfo, block_value))]

    add("page", [("live_schematic_page", {"document_spec": DOC}), ("live_schematic_page", {"document_spec": DOC})],
        page_script() + page_script(
            {"page_size": {"x_nm": "431800000", "y_nm": "279400000"}, "overlaps_checked": True,
             "overlap_count": 0}, {"page_size": "PS_US_LEDGER", "orientation": "PO_PORTRAIT",
                                   "drawing_sheet": "embeint.kicad_wks"}, {}))
    add("page_unchecked_overlaps", [("live_schematic_page", {"document_spec": DOC})],
        page_script({"page_size": {"x_nm": "1", "y_nm": "-1"}, "problems": [item("R1")]}, {}, {"comment9": "last"}))
    add("page_unknown_enum", [("live_schematic_page", {"document_spec": DOC})] * 2,
        page_script(settings_value={"page_size": 99}) + page_script(settings_value={"orientation": 7}))
    add("page_refused", [("live_schematic_page", {"document_spec": DOC})],
        page_script()[:1] + [("GetPageSettings", bad("no page"))])
    board_layout = dict(layout, board_outline=box(20000000, 20000000, 100000000, 80000000))
    add("pcb_page", [("live_pcb_page", {"board_document": BOARD}), ("live_pcb_page", {"board_document": DOC})],
        page_script(board_layout))

    set_settings = [("GetPageSettings", answer(types.PageSettings, dict(settings, drawing_sheet="a.kicad_wks"))),
                    ("SetPageSettings", answer(types.PageSettings, {}, "fx-5"))]
    set_block = [("GetTitleBlockInfo", answer(types.TitleBlockInfo, block)),
                 ("SetTitleBlockInfo", lambda r: reply(pc.empty()(), revision="fx-6"))]
    add("set_page", [("live_schematic_set_page", {"document_spec": DOC, "size": "a3", "orientation": "portrait",
                                                  "title": "parity - Root", "date": "2026-01-02", "revision": "",
                                                  "company": "Embeint Pty Ltd", "comments": ["One", "", "Three"]})],
        set_settings + set_block + page_script())
    add("set_page_today_and_names", [
        ("live_schematic_set_page", {"document_spec": DOC, "date": "today"}),
        ("live_schematic_set_page", {"document_spec": DOC, "size": "us-letter"}),
        ("live_schematic_set_page", {"document_spec": DOC, "size": "ANSI B", "comments": []}),
        ("live_schematic_set_page", {"document_spec": DOC, "size": "User"}),
        ("live_schematic_set_page", {"document_spec": DOC})],
        set_block + page_script() + set_settings + page_script() + set_settings + set_block + page_script()
        + set_settings + page_script() + page_script(), normalize=generated)
    add("set_page_refusals", [
        ("live_schematic_set_page", {"document_spec": DOC, "size": "B5"}),
        ("live_schematic_set_page", {"document_spec": DOC, "size": "A4", "orientation": "Landscape"}),
        ("live_schematic_set_page", {"document_spec": DOC, "comments": [str(n) for n in range(10)]}),
        ("live_schematic_set_page", {"document_spec": DOC, "orientation": "landscape", "title": "x"})],
        set_settings[:1] + set_settings[:1] + set_block[:1]
        + [set_settings[0], ("SetPageSettings", bad("read-only drawing sheet"))])
    add("set_page_lost_replies", [
        ("live_schematic_set_page", {"document_spec": DOC, "size": "A2"}),
        ("live_schematic_set_page", {"document_spec": DOC, "title": "t"}),
        ("live_schematic_set_page", {"document_spec": DOC, "size": "A1"})],
        [set_settings[0], ("SetPageSettings", lambda r: h.DROP),
         ("GetOperationStatus", status("OPS_COMMITTED", revision_after="fx-7"))] + page_script()
        + [set_block[0], ("SetTitleBlockInfo", lambda r: h.DROP), ("GetOperationStatus", status("OPS_UNKNOWN"))]
        + [set_settings[0], ("SetPageSettings", lambda r: h.DROP),
           ("GetOperationStatus", lambda r: failure(envelope.AS_BUSY, "busy"))])
    # Not atomic, as in Python: the page change commits, then the title block fails
    add("set_page_partial", [
        ("live_schematic_set_page", {"document_spec": DOC, "size": "A3", "comments": [str(n) for n in range(10)]}),
        ("live_schematic_set_page", {"document_spec": DOC, "size": "A3", "title": "t"})],
        set_settings + set_block[:1] + set_settings + [set_block[0], ("SetTitleBlockInfo", bad("locked"))])
    add("pcb_set_page", [("live_pcb_set_page", {"board_document": BOARD, "size": "A4", "company": "Embeint Pty Ltd"}),
                         ("live_pcb_set_page", {"board_document": {"type": "DOCTYPE_PCB"}, "size": "A4"})],
        set_settings + set_block + page_script(board_layout))

    # ---- Annotate, update from library, autoplace, move -------------------------------------
    annotated = ("AnnotateSchematic", answer(sc.AnnotateSchematicResponse, {"changes": [
        {"symbol": {"value": R1}, "old_reference": "R?", "new_reference": "R1"},
        {"symbol": {"value": U1}, "path": sheet(R1), "old_reference": "U3", "new_reference": "U1"}]}, "fx-2"))
    add("annotate", [("live_schematic_annotate", {"document_spec": DOC}),
                     ("live_schematic_annotate", {"document_spec": DOC, "scope": "symbols",
                                                  "symbols": ["{A1000000-0000-4000-8000-000000000001}",
                                                              "urn:uuid:b2000000000040008000000000000002", ""],
                                                  "order": "unsorted", "numbering": "sheet_x1000",
                                                  "reset_existing": True, "recursive": False, "start_number": -5}),
                     ("live_schematic_annotate", {"document_spec": DOC, "scope": "sheet", "order": "y",
                                                  "numbering": "sheet_x100", "start_number": 2 ** 31 - 1})],
        [BEGIN, annotated, END_OK, BEGIN, ("AnnotateSchematic", answer(sc.AnnotateSchematicResponse, {}, "fx-2")),
         END_OK, BEGIN, annotated, END_OK], normalize=generated)
    add("annotate_refusals", [
        ("live_schematic_annotate", {"document_spec": DOC, "scope": "page"}),
        ("live_schematic_annotate", {"document_spec": DOC, "order": "X"}),
        ("live_schematic_annotate", {"document_spec": DOC, "numbering": ""}),
        ("live_schematic_annotate", {"document_spec": DOC, "scope": "symbols"}),
        ("live_schematic_annotate", {"document_spec": DOC, "scope": "symbols", "symbols": []}),
        ("live_schematic_annotate", {"document_spec": DOC, "start_number": 2 ** 31}),
        ("live_schematic_annotate", {"document_spec": DOC, "start_number": -2 ** 31 - 1}),
        ("live_schematic_annotate", {"document_spec": DOC, "start_number": 2 ** 70, "symbols": ["bad"]}),
        ("live_schematic_annotate", {"document_spec": DOC, "symbols": ["bad"]}),
        ("live_schematic_annotate", {"document_spec": DOC, "symbols": ["g" * 32]})], [])
    add("annotate_refused_step", [("live_schematic_annotate", {"document_spec": DOC})],
        [BEGIN, ("AnnotateSchematic", lambda r: failure(envelope.AS_BUSY, "busy")), DROP_OK])
    add("annotate_unconfirmed_rollback", [("live_schematic_annotate", {"document_spec": DOC})],
        [BEGIN, ("AnnotateSchematic", bad("x")), ("EndCommit", bad("no commit"))])
    add("annotate_lost_end", [("live_schematic_annotate", {"document_spec": DOC})], [BEGIN, annotated] + lost_end)
    add("annotate_lost_end_in_progress", [("live_schematic_annotate", {"document_spec": DOC})],
        [BEGIN, annotated, ("EndCommit", lambda r: h.DROP), ("GetOperationStatus", status("OPS_IN_PROGRESS")),
         DROP_OK, ("GetOperationStatus", status("OPS_ROLLED_BACK"))])

    updated = ("UpdateSymbolsFromLibrary", answer(sc.UpdateSymbolsFromLibraryResponse, {"symbols": [
        {"symbol": {"value": R1}, "lib_id": {"library_nickname": "Device", "entry_name": "R"}, "updated": True},
        {"symbol": {"value": U1}, "lib_id": {"entry_name": "X"}, "error_message": "not found"}]}, "fx-2"))
    add("update_from_library", [
        ("live_schematic_update_from_library", {"document_spec": DOC}),
        ("live_schematic_update_from_library", {"document_spec": DOC, "symbols": [R1], "new_lib_id": "Device:R_Small",
                                                "reset_field_text": True, "reset_field_visibility": True,
                                                "reset_field_positions": True, "remove_extra_fields": True}),
        ("live_schematic_update_from_library", {"document_spec": DOC, "new_lib_id": ""})],
        [BEGIN, updated, END_OK, BEGIN, updated, END_OK, BEGIN, updated, END_OK])
    add("update_from_library_refusals", [
        ("live_schematic_update_from_library", {"document_spec": DOC, "new_lib_id": "R_Small"}),
        ("live_schematic_update_from_library", {"document_spec": DOC, "symbols": ["x"], "new_lib_id": "bad"})], [])
    add("update_from_library_lost_begin", [("live_schematic_update_from_library", {"document_spec": DOC})],
        [("BeginCommit", lambda r: h.DROP), ("GetOperationStatus", status("OPS_REJECTED", message="stale"))])

    add("update_from_library_step_refused_and_lost_end", [
        ("live_schematic_update_from_library", {"document_spec": DOC}),
        ("live_schematic_update_from_library", {"document_spec": DOC, "symbols": [R1]})],
        [BEGIN, ("UpdateSymbolsFromLibrary", bad("library not loaded")), h.DROP_OK, BEGIN, updated] + lost_end)
    add("autoplace", [("live_schematic_autoplace_fields", {"document_spec": DOC}),
                      ("live_schematic_autoplace_fields", {"document_spec": DOC, "ids": [R1, U1]}),
                      ("live_schematic_autoplace_fields", {"document_spec": DOC, "ids": ["nope"]})],
        [BEGIN, ("AutoplaceFields", lambda r: reply(pc.empty()(), revision="fx-2")), END_OK,
         BEGIN, ("AutoplaceFields", lambda r: reply(pc.empty()(), revision="fx-2"))] + lost_end)

    moved = ("TransformItems", answer(sc.TransformItemsResponse,
                                      {"dragged_items": [{"value": WIRE_ID}, {"value": "bend"}]}, "fx-2"))
    add("move", [("live_schematic_move", {"document_spec": DOC, "ids": [R1]}),
                 ("live_schematic_move", {"document_spec": DOC, "ids": [R1, U1], "dx_mm": 2.54, "dy_mm": "-1.27",
                                          "rotation": -90, "mirror_horizontally": True, "mirror_vertically": 1,
                                          "drag_wires": False}),
                 ("live_schematic_move", {"document_spec": DOC, "ids": [U1], "dx_mm": 0.0000015, "rotation": 180}),
                 ("live_schematic_move", {"document_spec": DOC, "ids": [U1], "dy_mm": -0.0000025, "rotation": 90})],
        [BEGIN, moved, END_OK] * 4)
    add("move_lost_replies", [
        ("live_schematic_move", {"document_spec": DOC, "ids": [R1], "dx_mm": 1}),
        ("live_schematic_move", {"document_spec": DOC, "ids": [R1], "dx_mm": 1})],
        [BEGIN, moved] + lost_end + [("BeginCommit", lambda r: h.DROP),
                                     ("GetOperationStatus", status("OPS_IN_PROGRESS")), h.DROP_OK])
    add("move_refusals", [
        ("live_schematic_move", {"document_spec": DOC, "ids": [R1], "rotation": 45}),
        ("live_schematic_move", {"document_spec": DOC, "ids": [R1], "rotation": 360}),
        ("live_schematic_move", {"document_spec": DOC, "ids": []}),
        ("live_schematic_move", {"document_spec": DOC, "ids": [R1] * 201}),
        ("live_schematic_move", {"document_spec": DOC, "ids": [R1], "dx_mm": -10001}),
        ("live_schematic_move", {"document_spec": DOC, "ids": ["x"], "dx_mm": 20000}),
        ("live_schematic_move", {"document_spec": DOC, "ids": ["x"]})], [])
    add("move_rejected", [("live_schematic_move", {"document_spec": DOC, "ids": [R1] * 200})],
        [BEGIN, ("TransformItems", bad("locked item")), DROP_OK])

    # ---- Undo and redo (schematic and board) ------------------------------------------------
    undone = lambda r: reply(parse(editor.UndoRedoResponse, {"performed": True, "undo_count": 3,  # noqa: E731
                                                             "redo_count": 1}), revision="fx-8")
    add("undo", [("live_schematic_undo", {"document_spec": DOC}),
                 ("live_schematic_undo", {"document_spec": DOC, "redo": True, "expected_revision": "fx-8",
                                          "operation_id": "op-undo"}),
                 ("live_schematic_undo", {"document_spec": DOC, "operation_id": ""})],
        [("UndoLastChange", undone), ("RedoLastChange", answer(editor.UndoRedoResponse, {}, "fx-9")),
         ("UndoLastChange", lambda r: reply(editor.UndoRedoResponse()))])
    for state in ("OPS_COMMITTED", "OPS_IN_PROGRESS", "OPS_UNKNOWN"):
        add("undo_lost_" + state, [("live_schematic_undo", {"document_spec": DOC, "expected_revision": "fx-1",
                                                            "operation_id": "op-undo"})],
            [("UndoLastChange", lambda r: h.DROP), ("GetOperationStatus", status(state, revision_after="fx-3"))])
    add("undo_lost_status_refused", [("live_schematic_undo", {"document_spec": DOC})],
        [("UndoLastChange", lambda r: h.CLOSE)])
    add("undo_refusals", [("live_schematic_undo", {"document_spec": DOC, "expected_revision": "old"}),
                          ("live_schematic_undo", {"document_spec": DOC, "operation_id": "used"}),
                          ("live_schematic_undo", {"document_spec": DOC}),
                          ("live_schematic_undo", {"document_spec": BOARD})],
        [("UndoLastChange", lambda r: failure(envelope.AS_REVISION_MISMATCH, "document changed", revision="fx-2")),
         ("UndoLastChange", lambda r: failure(envelope.AS_DUPLICATE_OPERATION, "already ran")),
         ("UndoLastChange", lambda r: failure(envelope.AS_BUSY, "a commit is open"))])
    add("pcb_undo_refused_and_in_progress", [
        ("live_pcb_undo", {"board_document": BOARD, "expected_revision": "old"}),
        ("live_pcb_undo", {"board_document": BOARD})],
        [("UndoLastChange", lambda r: failure(envelope.AS_REVISION_MISMATCH, "document changed", revision="fx-2")),
         ("UndoLastChange", lambda r: h.DROP), ("GetOperationStatus", status("OPS_IN_PROGRESS"))])
    add("pcb_undo", [("live_pcb_undo", {"board_document": BOARD}),
                     ("live_pcb_undo", {"board_document": BOARD, "redo": "true", "operation_id": "op-pcb"}),
                     ("live_pcb_undo", {"board_document": DOC})],
        [("UndoLastChange", undone), ("RedoLastChange", lambda r: h.DROP),
         ("GetOperationStatus", status("OPS_COMMITTED", revision_after="fx-4"))])

    # ---- Pins, netlist and connections ------------------------------------------------------
    add("symbol_pins", [("live_schematic_symbol_pins", {"document_spec": DOC}),
                        ("live_schematic_symbol_pins", {"document_spec": DOC, "symbols": [R1.upper(), ""]})],
        [pins, netlist, ("GetSymbolPins", answer(sc.SymbolPinsResponse, {})), netlist], normalize=generated)
    unknown_pin = {"symbols": [{"reference": "R1", "pins": [{"number": "1", "orientation": 9}]}]}
    add("symbol_pins_unknown_enum", [("live_schematic_symbol_pins", {"document_spec": DOC}),
                                     ("live_schematic_connect_pins", {"document_spec": DOC, "from_reference": "U1",
                                                                      "from_pin": "3", "to_reference": "R1",
                                                                      "to_pin": "2"})],
        [("GetSymbolPins", answer(sc.SymbolPinsResponse, unknown_pin)), netlist,
         ("GetSymbolPins", answer(sc.SymbolPinsResponse, {"symbols": [{"pins": [{"electrical_type": 77}]}]}))])
    add("symbol_pins_refusals", [("live_schematic_symbol_pins", {"document_spec": DOC, "symbols": ["R1"]})], [])
    add("netlist", [("live_schematic_netlist", {"document_spec": DOC}),
                    ("live_schematic_netlist", {"document_spec": DOC, "scope": "sheet"})],
        [netlist, ("GetSchematicHierarchy", lambda r: reply(h.HIERARCHY_REPLY)),
         pins, ("GetSymbolPins", answer(sc.SymbolPinsResponse, SUB_PINS)), netlist, pins])
    add("netlist_refusals", [("live_schematic_netlist", {"document_spec": DOC, "scope": "Sheet"})], [])

    connect = {"document_spec": DOC, "from_reference": "U1", "from_pin": "PA0", "to_reference": "R1", "to_pin": "2"}
    add("connect_pins", [
        ("live_schematic_connect_pins", connect),  # leaves U1.3 horizontally, then turns
        ("live_schematic_connect_pins", dict(connect, from_reference="R1", from_pin="1", to_reference="U1",
                                             to_pin="GND")),  # leaves R1.1 vertically
        ("live_schematic_connect_pins", dict(connect, to_reference="U1", to_pin="4")),  # one segment
        # The first U1 (unit 2) has no pin 5: the search goes on to the next symbol named U1
        ("live_schematic_connect_pins", dict(connect, from_pin="5"))],
        [pins, pins, BEGIN, h.created(1, 1), END_OK, pins, pins, BEGIN, h.created(1, 1), END_OK,
         pins, pins, BEGIN, h.created(1), END_OK, pins, pins, BEGIN, h.created(1, 1), END_OK], normalize=generated)
    add("connect_pins_lost_end", [("live_schematic_connect_pins", connect)],
        [pins, pins, BEGIN, h.created(1, 1), ("EndCommit", lambda r: h.DROP),
         ("GetOperationStatus", status("OPS_ROLLED_BACK"))], normalize=generated)
    add("connect_pins_refusals", [
        ("live_schematic_connect_pins", dict(connect, to_reference="U1", to_pin="3")),
        ("live_schematic_connect_pins", dict(connect, from_reference="R9")),
        ("live_schematic_connect_pins", dict(connect, to_pin="5"))],
        [pins, pins, pins, pins, pins])
    origin = {"symbols": [{"reference": "A", "pins": [{"number": "1", "position": {"x_nm": "1"}}]},
                          {"reference": "B", "pins": [{"number": "1", "position": {"x_nm": "2"}}]}]}
    add("connect_pins_unknown_orientation", [
        ("live_schematic_connect_pins", dict(connect, from_reference="A", from_pin="1", to_reference="B",
                                             to_pin="1"))],
        [("GetSymbolPins", answer(sc.SymbolPinsResponse, origin))] * 2)
    add("connect_pins_rejected", [("live_schematic_connect_pins", connect)],
        [pins, pins, BEGIN, h.created(1, editor.ISC_INVALID_DATA, message="overlaps a wire"), DROP_OK],
        normalize=generated)
    far = {"symbols": [{"reference": "A", "pins": [{"number": "1", "orientation": "SPO_LEFT",
                                                     "position": {"x_nm": "10000000001"}}]},
                       {"reference": "B", "pins": [{"number": "1", "orientation": "SPO_LEFT"}]}]}
    add("connect_pins_off_page", [
        ("live_schematic_connect_pins", dict(connect, from_reference="A", from_pin="1", to_reference="B",
                                             to_pin="1"))],
        [("GetSymbolPins", answer(sc.SymbolPinsResponse, far))] * 2)

    net = {"document_spec": DOC, "reference": "R1", "pin": "1", "net": "/SIG \u00b1"}
    add("connect_pin_to_net", [
        ("live_schematic_connect_pin_to_net", net),
        ("live_schematic_connect_pin_to_net", dict(net, pin="2", label="global", stub_mm=1.27)),
        ("live_schematic_connect_pin_to_net", dict(net, reference="U1", pin="PA0", label="hierarchical",
                                                   stub_mm="0.1")),
        ("live_schematic_connect_pin_to_net", dict(net, reference="U1", pin="GND", stub_mm=50, net=""))],
        [pins, BEGIN, h.created(1, 1), END_OK] * 4, normalize=generated)
    add("connect_pin_to_net_refusals", [
        ("live_schematic_connect_pin_to_net", dict(net, label="Local")),
        ("live_schematic_connect_pin_to_net", dict(net, stub_mm=0)),
        ("live_schematic_connect_pin_to_net", dict(net, stub_mm=-1)),
        ("live_schematic_connect_pin_to_net", dict(net, stub_mm=50.0001)),
        ("live_schematic_connect_pin_to_net", dict(net, pin="9"))], [pins])
    add("connect_pin_to_net_lost_end", [("live_schematic_connect_pin_to_net", net)],
        [pins, BEGIN, h.created(1, 1), ("EndCommit", lambda r: h.DROP),
         ("GetOperationStatus", lambda r: failure(envelope.AS_BUSY, "busy"))], normalize=generated)

    # ---- ERC, selection, highlighting -------------------------------------------------------
    task = pc.tasks
    started = ("StartErcTask", answer(task.StartTaskResponse, {"token": {"value": "erc-1"}}))

    def task_state(state, error=""):
        return ("GetTaskStatus", answer(task.TaskStatusResponse, {"state": state, "error_message": error}))

    erc_result = ("GetErcTaskResult", answer(pc.sch_tasks.ErcTaskResult, {
        "violations": [{"error_type": "ERCET_PIN_NOT_CONNECTED", "position": {"x_nm": "101600000", "y_nm": "-1"},
                        "items": [{"value": R1P1}], "description": "Pin not connected", "severity": "RS_ERROR"},
                       {"error_type": "ERCET_ENDPOINT_OFF_GRID", "description": "off grid",
                        "sheet_specific_path": sheet("00000000-0000-4000-8000-000000000001")}],
        "ignored_count": 2, "excluded_count": 1}))
    add("erc", [("live_schematic_erc", {"document_spec": DOC}), ("live_schematic_erc", {"document_spec": DOC})],
        [started, task_state("TS_RUNNING"), task_state("TS_UNKNOWN"), task_state("TS_COMPLETED"), erc_result,
         started, task_state("TS_COMPLETED"), ("GetErcTaskResult", answer(pc.sch_tasks.ErcTaskResult, {}))])
    add("erc_failures", [("live_schematic_erc", {"document_spec": DOC})] * 4,
        [started, task_state("TS_FAILED", "no schematic"), started, task_state("TS_CANCELLED"),
         ("StartErcTask", bad("busy")), started, task_state("TS_COMPLETED"), ("GetErcTaskResult", bad("gone"))])
    # A state Python does not know keeps the poll going, like TS_RUNNING
    add("erc_unknown_state", [("live_schematic_erc", {"document_spec": DOC})],
        [started, ("GetTaskStatus", answer(task.TaskStatusResponse, {"state": 9})), ("GetTaskStatus", bad("stop"))])

    def selection(*types_):
        def respond(request):
            result = editor.SelectionResponse()
            for name in types_:
                result.items.add().Pack(getattr(pc.sch, name)())
            return reply(result)
        return ("AddToSelection", respond)

    add("select", [("live_schematic_select", {"document_spec": DOC, "ids": [R1, U1.upper()]}),
                   ("live_schematic_select", {"document_spec": DOC, "ids": [R1], "add": True}),
                   ("live_schematic_select", {"document_spec": DOC, "ids": []}),
                   ("live_schematic_select", {"document_spec": DOC, "ids": [], "add": True}),
                   ("live_schematic_select", {"document_spec": DOC, "ids": ["bad"]}),
                   ("live_schematic_select", {"document_spec": DOC, "ids": [R1]})],
        [("ClearSelection", empty), selection("SchematicSymbolInstance", "LocalLabel"), selection(),
         ("ClearSelection", empty), ("ClearSelection", empty),
         ("ClearSelection", bad("no editor"))])
    add("highlight_nets", [("live_schematic_highlight_nets", {"document_spec": DOC, "nets": ["GND", "/SIG"]}),
                           ("live_schematic_highlight_nets", {"document_spec": DOC, "nets": []}),
                           ("live_schematic_highlight_nets", {"document_spec": BOARD, "nets": []})],
        [("HighlightNets", answer(pc.cross_probe.HighlightNetsResponse, {"status": "CPS_OK"})),
         ("HighlightNets", answer(pc.cross_probe.HighlightNetsResponse,
                                  {"status": "CPS_NOT_FOUND", "message": "no such net"}))])

    # ---- Argument validation (pydantic's wording is not compared) ---------------------------
    add("validation", [
        ("live_schematic_place_symbol", {"document_spec": DOC, "lib_id": "Device:R", "x_mm": "x", "y_mm": 1}),
        ("live_schematic_place_symbol", {"document_spec": DOC, "lib_id": "Device:R", "x_mm": 1, "y_mm": 1,
                                         "rotation": 1.5}),
        ("live_schematic_view", {"document_spec": DOC, "dpi": "high"}),
        ("live_schematic_view", {"document_spec": DOC, "frame": "maybe"}),
        ("live_schematic_set_page", {"document_spec": DOC, "comments": "A, B"}),
        ("live_schematic_focus", {"document_spec": DOC}),
        ("live_schematic_annotate", {"document_spec": DOC, "symbols": [1]}),
        ("live_schematic_move", {"document_spec": DOC, "ids": [R1], "dx_mm": [1]}),
        ("live_schematic_undo", {"document_spec": [DOC]}),
        ("live_pcb_undo", {}),
        ("live_pcb_page", {"board_document": "board"}),
        ("live_pcb_set_page", {"board_document": BOARD, "size": 4}),
        ("live_schematic_connect_pins", {"document_spec": DOC, "from_reference": "R1"}),
        ("live_schematic_connect_pin_to_net", {"document_spec": DOC, "reference": "R1", "pin": 1, "net": "N"}),
        ("live_schematic_netlist", {"document_spec": DOC, "scope": None}),
        ("live_schematic_select", {"document_spec": DOC, "ids": "R1"}),
        ("live_schematic_highlight_nets", {"document_spec": DOC, "nets": None}),
        ("live_schematic_open", {"path": 5}),
        ("live_schematic_symbol_pins", {"document_spec": DOC, "symbols": {"a": 1}}),
        ("live_schematic_update_from_library", {"document_spec": DOC, "reset_field_text": "sometimes"}),
        ("live_schematic_autoplace_fields", {"document_spec": None}),
        ("live_schematic_erc", {"document_spec": 1}),
        ("live_schematic_save", {}),
        ("live_schematic_page", {"document_spec": "not json"})], [], compare="error_prefix")
    add("document_refusals", [
        (tool, {"document_spec": spec}) for tool in ("live_schematic_page", "live_schematic_erc",
                                                     "live_schematic_save", "live_schematic_netlist")
        for spec in (BOARD, {"type": "DOCTYPE_SCHEMATIC", "project": {"name": "p"}}, {"bogus": 1})] + [
        ("live_schematic_set_page", {"document_spec": h.SYMBOL_DOC, "size": "A4"}),
        ("live_schematic_undo", {"document_spec": h.FOOTPRINT_DOC}),
        ("live_pcb_set_page", {"board_document": DOC, "title": "x"}),
        ("live_pcb_undo", {"board_document": dict(BOARD, board_filename="")})], [])
    # JSON text for objects and lists, as some clients send them
    add("args_as_json_text", [
        ("live_schematic_set_page", {"document_spec": json.dumps(DOC), "comments": json.dumps(["a", "b"])}),
        ("live_schematic_move", {"document_spec": json.dumps(DOC), "ids": json.dumps([R1]), "dx_mm": "1"})],
        set_block + page_script() + [BEGIN, moved, END_OK])
    return out
