# SPDX-License-Identifier: GPL-3.0-or-later
"""Manufacturing outputs: export defaults and options, and the fabrication package.

live_pcb_export and live_schematic_export send one export job built from a default (protobuf JSON)
with the caller's options merged over it.  live_pcb_fab_package checks the design (saved, DRC and
schematic parity clean), exports every output a fab and an assembler need into the Embeint
Outputs/ layout, checks what was written (layer count, holes, placements against the BOM) and
writes a manifest and a zip of the Gerbers folder.

The native Rust MCP (rust/embeint-ipc/src/fab.rs) ports this module: keep the defaults, the order
of requests, the result keys (in order) and the file checks identical, and run
rust/parity/mcp_parity.py after a change.
"""

import hashlib
import json
from pathlib import Path
import struct
import zlib

from google.protobuf import json_format

BJ = "kiapi.board.jobs."
SJ = "kiapi.schematic.jobs."

# ---- Single exports ----------------------------------------------------------------------------

BOARD_EXPORTS = {"gerbers": BJ + "RunBoardJobExportGerbers", "drill": BJ + "RunBoardJobExportDrill",
                 "position": BJ + "RunBoardJobExportPosition", "step": BJ + "RunBoardJobExport3D",
                 "pdf": BJ + "RunBoardJobExportPdf", "svg": BJ + "RunBoardJobExportSvg",
                 "dxf": BJ + "RunBoardJobExportDxf", "ipc2581": BJ + "RunBoardJobExportIpc2581",
                 "ipcd356": BJ + "RunBoardJobExportIpcD356", "odb": BJ + "RunBoardJobExportODB",
                 "stats": BJ + "RunBoardJobExportStats", "dsn": BJ + "RunBoardJobExportSpecctraDsn",
                 "gencad": BJ + "RunBoardJobExportGencad", "idf": BJ + "RunBoardJobExportIdf"}

PLOT_LAYERS = {"plot_settings": {"layers": ["BL_F_Cu", "BL_B_Cu", "BL_F_SilkS", "BL_Edge_Cuts"],
                                 "plot_drawing_sheet": True}}
BOARD_EXPORT_DEFAULTS = {
    "gerbers": {"use_board_plot_params": True, "create_gerber_job_file": True},
    "step": {"format": "B3D_STEP", "overwrite": True, "include_unspecified": True, "substitute_models": True,
             "export_board_body": True, "export_components": True},
    "pdf": PLOT_LAYERS, "svg": PLOT_LAYERS, "dxf": PLOT_LAYERS,
}

SCHEMATIC_EXPORTS = {"svg": SJ + "RunSchematicJobExportSvg", "pdf": SJ + "RunSchematicJobExportPdf",
                     "dxf": SJ + "RunSchematicJobExportDxf", "ps": SJ + "RunSchematicJobExportPs",
                     "netlist": SJ + "RunSchematicJobExportNetlist", "bom": SJ + "RunSchematicJobExportBOM"}

# An orderable BOM: one line per part (grouped by value, footprint, manufacturer and MPN), every
# reference listed (no R1-R4 ranges, so a pick list can be checked against it), DNP parts left out
BOM_COLUMNS = [("Reference", "Reference", False), ("${QUANTITY}", "Qty", False), ("Value", "Value", True),
               ("Footprint", "Footprint", True), ("Manufacturer", "Manufacturer", True), ("MPN", "MPN", True),
               ("Alt Manufacturer", "Alt Manufacturer", False), ("Alt MPN", "Alt MPN", False),
               ("Description", "Description", False), ("Datasheet", "Datasheet", False)]
BOM_DEFAULT = {
    "format": {"field_delimiter": ",", "string_delimiter": "\"", "ref_delimiter": ",", "ref_range_delimiter": ""},
    "fields": {"fields": [{"name": name, "label": label, "group_by": group} for name, label, group in BOM_COLUMNS],
               "sort_field": "Reference", "sort_direction": "BSD_ASCENDING"},
    "exclude_dnp": True, "group_symbols": True,
}
SCHEMATIC_EXPORT_DEFAULTS = {"netlist": {"format": "SNF_KICAD_SEXPR"}, "bom": BOM_DEFAULT}


def message_class(full_name):
    from commands import message_class as lookup
    return lookup(full_name)


def build_request(full_name, default, options=None):
    """The export request: the default, then the caller's options (protobuf JSON) merged over it.
    job_settings is not accepted: the document and output path come from the tool's arguments."""
    request = json_format.ParseDict(default or {}, message_class(full_name)())
    if options is not None:
        if not isinstance(options, dict):
            raise ValueError("options must be an object of protobuf JSON fields")
        if "job_settings" in options or "jobSettings" in options:
            raise ValueError("options cannot set job_settings; the document and output path come from the "
                             "tool's arguments")
        json_format.ParseDict(options, request)
    return request


def export_request(exports, defaults, kind, options=None):
    if kind not in exports:
        raise ValueError(f"kind must be one of {sorted(exports)}")
    return build_request(exports[kind], defaults.get(kind), options)


# ---- The fabrication package -------------------------------------------------------------------

# Every output, in the order it is exported: (folder, request type, default, file name or None for a
# folder output).  {name} is the project name.
PLAN = {
    # Explicit settings (not the board's plot dialog settings), so Gerbers, drill and placements share
    # the absolute origin; the layers (every copper layer and the technical layers) are added once
    # the board's copper layer count is known, unless options give them
    "gerbers": ("Gerbers", BOARD_EXPORTS["gerbers"],
                {"use_board_plot_params": False, "create_gerber_job_file": True,
                 "include_netlist_attributes": True, "use_x2_format": True, "precision": "GP_6",
                 "plot_settings": {"scale": 1.0, "drill_marks": "PDM_NONE", "plot_reference_designators": True,
                                   "plot_footprint_values": True}}, None),
    "drill": ("Gerbers", BOARD_EXPORTS["drill"],
              {"format": "DF_EXCELLON", "units": "U_MM", "origin": "DO_ABSOLUTE", "zeros_format": "DZF_DECIMAL",
               "excellon": {"mirror_y": False, "minimal_header": False, "combine_pth_npth": False},
               "map_format": "DMF_PDF", "report_format": "DRF_STANDARD"}, None),
    "position": ("PnP", BOARD_EXPORTS["position"],
                 # naked_filename: KiCad writes the given name, not <name>-pos-all-pos.csv
                 {"use_drill_place_file_origin": False, "exclude_dnp": True, "single_file": True,
                  "naked_filename": True, "side": "PS_BOTH", "units": "U_MM", "format": "PF_CSV"},
                 "{name}-pos.csv"),
    "step": ("STEP", BOARD_EXPORTS["step"], BOARD_EXPORT_DEFAULTS["step"], "{name}.step"),
    "assembly_pdf": ("Assembly", BOARD_EXPORTS["pdf"],
                     {"plot_settings": {"layers": ["BL_F_Fab", "BL_B_Fab"], "common_layers": ["BL_Edge_Cuts"],
                                        "sketch_pads_on_fab_layers": True,
                                        "crossout_dnp_footprints_on_fab_layers": True,
                                        "plot_reference_designators": True, "plot_drawing_sheet": True},
                      "single_document": True, "page_mode": "BJPM_EACH_LAYER_OWN_PAGE"},
                     "{name}-assembly.pdf"),
    "ipc2581": ("IPC-2581", BOARD_EXPORTS["ipc2581"],
                {"units": "U_MM", "version": "IPC2581V_C", "manufacturer_part_number_column": "MPN",
                 "manufacturer_column": "Manufacturer"}, "{name}-ipc2581.xml"),
    "odb": ("ODB", BOARD_EXPORTS["odb"],
            {"units": "U_MM", "compression": "ODBC_ZIP", "manufacturer_part_number_column": "MPN"},
            "{name}-odb.zip"),
    "ipcd356": ("Netlist", BOARD_EXPORTS["ipcd356"], {}, "{name}.d356"),
    "bom": ("BOM", SCHEMATIC_EXPORTS["bom"], BOM_DEFAULT, "{name}-bom.csv"),
    "schematic_pdf": ("Schematic", SCHEMATIC_EXPORTS["pdf"], {}, "{name}-schematic.pdf"),
}
OUTPUTS = tuple(PLAN)
DEFAULT_OUTPUTS = ("gerbers", "drill", "position", "step", "assembly_pdf", "bom", "schematic_pdf")
SCHEMATIC_OUTPUTS = ("bom", "schematic_pdf")
MANIFEST = "manifest.json"
MAX_VIOLATIONS = 50


TECHNICAL_LAYERS = ["BL_F_Mask", "BL_B_Mask", "BL_F_Paste", "BL_B_Paste", "BL_F_SilkS", "BL_B_SilkS",
                    "BL_Edge_Cuts"]


def gerber_layers(copper_layers):
    """Every copper layer of a board with this many, then the technical layers a fab needs."""
    inner = [f"BL_In{index}_Cu" for index in range(1, copper_layers - 1)]
    copper = ["BL_F_Cu"] + inner + (["BL_B_Cu"] if copper_layers > 1 else [])
    return copper + TECHNICAL_LAYERS


def select_outputs(outputs):
    """The outputs to export, in export order."""
    if outputs is None:
        return [name for name in OUTPUTS if name in DEFAULT_OUTPUTS]
    unknown = [name for name in outputs if name not in PLAN]
    if unknown:
        raise ValueError(f"Unknown outputs {unknown}; choose from {list(OUTPUTS)}")
    if not outputs:
        raise ValueError("outputs must name at least one output")
    return [name for name in OUTPUTS if name in outputs]


def package_requests(selected, options):
    """Every selected output's request (without job settings), so bad options fail before anything
    is sent."""
    options = options or {}
    stray = [name for name in options if name not in selected]
    if stray:
        raise ValueError(f"options name outputs that are not exported: {stray}")
    requests = {}
    for name in selected:
        value = options.get(name)
        if value is not None and not isinstance(value, dict):
            raise ValueError(f"options[{name!r}] must be an object of protobuf JSON fields")
        requests[name] = build_request(PLAN[name][1], PLAN[name][2], value)
    return requests


def name_stem(path):
    """The file name without folders and extension ("boards/demo.kicad_pcb" -> "demo")."""
    base = path.replace("\\", "/").rsplit("/", 1)[-1]
    stem, dot, _ = base.rpartition(".")
    return stem if dot and stem else base


def project_name(board):
    return board.project.name or name_stem(board.board_filename) or "board"


def join(directory, *parts):
    return "/".join([directory.rstrip("/\\")] + list(parts))


def default_output_dir(board):
    if not board.project.path:
        raise ValueError("output_dir is required: the board document names no project path")
    return join(board.project.path, "Outputs")


def check_output_dir(output_dir, selected, name, zip_gerbers, overwrite):
    """Refuse to mix new outputs with old ones: the output folders (and the manifest and zip) must
    be empty or absent unless overwrite is given.  Folders inside them are never removed."""
    root = Path(output_dir)
    if root.exists() and not root.is_dir():
        raise ValueError(f"output_dir {output_dir} is a file")
    folders = sorted({PLAN[output][0] for output in selected})
    existing = []
    for folder in folders:
        target = root / folder
        if target.is_file():
            raise ValueError(f"{join(output_dir, folder)} is a file")
        if target.is_dir():
            entries = sorted(target.iterdir(), key=lambda p: p.name)
            if any(entry.is_dir() for entry in entries):
                raise ValueError(f"{join(output_dir, folder)} contains folders; choose an empty output_dir")
            existing += [f"{folder}/{entry.name}" for entry in entries]
    for single in [MANIFEST] + ([zip_name(name)] if zip_gerbers and "Gerbers" in folders else []):
        if (root / single).is_dir():
            raise ValueError(f"{join(output_dir, single)} is a folder")
        if (root / single).is_file():
            existing.append(single)
    if existing and not overwrite:
        shown = ", ".join(existing[:10]) + (" ..." if len(existing) > 10 else "")
        raise ValueError(f"{output_dir} already has outputs ({shown}); pass overwrite=true to replace them")
    return folders, existing


def zip_name(name):
    return f"{name}-gerbers.zip"


def match_schematic(documents, board):
    """The open schematic of the board's project (the first, if several match).  KiCad reports the
    schematic's project path with a trailing separator and the board's without one."""
    path = board.project.path.rstrip("/\\")
    matches = [d for d in documents
               if not path or str(d.get("project", {}).get("path", "")).rstrip("/\\") == path]
    if not matches:
        raise ValueError("No schematic of this project is open; open it with live_schematic_open or pass "
                         "schematic_document")
    return matches[0]


def drc_summary(violations, ignored, excluded):
    """violations: (type, severity, description) as live_pcb_drc names them."""
    by_type = {}
    for kind, _, _ in violations:
        by_type[kind] = by_type.get(kind, 0) + 1
    return {"errors": sum(1 for v in violations if v[1] == "ERROR"),
            "warnings": sum(1 for v in violations if v[1] == "WARNING"),
            "by_type": by_type, "ignored": ignored, "excluded": excluded,
            "violations": [{"type": t, "severity": s, "description": d} for t, s, d in violations[:MAX_VIOLATIONS]],
            "violations_truncated": len(violations) > MAX_VIOLATIONS}


# ---- Reading what was written --------------------------------------------------------------------

# Whitespace stripped from cells and lines (the same set in the Rust port)
BLANK = " \t\r\n"


def read_text(path):
    return Path(path).read_bytes().decode("utf-8", errors="replace")


def parse_csv(text):
    """Rows of comma-separated values with "quoted" cells; blank rows are dropped."""
    text = text.removeprefix("﻿")
    rows, row, cell, quoted, index = [], [], [], False, 0
    while index < len(text):
        char = text[index]
        if quoted:
            if char == '"':
                if index + 1 < len(text) and text[index + 1] == '"':
                    cell.append('"')
                    index += 1
                else:
                    quoted = False
            else:
                cell.append(char)
        elif char == '"':
            quoted = True
        elif char == ",":
            row.append("".join(cell))
            cell = []
        elif char in "\r\n":
            if char == "\r" and index + 1 < len(text) and text[index + 1] == "\n":
                index += 1
            row.append("".join(cell))
            cell = []
            rows.append(row)
            row = []
        else:
            cell.append(char)
        index += 1
    if cell or row:
        row.append("".join(cell))
        rows.append(row)
    return [r for r in rows if any(c.strip(BLANK) for c in r)]


def column(header, label):
    names = [cell.strip(BLANK) for cell in header]
    return names.index(label) if label in names else None


def cell(row, index):
    return row[index].strip(BLANK) if index is not None and index < len(row) else ""


def split_refs(text):
    return [ref.strip(BLANK) for ref in text.split(",") if ref.strip(BLANK)]


def check_gerbers(directory, files, copper_layers, warnings):
    jobs = [f for f in files if f.endswith(".gbrjob")]
    result = {"files": len([f for f in files if not f.endswith(".gbrjob")]), "job_file": jobs[0] if jobs else None,
              "copper_layers": None, "functions": []}
    if not jobs:
        warnings.append("gerbers: no Gerber job file (.gbrjob) was written")
        return result
    try:
        job = json.loads(read_text(Path(directory) / jobs[0]))
        attributes = job.get("FilesAttributes", []) if isinstance(job, dict) else []
        functions = [a.get("FileFunction", "") for a in attributes if isinstance(a, dict)]
        functions = [f for f in functions if isinstance(f, str)]
    except ValueError:
        warnings.append("gerbers: the Gerber job file is not valid JSON")
        return result
    copper = sum(1 for f in functions if f.startswith("Copper"))
    result["copper_layers"] = copper
    result["functions"] = functions
    if copper_layers is not None and copper != copper_layers:
        warnings.append(f"gerbers: {copper} copper layers plotted, the board has {copper_layers}")
    if not any(f.startswith("Profile") for f in functions):
        warnings.append("gerbers: no board outline (Edge.Cuts profile) was plotted")
    return result


def parse_excellon(text):
    """(tools, holes) in an Excellon drill file: tool definitions in the header, and every
    coordinate (drill hit, slot or route start) in the body."""
    tools = holes = 0
    header = False
    for raw in text.replace("\r\n", "\n").replace("\r", "\n").split("\n"):
        line = raw.strip(BLANK)
        if line == "M48":
            header = True
        elif header:
            if line in ("%", "M95"):
                header = False
            elif line.startswith("T") and "C" in line:
                tools += 1
        elif line.startswith(("X", "Y", "G00")):
            holes += 1
    return tools, holes


def check_drill(directory, files, warnings):
    result = {"files": {}, "holes": 0, "other_files": []}
    for name in files:
        if name.lower().endswith(".drl"):
            tools, holes = parse_excellon(read_text(Path(directory) / name))
            result["files"][name] = {"tools": tools, "holes": holes}
            result["holes"] += holes
        else:
            result["other_files"].append(name)
    if not result["files"]:
        warnings.append("drill: no Excellon drill file (.drl) was written")
    elif result["holes"] == 0:
        warnings.append("drill: the drill files have no holes")
    return result


def check_position(path, warnings):
    rows = parse_csv(read_text(path))
    if not rows:
        warnings.append("position: the placement file is empty")
        return {"parts": 0, "top": 0, "bottom": 0}, []
    header, body = rows[0], rows[1:]
    ref, side = column(header, "Ref"), column(header, "Side")
    sides = [cell(row, side).lower() for row in body]
    return ({"parts": len(body), "top": sides.count("top"), "bottom": sides.count("bottom")},
            [cell(row, ref) for row in body if cell(row, ref)])


def check_bom(path, warnings):
    rows = parse_csv(read_text(path))
    if not rows:
        warnings.append("bom: the BOM is empty")
        return {"lines": 0, "parts": 0, "lines_without_mpn": 0}, []
    header, body = rows[0], rows[1:]
    ref, qty, mpn = column(header, "Reference"), column(header, "Qty"), column(header, "MPN")
    refs, parts, missing = [], 0, []
    for row in body:
        line_refs = split_refs(cell(row, ref))
        refs += line_refs
        count = cell(row, qty)
        parts += int(count) if count.isascii() and count.isdigit() and len(count) <= 9 else len(line_refs)
        if mpn is not None and not cell(row, mpn):
            missing += line_refs or ["?"]
    if mpn is None:
        warnings.append("bom: no MPN column; order codes cannot be checked")
    elif missing:
        warnings.append(f"bom: {len(missing)} part(s) without an MPN: {shorten(missing)}")
    return {"lines": len(body), "parts": parts, "lines_without_mpn": len(missing)}, refs


def shorten(items, limit=20):
    return ", ".join(items[:limit]) + (f" and {len(items) - limit} more" if len(items) > limit else "")


def check_magic(path, magic, label, warnings):
    with open(path, "rb") as handle:
        start = handle.read(len(magic))
    if start != magic:
        warnings.append(f"{label}: {Path(path).name} is not a {magic.decode()} file")


def csv_file(names, expected):
    """The file to check: the expected name, else the only CSV written (options can change how
    KiCad names it), else None."""
    if expected in names:
        return expected
    tables = [n for n in names if n.lower().endswith(".csv")]
    return tables[0] if len(tables) == 1 else None


def verify(output_dir, outputs, copper_layers, name):
    """What each output contains, and warnings for anything a fab or assembler would reject."""
    warnings, result = [], {}
    root = Path(output_dir)
    for output, paths in outputs.items():
        for path in paths:
            if (root / path).stat().st_size == 0:
                warnings.append(f"{output}: {path} is empty")
    pnp_refs = bom_refs = None
    for output, paths in outputs.items():
        folder = root / PLAN[output][0]
        names = [p.split("/", 1)[1] for p in paths]
        if output == "gerbers":
            result[output] = check_gerbers(folder, names, copper_layers, warnings)
        elif output == "drill":
            result[output] = check_drill(folder, names, warnings)
        elif output in ("position", "bom"):
            target = csv_file(names, PLAN[output][3].format(name=name))
            if target is None:
                warnings.append(f"{output}: no single CSV file to check ({', '.join(names)})")
            elif output == "position":
                result[output], pnp_refs = check_position(folder / target, warnings)
            else:
                result[output], bom_refs = check_bom(folder / target, warnings)
        elif output in ("assembly_pdf", "schematic_pdf"):
            for item in names:
                check_magic(folder / item, b"%PDF", output, warnings)
        elif output == "step":
            for item in names:
                check_magic(folder / item, b"ISO-10303-21", output, warnings)
    if pnp_refs is not None and bom_refs is not None:
        bom_only = sorted(set(bom_refs) - set(pnp_refs))
        pnp_only = sorted(set(pnp_refs) - set(bom_refs))
        result["bom_vs_position"] = {"bom_only": bom_only, "position_only": pnp_only}
        if bom_only:
            warnings.append(f"bom: {len(bom_only)} part(s) in the BOM have no placement: {shorten(bom_only)}")
    return result, warnings


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def stored_zip(entries):
    """A deterministic ZIP archive (stored, dated 1980-01-01) of (name, bytes) entries."""
    out, central = bytearray(), bytearray()
    for name, data in entries:
        encoded = name.encode("utf-8")
        if len(data) > 0xFFFFFFFF or len(out) > 0xFFFFFFFF:
            raise ValueError("the Gerber archive would need ZIP64; leave zip_gerbers off")
        flags = 0 if name.isascii() else 0x0800
        crc = zlib.crc32(data) & 0xFFFFFFFF
        offset = len(out)
        out += struct.pack("<IHHHHHIIIHH", 0x04034B50, 20, flags, 0, 0, 0x21, crc, len(data), len(data),
                           len(encoded), 0) + encoded + data
        central += struct.pack("<IHHHHHHIIIHHHHHII", 0x02014B50, 20, 20, flags, 0, 0, 0x21, crc, len(data),
                               len(data), len(encoded), 0, 0, 0, 0, 0, offset) + encoded
    return bytes(out + central + struct.pack("<IHHHHIIH", 0x06054B50, 0, 0, len(entries), len(entries),
                                             len(central), len(out), 0))


def manifest_text(manifest):
    return json.dumps(manifest, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


class FabPackageTools:
    """live_pcb_fab_package; LiveSchematic provides send, document_revision and board_drc."""

    def fab_package(self, board, output_dir=None, schematic=None, outputs=None, options=None, checks=True,
                    allow_unsaved=False, overwrite=False, zip_gerbers=True):
        from board_client import bc, editor, jobs, types
        from client import document
        selected = select_outputs(outputs)
        requests = package_requests(selected, options)
        name = project_name(board)
        output_dir = output_dir or default_output_dir(board)
        zip_gerbers = bool(zip_gerbers) and "Gerbers" in {PLAN[o][0] for o in selected}
        folders, existing = check_output_dir(output_dir, selected, name, zip_gerbers, overwrite)
        with self.lock:
            # The design must be what is saved, and clean
            board_state = self.document_revision(board)
            unsaved = [board.board_filename] if board_state["modified"] else []
            schematic_state = None
            if any(o in SCHEMATIC_OUTPUTS for o in selected):
                if schematic is None:
                    try:
                        opened = self.send(editor.GetOpenDocuments(type=types.DOCTYPE_SCHEMATIC),
                                           editor.GetOpenDocumentsResponse)
                        found = json_format.MessageToDict(opened, preserving_proto_field_name=True)
                    except RuntimeError as error:
                        if "AS_UNHANDLED" not in str(error):
                            raise
                        found = {}
                    schematic = document(match_schematic(found.get("documents", []), board))
                schematic_state = self.document_revision(schematic)
                if schematic_state["modified"]:
                    unsaved.append(f"{schematic.project.name}.kicad_sch")
            result = {"exported": False, "output_dir": output_dir, "project": name,
                      "board_revision": board_state["revision"],
                      "schematic_revision": schematic_state["revision"] if schematic_state else None,
                      "unsaved": unsaved, "checks": None}
            if unsaved and not allow_unsaved:
                result["blocked_by"] = "unsaved"
                result["message"] = ("Save the design first (live_pcb_save / live_schematic_save) so the outputs "
                                     "match the files, or pass allow_unsaved=true")
                return result
            if checks:
                drc = self.board_drc(board, parity=True, refill=True, limit=None)
                result["checks"] = {"drc": drc_summary(
                    [(v["type"], v["severity"], v["description"]) for v in drc["violations"]],
                    drc["ignored"], drc["excluded"])}
                if result["checks"]["drc"]["errors"]:
                    result["blocked_by"] = "drc"
                    result["message"] = ("DRC (with schematic parity) reports errors; fix them (live_pcb_drc lists "
                                         "where) or pass checks=false")
                    return result
            copper_layers = None
            if "gerbers" in selected:
                from board_client import bt
                copper_layers = self.send(bc.GetBoardEnabledLayers(board=board),
                                          bc.BoardEnabledLayersResponse).copper_layer_count
                settings = requests["gerbers"].plot_settings
                if not settings.layers:
                    settings.layers.extend(bt.BoardLayer.Value(n) for n in gerber_layers(copper_layers))

            # Export, each output into its folder
            root = Path(output_dir)
            for item in existing:
                (root / item).unlink()
            for folder in folders:
                (root / folder).mkdir(parents=True, exist_ok=True)
            produced, kicad_warnings = {}, []
            for output in selected:
                folder, _, _, file_name = PLAN[output]
                request = requests[output]
                request.job_settings.document.CopyFrom(schematic if output in SCHEMATIC_OUTPUTS else board)
                request.job_settings.output_path = (join(output_dir, folder, file_name.format(name=name)) if file_name
                                                    else join(output_dir, folder) + "/")
                before = {p.name for p in (root / folder).iterdir()}
                response = self.send(request, jobs.RunJobResponse)
                if response.status not in (jobs.JS_SUCCESS, jobs.JS_WARNING):
                    raise RuntimeError(f"{output} export failed: {response.message}")
                new = sorted(p.name for p in (root / folder).iterdir() if p.name not in before and p.is_file())
                if not new:
                    raise RuntimeError(f"{output} export reported success but wrote nothing in "
                                       f"{join(output_dir, folder)}"
                                       + (f"; KiCad: {response.message}" if response.message else ""))
                produced[output] = [f"{folder}/{n}" for n in new]
                if response.message:
                    kicad_warnings.append(f"{output}: KiCad: {response.message}")

        verification, warnings = verify(output_dir, produced, copper_layers, name)
        warnings = kicad_warnings + warnings
        files = sorted(p for paths in produced.values() for p in paths)
        archive = None
        if zip_gerbers:
            archive = zip_name(name)
            members = [p for p in files if p.startswith("Gerbers/")]
            (root / archive).write_bytes(stored_zip([(p.split("/", 1)[1], (root / p).read_bytes()) for p in members]))
            files.append(archive)
        manifest = {"generator": "live_pcb_fab_package", "project": name, "board": board.board_filename,
                    "board_revision": result["board_revision"], "schematic_revision": result["schematic_revision"],
                    "unsaved": unsaved, "outputs": produced, "checks": result["checks"],
                    "verification": verification, "warnings": warnings,
                    "files": [{"path": p, "bytes": (root / p).stat().st_size, "sha256": sha256(root / p)}
                              for p in files]}
        (root / MANIFEST).write_text(manifest_text(manifest), encoding="utf-8", newline="\n")
        result.update({"exported": True, "outputs": produced, "verification": verification, "warnings": warnings,
                       "zip": archive, "manifest": MANIFEST, "files": len(files) + 1})
        return result
