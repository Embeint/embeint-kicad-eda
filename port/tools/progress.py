#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Build the public Rust port progress site using Python 3.11+ (standard library only).

Repository records describe inventory and declared progress, never a current CI pass.
No network, clock, working-tree designs or executable Markdown are used in the build.
"""

import argparse
from collections import Counter
import gzip
import html
import json
from pathlib import Path, PurePosixPath
import re
import shutil
import sys
import tomllib
from urllib.parse import quote, urlsplit


ROOT = Path(__file__).resolve().parents[2]
REPO = "https://github.com/Embeint/embeint-kicad-eda"
STATUSES = ("todo", "claimed", "partial", "green", "blocked")
COMPLETENESS = ("inventory_complete", "parse_complete", "compile_database_complete",
                "all_configurations_complete", "gate_complete")
CORE = ("id", "upstream", "rust_module", "depends_on", "soft_depends_on", "est_lines",
        "layer", "tests", "status", "claimed_by", "claimed_at", "notes")
UNIT_ID = re.compile(r"[A-Za-z0-9_][A-Za-z0-9_.-]*\Z")
esc = html.escape


def public_path(value):
    """Accept repository-relative POSIX paths only, including when running on Windows."""
    if not isinstance(value, str) or not value or "\\" in value or ":" in value:
        raise ValueError("expected a repository-relative POSIX path")
    path = PurePosixPath(value)
    if path.is_absolute() or ".." in path.parts or any(ord(c) < 32 for c in value):
        raise ValueError("expected a repository-relative POSIX path")
    return path.as_posix()


def read_text(root, relative):
    path = root / public_path(relative)
    resolved = path.resolve()
    if not resolved.is_relative_to(root.resolve()):
        raise ValueError(f"{relative}: source escapes the repository")
    if path.suffix == ".gz":
        with gzip.open(path, "rt", encoding="utf-8") as stream:
            return stream.read()
    return path.read_text(encoding="utf-8")


def files(root, directory, pattern):
    return sorted(p.relative_to(root).as_posix() for p in (root / directory).glob(pattern)
                  if p.is_file())


def source_url(relative, tree=False):
    if relative.startswith('@build/'):
        relative = 'port/extractor/generated/sources/' + relative.removeprefix('@build/')
    return f"{REPO}/{'tree' if tree else 'blob'}/rust/{quote(public_path(relative), safe='/')}"


def link(url, label):
    return f'<a href="{esc(url, quote=True)}">{esc(str(label))}</a>'


def source_link(relative, label=None, tree=False):
    return link(source_url(relative, tree), label or relative)


def safe_url(value):
    if not isinstance(value, str) or any(c.isspace() or ord(c) < 32 for c in value):
        return False
    try:
        parsed = urlsplit(value)
        return (parsed.scheme in ("https", "http") and bool(parsed.hostname)
                and parsed.username is None and parsed.password is None)
    except ValueError:
        return False


def inline(text):
    """Render a deliberately small Markdown subset; raw HTML always stays text."""
    token = re.compile(r"`([^`\n]+)`|\[([^]\n]+)\]\(([^)\n]+)\)|(https?://[^\s<>]+)|(?<!\w)#([1-9][0-9]*)")
    result, last = [], 0
    for match in token.finditer(str(text)):
        result.append(esc(str(text)[last:match.start()]))
        code, label, url, bare, pr = match.groups()
        if code is not None:
            result.append(f"<code>{esc(code)}</code>")
        elif pr:
            result.append(link(f"{REPO}/pull/{pr}", f"#{pr}"))
        elif safe_url(url or bare):
            result.append(link(url or bare, label or bare))
        else:
            result.append(esc(match.group()))
        last = match.end()
    result.append(esc(str(text)[last:]))
    return "".join(result)


def markdown(text):
    # Each line is escaped independently; no HTML, embedded images or remote assets.
    return "\n".join(f'<p>{inline(line)}</p>' for line in text.splitlines() if line.strip())


def load_units(root):
    units = []
    for path in files(root, "port/units", "*.toml"):
        unit = tomllib.loads(read_text(root, path))
        uid = unit.get("id")
        if not isinstance(uid, str) or not UNIT_ID.fullmatch(uid) or uid != Path(path).stem:
            raise ValueError(f"{path}: id must match the file name and be a safe unit id")
        if missing := set(CORE) - unit.keys():
            raise ValueError(f"{path}: missing fields: {', '.join(sorted(missing))}")
        if unit["status"] not in STATUSES:
            raise ValueError(f"{path}: unknown status {unit['status']!r}")
        for field in ("est_lines", "layer"):
            if type(unit[field]) is not int or unit[field] < 0:
                raise ValueError(f"{path}: {field} must be a nonnegative integer")
        for field in ("rust_module", "claimed_by", "claimed_at", "notes"):
            if not isinstance(unit[field], str):
                raise ValueError(f"{path}: {field} must be a string")
        for field in ("upstream", "depends_on", "soft_depends_on", "tests"):
            if not isinstance(unit[field], list) or not all(isinstance(v, str) for v in unit[field]):
                raise ValueError(f"{path}: {field} must be a string array")
        for upstream in unit["upstream"]:
            public_path(upstream)
        units.append(unit)
    known = {unit["id"] for unit in units}
    for unit in units:
        for dependency in unit["depends_on"] + unit["soft_depends_on"]:
            if dependency not in known:
                raise ValueError(f"{unit['id']}: unknown dependency {dependency!r}")
    return units


def load_pins(root):
    if not (root / "port/pins.toml").exists():
        return {}
    pins = tomllib.loads(read_text(root, "port/pins.toml"))
    if pins.get("schema_version") != 1:
        raise ValueError("port/pins.toml: unsupported schema_version")
    for name in ("kicad", "solidcore"):
        pin = pins.get(name, {})
        if not safe_url(pin.get("url")) or not re.fullmatch(r"[0-9a-f]{40}", pin.get("revision", "")):
            raise ValueError(f"port/pins.toml: invalid {name} URL or revision")
    return pins


def no_duplicate_keys(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key {key!r}")
        result[key] = value
    return result


def invalid_number(value):
    raise ValueError(f"non-JSON number {value}")


def json_load(text):
    return json.loads(text, object_pairs_hook=no_duplicate_keys, parse_constant=invalid_number)


def record_counts(root, uid, directory):
    path = f"port/{directory}/{uid}.jsonl"
    if not (root / path).exists():
        return None
    count, deferred, errors, ids = 0, 0, 0, set()
    for number, line in enumerate(read_text(root, path).splitlines(), 1):
        if not line.strip():
            continue
        record = json_load(line)
        case = record.get("case") if isinstance(record, dict) else None
        if (not isinstance(record, dict) or record.get("unit") != uid
                or not isinstance(case, str) or not case or case in ids):
            raise ValueError(f"{path}:{number}: invalid or duplicate case identity")
        if directory == "cases":
            if set(record) != {"unit", "case", "input"} or not isinstance(record["input"], dict):
                raise ValueError(f"{path}:{number}: invalid request")
        else:
            payload = set(record) - {"unit", "case"}
            if payload not in ({"result"}, {"error"}):
                raise ValueError(f"{path}:{number}: invalid reply")
            if "error" in record:
                error = record["error"]
                if not isinstance(error, dict) or not isinstance(error.get("kind"), str):
                    raise ValueError(f"{path}:{number}: invalid error")
                if error["kind"] in ("unported", "deferred"):
                    deferred += 1
                else:
                    errors += 1
        ids.add(case)
        count += 1
    return {"path": path, "records": count, "deferred": deferred, "failures": errors}


def summary(units):
    counts = Counter(unit["status"] for unit in units)
    return {"units": len(units), "count": {s: counts[s] for s in STATUSES},
            "total_lines": sum(u["est_lines"] for u in units),
            "green_lines": sum(u["est_lines"] for u in units if u["status"] == "green"),
            "partial_lines": sum(u["est_lines"] for u in units if u["status"] == "partial")}


def load_extraction(root):
    artifacts, paths = {}, {}
    for name in ("graph", "inventory", "units"):
        path = f"port/extractor/generated/{name}.json"
        if name == "graph" and not (root / path).exists():
            path += ".gz"
        if (root / path).exists():
            data = json_load(read_text(root, path))
            if not isinstance(data, dict) or data.get("schema_version") != 1:
                raise ValueError(f"{path}: unsupported extraction schema")
            artifacts[name] = data
            paths[name] = path
    if not artifacts:
        return None
    provenance = artifacts.get("inventory", artifacts.get("graph", {})).get("provenance", {})
    if "graph" in artifacts and "inventory" in artifacts:
        if artifacts["graph"].get("provenance") != provenance:
            raise ValueError("graph and inventory provenance disagree")
    for key in COMPLETENESS:
        if key in provenance and type(provenance[key]) is not bool:
            raise ValueError(f"extractor provenance: {key} must be a boolean")
    entries = artifacts.get("inventory", {}).get("files", [])
    if not isinstance(entries, list):
        raise ValueError("inventory.files must be an array")
    for entry in entries:
        public_path(entry["path"])
    graph = artifacts.get("graph", {})
    functions = graph.get("functions", {})
    back_edges = artifacts.get("units", {}).get("back_edges", [])
    for edge in back_edges:
        if any(not isinstance(edge.get(k), str) or not UNIT_ID.fullmatch(edge[k])
               for k in ("caller", "callee")):
            raise ValueError("back-edge caller/callee must be unit ids")
    return {"artifacts": sorted(artifacts), "paths": paths, "provenance": provenance, "files": entries,
            "function_count": len(functions) if "graph" in artifacts else None,
            "edge_count": len(graph.get("edges", [])) if "graph" in artifacts else None,
            "function_wx": dict(Counter(f.get("wx_class", "unknown") for f in functions.values())),
            "back_edges": back_edges}


def table(headers, rows):
    return ('<div class="table-wrap"><table><thead><tr>'
            + "".join(f"<th scope=\"col\">{esc(h)}</th>" for h in headers)
            + "</tr></thead><tbody>"
            + "".join("<tr>" + "".join(f"<td>{cell}</td>" for cell in row) + "</tr>" for row in rows)
            + "</tbody></table></div>")


def badge(status):
    return f'<span class="badge {status}">{status}</span>'


def status_chart(stats):
    if not stats["units"]:
        return '<p class="empty">No unit records available. Progress cannot be calculated.</p>'
    x, rectangles = 0, []
    for status in STATUSES:
        width = 100 * stats["count"][status] / stats["units"]
        rectangles.append(f'<rect class="{status}" x="{x:.6f}" y="0" width="{width:.6f}" height="12"/>')
        x += width
    return ('<svg class="status-chart" viewBox="0 0 100 12" role="img" '
            'aria-label="Unit counts by reported status; partial is separate from green">'
            '<title>Reported unit status</title>' + "".join(rectangles) + '</svg>')


def page(title, body, depth=0):
    prefix = "../" * depth
    nav = "".join(f'<a href="{prefix}{path}">{label}</a>' for path, label in (
        ("index.html", "Overview"), ("units/index.html", "Units"), ("layers.html", "Dependencies"),
        ("inventory.html", "Inventory"), ("records.html", "Findings & issues")))
    return f'''<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'self'; img-src 'self'; base-uri 'none'; form-action 'none'">
<title>{esc(title)} | Embeint KiCad Rust port</title><link rel="stylesheet" href="{prefix}style.css"></head>
<body><a class="skip" href="#content">Skip to content</a><header><p class="eyebrow">Embeint / KiCad</p>
<p class="brand">Rust port progress</p><nav aria-label="Main">{nav}</nav></header>
<main id="content"><h1>{esc(title)}</h1>{body}</main>
<footer>Generated from public repository records. Declared status and stored expectations are not current CI results.
{link(REPO + '/actions?query=branch%3Arust', 'CI on rust')} · {source_link('rust/PORT_PLAN.md', 'Port plan')} · GPL-3.0-or-later</footer></body></html>
'''


def evidence_files(root):
    return sorted(set(files(root, "port/goldens/evidence", "*.json")
                      + files(root, "port/evidence", "*.json")
                      + files(root, "port/evidence", "*.md")))


def unit_evidence(uid, evidence):
    return [path for path in evidence if Path(path).stem == uid or Path(path).stem.startswith(uid + "-")]


def counts_cell(counts):
    if counts is None:
        return "Not available"
    return source_link(counts["path"], f'{counts["records"]:,} records')


def records_body(root):
    parts = []
    for directory, title in (("findings", "Findings"), ("upstream-issues", "Upstream issues")):
        parts.append(f"<h2>{title}</h2>")
        paths = files(root, f"port/{directory}", "*.md")
        if not paths:
            parts.append("<p>No records available. This is not evidence that no issues exist.</p>")
        for path in paths:
            parts.append(f'<article><h3>{source_link(path)}</h3>{markdown(read_text(root, path))}</article>')
    return "\n".join(parts)


def inventory_body(extraction, units, details=False):
    if extraction is None:
        return ("<p>Generated inventory not available. Unit records alone do not establish complete extraction.</p>"
                "<p>The compile database covers one build configuration. Platform and uncompiled sources, "
                "parse failures and unresolved calls must be accounted for explicitly.</p>")
    provenance, entries = extraction["provenance"], extraction["files"]
    labels = {"inventory_complete": "Tracked source inventory",
              "parse_complete": "Parsing attempted commands",
              "compile_database_complete": "Compile database processing",
              "all_configurations_complete": "All platform / preprocessor configurations",
              "gate_complete": "Extractor completeness (not the plan phase 0 gate)"}
    rows = []
    for key in COMPLETENESS:
        value = provenance.get(key)
        rows.append([esc(labels[key]), f"<code>{key}</code>",
                     "Not available" if value is None else "true" if value else "false"])
    body = '<p>Generated inventory available. Completeness is reported independently for each scope.</p>'
    body += table(["Scope", "Provenance field", "Reported value"], rows)
    missing = sorted({"graph", "inventory", "units"} - set(extraction["artifacts"]))
    if missing:
        body += '<p class="warning">Generated artifacts not available: ' + esc(", ".join(missing)) + '. Accounting is incomplete.</p>'
    for key, label in (("compile_commands_count", "Compile commands in database"),
                       ("commands_attempted", "Commands attempted"),
                       ("unresolved_call_count", "Unresolved calls")):
        body += f'<p>{label}: {esc(str(provenance.get(key, "Not available")))}</p>'
    coverage = Counter(entry.get("coverage", "unknown") for entry in entries)
    categories = Counter(entry.get("category", "unknown") for entry in entries)
    wx = Counter(unit.get("wx_class", "unknown") for unit in units)
    paths = f"{len(entries):,}" if "inventory" in extraction["artifacts"] else "Not available:"
    functions = f'{extraction["function_count"]:,}' if extraction["function_count"] is not None else "Not available:"
    edges = f'{extraction["edge_count"]:,}' if extraction["edge_count"] is not None else "Not available:"
    body += f'<p>{paths} tracked C/C++ paths; {functions} extracted functions; {edges} call edges.</p>'
    body += table(["Source category", "Paths"], [[esc(k), str(v)] for k, v in sorted(categories.items())])
    body += table(["Coverage", "Paths"], [[esc(k), str(v)] for k, v in sorted(coverage.items())])
    body += table(["wx classification", "Units", "Functions"], [
        [esc(k), str(wx[k]), str(extraction["function_wx"].get(k, 0))]
        for k in sorted(set(wx) | set(extraction["function_wx"]))])
    failures = provenance.get("parse_failures")
    uncompiled = provenance.get("uncompiled_application_sources")
    body += '<p>Parse failures: ' + (str(len(failures)) if isinstance(failures, list) else "Not available")
    body += '. Uncompiled application / generated sources: ' + (str(len(uncompiled)) if isinstance(uncompiled, list) else "Not available") + '.</p>'
    body += '<p>A Linux WSL graph covers active preprocessor branches in that compile database. '
    body += 'It does not establish Windows, macOS or inactive-branch coverage. These diagnostics do not redefine the plan phase 0 gate.</p>'
    body += '<p>Back-edges recorded by the weighted cycle breaker: ' + (
        str(len(extraction["back_edges"])) if "units" in extraction["artifacts"] else "Not available") + '.</p>'
    body += '<p>' + " · ".join(source_link(extraction["paths"][name], Path(extraction["paths"][name]).name)
                                for name in extraction["artifacts"]) + '</p>'
    if not details:
        return body + '<p><a href="inventory.html">Inspect coverage, source accounting and limitations</a>.</p>'
    body += '<h2>Configuration and provenance</h2>'
    # Publish identity and hashes, never local build paths or compiler diagnostics.
    for key in ("configuration", "upstream_pin", "source_commit", "source_inventory_sha256",
                "compile_commands_sha256", "clang_python"):
        if key in provenance:
            body += f'<p>{esc(key)}: <code>{esc(str(provenance[key]))}</code></p>'
    for limitation in provenance.get("limitations", []):
        body += f'<p>{esc(str(limitation))}</p>'
    body += '<h2>Parse failures</h2>'
    if failures:
        body += table(["Source", "Status"], [[source_link(f["file"]), esc(str(f.get("status", "unknown")))] for f in failures])
        body += '<p>Detailed diagnostics remain in the source artifact; local build paths are not reproduced here.</p>'
    else:
        body += '<p>No recorded parse failures.</p>' if failures == [] else '<p>Parse failure accounting not available.</p>'
    body += '<h2>Uncompiled application / generated sources</h2>'
    body += ("<ul>" + "".join(f"<li>{source_link(p)}</li>" for p in sorted(uncompiled)) + "</ul>"
             if uncompiled else '<p>None recorded.</p>' if uncompiled == [] else '<p>Accounting not available.</p>')
    body += '<h2>Per-file source accounting</h2>'
    body += table(["Source on rust", "Category", "Coverage", "Physical lines"], [
        [source_link(entry["path"]), esc(str(entry.get("category", "unknown"))),
         esc(str(entry.get("coverage", "unknown"))), esc(str(entry.get("lines", "unknown")))]
        for entry in sorted(entries, key=lambda e: e["path"])])
    return body


def build(root, out):
    root, out = Path(root).resolve(), Path(out).resolve()
    if out == root or root.is_relative_to(out) or out.is_relative_to(root / "port"):
        raise ValueError("output must be outside source directories (for example _site)")
    declared_units, pins = load_units(root), load_pins(root)
    extraction = load_extraction(root)
    evidence = evidence_files(root)
    cases = {u["id"]: record_counts(root, u["id"], "cases") for u in declared_units}
    goldens = {u["id"]: record_counts(root, u["id"], "goldens") for u in declared_units}
    units = []
    for unit in declared_units:
        unit = dict(unit, declared_status=unit["status"])
        stored = goldens[unit["id"]]
        if unit["status"] == "green":
            if stored and stored["failures"]:
                unit["status"] = "blocked"
            elif unit.get("coverage_gaps") or not unit["tests"] or (stored and stored["deferred"]):
                unit["status"] = "partial"
        units.append(unit)
    stats = summary(units)
    out.mkdir(parents=True, exist_ok=True)
    marker = out / ".progress-site"
    if any(out.iterdir()) and not marker.is_file():
        raise ValueError("output is nonempty and is not owned by the progress generator")
    for path in ("units", "style.css", "index.html", "layers.html", "inventory.html", "records.html",
                 "progress.json", ".nojekyll", ".progress-site"):
        candidate = out / path
        if candidate.is_symlink() or not candidate.resolve().is_relative_to(out):
            raise ValueError("output must not contain symlinks")
    (out / "units").mkdir(exist_ok=True)
    for path in (out / "units").glob("*.html"):
        if path.is_symlink() or not path.resolve().is_relative_to(out):
            raise ValueError("output must not contain symlinks")
        path.unlink()
    marker.write_text("Generated by port/tools/progress.py\n", encoding="utf-8", newline="\n")
    shutil.copyfile(ROOT / "port/site/style.css", out / "style.css")

    def write(path, title, body, depth=0):
        (out / path).write_text(page(title, body, depth), encoding="utf-8", newline="\n")

    pin_body = "<p>Upstream pin not available. Waiting for port/pins.toml.</p>"
    if pins:
        pin_body = "<dl>"
        for name, label in (("kicad", "KiCad upstream"), ("solidcore", "solidcore reference")):
            pin = pins[name]
            pin_body += f'<dt>{label}</dt><dd>{link(pin["url"].removesuffix(".git") + "/tree/" + pin["revision"], pin["revision"])}</dd>'
        trailer = pins["solidcore"].get("solidcraft_revision")
        if trailer:
            pin_body += f'<dt>SolidCraft trailer</dt><dd>{esc(str(trailer))}</dd>'
        pin_body += f'</dl><p>{source_link("port/pins.toml", "Canonical pins")}</p>'
    legend = " ".join(f'{badge(s)} {stats["count"][s]:,}' for s in STATUSES)
    overview = f'''<p class="lead">A faithful port of KiCad to Rust, built in dependency layers and checked against pinned C++ KiCad.</p>
<section><h2>Unit status from repository records</h2>{status_chart(stats)}<p class="legend">{legend}</p>
<p><strong>{stats['count']['green']:,} / {stats['units']:,} units declared green without recorded contradictions</strong>.
{stats['count']['partial']:,} partial units remain incomplete. Deferred cases never qualify as green.</p>
<p>A green declaration contradicted by stored deferrals, coverage gaps or missing tests is reported as partial here;
stored failed replies are reported as blocked.
the original declaration remains visible on its unit page.</p>
<p>{stats['green_lines']:,} / {stats['total_lines']:,} estimated upstream lines in green units;
{stats['partial_lines']:,} lines in partial units, shown separately.</p></section>
<section class="gate"><h2>Phase 0 incomplete: pending pilot verification</h2>
<p>The plan gate requires at least three units green against the live oracle in CI and every generated unit file valid, with coverage gaps disclosed.
Zero records cannot satisfy it. Unit status, case counts, golden records and blessing evidence alone do not certify this gate.</p>
<p>This site does not execute the oracle or certify current CI. Extractor completeness and all-platform coverage are separate diagnostics.</p>
<p>{link(REPO + '/actions?query=branch%3Arust', 'Inspect the current CI evidence')}.</p></section>
<section><h2>Pinned references</h2>{pin_body}</section>
<section><h2>Inventory coverage</h2>{inventory_body(extraction, units)}</section>
<section><h2>Comparison contract</h2><p>Unit oracle results compare exactly, with no tolerance.
Stored goldens record expectations; they prove neither correctness nor a current runtime pass.
Unported or deferred replies remain incomplete. Other error replies are failures; legitimate upstream operation statuses belong inside result.</p>
<p>Public test data only: KiCad QA data, demos, official libraries and generated cases.</p></section>'''
    write("index.html", "Progress overview", overview)
    rows, layer_rows, back_rows = [], [], []
    status = {u["id"]: u["status"] for u in units}
    if extraction:
        for edge in sorted(extraction["back_edges"], key=lambda e: (e["caller"], e["callee"])):
            back_rows.append([link(f"units/{quote(edge['caller'])}.html", edge["caller"]),
                              link(f"units/{quote(edge['callee'])}.html", edge["callee"]),
                              esc(str(edge.get("weight", "unknown"))), str(len(edge.get("calls", [])))])
    for unit in sorted(units, key=lambda u: (u["layer"], u["id"])):
        uid = unit["id"]
        unit_link = f'<a href="{quote(uid)}.html">{esc(uid)}</a>'
        rows.append([unit_link, badge(unit["status"]), str(unit["layer"]),
                     esc(str(unit.get("wx_class", "Not available"))),
                     esc(str(unit.get("source_category", "Not available"))), f'{unit["est_lines"]:,}',
                     counts_cell(cases[uid]), counts_cell(goldens[uid])])
        dependencies = ", ".join(esc(d) + " (" + badge(status[d]) + ")" for d in unit["depends_on"]) or "None"
        soft = ", ".join(esc(d) for d in unit["soft_depends_on"]) or "None"
        layer_rows.append([str(unit["layer"]), link(f"units/{quote(uid)}.html", uid), dependencies, soft])
        upstream = "<ul>" + "".join(f"<li>{source_link(p)}</li>" for p in unit["upstream"]) + "</ul>"
        tests = "<ul>" + "".join(f"<li>{inline(t)}</li>" for t in unit["tests"]) + "</ul>"
        stored = goldens[uid]
        warning = ""
        if unit["declared_status"] != unit["status"]:
            warning += f'<p class="warning">Declared status: green. Reported as {unit["status"]} because recorded evidence cannot support green.</p>'
        if stored and stored["failures"]:
            warning += (f'<p class="warning">{stored["failures"]} stored failed replies: this evidence cannot support green. '
                        'Failures are separate from deferrals.</p>')
        if stored and stored["deferred"]:
            warning += (f'<p class="warning">{stored["deferred"]} stored deferred replies: this evidence cannot support green. '
                       'The declared status above is preserved for audit.</p>')
        unit_records = unit_evidence(uid, evidence)
        evidence_body = "<ul>" + "".join(f"<li>{source_link(p)}</li>" for p in unit_records) + "</ul>" if unit_records else "<p>No recorded evidence available.</p>"
        extras = {key: value for key, value in unit.items() if key not in CORE and key != "declared_status"}
        extra_body = ""
        if extras:
            extra_body = '<h2>Extractor extensions</h2><pre>' + esc(json.dumps(extras, ensure_ascii=False, sort_keys=True, indent=2)) + '</pre>'
        ranges = []
        for source_range in unit.get("source_ranges", []):
            path, start, end = source_range.rsplit(":", 2)
            if not start.isdigit() or not end.isdigit():
                raise ValueError(f"{uid}: invalid source range")
            ranges.append(f'<li>{link(source_url(path) + "#L" + start + "-L" + end, source_range)}</li>')
        if ranges:
            extra_body += '<h2>Owned source ranges</h2><ul>' + "".join(ranges) + '</ul>'
        unit_back_edges = [] if not extraction else [e for e in extraction["back_edges"] if e["caller"] == uid]
        if unit_back_edges:
            extra_body += '<h2>Recorded cycle back-edges</h2>' + table(
                ["Callee unit", "Weight", "Calls"], [[link(quote(e["callee"]) + ".html", e["callee"]),
                esc(str(e.get("weight", "unknown"))), str(len(e.get("calls", [])))] for e in unit_back_edges])
        body = f'''<p>{badge(unit['status'])} · Layer {unit['layer']} · {unit['est_lines']:,} estimated upstream lines</p>
<p>{source_link(f'port/units/{uid}.toml', 'Unit source on rust')}</p>{warning}
<h2>Target and claim</h2><p><code>{esc(unit['rust_module'])}</code></p>
<p>{inline(unit['claimed_by']) or 'No claim recorded'} {inline(unit['claimed_at'])}</p>
<h2>Dependencies</h2><p>Hard: {dependencies}</p><p>Soft / cycle back-edges: {soft}</p>
<h2>Upstream source on rust</h2>{upstream}<h2>Upstream tests / fixtures</h2>{tests}
<h2>Stored cases and goldens</h2><p>Cases: {counts_cell(cases[uid])}. Goldens: {counts_cell(stored)}.</p>
<p>Stored counts are not executed test results.</p><h2>Recorded evidence</h2>{evidence_body}
<h2>Notes</h2>{markdown(unit['notes']) or '<p>No notes recorded.</p>'}{extra_body}'''
        write(f"units/{uid}.html", uid, body, 1)
    write("units/index.html", "Unit records", table(
        ["Unit", "Reported status", "Layer", "wx class", "Source category", "Upstream lines", "Cases", "Goldens"], rows)
        if rows else '<p>No unit records available.</p>', 1)
    write("layers.html", "Dependency layers", '<p>Hard dependencies and soft dependencies are listed separately. '
          'A partial dependency still has deferred work; readiness is not completion.</p>'
          + table(["Layer", "Unit", "Hard dependencies", "Soft dependencies / back-edges"], layer_rows)
          + '<h2>Recorded cycle back-edges</h2>' + (table(["Caller", "Callee", "Weight", "Calls"], back_rows)
          if back_rows else '<p>No recorded back-edges.</p>' if extraction and "units" in extraction["artifacts"]
          else '<p>Generated back-edge inventory not available.</p>'))
    write("inventory.html", "Generated inventory", inventory_body(extraction, units, details=True))
    write("records.html", "Findings and upstream issues", records_body(root)
          + '<h2>Recorded evidence files</h2>' + ("<ul>" + "".join(f"<li>{source_link(p)}</li>" for p in evidence)
          + "</ul>" if evidence else "<p>No recorded evidence available.</p>"))
    data = {"schema_version": 1, "summary": stats, "phase0_gate": "not_verified",
            "extraction": None if not extraction else {
                "artifacts": extraction["artifacts"],
                "completeness": {key: extraction["provenance"].get(key) for key in COMPLETENESS},
                "tracked_paths": len(extraction["files"]) if "inventory" in extraction["artifacts"] else None,
                "back_edges": len(extraction["back_edges"]) if "units" in extraction["artifacts"] else None},
            "units": [{"id": u["id"], "status": u["status"], "declared_status": u["declared_status"], "layer": u["layer"],
                       "cases": cases[u["id"]], "goldens": goldens[u["id"]]} for u in units]}
    (out / "progress.json").write_text(json.dumps(data, sort_keys=True, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")
    (out / ".nojekyll").write_text("", encoding="utf-8")
    return data


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--out", type=Path, default=ROOT / "_site")
    args = parser.parse_args(argv)
    try:
        data = build(args.root, args.out)
    except (OSError, ValueError, TypeError) as error:
        print(f"progress: {error}", file=sys.stderr)
        return 1
    print(f"Built public progress site: {data['summary']['units']} unit records; phase 0 gate not verified by site")
    return 0


if __name__ == "__main__":
    sys.exit(main())
