# SPDX-License-Identifier: GPL-3.0-or-later
"""File-format version headers and export normalization for the KiCad 10.0.x compatibility checks.

Nothing here talks to KiCad.  `file_versions` reads the version a file actually declares (never the
application release number).  `normalize` removes the volatile parts of an export (creation dates,
generator versions, absolute paths, PDF cross-reference offsets) so the remaining bytes can be
compared with a committed golden hash.
"""

from hashlib import sha256
import io
import json
from pathlib import Path
import re
import tarfile
import zipfile

SEXPR_ROOTS = ("kicad_sch", "kicad_pcb", "kicad_symbol_lib", "footprint", "module", "kicad_wks",
               "sym_lib_table", "fp_lib_table", "design_block_lib_table")
LIB_TABLES = ("sym-lib-table", "fp-lib-table", "design-block-lib-table")


def _sexpr_header(text):
    """Root token, (version N), (generator "x") and (generator_version "x") near the top of a file."""
    head = text[:4096]
    root = re.match(r"\s*\(\s*([A-Za-z_]+)", head)
    result = {"root": root.group(1) if root else None}
    for key, pattern in (("version", r"\(\s*version\s+(\d+)\s*\)"),
                         ("generator", r"\(\s*generator\s+\"?([^\")\s]+)\"?\s*\)"),
                         ("generator_version", r"\(\s*generator_version\s+\"([^\"]*)\"\s*\)")):
        found = re.search(pattern, head)
        result[key] = (int(found.group(1)) if key == "version" else found.group(1)) if found else None
    return result


def _json_versions(data, path=""):
    """Every nested {"meta": {"version": N}} of a KiCad JSON settings file, keyed by its JSON path."""
    found = {}
    if isinstance(data, dict):
        meta = data.get("meta")
        if isinstance(meta, dict) and "version" in meta:
            found[path or "/"] = meta["version"]
        for key, value in data.items():
            if key != "meta":
                found.update(_json_versions(value, f"{path}/{key}"))
    return found


def file_versions(path):
    """The format version a KiCad file declares, read from its contents."""
    path = Path(path)
    name = path.name
    if path.suffix in (".kicad_pro", ".kicad_prl") or path.suffix == ".json":
        return {"kind": path.suffix.lstrip(".") or "json",
                "versions": _json_versions(json.loads(path.read_text(encoding="utf-8")))}
    text = path.read_text(encoding="utf-8", errors="replace")
    if path.suffix == ".kicad_dru":
        found = re.search(r"\(\s*version\s+(\d+)\s*\)", text)
        return {"kind": "kicad_dru", "version": int(found.group(1)) if found else None,
                "rules": len(re.findall(r"\(\s*rule\s", text))}
    header = _sexpr_header(text)
    header["kind"] = name if name in LIB_TABLES else path.suffix.lstrip(".")
    if path.suffix in (".kicad_mod", ".kicad_pcb", ".kicad_sch", ".kicad_sym"):
        header["embedded_files"] = len(re.findall(r"\(\s*embedded_files\b", text)) > 0
    return header


def project_versions(folder):
    """file_versions for every KiCad file in a project folder (libraries included), by relative path."""
    folder = Path(folder)
    patterns = ("*.kicad_pro", "*.kicad_prl", "*.kicad_sch", "*.kicad_pcb", "*.kicad_sym", "*.kicad_dru",
                "*.pretty/*.kicad_mod", *LIB_TABLES)
    result = {}
    for pattern in patterns:
        for path in sorted(folder.glob(pattern)):
            result[path.relative_to(folder).as_posix()] = file_versions(path)
    return result


# ---- Export normalization ------------------------------------------------------------------------

_ISO_DATETIME = rb"\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}(?::\d{2})?(?:[.,]\d+)?(?:Z|[+-]\d{2}:?\d{2})?"
_SLASH_DATETIME = rb"\d{4}/\d{2}/\d{2}[ .]\d{2}:\d{2}(?::\d{2})?"
_LOCALE_DATETIME = rb"\d{1,2}/\d{1,2}/\d{4},? \d{1,2}:\d{2}(?::\d{2})?(?: ?[AaPp]\.?[Mm]\.?)?"
_CTIME = rb"(?:Mon|Tue|Wed|Thu|Fri|Sat|Sun),? \d{1,2}? ?(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]* " \
         rb"\d{1,2},? (?:\d{4} )?\d{1,2}:\d{2}:\d{2}(?: ?[AP]M)?(?: [A-Z]{2,5})?(?: \d{4})?"
# An application version (10.0.6, 10.99, 10.99.0-unknown, 10.99.0-123-gabc1234), never running into
# a following digit or ".digit".  It is only replaced after one of _VERSION_CONTEXTS: a bare number
# such as an SVG coordinate (M10.000), a footprint name (CP_Axial_L10.0mm) or a value (10.0k) is
# design content and must stay distinct.
_VERSION = rb"\d+\.\d+(?:\.\d+)*(?:-[0-9A-Za-z.+_-]*[0-9A-Za-z+_])?(?:,? ?release build)?(?!\.?\d)"

# Every place the exports of KiCad 10.0.6 and the fork name the generating application's version
# (found by scanning both builds' outputs of the fixture and of the reference demo project).
_VERSION_CONTEXTS = (
    rb"\(\s*generator_version\s+\"",                         # s-expression files (library rewrites)
    rb"\(\s*tool\s+\"(?:Eeschema|Pcbnew)\s+",                 # KiCad netlist (design (tool ...))
    rb"\(\s*host_version\s+\"",                              # Specctra DSN
    rb"\"kicad_version\"\s*:\s*\"",                          # ERC and DRC JSON reports
    rb"KiCad E\.D\.A\.\s+",                                  # ${KICAD_VERSION} in plotted title blocks
    rb"KiCad EDA\s+",                                        # ODB++ HDR and SAVE_APP
    rb"(?:Created|Printed) by KiCad(?: version)?\s+",        # IDF headers, position files
    rb"\((?:PCBNEW|EESCHEMA)\s+",                            # Gerber "G04 Created by KiCad (PCBNEW x)"
    rb"TF\.GenerationSoftware,(?i:kicad),(?:Pcbnew|Eeschema),",  # Gerber and drill attributes
    rb"DRILL file \{?KiCad\s+",                              # Excellon header
    rb"^USER\s+\"KiCad\s+",                                  # GenCAD header
    rb"<SoftwarePackage\s+name=\"KiCad\"\s+revision=\"",     # IPC-2581
)
_VERSION_RULE = re.compile(rb"(?P<context>" + b"|".join(_VERSION_CONTEXTS) + rb")" + _VERSION, re.M)

# Rules that hold for every text export: generation time stamps and the application version string.
_COMMON_RULES = (
    (re.compile(_ISO_DATETIME), b"<DATETIME>"),
    (re.compile(_SLASH_DATETIME), b"<DATETIME>"),
    (re.compile(_LOCALE_DATETIME), b"<DATETIME>"),
    (re.compile(_CTIME), b"<DATETIME>"),
    (re.compile(rb"\(\s*date\s+\"[^\"]*\"\s*\)"), b"(date \"<DATE>\")"),
)

_SUFFIX_RULES = {
    ".pdf": (
        (re.compile(rb"/(CreationDate|ModDate)\s*\(D:[^)]*\)"), rb"/\1 (<DATE>)"),
        (re.compile(rb"/(Producer|Creator)\s*\((?:[^()\\]|\\.)*\)"), rb"/\1 (<TOOL>)"),
        (re.compile(rb"/ID\s*\[\s*<[0-9A-Fa-f]*>\s*<[0-9A-Fa-f]*>\s*\]"), b"/ID [<ID>]"),
        # Byte offsets move with any length change above; they carry no content
        (re.compile(rb"\nxref\r?\n.*?(?=\ntrailer)", re.S), b"\nxref <OFFSETS>"),
        (re.compile(rb"startxref\s+\d+"), b"startxref <OFFSET>"),
    ),
    ".ps": (
        (re.compile(rb"^%%(CreationDate|Creator|Title):.*$", re.M), rb"%%\1: <VOLATILE>"),
    ),
    ".step": (
        (re.compile(rb"FILE_NAME\(.*?\);", re.S), b"FILE_NAME(<VOLATILE>);"),
    ),
    ".stp": (
        (re.compile(rb"FILE_NAME\(.*?\);", re.S), b"FILE_NAME(<VOLATILE>);"),
    ),
    ".gbrjob": (
        (re.compile(rb"\"(CreationDate|GenerationSoftware)\"\s*:\s*(\{[^}]*\}|\"[^\"]*\")"), rb'"\1": "<VOLATILE>"'),
    ),
    ".odb": (),
}

# Gerber/drill attribute lines that only identify the generating application or time
_GERBER_RULES = (
    (re.compile(rb"^G04 Created by KiCad.*$", re.M), b"G04 Created by KiCad <VOLATILE>*"),
    (re.compile(rb"^(%?)(; #@! )?TF\.(CreationDate|GenerationSoftware),[^*\r\n]*", re.M), rb"\1\2TF.\3,<VOLATILE>"),
    (re.compile(rb"^%TF\.(CreationDate|GenerationSoftware),[^*]*", re.M), rb"%TF.\1,<VOLATILE>"),
    (re.compile(rb"^; DRILL file \{.*$", re.M), b"; DRILL file <VOLATILE>"),
)
for _suffix in (".gbr", ".gtl", ".gbl", ".gto", ".gbo", ".gts", ".gbs", ".gtp", ".gbp", ".gm1", ".gko", ".drl"):
    _SUFFIX_RULES[_suffix] = _GERBER_RULES

_LINE_RULES = (
    # Position, IPC-D-356, GenCAD, IDF, statistics and Specctra headers name the tool and time
    (re.compile(rb"^### Footprint positions - created on .*$", re.M), b"### Footprint positions - created on <DATE>"),
    (re.compile(rb"^### Printed by KiCad.*$", re.M), b"### Printed by KiCad <VERSION>"),
    (re.compile(rb"^C  (IPC-D-356 Netlist Generated by|File:|Created by|Date|Generated).*$", re.M), b"C  <VOLATILE>"),
    (re.compile(rb"^(USER|DRAWING|REVISION) .*$", re.M), rb"\1 <VOLATILE>"),
    (re.compile(rb"^(BOARD_FILE|LIBRARY_FILE) (\S+) .*$", re.M), rb"\1 \2 <VOLATILE>"),
    (re.compile(rb"\(host_version\s+\"[^\"]*\"\)"), b"(host_version \"<VERSION>\")"),
    (re.compile(rb"^- Date: .*$", re.M), b"- Date: <DATETIME>"),
)

# Exports that are not reproducible between identical runs, compared by structure only:
# - IPC-2581 and ODB++: KiCad 10.0.6 emits records in hash order and numbers its symbol/padstack
#   tables in that order, so even individual lines change (histogram of XML element names, or of
#   record types per ODB++ member).
# - STEP: OpenCASCADE numbers products and assembly usages with per-process counters and orders the
#   colour (styled item) entities differently on each export (histogram of entity types).
# Record counts still catch added or lost items; changed coordinates are not detected.
STRUCTURAL = (".xml", ".step", ".stp")

_ODB_INFO = re.compile(rb"^(CREATION_DATE|SAVE_DATE|SAVE_APP|SAVE_USER)=.*$", re.M)


def _replace(data, rules):
    for pattern, replacement in rules:
        data = pattern.sub(replacement, data)
    return data


def _histogram(keys):
    counts = {}
    for key in keys:
        counts[key] = counts.get(key, 0) + 1
    return json.dumps(dict(sorted(counts.items())), indent=0).encode()


def structure(data, kind=""):
    """Order- and numbering-independent summary of an export (see STRUCTURAL)."""
    if kind == ".xml":
        return _histogram(m.decode() for m in re.findall(rb"<([A-Za-z][\w:.-]*)", data))
    if kind in (".step", ".stp"):
        return _histogram(m.decode() for m in re.findall(rb"^#\d+\s*=\s*([A-Z0-9_]+)\s*\(", data, re.M))
    return _histogram(line.split(None, 1)[0].decode(errors="replace") if line.strip() else ""
                      for line in data.splitlines())


def _path_spellings(folder):
    """A folder as KiCad may write it: POSIX, WSL /mnt/<drive>, Windows, and s-expression escaped."""
    posix = folder.replace("\\", "/")
    forms = {posix}
    drive = re.match(r"^/mnt/([a-zA-Z])(/.*)$", posix)
    if drive:
        forms.add(f"{drive.group(1).upper()}:{drive.group(2)}")
    drive = re.match(r"^([a-zA-Z]):(/.*)$", posix)
    if drive:
        forms.add(f"/mnt/{drive.group(1).lower()}{drive.group(2)}")
    for form in list(forms):
        if re.match(r"^[A-Z]:", form):
            forms.add(form.replace("/", "\\"))
            forms.add(form.replace("/", "\\\\"))
    return sorted(forms, key=len, reverse=True)


def normalize_bytes(data, name, project_dirs=(), structural=False):
    """Normalized content of one export file called `name`.

    project_dirs are absolute folders (the project copy, the output folder) to replace by a
    placeholder, in every spelling KiCad might use (WSL, Windows, forward or back slashes).
    """
    suffix = Path(name).suffix.lower()
    if suffix in (".zip", ".tgz") or name.endswith(".tar.gz"):
        return _normalize_archive(data, suffix, project_dirs)
    # Longest first, so a folder inside another is replaced before its parent; ties by name keep the
    # placeholder numbering independent of set order
    for index, folder in enumerate(sorted({str(d) for d in project_dirs if d}, key=lambda d: (-len(d), d))):
        for spelling in _path_spellings(folder):
            data = data.replace(spelling.encode(), b"<DIR%d>" % index)
    data = _replace(data, _SUFFIX_RULES.get(suffix, ()))
    data = _replace(data, _COMMON_RULES)
    if suffix not in (".pdf",):
        data = _replace(data, _LINE_RULES)
    if Path(name).name == "info" or suffix == "":
        data = _ODB_INFO.sub(rb"\1=<VOLATILE>", data)
    data = _VERSION_RULE.sub(rb"\g<context><VERSION>", data).replace(b"\r\n", b"\n")
    if suffix in STRUCTURAL or structural:
        return structure(data, suffix if suffix in STRUCTURAL else "")
    return data


def _normalize_archive(data, suffix, project_dirs):
    """An archive (ODB++) normalizes to a manifest of its members' structural hashes; archive member
    dates are ignored."""
    members = {}
    if suffix == ".zip":
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            for info in archive.infolist():
                if not info.is_dir():
                    members[info.filename] = archive.read(info)
    else:
        with tarfile.open(fileobj=io.BytesIO(data)) as archive:
            for info in archive.getmembers():
                if info.isfile():
                    members[info.name] = archive.extractfile(info).read()
    manifest = {name: sha256(normalize_bytes(content, name, project_dirs, structural=True)).hexdigest()
                for name, content in members.items()}
    return json.dumps(manifest, sort_keys=True, indent=1).encode()


def normalized_tree(folder, project_dirs=()):
    """{relative path: sha256 of normalized content} for every file below folder."""
    folder = Path(folder)
    result = {}
    for path in sorted(p for p in folder.rglob("*") if p.is_file()):
        relative = path.relative_to(folder).as_posix()
        result[relative] = sha256(normalize_bytes(path.read_bytes(), path.name,
                                                  (*project_dirs, folder))).hexdigest()
    return result


def tree_hash(manifest):
    """One hash for a {path: hash} manifest (canonical JSON), used as the golden value of an export."""
    return sha256(json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
