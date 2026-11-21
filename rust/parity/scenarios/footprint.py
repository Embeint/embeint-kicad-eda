# SPDX-License-Identifier: GPL-3.0-or-later
"""Parity scenarios for the footprint area: the Footprint Editor tools, footprint generation,
embedded files, 3D model plans and checks, and live_kicad_cli (rust/embeint-mcp/src/tools/footprint.rs).

The kicad-cli tools run a fake kicad-cli that prepare() writes into the scenario's <out> folder
(install/bin/kicad-cli.cmd on Windows, a /bin/sh script elsewhere).  It is test fixture code:
by its arguments it echoes them and which variables are set, fails with stderr, prints more than
the output limit, sleeps past a timeout, or plays kicad-cli's render and STEP export by copying a
fixture PNG/STEP to the -o path and the scratch board it was given into <out>, so the harness
compares the board both servers wrote byte for byte.  It never prints a resolved path: the two
servers' runs use different temporary folders.
"""

import base64
import os
import stat
import sys
from pathlib import Path

WINDOWS = sys.platform == "win32"
CLI_NAME = "kicad-cli.cmd" if WINDOWS else "kicad-cli"

FAKE_CMD = r"""@echo off
setlocal
set "HERE=%~dp0"
set "OUT=%~dp0..\..\"
if "%~1"=="pcb" if "%~2"=="render" goto render
if "%~1"=="pcb" if "%~2"=="export" if "%~3"=="step" goto step
if "%~1"=="pcb" if "%~2"=="drc" goto fail
if "%~1"=="pcb" if "%~2"=="export" if "%~3"=="svg" goto slow
if "%~1"=="sch" if "%~2"=="export" goto big
if "%~3"=="cwd-probe" echo marker> cwd-marker.txt
echo kicad-cli fixture
:args
if "%~1"=="" goto env
echo arg %~1
shift
goto args
:env
if defined KICAD_CONFIG_HOME echo KICAD_CONFIG_HOME is set
if defined KICAD10_SYMBOL_DIR echo KICAD10_SYMBOL_DIR is set
if defined FIXTURE_VARIABLE echo FIXTURE_VARIABLE=%FIXTURE_VARIABLE%
>&2 echo fixture stderr
exit /b 0
:render
if exist "%OUT%fail-render" goto renderfail
call :find %*
copy /y "%OUT%fixture.png" "%OUTPUT%" >nul
copy /y "%BOARD%" "%OUT%render-board.kicad_pcb" >nul
exit /b 0
:renderfail
>&2 echo render exploded
exit /b 3
:step
call :find %*
copy /y "%OUT%fixture.step" "%OUTPUT%" >nul
copy /y "%BOARD%" "%OUT%step-board.kicad_pcb" >nul
exit /b 0
:fail
echo some output
>&2 echo drc failed badly
exit /b 5
:slow
ping -n 4 127.0.0.1 >nul 2>nul
exit /b 0
:big
for /L %%i in (1,1,2000) do @echo 0123456789abcdef
exit /b 0
:find
if "%~1"=="" goto :eof
if "%~1"=="-o" set "OUTPUT=%~2"
set "BOARD=%~1"
shift
goto find
"""

FAKE_SH = """#!/bin/sh
OUT="$(dirname "$0")/../.."
find_args() { while [ $# -gt 0 ]; do [ "$1" = -o ] && OUTPUT=$2; BOARD=$1; shift; done; }
if [ "$1" = pcb ] && [ "$2" = render ]; then
  if [ -e "$OUT/fail-render" ]; then echo render exploded >&2; exit 3; fi
  find_args "$@"; cp "$OUT/fixture.png" "$OUTPUT"; cp "$BOARD" "$OUT/render-board.kicad_pcb"; exit 0
elif [ "$1" = pcb ] && [ "$2" = export ] && [ "$3" = step ]; then
  find_args "$@"; cp "$OUT/fixture.step" "$OUTPUT"; cp "$BOARD" "$OUT/step-board.kicad_pcb"; exit 0
elif [ "$1" = pcb ] && [ "$2" = drc ]; then
  echo some output; echo drc failed badly >&2; exit 5
elif [ "$1" = pcb ] && [ "$2" = export ] && [ "$3" = svg ]; then
  sleep 3; exit 0
elif [ "$1" = sch ] && [ "$2" = export ]; then
  i=0; while [ $i -lt 2000 ]; do echo 0123456789abcdef; i=$((i+1)); done; exit 0
fi
[ "$3" = cwd-probe ] && echo marker > cwd-marker.txt
echo kicad-cli fixture
for a in "$@"; do echo "arg $a"; done
[ -n "$KICAD_CONFIG_HOME" ] && echo KICAD_CONFIG_HOME is set
[ -n "$KICAD10_SYMBOL_DIR" ] && echo KICAD10_SYMBOL_DIR is set
[ -n "$FIXTURE_VARIABLE" ] && echo "FIXTURE_VARIABLE=$FIXTURE_VARIABLE"
echo fixture stderr >&2
exit 0
"""

# The model KiCad has embedded, as stored in the saved footprint, and kicad-cli's STEP export
MODEL_STEP = b"ISO-10303-21;\r\nHEADER;\r\nFILE_NAME('R.step');\r\nENDSEC;\r\n" + bytes(range(32, 127)) * 30
EXPORT_STEP = (b"ISO-10303-21;\r\nDATA;\r\n#1=MANIFOLD_SOLID_BREP('a',#3);\r\n#2=MANIFOLD_SOLID_BREP('b',#4);\r\n"
               b"#5=SHELL;\r\nENDSEC;\r\nEND-ISO-10303-21;\r\n")
PNG = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDRfixture-render"


def scenarios(h):
    import zstandard
    from google.protobuf import empty_pb2
    import footprint_client as fc

    bt, bc, base, library, lib_types = fc.bt, fc.bc, fc.base, fc.library, fc.lib_types
    editor, types, reply, parse, failure, S = h.editor, h.types, h.reply, h.parse, h.failure, h.S
    envelope = h.envelope
    DOC, SYMBOL = h.FOOTPRINT_DOC, h.SYMBOL_DOC
    BEGIN, END_OK, DROP_OK = h.BEGIN, h.END_OK, h.DROP_OK
    state = {}

    def empty(request):
        return reply(empty_pb2.Empty())

    OPEN = ("OpenFootprintEditor", empty)

    def any_of(type_name, value):
        return {"@type": f"type.googleapis.com/{type_name}", **value}

    def items(*packed):
        return lambda r: reply(parse(editor.GetItemsResponse, {"status": "IRS_OK", "items": list(packed)}))

    def updated(request):
        return reply(editor.UpdateItemsResponse(status=types.IRS_OK))

    def created(reject=None):
        def respond(request):
            create = editor.CreateItems()
            request.message.Unpack(create)
            result = editor.CreateItemsResponse(status=types.IRS_OK)
            for index, packed in enumerate(create.items):
                entry = result.created_items.add()
                if reject and index in reject:
                    entry.status.code, entry.status.error_message = reject[index]
                else:
                    entry.status.code = editor.ISC_OK
                entry.item.CopyFrom(packed)
            return reply(result, revision="fx-2")
        return ("CreateItems", respond)

    def deleted(request):
        return reply(editor.DeleteItemsResponse(status=types.IRS_OK))

    def file_info(**fields):
        return parse(editor.EmbeddedFileInfo, fields)

    # Items in an open footprint: KiCad's default ${REFERENCE} fab text, the two fields, a pad,
    # graphics and a 3D-less group KiCad might hold
    REF_TEXT = any_of("kiapi.board.types.BoardText", {
        "id": {"value": "aaaaaaaa-0000-1000-8000-000000000001"}, "layer": "BL_F_Fab",
        "text": {"text": "${REFERENCE}", "position": {"x_nm": "0", "y_nm": "1000000"},
                 "attributes": {"font_name": "KiCad Font", "italic": True}}})
    USER_TEXT = any_of("kiapi.board.types.BoardText", {
        "id": {"value": "aaaaaaaa-0000-1000-8000-000000000002"}, "layer": "BL_F_SilkS", "text": {"text": "note"}})
    REFERENCE = any_of("kiapi.board.types.Field", {
        "id": {"id": 1}, "name": "Reference", "visible": False,
        "text": {"id": {"value": "aaaaaaaa-0000-1000-8000-000000000003"}, "layer": "BL_F_SilkS",
                 "text": {"text": "REF**", "position": {"x_nm": "0", "y_nm": "-1000000"}}}})
    VALUE = any_of("kiapi.board.types.Field", {
        "id": {"id": 2}, "name": "Value",
        "text": {"layer": "BL_F_Fab", "text": {"text": "R_0603", "attributes": {"bold": True}}}})
    DATASHEET = any_of("kiapi.board.types.Field", {"id": {"id": 3}, "name": "Datasheet"})
    PAD = any_of("kiapi.board.types.Pad", {
        "id": {"value": "aaaaaaaa-0000-1000-8000-000000000004"}, "number": "1", "type": "PT_SMD",
        "position": {"x_nm": "-825000", "y_nm": "0"},
        "pad_stack": {"copper_layers": [{"layer": "BL_F_Cu", "shape": "PSS_ROUNDRECT",
                                         "size": {"x_nm": "800000", "y_nm": "950000"}}]}})
    PAD_BARE = any_of("kiapi.board.types.Pad", {"number": "2", "type": "PT_NPTH"})
    LINE = any_of("kiapi.board.types.BoardGraphicShape", {
        "id": {"value": "aaaaaaaa-0000-1000-8000-000000000005"}, "layer": "BL_F_Fab",
        "shape": {"segment": {"start": {"x_nm": "1"}, "end": {"x_nm": "2"}}}})
    LINE_NO_ID = any_of("kiapi.board.types.BoardGraphicShape", {"layer": "BL_F_SilkS"})
    COURTYARD = any_of("kiapi.board.types.BoardGraphicShape", {
        "id": {"value": "aaaaaaaa-0000-1000-8000-000000000006"}, "layer": "BL_F_CrtYd"})
    ZONE = any_of("kiapi.board.types.Zone", {"id": {"value": "aaaaaaaa-0000-1000-8000-000000000007"}})

    def render_reply(request):
        return reply(bc.RenderLibraryFootprintResponse(image=b"\x89PNG fixture image", mime_type="image/png"))

    def table(*rows):
        def respond(request):
            listed = [dict(row, path=str(state["out"] / row["path"][len("<out>/"):]))
                      if row["path"].startswith("<out>/") else row for row in rows]
            return reply(library.LibraryTableResponse(rows=[
                lib_types.LibraryTableEntry(nickname=row["nickname"], uri=row["path"], type=lib_types.LT_FOOTPRINT,
                                            scope=lib_types.LibraryTableScope.Value(row["scope"]),
                                            writable=row.get("writable", False))
                for row in listed]))
        return ("GetLibraryTable", respond)

    LIB_TABLE = table({"nickname": "Other", "path": "/elsewhere/Other.pretty", "scope": "LTS_GLOBAL"},
                      {"nickname": "Lib", "path": "<out>/Lib.pretty", "scope": "LTS_PROJECT", "writable": True})

    def library_status(code="LCS_OK", message=None):
        status = lib_types.LibraryCommandStatus(code=lib_types.LibraryCommandStatus.Code.Value(code))
        if message is not None:
            status.error_message = message
        return lambda r: reply(status)

    # ---- the fake kicad-cli and the saved library ------------------------------------------

    def footprint_file():
        compressed = base64.b64encode(zstandard.ZstdCompressor(level=3).compress(MODEL_STEP)).decode()
        chunks = " ".join(f"|{compressed[i:i + 60]}|" for i in range(0, len(compressed), 60))
        notes = base64.b64encode(zstandard.ZstdCompressor().compress(b"  not a model")).decode()
        return ('(footprint "R_0603"\r\n\t(version 20241229)\r\n\t(generator "pcbnew")\r\n'
                '\t(generator_version "9.0")\r\n\t(layer "F.Cu")\r\n'
                '\t(property "Reference" "REF**" (at 0 -1.43 0) (layer "F.SilkS"))\r\n'
                '\t(pad "1" smd roundrect (at -0.825 0) (size 0.8 0.95) (layers "F.Cu" "F.Mask" "F.Paste"))\r\n'
                '\t(pad "2" smd roundrect (at 0.825 0) (size 0.8 0.95) (layers "F.Cu" "F.Mask" "F.Paste"))\r\n'
                '\t(fp_line (start -1.48 -0.73) (end 1.48 0.73) (layer "F.CrtYd"))\r\n'
                f'\t(embedded_files\r\n\t\t(file (name "R.step") (type model) (data {chunks}) (checksum "0"))\r\n'
                f'\t\t(file (name "notes.txt") (type other) (data |{notes}|))\r\n\t)\r\n'
                '\t(model "kicad-embed://R.step" (offset (xyz 0 0 0)))\r\n)\r\n')

    def prepare_cli(out, fail_render=False):
        state["out"] = out
        bin_dir = out / "install" / "bin"
        bin_dir.mkdir(parents=True)
        cli = bin_dir / CLI_NAME
        if WINDOWS:
            cli.write_bytes(FAKE_CMD.replace("\n", "\r\n").encode())
        else:
            cli.write_bytes(FAKE_SH.encode())
            cli.chmod(cli.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
        # An install layout: <root>/config and <root>/share/symbols beside install/
        (out / "config").mkdir()
        (out / "share" / "symbols").mkdir(parents=True)
        (out / "fixture.png").write_bytes(PNG)
        (out / "fixture.step").write_bytes(EXPORT_STEP)
        (out / "Lib.pretty").mkdir()
        (out / "Lib.pretty" / "R_0603.kicad_mod").write_bytes(footprint_file().encode())
        (out / "source.step").write_bytes(MODEL_STEP)
        (out / "other.step").write_bytes(b"ISO-10303-21; a different model")
        if fail_render:
            (out / "fail-render").write_bytes(b"")

    def cli_path(request):
        return reply(base.PathResponse(path=str(state["out"] / "install" / "bin" / CLI_NAME)))

    BINARY = ("GetKiCadBinaryPath", cli_path)
    ENV_UNHANDLED = ("GetKiCadEnvironment", lambda r: failure(envelope.AS_UNHANDLED, "no handler"))
    ENV_KICAD = ("GetKiCadEnvironment", lambda r: reply(base.KiCadEnvironmentResponse(
        variables={"FIXTURE_VARIABLE": "from-kicad", "KICAD_CONFIG_HOME": "/kicad/config"})))
    ENV_EMPTY = ("GetKiCadEnvironment", lambda r: reply(base.KiCadEnvironmentResponse()))

    def out_path(*parts):
        return "<out>" + ("\\" if WINDOWS else "/") + ("\\" if WINDOWS else "/").join(parts)

    missing_source = "C:/nonexistent-embeint/nope.step" if WINDOWS else "/nonexistent-embeint/nope.step"
    missing_folder = "C:/nonexistent-embeint" if WINDOWS else "/nonexistent-embeint"

    SOT23 = {"footprint_document": DOC, "package": "sot", "params": {"variant": "23"}}
    SOIC = {"footprint_document": DOC, "package": "soic", "replace": True,
            "params": {"pins": 8, "pitch": 1.27, "body_width": 3.9, "body_length": 4.9, "lead_span": 6.0,
                       "exposed_pad": [2.3, 2.3], "name": "SOIC-8-1EP"}}

    def generate_script(*extra_before, create=None, after_create=True, end=END_OK):
        """BeginCommit, [replace...], GetItems (fab reference) and its update, CreateItems, fields."""
        script = [BEGIN, *extra_before, ("GetItems", items(REF_TEXT, USER_TEXT, REFERENCE, VALUE, PAD)),
                  ("UpdateItems", updated), create or created()]
        if after_create:
            script += [("GetItems", items(REF_TEXT, REFERENCE, VALUE, DATASHEET)), ("UpdateItems", updated),
                       ("SetLibraryFootprintProperties", empty), end]
        return script

    return [
        # ---- Library reads and the Footprint Editor ------------------------------------------
        S("footprint_search", [
            ("live_footprint_search", {"query": "SOIC-8*", "limit": 2}),
            ("live_footprint_search", {"query": "R_0603", "limit": 0}),
            ("live_footprint_search", {"query": "x", "limit": 500})], [
            OPEN, ("SearchLibraries", lambda r: reply(parse(library.SearchLibrariesResponse, {
                "items": [{"library_nickname": "Package_SO", "entry_name": f"SOIC-8_{n}"} for n in range(3)],
                "libraries_loading": 2}))),
            OPEN, ("SearchLibraries", lambda r: reply(parse(library.SearchLibrariesResponse, {
                "items": [{"library_nickname": "Resistor_SMD", "entry_name": "R_0603_1608Metric"},
                          {"library_nickname": "Resistor_SMD", "entry_name": "R_0603_Pad"}]}))),
            OPEN, ("SearchLibraries", lambda r: reply(library.SearchLibrariesResponse()))]),
        S("footprint_search_editor_refused", [("live_footprint_search", {"query": "x"})],
          [("OpenFootprintEditor", lambda r: failure(envelope.AS_BUSY, "a dialog is open"))]),
        S("footprint_get", [
            ("live_footprint_get", {"lib_id": "Lib:R_0603"}),
            ("live_footprint_get", {"lib_id": "Lib:Missing"}),
            ("live_footprint_get", {"lib_id": "NoColon"})], [
            OPEN, ("GetItemsFromLibrary", lambda r: reply(parse(editor.GetItemsResponse, {"items": [
                any_of("kiapi.board.types.Footprint", {
                    "id": {"library_nickname": "Lib", "entry_name": "R_0603"},
                    "attributes": {"description": "Resistor 0603", "keywords": "R res",
                                   "mounting_style": "FMS_SMD"},
                    "value_field": {"name": "Value", "text": {"text": {"text": "R_0603"}}},
                    "datasheet_field": {"name": "Datasheet", "text": {"text": {"text": "https://x/ds.pdf"}}},
                    "items": [
                        any_of("kiapi.board.types.Pad", {"number": "10", "type": "PT_SMD",
                                                         "position": {"x_nm": "1500000", "y_nm": "-250000"},
                                                         "pad_stack": {"copper_layers": [
                                                             {"size": {"x_nm": "600000", "y_nm": "1950000"}}]}}),
                        any_of("kiapi.board.types.Pad", {"number": "2", "type": "PT_PTH"}),
                        any_of("kiapi.board.types.Footprint3DModel", {"filename": "kicad-embed://R.step"}),
                        any_of("kiapi.board.types.Pad", {"number": "1", "position": {"x_nm": "-825000"}}),
                        any_of("kiapi.board.types.Pad", {"number": "A1"}),
                        any_of("kiapi.board.types.BoardText", {"text": {"text": "x"}})]})]}))),
            OPEN, ("GetItemsFromLibrary", lambda r: reply(editor.GetItemsResponse())),
            OPEN, ("GetItemsFromLibrary", lambda r: reply(parse(editor.GetItemsResponse, {"items": [
                any_of("kiapi.board.types.Pad", {"number": "1"})]})))]),
        S("footprint_new_and_edit", [
            ("live_footprint_new", {"library": "Lib", "name": "R_0603"}),
            ("live_footprint_new", {"library": "Lib", "name": "J1", "mounting": "tht", "replace_existing": True}),
            ("live_footprint_new", {"library": "Lib", "name": "X", "mounting": "bga"}),
            ("live_footprint_new", {"library": "RO", "name": "X", "mounting": "unspecified"}),
            ("live_footprint_edit", {"lib_id": "Lib:R_0603"}),
            ("live_footprint_edit", {"lib_id": "Lib"})], [
            OPEN, ("NewLibraryFootprint", lambda r: reply(bc.LibraryFootprintResponse(
                document=parse(types.DocumentSpecifier, DOC)))),
            OPEN, ("NewLibraryFootprint", lambda r: reply(bc.LibraryFootprintResponse())),
            OPEN,
            OPEN, ("NewLibraryFootprint", lambda r: failure(envelope.AS_BAD_REQUEST, "library RO is read-only")),
            OPEN, ("EditLibraryFootprint", lambda r: reply(bc.LibraryFootprintResponse(
                document=parse(types.DocumentSpecifier, DOC)))),
            OPEN, ("EditLibraryFootprint", lambda r: failure(envelope.AS_BAD_REQUEST, "no footprint Lib:"))]),

        # ---- Generation (one transaction) --------------------------------------------------
        S("footprint_generate", [("live_footprint_generate", SOT23)], generate_script(), random_ids=True),
        S("footprint_generate_replace", [("live_footprint_generate", SOIC)], [
            BEGIN, ("GetItems", items(PAD, PAD_BARE, LINE, LINE_NO_ID, COURTYARD, USER_TEXT, REFERENCE, VALUE, ZONE)),
            ("DeleteItems", deleted), ("GetItems", items(PAD)), created(), ("GetItems", items(PAD)),
            ("SetLibraryFootprintProperties", empty), END_OK], random_ids=True),
        S("footprint_generate_replace_only_fields", [("live_footprint_generate", dict(SOT23, replace=True))], [
            BEGIN, ("GetItems", items(REFERENCE, VALUE)), ("GetItems", items()), created(),
            ("GetItems", items(VALUE)), ("UpdateItems", updated), ("SetLibraryFootprintProperties", empty), END_OK],
          random_ids=True),
        S("footprint_generate_packages", [
            ("live_footprint_generate", {"footprint_document": DOC, "package": "pin_header",
                                         "params": {"pins_per_row": 3, "rows": 2, "pitch": 2, "drill": 0.9}}),
            ("live_footprint_generate", {"footprint_document": DOC, "package": "QFN",
                                         "params": {"pins_x": 3, "pins_y": 2, "pitch": 0.5, "body_x": 3,
                                                    "body_y": 2.5, "exposed_pad": [1.2, 1]}}),
            ("live_footprint_generate", {"footprint_document": DOC, "package": "chip",
                                         "params": {"size": "0805", "prefix": "C"}}),
            ("live_footprint_generate", {"footprint_document": DOC, "package": "dfn",
                                         "params": "{\"pins_y\": 3, \"pitch\": 0.95, \"body_x\": 2, \"body_y\": 2}"})],
          [step for _ in range(4) for step in
           [BEGIN, ("GetItems", items()), created(), ("GetItems", items()), ("SetLibraryFootprintProperties", empty),
            END_OK]], random_ids=True),
        S("footprint_generate_rejected", [("live_footprint_generate", SOT23)], generate_script(
            create=created({2: (editor.ISC_INVALID_DATA, "pad 3 overlaps"), 4: (editor.ISC_EXISTING, "")}),
            after_create=False) + [DROP_OK], random_ids=True),
        S("footprint_generate_unconfirmed_rollback", [("live_footprint_generate", SOT23)], generate_script(
            create=created({0: (editor.ISC_INVALID_TYPE, "")}), after_create=False) + [
            ("EndCommit", lambda r: failure(envelope.AS_BAD_REQUEST, "no such commit"))], random_ids=True),
        S("footprint_generate_fails_inside", [
            ("live_footprint_generate", {"footprint_document": DOC, "package": "soic",
                                         "params": {"pins": 4, "pitch": 1.27, "body_width": 3.9, "body_length": 4.9,
                                                    "pad_length": 2, "row_pitch": 5, "pad_width": 0}})],
          [BEGIN, ("GetItems", items()), DROP_OK]),
        S("footprint_generate_step_refused", [("live_footprint_generate", SOT23)], [
            BEGIN, ("GetItems", lambda r: failure(envelope.AS_BUSY, "editor busy")), DROP_OK]),
        S("footprint_generate_lost_commit", [("live_footprint_generate", SOT23)],
          generate_script(end=("EndCommit", lambda r: h.DROP)) + [
              ("GetOperationStatus", h.status("OPS_COMMITTED", revision_after="fx-9"))], random_ids=True),
        S("footprint_generate_lost_commit_rolled_back", [("live_footprint_generate", SOT23)],
          generate_script(end=("EndCommit", lambda r: h.DROP)) + [
              ("GetOperationStatus", h.status("OPS_ROLLED_BACK", "lease expired"))], random_ids=True),
        S("footprint_generate_lost_begin", [("live_footprint_generate", SOT23)], [
            ("BeginCommit", lambda r: h.DROP), ("GetOperationStatus", h.status("OPS_IN_PROGRESS")), DROP_OK]),
        S("footprint_generate_stale_commit", [("live_footprint_generate", SOT23)],
          generate_script(end=("EndCommit", lambda r: failure(envelope.AS_REVISION_MISMATCH, "document changed"))),
          random_ids=True),
        S("footprint_generate_refusals", [
            ("live_footprint_generate", {"footprint_document": DOC, "package": "bga", "params": {}}),
            ("live_footprint_generate", {"footprint_document": DOC, "package": "soic", "params": {"pins": 8}}),
            ("live_footprint_generate", {"footprint_document": DOC, "package": "soic",
                                         "params": {"pins": 7, "pitch": 1, "body_width": 3, "body_length": 4}}),
            ("live_footprint_generate", {"footprint_document": DOC, "package": "soic",
                                         "params": {"pins": 8, "pitch": 1, "body_width": 3, "body_length": 4}}),
            ("live_footprint_generate", {"footprint_document": DOC, "package": "chip", "params": {"size": "0201"}}),
            ("live_footprint_generate", {"footprint_document": DOC, "package": "sot", "params": {"variant": "23", "x": 1}}),
            ("live_footprint_generate", {"footprint_document": DOC, "package": "sot", "params": {"package": "sot"}}),
            ("live_footprint_generate", {"footprint_document": DOC, "package": "pin_header",
                                         "params": {"pins_per_row": 2.0}}),
            ("live_footprint_generate", {"footprint_document": SYMBOL, "package": "sot", "params": {"variant": "23"}}),
            ("live_footprint_generate", {"footprint_document": {"type": "DOCTYPE_FOOTPRINT"}, "package": "sot",
                                         "params": {"variant": "23"}})], []),

        # ---- Properties and 3D models -------------------------------------------------------
        S("footprint_set_properties", [
            ("live_footprint_set_properties", {"footprint_document": DOC, "description": "", "keywords": "R res",
                                               "mounting": "tht", "value": "10k", "datasheet": "https://x/ds.pdf",
                                               "exclude_from_bom": False, "exclude_from_position_files": True}),
            ("live_footprint_set_properties", {"footprint_document": DOC}),
            ("live_footprint_set_properties", {"footprint_document": DOC, "mounting": "smt"}),
            ("live_footprint_set_properties", {"footprint_document": DOC, "keywords": "x"})], [
            BEGIN, ("SetLibraryFootprintProperties", empty), END_OK,
            BEGIN, ("SetLibraryFootprintProperties", empty), END_OK,
            BEGIN, ("SetLibraryFootprintProperties", lambda r: failure(envelope.AS_BAD_REQUEST, "not editable")),
            DROP_OK]),
        S("footprint_set_properties_lost_commit", [("live_footprint_set_properties", {"footprint_document": DOC,
                                                                                      "value": "1k"})], [
            BEGIN, ("SetLibraryFootprintProperties", empty), ("EndCommit", lambda r: h.DROP),
            ("GetOperationStatus", h.status("OPS_IN_PROGRESS")), DROP_OK,
            ("GetOperationStatus", h.status("OPS_COMMITTED", revision_after="fx-5"))]),
        S("footprint_set_properties_lost_status", [("live_footprint_set_properties", {"footprint_document": DOC,
                                                                                      "value": "1k"})], [
            BEGIN, ("SetLibraryFootprintProperties", empty), ("EndCommit", lambda r: h.DROP),
            ("GetOperationStatus", lambda r: h.DROP)]),
        S("footprint_attach_step", [
            ("live_footprint_attach_step", {"footprint_document": DOC, "step_path": "/mnt/c/models/R_0603.STEP",
                                            "offset_mm": [1, 2.5, 0], "rotation_deg": [0, 0, 90]}),
            ("live_footprint_attach_step", {"footprint_document": DOC, "step_path": "C:\\models\\R.wrl",
                                            "offset_mm": []}),
            ("live_footprint_attach_step", {"footprint_document": DOC, "step_path": "/models/R.txt"}),
            ("live_footprint_attach_step", {"footprint_document": DOC, "step_path": "/models/.step"})], [
            BEGIN, ("EmbedFileFromPath", lambda r: reply(editor.EmbedFileFromPathResponse(file=file_info(
                name="R_0603.STEP", type="EFT_MODEL", uri="kicad-embed://R_0603.STEP", stored_size="1234")))), END_OK,
            BEGIN, ("SetLibraryFootprintProperties", empty), END_OK,
            BEGIN, ("EmbedFileFromPath", lambda r: reply(editor.EmbedFileFromPathResponse(file=file_info(
                name="R.wrl", uri="kicad-embed://R.wrl")))), END_OK,
            BEGIN, ("SetLibraryFootprintProperties", empty), END_OK]),
        S("footprint_attach_step_after_embed", [
            ("live_footprint_attach_step", {"footprint_document": DOC, "step_path": "/m/x.stp", "offset_mm": [1, 2]}),
            ("live_footprint_attach_step", {"footprint_document": DOC, "step_path": "/m/x.stpz",
                                            "rotation_deg": [1, 2, 3, 4]}),
            ("live_footprint_attach_step", {"footprint_document": DOC, "step_path": "/m/x.wrz"}),
            ("live_footprint_attach_step", {"footprint_document": DOC, "step_path": "/m/y.step"})], [
            BEGIN, ("EmbedFileFromPath", lambda r: reply(editor.EmbedFileFromPathResponse(file=file_info(
                name="x.stp", uri="kicad-embed://x.stp")))), END_OK,
            BEGIN, ("EmbedFileFromPath", lambda r: reply(editor.EmbedFileFromPathResponse(file=file_info(
                name="x.stpz", uri="kicad-embed://x.stpz")))), END_OK,
            BEGIN, ("EmbedFileFromPath", lambda r: reply(editor.EmbedFileFromPathResponse(file=file_info(
                name="x.wrz")))), END_OK,
            BEGIN, ("EmbedFileFromPath", lambda r: reply(editor.EmbedFileFromPathResponse(file=file_info(
                uri="kicad-embed://y.step")))), END_OK,
            BEGIN, ("SetLibraryFootprintProperties", empty), END_OK]),
        S("footprint_attach_step_rollback", [
            ("live_footprint_attach_step", {"footprint_document": DOC, "step_path": "/m/x.step"})], [
            BEGIN, ("EmbedFileFromPath", lambda r: failure(envelope.AS_BAD_REQUEST, "cannot read /m/x.step")),
            DROP_OK]),
        S("footprint_attach_step_properties_lost", [
            ("live_footprint_attach_step", {"footprint_document": DOC, "step_path": "/m/x.step"})], [
            BEGIN, ("EmbedFileFromPath", lambda r: reply(editor.EmbedFileFromPathResponse(file=file_info(
                name="x.step", uri="kicad-embed://x.step")))), END_OK,
            BEGIN, ("SetLibraryFootprintProperties", lambda r: h.DROP), DROP_OK]),
        S("footprint_attach_step_lost_begin", [
            ("live_footprint_attach_step", {"footprint_document": DOC, "step_path": "/m/x.step"})], [
            ("BeginCommit", lambda r: h.DROP), ("GetOperationStatus", h.status("OPS_UNKNOWN"))]),

        # ---- Embedded files ---------------------------------------------------------------
        S("footprint_embedded_files", [
            ("live_embedded_files", {"document_spec": DOC}),
            ("live_embedded_files", {"document_spec": SYMBOL}),
            ("live_embedded_files", {"document_spec": h.SHEET_DOC}),
            ("live_embed_file", {"document_spec": SYMBOL, "path": "C:/docs/ds.pdf"}),
            ("live_embed_file", {"document_spec": DOC, "path": "/m/font.ttf", "overwrite": True}),
            ("live_remove_embedded_file", {"document_spec": DOC, "name": "font.ttf"}),
            ("live_remove_embedded_file", {"document_spec": {"type": "DOCTYPE_SYMBOL"}, "name": "x"})], [
            ("GetEmbeddedFileList", lambda r: reply(editor.EmbeddedFileListResponse(files=[
                file_info(name="R.step", type="EFT_MODEL", uri="kicad-embed://R.step", stored_size="4096"),
                file_info(name="notes.txt", type="EFT_OTHER", uri="kicad-embed://notes.txt")]))),
            ("GetEmbeddedFileList", lambda r: reply(editor.EmbeddedFileListResponse())),
            BEGIN, ("EmbedFileFromPath", lambda r: reply(editor.EmbedFileFromPathResponse(file=file_info(
                name="ds.pdf", type="EFT_DATASHEET", uri="kicad-embed://ds.pdf")))), END_OK,
            BEGIN, ("EmbedFileFromPath", lambda r: reply(editor.EmbedFileFromPathResponse())), END_OK,
            BEGIN, ("RemoveEmbeddedFile", empty), END_OK]),
        S("footprint_embedded_file_faults", [
            ("live_embed_file", {"document_spec": DOC, "path": "/m/a.bin"}),
            ("live_remove_embedded_file", {"document_spec": DOC, "name": "missing"}),
            ("live_remove_embedded_file", {"document_spec": DOC, "name": "a.bin"}),
            ("live_embedded_files", {"document_spec": DOC})], [
            BEGIN, ("EmbedFileFromPath", lambda r: h.DROP), DROP_OK,
            BEGIN, ("RemoveEmbeddedFile", lambda r: failure(envelope.AS_BAD_REQUEST, "no file missing")), DROP_OK,
            BEGIN, ("RemoveEmbeddedFile", empty), ("EndCommit", lambda r: h.DROP),
            ("GetOperationStatus", h.status("OPS_COMMITTED", revision_after="fx-7")),
            ("GetEmbeddedFileList", lambda r: failure(envelope.AS_UNHANDLED, "no handler"))]),

        # ---- The open footprint: view, summary, save --------------------------------------
        S("footprint_view_summary", [
            ("live_footprint_view", {"footprint_document": DOC}),
            ("live_footprint_view", {"footprint_document": DOC, "dpi": "300"}),
            ("live_footprint_view", {"footprint_document": DOC, "dpi": 2 ** 40}),
            ("live_footprint_summary", {"footprint_document": DOC}),
            ("live_footprint_summary", {"footprint_document": SYMBOL})], [
            ("RenderLibraryFootprint", render_reply), ("RenderLibraryFootprint", render_reply),
            ("GetItems", items(PAD, PAD_BARE, LINE, LINE_NO_ID, COURTYARD, REF_TEXT, USER_TEXT, REFERENCE, ZONE))]),
        S("footprint_save", [
            ("live_footprint_save", {"footprint_document": DOC}),
            ("live_footprint_save", {"footprint_document": dict(DOC, lib_id={"library_nickname": "Gone",
                                                                              "entry_name": "X"})}),
            ("live_footprint_save", {"footprint_document": DOC}),
            ("live_footprint_save", {"footprint_document": DOC})], [
            ("SaveDocument", empty), OPEN, LIB_TABLE, ("ReloadLibrary", library_status()),
            ("SaveDocument", empty), OPEN, LIB_TABLE,
            ("SaveDocument", empty), OPEN, LIB_TABLE, ("ReloadLibrary", library_status("LCS_READ_ONLY", "locked ")),
            ("SaveDocument", lambda r: failure(envelope.AS_BAD_REQUEST, "save failed"))],
          prepare=lambda out: state.update(out=out)),
        S("footprint_save_unknown_scope", [("live_footprint_save", {"footprint_document": DOC})], [
            ("SaveDocument", empty), OPEN,
            table({"nickname": "Lib", "path": "/x/Lib.pretty", "scope": "LTS_BOTH"}),
        ], prepare=lambda out: state.update(out=out)),

        # ---- SolidCraft plans (no KiCad) ----------------------------------------------------
        S("footprint_model_plan", [
            ("live_footprint_model_plan", {"package": "soic", "body_height": 1.75, "export_path": out_path("models", "SOIC-8.step"),
                                           "params": {"pins": 8, "pitch": 1.27, "body_width": 3.9, "body_length": 4.9,
                                                      "lead_span": 6.0, "lead_width": 1}}),
            ("live_footprint_model_plan", {"package": "sot", "params": {"variant": "223"}, "body_height": "1.8",
                                           "standoff": 0, "lead_thickness": 0.25, "lead_width": 0.7, "lead_span": 7,
                                           "foot_length": 0.9, "export_path": out_path("a", "b", "SOT.step")}),
            ("live_footprint_model_plan", {"package": "qfn", "body_height": 0.9, "export_path": out_path("q.step"),
                                           "params": {"pins_x": 4, "pins_y": 4, "pitch": 0.5, "body_x": 3,
                                                      "body_y": 3, "exposed_pad": [1.7, 1.7]}}),
            ("live_footprint_model_plan", {"package": "chip", "params": {"size": "0603"}, "body_height": 0.8,
                                           "export_path": out_path("c.step")}),
            ("live_footprint_model_plan", {"package": "pin_header", "params": {"pins_per_row": 2},
                                           "body_height": 2.5, "export_path": out_path("p.step")}),
            ("live_footprint_model_plan", {"package": "bga", "params": {}, "body_height": 1,
                                           "export_path": out_path("made-before-check", "x.step")}),
            ("live_footprint_model_plan", {"package": "soic", "params": {"pins": 8}, "body_height": 1,
                                           "export_path": out_path("y.step")})], []),

        # ---- kicad-cli: KiCad's 3D render and STEP check of a saved footprint --------------
        S("footprint_view_3d", [
            ("live_footprint_view_3d", {"lib_id": "Lib:R_0603"}),
            ("live_footprint_view_3d", {"lib_id": "Lib:R_0603", "rotate": "", "zoom": 2})], [
            BINARY, ENV_KICAD, OPEN, LIB_TABLE, BINARY, ENV_UNHANDLED, OPEN, LIB_TABLE],
          prepare=prepare_cli),
        S("footprint_view_3d_refusals", [
            ("live_footprint_view_3d", {"lib_id": "Lib:Unsaved"}),
            ("live_footprint_view_3d", {"lib_id": "Nowhere:R"}),
            ("live_footprint_view_3d", {"lib_id": "Lib:R_0603"}),
            ("live_footprint_view_3d", {"lib_id": "Lib:R_0603"})], [
            BINARY, ENV_EMPTY, OPEN, LIB_TABLE,
            BINARY, ENV_KICAD, OPEN, LIB_TABLE,
            ("GetKiCadBinaryPath", lambda r: reply(base.PathResponse(path=""))),
            ("GetKiCadBinaryPath", lambda r: reply(base.PathResponse(path="C:/nonexistent-embeint/kicad-cli")))],
          prepare=prepare_cli),
        S("footprint_view_3d_render_fails", [("live_footprint_view_3d", {"lib_id": "Lib:R_0603"})],
          [BINARY, ENV_KICAD, OPEN, LIB_TABLE], prepare=lambda out: prepare_cli(out, fail_render=True)),
        S("footprint_check_model", [
            ("live_footprint_check_model", {"lib_id": "Lib:R_0603"}),
            ("live_footprint_check_model", {"lib_id": "Lib:R_0603", "source_path": out_path("source.step")}),
            ("live_footprint_check_model", {"lib_id": "Lib:R_0603", "source_path": out_path("other.step")}),
            ("live_footprint_check_model", {"lib_id": "Lib:R_0603", "source_path": missing_source}),
            ("live_footprint_check_model", {"lib_id": "Lib:Unsaved"})], [
            OPEN, LIB_TABLE, BINARY, ENV_KICAD,
            OPEN, LIB_TABLE, BINARY, ENV_UNHANDLED,
            OPEN, LIB_TABLE, BINARY, ("GetKiCadEnvironment", lambda r: failure(envelope.AS_BAD_REQUEST, "refused")),
            OPEN, LIB_TABLE,
            OPEN, LIB_TABLE], prepare=prepare_cli),

        # ---- live_kicad_cli -----------------------------------------------------------------
        S("footprint_kicad_cli", [
            ("live_kicad_cli", {"args": ["sch", "erc", "--format", "json", "-o", out_path("erc.json"),
                                         out_path("design.kicad_sch")]}),
            ("live_kicad_cli", {"args": ["version"], "timeout_s": 30}),
            ("live_kicad_cli", {"args": ["sym", "export", "svg", "-o", out_path("svg"), out_path("lib.kicad_sym")]}),
            ("live_kicad_cli", {"args": ["pcb", "drc", out_path("board.kicad_pcb")]}),
            ("live_kicad_cli", {"args": ["sch", "export", "netlist", out_path("design.kicad_sch")]}),
            ("live_kicad_cli", {"args": ["sch", "erc", "cwd-probe"], "working_directory": "<out>"})], [
            BINARY, ENV_UNHANDLED, BINARY, ENV_KICAD, BINARY, ENV_EMPTY, BINARY, ENV_UNHANDLED,
            BINARY, ENV_UNHANDLED, BINARY, ENV_UNHANDLED], prepare=prepare_cli),
        S("footprint_kicad_cli_process_faults", [
            ("live_kicad_cli", {"args": ["pcb", "export", "svg", out_path("b.kicad_pcb")], "timeout_s": 0.5}),
            ("live_kicad_cli", {"args": ["version"], "working_directory": missing_folder}),
            ("live_kicad_cli", {"args": ["version"], "timeout_s": 0}),
            ("live_kicad_cli", {"args": ["version"], "timeout_s": 600.5})], [
            BINARY, ENV_UNHANDLED, BINARY, ENV_UNHANDLED, BINARY, ENV_UNHANDLED, BINARY, ENV_UNHANDLED],
          prepare=prepare_cli),
        S("footprint_kicad_cli_launch_faults", [
            ("live_kicad_cli", {"args": ["version"]}),
            ("live_kicad_cli", {"args": ["version"]}),
            ("live_kicad_cli", {"args": ["version"]}),
            ("live_kicad_cli", {"args": ["version"]})], [
            ("GetKiCadBinaryPath", lambda r: reply(base.PathResponse(path=""))),
            ("GetKiCadBinaryPath", lambda r: failure(envelope.AS_UNHANDLED, "project manager only")),
            BINARY, ("GetKiCadEnvironment", lambda r: failure(envelope.AS_BAD_REQUEST, "refused")),
            ("GetKiCadBinaryPath", lambda r: h.DROP)], prepare=prepare_cli),
        S("footprint_kicad_cli_refusals", [
            ("live_kicad_cli", {"args": args}) for args in (
                [], ["jobset", "run", "jobs.kicad_jobset"], ["upgrade", "x.kicad_sch"], ["sch"], ["pcb", "export"],
                ["pcb", "export", "brd"], ["sym", "export", "png"], ["fp", "upgrade"], ["version\0"],
                ["pcb", "drc", "--save-board", "b.kicad_pcb"], ["pcb", "drc", "--save-board=true"],
                ["sch", "erc", "-no", "x.json"], ["sch", "erc", "-o", "C:\\p\\design.kicad_sch"],
                ["sch", "erc", "--output=../board.KICAD_PCB"], ["pcb", "export", "step", "-o", "Lib.pretty/x.step"],
                ["pcb", "export", "vrml", "--models-dir", "C:/libs/My.Pretty/"],
                ["pcb", "drc", "--report-path", "fp-lib-table"], ["pcb", "drc", "--report-path=/p/x.kicad_pro"],
                ["sch", "export", "pdf", "-o", "/p/sym-lib-table"], ["sch", "erc", "--output", "x.kicad_dru"])]
          + [("live_kicad_cli", {"args": ["pcb", "drc", "--drc-severity", "-1", "-o", "/tmp/ok.json", "--save-board"]})],
          []),
        # Names Windows rewrites (trailing dots and spaces, stream suffixes) and footprint-library
        # working directories, refused before KiCad is asked.  (Short 8.3 aliases of existing files
        # are resolved too; the unit tests of both servers cover them with real files.)
        S("footprint_kicad_cli_windows_names", [
            ("live_kicad_cli", {"args": ["sch", "export", "netlist", "-o", output, "C:\\p\\design.kicad_sch"]})
            for output in ("C:\\p\\design.kicad_sch.", "C:\\p\\design.kicad_sch ", "C:\\p\\design.kicad_sch::$DATA",
                           "/p/design.kicad_sch . .", "C:\\libs\\Lib.pretty.\\x.net",
                           "C:\\libs\\Lib.pretty::$INDEX_ALLOCATION\\x.net")]
          + [("live_kicad_cli", {"args": ["sch", "export", "netlist", "--output=C:\\p\\design.kicad_sch:x", "d"]})]
          + [("live_kicad_cli", {"args": ["version"], "working_directory": cwd})
             for cwd in ("C:\\libs\\Lib.pretty", "/libs/x.Pretty/sub", "C:\\libs\\Lib.pretty.")],
          []),

        # ---- Argument validation (pydantic's wording is not compared) -----------------------
        S("footprint_argument_validation", [
            ("live_footprint_search", {"query": "x", "limit": "many"}),
            ("live_footprint_generate", {"footprint_document": DOC, "package": "sot", "params": [1]}),
            ("live_footprint_set_properties", {"footprint_document": DOC, "exclude_from_bom": "perhaps"}),
            ("live_footprint_attach_step", {"footprint_document": DOC, "step_path": "/m/x.step", "offset_mm": ["a"]}),
            ("live_footprint_model_plan", {"package": "sot", "params": {"variant": "23"}}),
            ("live_footprint_view_3d", {"lib_id": "Lib:R", "zoom": "close"}),
            ("live_kicad_cli", {"args": "version"}),
            ("live_kicad_cli", {"args": ["version"], "timeout_s": None}),
            ("live_embed_file", {"document_spec": DOC}),
            ("live_footprint_view", {"footprint_document": DOC, "dpi": 1.5}),
            ("live_footprint_new", {"library": "Lib"}),
            ("live_footprint_get", {}),
            ("live_footprint_edit", {"lib_id": 5}),
            ("live_footprint_summary", {"footprint_document": "not json"}),
            ("live_footprint_save", {}),
            ("live_footprint_check_model", {"lib_id": "x", "source_path": 3}),
            ("live_embedded_files", {}),
            ("live_remove_embedded_file", {"document_spec": DOC})], [], compare="error_prefix"),
    ]
