# SPDX-License-Identifier: GPL-3.0-or-later
"""Require document-open failures to return without blocking the editor on a dialog."""

import json
import asyncio
import os
from pathlib import Path
import tempfile
import time

from run import ROOT, MCP, prepare, start, stop, disk_hashes, write_json


SCHEMATIC = '''(kicad_sch (version 20250114) (generator "eeschema")
 (uuid "7855b1ef-9f07-4f35-b2e8-40002cbaae99") (paper "A4") (lib_symbols)
 (sheet (at 80 50) (size 40 30)
  (stroke (width 0) (type default)) (fill (color 0 0 0 0))
  (uuid "edee9f8c-0eb4-4295-b743-e9cb65719999")
  (property "Sheetname" "Missing" (at 80 49 0) (effects (font (size 1.27 1.27))))
  (property "Sheetfile" "missing.kicad_sch" (at 80 81 0) (effects (font (size 1.27 1.27)))))
 (sheet_instances (path "/" (page "1"))))
'''


def main():
    if not Path("/.dockerenv").exists() or not (MCP / "server.py").is_file():
        raise SystemExit("Run inside the isolated qualification container")
    from client import LiveSchematic, base, project, types
    from client import as_dict
    import server

    output = ROOT / "evidence/open-cases"
    output.mkdir(parents=True, exist_ok=False)
    results = {}
    report = {"source_revision": os.environ["KICAD_SOURCE_REVISION"], "cases": results,
              "schema_sha256": asyncio.run(server.live_mcp_capabilities())["schema_sha256"]}
    cases = [
        ("malformed-schematic", "kicad_sch", "(kicad_sch (version 20250114) (broken", False),
        ("future-schematic", "kicad_sch", "(kicad_sch (version 99999999) (generator eeschema))", False),
        ("missing-child", "kicad_sch", SCHEMATIC, False),
        ("malformed-board", "kicad_pcb", "(kicad_pcb (version 20250114) (broken", False),
        ("future-board", "kicad_pcb", "(kicad_pcb (version 99999999) (generator pcbnew) (unknown_future_construct))", False),
        # A newer header alone is accepted by the PCB parser. Preserve that
        # compatibility behavior while requiring incompatible syntax to fail.
        ("compatible-future-board", "kicad_pcb", "(kicad_pcb (version 99999999) (generator pcbnew))", True),
    ]
    for name, suffix, contents, expect_open in cases:
        with tempfile.TemporaryDirectory(prefix="emk-open-", dir="/tmp") as temp:
            folder = Path(temp)
            project_dir = prepare(folder)
            (project_dir / ("live-demo." + suffix)).write_text(contents)
            before = disk_hashes(project_dir)
            (folder / "tmp").mkdir()
            env = {**os.environ, "TMPDIR": str(folder / "tmp"),
                   "KICAD_CONFIG_HOME": str(folder / "config"), "KICAD_API_TOKEN": ""}
            proc = log = None
            result = {"result": "failed", "expected_open": expect_open}
            try:
                proc, log, manager = start(project_dir, folder, output, env, name, open_editor=False)
                report["application_version"] = as_dict(manager.send(base.GetVersion(), base.GetVersionResponse))
                client = LiveSchematic(socket_url=manager.socket_url, timeout_ms=2000)
                document_type = types.DOCTYPE_PCB if suffix == "kicad_pcb" else types.DOCTYPE_SCHEMATIC
                started = time.monotonic()
                try:
                    client.send(project.OpenDocument(type=document_type), project.OpenDocumentResponse)
                except RuntimeError as error:
                    assert not expect_open, error
                    assert "AS_BAD_REQUEST" in str(error), error
                else:
                    assert expect_open, "an invalid document was reported as opened"
                result["elapsed_seconds"] = round(time.monotonic() - started, 3)
                # A timeout alone cannot establish absence of a modal dialog. A
                # subsequent read must still be serviced by the native UI thread.
                client.send(base.GetVersion(), base.GetVersionResponse)
                assert disk_hashes(project_dir) == before, "failed open changed document files"
                result["result"] = "passed"
            except Exception as error:
                result["error"] = f"{type(error).__name__}: {error}"
            finally:
                if proc is not None:
                    stop(proc, log)
                results[name] = result
                write_json(output / "report.json", report)
            print(name, json.dumps(result), flush=True)
    if any(value["result"] != "passed" for value in results.values()):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
