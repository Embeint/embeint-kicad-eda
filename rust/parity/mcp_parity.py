#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Gate 4 parity: the native Rust MCP (embeint-mcp) against the Python MCP (server.py).

Both servers run as real MCP stdio subprocesses driven by the same MCP client.  Checks:

  schema      Every Rust tool is advertised with the Python tool's exact name, description and
              input/output schema; the initialize replies are compatible.
  manifest    live_mcp_capabilities: Rust's command entries equal commands.describe(), and its
              schema hash is Python's hash of the same inventory.
  codec       Protobuf JSON (json_format.ParseDict/MessageToDict), parse_items, document checks,
              endpoint URL validation, Windows pipe candidates and the temp-directory rule, on
              fixed inputs, against the Python functions they port.
  discovery   The offline inventory (instance_discovery.py against embeint-mcp --discover).
  fixtures    Protocol fixtures: scripted NNG peers speaking real protobuf envelopes.  Each
              scenario runs against both servers; the requests KiCad receives and the tool
              results must be identical.  They cover reads, the create_items transaction,
              rollback, every timeout reconciliation path, revision and duplicate refusals,
              token pinning and endpoint discovery.
  live        (--live) A running KiCad with a disposable project (e2e_guard decides): read tools
              compared, then the create_items transaction, its rollback and a stale-revision
              refusal through each server, each undone afterwards.  Nothing is saved.

Run with the MCP interpreter (it needs mcp, pynng, protobuf and the generated protocol code):

  python rust/parity/mcp_parity.py --build [--live] [--report report.json]
  python rust/parity/mcp_parity.py --write-schemas    # refresh rust/embeint-mcp/tools.json
"""

import argparse
import asyncio
import contextlib
import copy
import json
import os
from pathlib import Path, PureWindowsPath
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parents[2]
MCP_DIR = ROOT / "tools" / "live_schematic_mcp"
RUST_DIR = ROOT / "rust"
TOOLS_JSON = RUST_DIR / "embeint-mcp" / "tools.json"
sys.path.insert(0, str(MCP_DIR))

import pynng  # noqa: E402
from google.protobuf import any_pb2, json_format  # noqa: E402
from mcp import ClientSession, StdioServerParameters  # noqa: E402
from mcp.client.stdio import stdio_client  # noqa: E402

import client as pyclient  # noqa: E402
from client import base, editor, envelope, sch, types  # noqa: E402
from client import commands as sch_commands  # noqa: E402
import commands as registry  # noqa: E402
import instance_discovery  # noqa: E402

# The tools the Rust server implements, in tools/list order.  Moving a tool over adds it here,
# to tools.json (--write-schemas) and to the fixtures.
PORTED = [
    "live_mcp_capabilities",
    "live_schematic_documents",
    "live_schematic_hierarchy",
    "live_schematic_items",
    "live_document_revision",
    "live_operation_status",
    "live_schematic_create_items",
    "live_pcb_documents",
]

# Short reply timeout for the fixtures, so lost replies are quick to exercise
FIXTURE_TIMEOUT_MS = "400"
DROP = object()  # a fixture action: receive the request and never reply
TOKEN = "fixture-token-A"
OP_PATTERN = re.compile(r"mcp-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")

DEVNULL = open(os.devnull, "w")  # noqa: SIM115 - the servers' stderr, for the whole run


def binary_path():
    name = "embeint-mcp.exe" if sys.platform == "win32" else "embeint-mcp"
    return RUST_DIR / "target" / "release" / name


def build_binary():
    subprocess.run(["cargo", "build", "--locked", "--release", "-p", "embeint-mcp"], cwd=RUST_DIR, check=True)


def server_env(extra=None):
    env = {k: v for k, v in os.environ.items()
           if k not in ("KICAD_API_SOCKET", "KICAD_API_TOKEN", "KICAD_API_TIMEOUT_MS")}
    env.update(extra or {})
    return env


def servers(binary):
    """(label, StdioServerParameters factory) for both implementations."""
    def python(env):
        return StdioServerParameters(command=sys.executable, args=[str(MCP_DIR / "server.py")],
                                     cwd=str(MCP_DIR), env=env)

    def rust(env):
        return StdioServerParameters(command=str(binary), args=[], cwd=str(MCP_DIR), env=env)
    return [("python", python), ("rust", rust)]


async def run_session(params, steps, between=None):
    """initialize, then each (tool, arguments) step; between[i]() runs before step i."""
    async with stdio_client(params, errlog=DEVNULL) as (read, write):
        async with ClientSession(read, write) as session:
            init = await session.initialize()
            tools = await session.list_tools()
            results = []
            for index, (tool, arguments) in enumerate(steps):
                if between and index in between:
                    between[index]()
                result = await session.call_tool(tool, arguments)
                results.append(result.model_dump(mode="json", exclude_none=True))
            return init.model_dump(mode="json", exclude_none=True), tools, results


# ---- Schema compatibility ------------------------------------------------------------------------

def tool_entry(tool):
    """A tool's advertised definition.  FastMCP sends the raw docstring, and Python 3.13 and later
    strip docstring indentation when compiling, so the Python server's descriptions differ in
    whitespace between interpreters; both sides are compared as inspect.cleandoc gives them, and
    tools.json stores that form.  Schemas are compared exactly."""
    import inspect
    entry = {"name": tool.name, "description": inspect.cleandoc(tool.description or ""),
             "inputSchema": tool.inputSchema}
    if tool.outputSchema is not None:
        entry["outputSchema"] = tool.outputSchema
    return entry


def python_tools():
    init, tools, _ = asyncio.run(run_session(servers(None)[0][1](server_env()), []))
    return init, {tool.name: tool_entry(tool) for tool in tools.tools}


def write_schemas():
    _, tools = python_tools()
    missing = [name for name in PORTED if name not in tools]
    if missing:
        raise SystemExit(f"Python server has no tool {missing}")
    TOOLS_JSON.write_text(json.dumps([tools[name] for name in PORTED], indent=2) + "\n", encoding="utf-8")
    print(f"wrote {TOOLS_JSON.relative_to(ROOT)} ({len(PORTED)} tools)")


def check_schema(binary, failures):
    py_init, py_tools = python_tools()
    rs_init, rs_listed, _ = asyncio.run(run_session(servers(binary)[1][1](server_env()), []))
    rs_tools = {tool.name: tool_entry(tool) for tool in rs_listed.tools}
    if sorted(rs_tools) != sorted(PORTED):
        failures.append(f"schema: Rust advertises {sorted(rs_tools)}, expected {sorted(PORTED)}")
    for name, entry in rs_tools.items():
        if py_tools.get(name) != entry:
            failures.append(f"schema: {name} differs from the Python tool")
    stored = json.loads(TOOLS_JSON.read_text(encoding="utf-8"))
    if stored != [py_tools.get(name) for name in PORTED]:
        failures.append("schema: tools.json is out of date; run mcp_parity.py --write-schemas")
    for key in ("protocolVersion", "capabilities"):
        if py_init[key] != rs_init[key]:
            failures.append(f"schema: initialize {key} differs: {py_init[key]} != {rs_init[key]}")
    if rs_init["serverInfo"]["name"] != py_init["serverInfo"]["name"]:
        failures.append("schema: serverInfo name differs")
    return {"python_tools": len(py_tools), "rust_tools": len(rs_tools), "compared": sorted(rs_tools)}


# ---- The --parity interface: pure functions ---------------------------------------------------

class ParityProcess:
    def __init__(self, binary):
        self.process = subprocess.Popen([str(binary), "--parity"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        text=True, encoding="utf-8")

    def ask(self, request):
        self.process.stdin.write(json.dumps(request) + "\n")
        self.process.stdin.flush()
        return json.loads(self.process.stdout.readline())

    def close(self):
        self.process.stdin.close()
        self.process.wait(timeout=10)


def python_error(error):
    return {"error": {"class": type(error).__name__, "message": str(error)}}


def py_parse(type_name, value):
    from google.protobuf import descriptor_pool
    from commands import message_class
    try:
        descriptor_pool.Default().FindMessageTypeByName(type_name)
        message = message_class(type_name)()
        json_format.ParseDict(copy.deepcopy(value), message)
        return {"ok": json_format.MessageToDict(message, preserving_proto_field_name=True)}
    except Exception as error:  # noqa: BLE001 - every failure is compared
        return python_error(error)


def py_items(values):
    try:
        items = pyclient.parse_items(copy.deepcopy(values))
    except Exception as error:  # noqa: BLE001
        return python_error(error)
    result = []
    for item in items:
        packed = any_pb2.Any()
        packed.Pack(item)
        result.append(json_format.MessageToDict(packed, preserving_proto_field_name=True))
    return {"ok": result}


def py_document(kind, value):
    import server
    try:
        if kind == "schematic":
            message = pyclient.document(copy.deepcopy(value))
        elif kind == "board":
            message = pyclient.board_document(copy.deepcopy(value))
        else:
            message = server.editor_document(copy.deepcopy(value))
        return {"ok": json_format.MessageToDict(message, preserving_proto_field_name=True)}
    except Exception as error:  # noqa: BLE001
        return python_error(error)


def py_url(platform, url):
    saved = instance_discovery.sys.platform
    instance_discovery.sys.platform = "win32" if platform == "windows" else "linux"
    try:
        path = instance_discovery.local_socket_path(url)
        if platform == "unix":
            # What Path() prints on Linux; on a Windows host Path() would read it as a Windows path
            from pathlib import PurePosixPath
            return {"ok": str(PurePosixPath(url[len("ipc://"):]))}
        return {"ok": str(path)}
    except instance_discovery.DiscoveryError as error:
        return {"error": {"class": "DiscoveryError", "code": error.code, "message": str(error)}}
    finally:
        instance_discovery.sys.platform = saved


def py_candidates(root, pipes):
    saved = (instance_discovery.list_pipes, instance_discovery.directory_key)
    instance_discovery.list_pipes = lambda: list(pipes)
    instance_discovery.directory_key = str
    try:
        return {"ok": instance_discovery._windows_candidates(PureWindowsPath(root))}
    finally:
        instance_discovery.list_pipes, instance_discovery.directory_key = saved


def py_temp(env):
    return {"ok": instance_discovery.windows_temp_dir(env)}


SHEET_UUID = "00000000-0000-4000-8000-000000000001"
SHEET_DOC = {"type": "DOCTYPE_SCHEMATIC", "project": {"name": "parity", "path": "C:/fixture/parity"},
             "sheet_path": {"path": [{"value": SHEET_UUID}], "path_human_readable": "/"}}
BOARD_DOC = {"type": "DOCTYPE_PCB", "project": {"name": "parity", "path": "C:/fixture/parity"},
             "board_filename": "parity.kicad_pcb"}
SYMBOL_DOC = {"type": "DOCTYPE_SYMBOL", "lib_id": {"library_nickname": "Lib", "entry_name": "R"}}
FOOTPRINT_DOC = {"type": "DOCTYPE_FOOTPRINT", "lib_id": {"library_nickname": "Lib", "entry_name": "R_0603"}}

LINE = {"@type": "type.googleapis.com/kiapi.schematic.types.SchematicLine",
        "start": {"x_nm": "10160000", "y_nm": "20320000"}, "end": {"x_nm": 15240000, "y_nm": "20320000"},
        "type": "SLT_WIRE"}
TEXT = {"@type": "type.googleapis.com/kiapi.schematic.types.SchematicText",
        "text": {"position": {"x_nm": "25400000", "y_nm": "25400000"}, "text": "parity \u00b1 note",
                 "attributes": {"horizontal_alignment": "HA_LEFT", "line_spacing": 0.0,
                                "angle": {"value_degrees": 90.5}, "stroke_width": {"value_nm": "0"}}}}
JUNCTION = {"@type": "type.googleapis.com/kiapi.schematic.types.Junction",
            "position": {"xNm": "10160000", "yNm": 20320000.0}, "diameter": {"value_nm": "1016000"},
            "color": {"r": 0.1, "g": "0.25", "b": 1, "a": True}}

CODEC_CASES = [
    # (type, value): valid shapes
    ("kiapi.common.types.DocumentSpecifier", SHEET_DOC),
    ("kiapi.common.types.DocumentSpecifier", BOARD_DOC),
    ("kiapi.common.types.DocumentSpecifier", {"type": 2, "libId": {"entryName": "R"}}),
    ("kiapi.common.types.DocumentSpecifier", {"type": "3", "board_filename": ""}),
    ("kiapi.common.types.DocumentSpecifier", {"type": 99}),
    ("kiapi.common.types.DocumentSpecifier", {"type": None, "project": None}),
    ("kiapi.common.types.DocumentSpecifier", {"sheet_path": {}}),
    ("google.protobuf.Any", LINE),
    ("google.protobuf.Any", TEXT),
    ("google.protobuf.Any", JUNCTION),
    ("google.protobuf.Any", {}),
    ("kiapi.common.types.Vector2", {"x_nm": "-9223372036854775808", "y_nm": 9223372036854775807}),
    ("kiapi.common.types.Vector2", {"x_nm": 1e3, "y_nm": "+12"}),
    ("kiapi.common.types.Vector2", {"x_nm": "1_000"}),
    ("kiapi.common.types.Color", {"r": "NaN", "g": "Infinity", "b": "-Infinity", "a": 1e-7}),
    ("kiapi.common.types.Color", {"r": 123456789.123456789, "g": -0.0}),
    ("kiapi.common.types.TextAttributes", {"line_spacing": 0, "bold": False, "italic": True}),
    ("kiapi.common.commands.SetTextVariables",
     {"variables": {"variables": {"B": "2", "A": "\u00e9\u20ac\U0001F600"}}}),
    ("kiapi.common.types.ItemHeader", {"field_mask": "startPoint,endPoint.xNm"}),
    ("kiapi.common.commands.GetItems", {"types": ["KOT_SCH_LINE", 0, "KOT_SCH_JUNCTION"]}),
    ("kiapi.common.commands.EndCommit", {"action": "CMA_DROP", "message": "x", "id": {"value": "u"}}),
    ("kiapi.common.commands.BeginCommit", {"lease_seconds": 60}),
    # invalid shapes: both must refuse; messages are compared too
    ("kiapi.common.types.KIID", {"typo": 1}),
    ("kiapi.common.types.Vector2", {"x_nm": 1.5}),
    ("kiapi.common.types.Vector2", {"x_nm": True}),
    ("kiapi.common.types.Vector2", {"x_nm": "1 2"}),
    ("kiapi.common.types.Vector2", {"x_nm": "abc"}),
    ("kiapi.common.types.Vector2", {"x_nm": "99999999999999999999"}),
    ("kiapi.common.commands.BeginCommit", {"lease_seconds": -1}),
    ("kiapi.common.types.DocumentSpecifier", {"type": "DOCTYPE_NOPE"}),
    ("kiapi.common.types.DocumentSpecifier", {"board_filename": "a", "sheet_path": {}}),
    ("kiapi.common.types.DocumentSpecifier", {"board_filename": 5}),
    ("kiapi.common.types.TextAttributes", {"bold": "true"}),
    ("kiapi.common.types.Color", {"r": "nan"}),
    ("kiapi.common.types.Color", {"r": "fast"}),
    ("kiapi.common.commands.GetItems", {"types": "KOT_SCH_LINE"}),
    ("kiapi.common.commands.GetItems", {"types": [None]}),
    ("google.protobuf.Any", {"x_nm": 1}),
    ("google.protobuf.Any", {"@type": "type.googleapis.com/kiapi.nope.Missing"}),
    ("google.protobuf.Any", dict(LINE, bogus=1)),
    ("kiapi.common.commands.SetTextVariables", {"variables": {"variables": []}}),
]

ITEM_CASES = [
    [LINE, TEXT, JUNCTION],
    [{"@type": "type.googleapis.com/kiapi.common.types.KIID", "value": "x"}],
    [{"@type": "type.googleapis.com/kiapi.board.types.Track"}],
    [{}],
    [{"@type": "kiapi.schematic.types.SchematicLine"}],
    [{"@type": "type.googleapis.com/kiapi.schematic.types.SchematicLine", "start": {"z": 1}}],
    [{"@type": "type.googleapis.com/kiapi.schematic.types.SheetSymbol", "name_field": {"text": {"text": "S"}}}],
]

DOCUMENT_CASES = [
    ("schematic", SHEET_DOC), ("schematic", {"type": "DOCTYPE_SCHEMATIC"}),
    ("schematic", dict(SHEET_DOC, project={"name": ""})), ("schematic", BOARD_DOC),
    ("schematic", {"type": "DOCTYPE_SCHEMATIC", "nope": 1}),
    ("board", BOARD_DOC), ("board", dict(BOARD_DOC, board_filename="")),
    ("editor", SHEET_DOC), ("editor", BOARD_DOC), ("editor", SYMBOL_DOC), ("editor", FOOTPRINT_DOC),
    ("editor", {"type": "DOCTYPE_SYMBOL"}), ("editor", {"type": "DOCTYPE_FOOTPRINT", "lib_id": {}}),
    ("editor", {"type": 3, "board_filename": "x.kicad_pcb"}), ("editor", {}),
]

URL_CASES = [
    ("windows", r"ipc://C:\Temp\kicad\api.sock"), ("windows", r"ipc://c:\Users\A~1\x\api-12.sock"),
    ("windows", r"ipc://\\server\pipe\api.sock"), ("windows", r"ipc://\\.\pipe\api.sock"),
    ("windows", "ipc://C:/Temp/api.sock"), ("windows", r"ipc://C:\Temp\..\api.sock"),
    ("windows", r"ipc://C:\Temp\.\api.sock"), ("windows", r"ipc://C:\Temp \api.sock"),
    ("windows", r"ipc://C:\Temp.\api.sock"), ("windows", r"ipc://C:\Te|mp\api.sock"),
    ("windows", r"ipc://C:\Temp\\api.sock"), ("windows", r"ipc://C:api.sock"), ("windows", r"ipc://C:\\"),
    ("windows", "ipc://C:\\" + "a" * 247), ("windows", "ipc://C:\\" + "a" * 248),
    ("windows", "tcp://127.0.0.1:5555"), ("windows", r"ipc://C:\Temp\api.sock#x"),
    ("windows", "ipc://C:\\Temp\\a\u00e9\\api.sock"), ("windows", "ipc://C:\\Temp\\a\tb\\api.sock"),
    ("unix", "ipc:///tmp/kicad/api.sock"), ("unix", "ipc:///"), ("unix", "ipc://tmp/api.sock"),
    ("unix", "ipc:///tmp/api.sock?x=1"), ("unix", "ws://localhost/x"), ("unix", "ipc:///tmp/a\x01b"),
    ("unix", "IPC:///tmp/api.sock"), ("unix", ""), ("unix", "ipc:///tmp//kicad/./api.sock/"),
    ("unix", "ipc:////tmp/x.sock"),
]

CANDIDATE_CASES = [
    (r"C:\Temp\kicad", [r"C:\Temp\kicad\api.sock", r"C:\Temp\kicad\api-12.sock", r"C:\Other\kicad\api.sock",
                        r"C_\repo\.git\fsmonitor--daemon.ipc", r"C:\Temp\kicad\API-3.SOCK",
                        r"C:\Temp\kicad\api-.sock", r"C:\Temp\kicad\notes.sock", "mojo.1234.5678",
                        r"C:\Temp\kicad\sub\api.sock", r"\\.\pipe\x", r"C:\Temp\kicad\api-7.sock"]),
    (r"C:\Temp\kicad", []),
    ("C:/Temp/kicad/", [r"C:\Temp\kicad\api.sock"]),
]

TEMP_CASES = [
    {"TMPDIR": "C:/X/./y//", "TMP": r"C:\T"}, {"TMP": r"C:\T\\", "TEMP": r"C:\E"}, {"TEMP": r"C:\E"},
    {"USERPROFILE": r"C:\Users\u"}, {"SystemRoot": r"C:\Windows"}, {"TMP": "", "TEMP": r"C:\E"},
]


def compare_codec(binary, failures):
    rust = ParityProcess(binary)
    counts = {"cases": 0, "accepted": 0, "refused": 0, "message_differences": []}

    def compare(label, python, request):
        counts["cases"] += 1
        native = rust.ask(request)
        if "ok" in python:
            counts["accepted"] += 1
            if native != python:
                failures.append(f"codec {label}: Python {python} != Rust {native}")
        else:
            counts["refused"] += 1
            if "error" not in native:
                failures.append(f"codec {label}: Python refuses ({python['error']}), Rust accepts {native}")
            elif native["error"] != python["error"]:
                counts["message_differences"].append({"case": label, "python": python["error"],
                                                      "rust": native["error"]})

    try:
        for type_name, value in CODEC_CASES:
            compare(f"parse {type_name} {json.dumps(value)[:80]}", py_parse(type_name, value),
                    {"op": "parse", "type": type_name, "value": value})
        for values in ITEM_CASES:
            compare(f"items {json.dumps(values)[:80]}", py_items(values), {"op": "items", "values": values})
        for kind, value in DOCUMENT_CASES:
            compare(f"document {kind} {json.dumps(value)[:60]}", py_document(kind, value),
                    {"op": "document", "kind": kind, "value": value})
        for platform, url in URL_CASES:
            python = py_url(platform, url)
            native = rust.ask({"op": "url", "platform": platform, "url": url})
            counts["cases"] += 1
            if python != native:
                failures.append(f"codec url {platform} {url!r}: Python {python} != Rust {native}")
        for root, pipes in CANDIDATE_CASES:
            python = py_candidates(root, pipes)
            native = rust.ask({"op": "candidates", "root": root, "pipes": pipes})
            counts["cases"] += 1
            if python != native:
                failures.append(f"codec candidates {root}: Python {python} != Rust {native}")
        for env in TEMP_CASES:
            python = py_temp(env)
            native = rust.ask({"op": "temp", "env": env})
            counts["cases"] += 1
            if python != native:
                failures.append(f"codec temp {env}: Python {python} != Rust {native}")
        compare_manifest(rust.ask({"op": "manifest"})["ok"], failures, counts)
    finally:
        rust.close()
    return counts


def compare_manifest(manifest, failures, counts):
    """Rust's command entries are Python's; its hash is Python's hash of the same inventory."""
    import hashlib
    for entry in manifest["commands"]:
        expected = registry.describe(entry["command"])
        if entry != expected:
            failures.append(f"manifest: {entry['command']} differs from commands.describe(): {expected}")
    inventory = {"tools": manifest["tools"], "commands": manifest["commands"]}
    digest = hashlib.sha256(json.dumps(inventory, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    if digest != manifest["schema_sha256"]:
        failures.append("manifest: schema_sha256 is not Python's hash of the inventory")
    rust_tools = {t["name"]: t for t in manifest["tools"]}
    for tool in json.loads(TOOLS_JSON.read_text(encoding="utf-8")):
        entry = rust_tools.get(tool["name"])
        if entry is None or entry["input_schema"] != tool["inputSchema"]:
            failures.append(f"manifest: {tool['name']} schema is not the advertised one")
    counts["manifest_commands"] = len(manifest["commands"])


def compare_inventory(binary, failures):
    """instance_discovery.py and embeint-mcp --discover on this host's real endpoints."""
    env = server_env()
    python = subprocess.run([sys.executable, str(MCP_DIR / "instance_discovery.py")], capture_output=True,
                            text=True, env=env, cwd=str(MCP_DIR))
    native = subprocess.run([str(binary), "--discover"], capture_output=True, text=True, env=env)
    result = {"python": json.loads(python.stdout), "rust": json.loads(native.stdout),
              "python_status": python.returncode, "rust_status": native.returncode}
    if result["python"] != result["rust"] or python.returncode != native.returncode:
        failures.append(f"discovery: inventories differ: {result}")
    return result


# ---- Protocol fixtures ----------------------------------------------------------------------------

def reply(message=None, status=None, error="", token=TOKEN, revision=""):
    response = envelope.ApiResponse()
    response.header.kicad_token = token
    response.header.document_revision = revision
    response.status.status = envelope.AS_OK if status is None else status
    response.status.error_message = error
    if message is not None:
        response.message.Pack(message)
    return response


def parse(cls, value):
    return json_format.ParseDict(value, cls())


def version(major=10, minor=99):
    response = base.GetVersionResponse()
    response.version.major, response.version.minor = major, minor
    response.version.full_version = f"{major}.{minor}.0-fixture"
    return reply(response)


def documents(*docs, revision=""):
    return reply(parse(editor.GetOpenDocumentsResponse, {"documents": list(docs)}), revision=revision)


def status(state, message="", revision_after="", operation_id=""):
    def respond(request):
        query = editor.GetOperationStatus()
        request.message.Unpack(query)
        return reply(parse(editor.OperationStatusResponse, {
            "operation_id": query.operation_id, "state": state, "request_type": "kiapi.common.commands.BeginCommit",
            "document": SHEET_DOC, "revision_before": "fx-1", "revision_after": revision_after,
            "message": message, "client_name": "kicad.client", "history_capacity": 1024}))
    return respond


def failure(code, message, revision=""):
    return reply(status=code, error=message, revision=revision)


BEGIN = ("BeginCommit", lambda r: reply(parse(editor.BeginCommitResponse, {"id": {"value": "c0mm17"}}),
                                        revision="fx-2"))
END_OK = ("EndCommit", lambda r: reply(editor.EndCommitResponse(), revision="fx-3"))
DROP_OK = ("EndCommit", lambda r: reply(editor.EndCommitResponse(), revision="fx-4"))


def created(*codes, message=""):
    def respond(request):
        create = editor.CreateItems()
        request.message.Unpack(create)
        result = editor.CreateItemsResponse(status=types.IRS_OK)
        result.header.CopyFrom(create.header)
        for packed, code in zip(create.items, codes):
            entry = result.created_items.add()
            entry.status.code = code
            if code != editor.ISC_OK:
                entry.status.error_message = message
            entry.item.CopyFrom(packed)
        return reply(result, revision="fx-2")
    return ("CreateItems", respond)


ITEMS_REPLY = parse(editor.GetItemsResponse, {
    "header": {"document": SHEET_DOC}, "status": "IRS_OK",
    "items": [dict(LINE, id={"value": "11111111-1111-4111-8111-111111111111"}),
              dict(TEXT, id={"value": "22222222-2222-4222-8222-222222222222"}),
              {"@type": "type.googleapis.com/kiapi.schematic.types.Junction",
               "id": {"value": "33333333-3333-4333-8333-333333333333"},
               "position": {"x_nm": "10160000", "y_nm": "20320000"}, "diameter": {"value_nm": "0"},
               "color": {"r": 0.1, "g": 0.2, "b": 0.30000000000000004, "a": 1.0}}]})

HIERARCHY_REPLY = parse(sch_commands.SchematicHierarchyResponse, {
    "document": SHEET_DOC,
    "top_level_sheets": [{"path": {"path": [{"value": SHEET_UUID}], "path_human_readable": "/"}, "name": "Root",
                          "filename": "parity.kicad_sch", "page_number": "1", "children": [
                              {"path": {"path": [{"value": SHEET_UUID}, {"value": "44444444-4444-4444-8444-4444"}],
                                        "path_human_readable": "/Power/"},
                               "name": "Power", "filename": "power.kicad_sch", "page_number": "2"}]}]})

CREATE = {"document_spec": SHEET_DOC, "items": [LINE, TEXT]}
CREATE_GUARDED = dict(CREATE, expected_revision="fx-1", operation_id="op-fixture-1")


def scenario(name, steps, script, **options):
    return {"name": name, "steps": steps, "script": script, **options}


S = scenario
OPS = editor  # OperationState values live in editor_commands
SCENARIOS = [
    # Reads
    S("documents", [("live_schematic_documents", {})],
      [("GetVersion", lambda r: version()), ("GetOpenDocuments", lambda r: documents(SHEET_DOC))]),
    S("documents_editor_closed", [("live_schematic_documents", {})],
      [("GetVersion", lambda r: version()),
       ("GetOpenDocuments", lambda r: failure(envelope.AS_UNHANDLED, "no handler"))]),
    S("documents_old_kicad", [("live_schematic_documents", {})], [("GetVersion", lambda r: version(10, 0))]),
    S("documents_not_ready", [("live_schematic_documents", {})],
      [("GetVersion", lambda r: failure(envelope.AS_NOT_READY, "KiCad is starting"))]),
    S("documents_wrong_reply", [("live_schematic_documents", {})],
      [("GetVersion", lambda r: reply(editor.EndCommitResponse()))]),
    S("documents_lost_reply", [("live_schematic_documents", {})], [("GetVersion", lambda r: DROP)]),
    S("token_pinned_after_first_reply", [("live_schematic_documents", {}), ("live_pcb_documents", {})],
      [("GetVersion", lambda r: version()), ("GetOpenDocuments", lambda r: documents(SHEET_DOC)),
       ("GetOpenDocuments", lambda r: documents(BOARD_DOC, BOARD_DOC))]),
    S("pcb_documents_closed", [("live_pcb_documents", {})],
      [("GetOpenDocuments", lambda r: failure(envelope.AS_UNHANDLED, "no PCB editor"))]),
    S("pcb_documents_none", [("live_pcb_documents", {})], [("GetOpenDocuments", lambda r: documents())]),
    S("pcb_documents_error", [("live_pcb_documents", {})],
      [("GetOpenDocuments", lambda r: failure(envelope.AS_BAD_REQUEST, "bad"))]),
    S("hierarchy", [("live_schematic_hierarchy", {"document_spec": SHEET_DOC})],
      [("GetSchematicHierarchy", lambda r: reply(HIERARCHY_REPLY, revision="fx-1"))]),
    S("hierarchy_document_refused", [("live_schematic_hierarchy", {"document_spec": BOARD_DOC})], []),
    S("hierarchy_document_invalid_json", [("live_schematic_hierarchy",
                                           {"document_spec": dict(SHEET_DOC, nope=1)})], []),
    S("items", [("live_schematic_items", {"document_spec": SHEET_DOC})],
      [("GetItems", lambda r: reply(ITEMS_REPLY, revision="fx-1"))]),
    S("items_document_not_found", [("live_schematic_items", {"document_spec": SHEET_DOC})],
      [("GetItems", lambda r: reply(editor.GetItemsResponse(status=types.IRS_DOCUMENT_NOT_FOUND)))]),
    S("items_spec_as_json_text", [("live_schematic_items", {"document_spec": json.dumps(SHEET_DOC)})],
      [("GetItems", lambda r: reply(ITEMS_REPLY))]),
    S("items_spec_missing", [("live_schematic_items", {})], [], compare="error_prefix"),
    S("revision_schematic", [("live_document_revision", {"document_spec": SHEET_DOC})],
      [("GetDocumentRevision", lambda r: reply(parse(editor.DocumentRevisionResponse,
                                                     {"revision": "fx-7", "open_commits": 2, "modified": True})))]),
    S("revision_board", [("live_document_revision", {"document_spec": BOARD_DOC})],
      [("GetDocumentRevision", lambda r: reply(editor.DocumentRevisionResponse()))]),
    S("revision_symbol_and_footprint", [("live_document_revision", {"document_spec": SYMBOL_DOC}),
                                        ("live_document_revision", {"document_spec": FOOTPRINT_DOC})],
      [("GetDocumentRevision", lambda r: reply(parse(editor.DocumentRevisionResponse, {"revision": "s-1"}))),
       ("GetDocumentRevision", lambda r: failure(envelope.AS_UNIMPLEMENTED, "headless"))]),
    S("revision_board_without_file", [("live_document_revision",
                                       {"document_spec": dict(BOARD_DOC, board_filename="")})], []),
    S("operation_status", [("live_operation_status", {"operation_id": "op-fixture-1"})],
      [("GetOperationStatus", status("OPS_COMMITTED", revision_after="fx-3"))]),
    S("operation_status_unknown", [("live_operation_status", {"operation_id": "op-x"})],
      [("GetOperationStatus", lambda r: reply(editor.OperationStatusResponse()))]),
    S("operation_status_empty", [("live_operation_status", {"operation_id": ""})], []),
    S("operation_status_wrong_type", [("live_operation_status", {"operation_id": 7})], [],
      compare="error_prefix"),
    S("unknown_tool", [("live_schematic_nope", {})], []),
    # The edit transaction
    S("create_committed", [("live_schematic_create_items", CREATE_GUARDED)], [BEGIN, created(1, 1), END_OK]),
    S("create_committed_generated_id", [("live_schematic_create_items", CREATE)],
      [BEGIN, created(1, 1), END_OK]),
    S("create_args_as_json_text", [("live_schematic_create_items",
                                    {"document_spec": json.dumps(SHEET_DOC), "items": json.dumps([LINE]),
                                     "expected_revision": "null"})],
      [BEGIN, created(1), END_OK]),
    S("create_item_rejected_rolls_back", [("live_schematic_create_items", CREATE_GUARDED)],
      [BEGIN, created(1, editor.ISC_INVALID_DATA, message="text outside the sheet"), DROP_OK]),
    S("create_item_rejected_without_message", [("live_schematic_create_items", CREATE)],
      [BEGIN, created(editor.ISC_EXISTING, editor.ISC_INVALID_TYPE), DROP_OK]),
    S("create_partial_ack_rolls_back", [("live_schematic_create_items", CREATE)],
      [BEGIN, created(1), DROP_OK]),
    S("create_step_refused_rolls_back", [("live_schematic_create_items", CREATE)],
      [BEGIN, ("CreateItems", lambda r: failure(envelope.AS_BAD_REQUEST, "sheet is locked")), DROP_OK]),
    S("create_rollback_unconfirmed", [("live_schematic_create_items", CREATE_GUARDED)],
      [BEGIN, created(1, editor.ISC_INVALID_DATA, message="bad"),
       ("EndCommit", lambda r: failure(envelope.AS_BAD_REQUEST, "no such commit"))]),
    S("create_step_reply_lost_rolls_back", [("live_schematic_create_items", CREATE)],
      [BEGIN, ("CreateItems", lambda r: DROP), DROP_OK]),
    S("create_stale_revision_refused", [("live_schematic_create_items", CREATE_GUARDED)],
      [("BeginCommit", lambda r: failure(envelope.AS_REVISION_MISMATCH, "document is at fx-9", "fx-9"))]),
    S("create_duplicate_operation", [("live_schematic_create_items", CREATE_GUARDED)],
      [("BeginCommit", lambda r: failure(envelope.AS_DUPLICATE_OPERATION, "op-fixture-1 already seen"))]),
    S("create_conflict_at_commit", [("live_schematic_create_items", CREATE_GUARDED)],
      [BEGIN, created(1, 1),
       ("EndCommit", lambda r: failure(envelope.AS_REVISION_MISMATCH, "changed while open", "fx-8"))]),
    S("create_commit_reply_lost_committed", [("live_schematic_create_items", CREATE_GUARDED)],
      [BEGIN, created(1, 1), ("EndCommit", lambda r: DROP),
       ("GetOperationStatus", status("OPS_COMMITTED", revision_after="fx-5"))]),
    S("create_commit_reply_lost_rolled_back", [("live_schematic_create_items", CREATE)],
      [BEGIN, created(1, 1), ("EndCommit", lambda r: DROP),
       ("GetOperationStatus", status("OPS_ROLLED_BACK", message="lease expired"))]),
    S("create_commit_reply_lost_rolled_back_silently", [("live_schematic_create_items", CREATE)],
      [BEGIN, created(1, 1), ("EndCommit", lambda r: DROP), ("GetOperationStatus", status("OPS_ROLLED_BACK"))]),
    S("create_commit_in_progress_then_committed", [("live_schematic_create_items", CREATE_GUARDED)],
      [BEGIN, created(1, 1), ("EndCommit", lambda r: DROP),
       ("GetOperationStatus", status("OPS_IN_PROGRESS")),
       ("EndCommit", lambda r: failure(envelope.AS_BAD_REQUEST, "no commit to drop")),
       ("GetOperationStatus", status("OPS_COMMITTED", revision_after="fx-6"))]),
    S("create_commit_in_progress_dropped", [("live_schematic_create_items", CREATE_GUARDED)],
      [BEGIN, created(1, 1), ("EndCommit", lambda r: DROP),
       ("GetOperationStatus", status("OPS_IN_PROGRESS")), DROP_OK,
       ("GetOperationStatus", status("OPS_ROLLED_BACK", message="dropped by its client"))]),
    S("create_commit_in_progress_drop_lost", [("live_schematic_create_items", CREATE)],
      [BEGIN, created(1, 1), ("EndCommit", lambda r: DROP),
       ("GetOperationStatus", status("OPS_IN_PROGRESS")), ("EndCommit", lambda r: DROP)]),
    S("create_commit_outcome_unknown", [("live_schematic_create_items", CREATE)],
      [BEGIN, created(1, 1), ("EndCommit", lambda r: DROP), ("GetOperationStatus", status("OPS_UNKNOWN"))]),
    S("create_commit_status_query_lost", [("live_schematic_create_items", CREATE_GUARDED)],
      [BEGIN, created(1, 1), ("EndCommit", lambda r: DROP), ("GetOperationStatus", lambda r: DROP)]),
    S("create_commit_status_query_refused", [("live_schematic_create_items", CREATE_GUARDED)],
      [BEGIN, created(1, 1), ("EndCommit", lambda r: DROP),
       ("GetOperationStatus", lambda r: failure(envelope.AS_BUSY, "busy"))]),
    S("create_begin_reply_lost_open", [("live_schematic_create_items", CREATE_GUARDED)],
      [("BeginCommit", lambda r: DROP), ("GetOperationStatus", status("OPS_IN_PROGRESS")), DROP_OK]),
    S("create_begin_reply_lost_rejected", [("live_schematic_create_items", CREATE_GUARDED)],
      [("BeginCommit", lambda r: DROP), ("GetOperationStatus", status("OPS_REJECTED", message="stale"))]),
    S("create_begin_reply_lost_unseen", [("live_schematic_create_items", CREATE)],
      [("BeginCommit", lambda r: DROP), ("GetOperationStatus", status("OPS_UNKNOWN"))]),
    S("create_begin_reply_lost_drop_refused", [("live_schematic_create_items", CREATE)],
      [("BeginCommit", lambda r: DROP), ("GetOperationStatus", status("OPS_IN_PROGRESS")),
       ("EndCommit", lambda r: failure(envelope.AS_BAD_REQUEST, "nothing open"))]),
    S("create_two_calls_number_nothing_across_calls",
      [("live_schematic_create_items", CREATE_GUARDED),
       ("live_schematic_create_items", dict(CREATE, operation_id="op-fixture-2"))],
      [BEGIN, created(1, 1), END_OK, BEGIN, created(1, 1), END_OK]),
    S("create_refuses_other_item_types", [("live_schematic_create_items", dict(
        CREATE, items=[{"@type": "type.googleapis.com/kiapi.board.types.Track"}]))], []),
    S("create_refuses_no_items", [("live_schematic_create_items", dict(CREATE, items=[]))], []),
    S("create_refuses_too_many_items", [("live_schematic_create_items", dict(CREATE, items=[LINE] * 201))], []),
    S("create_refuses_invalid_item_json", [("live_schematic_create_items",
                                            dict(CREATE, items=[dict(LINE, start={"x": 1})]))], []),
    S("create_refuses_items_not_a_list", [("live_schematic_create_items", dict(CREATE, items=5))], [],
      compare="error_prefix"),
    # Endpoint discovery (no KICAD_API_SOCKET; TMPDIR points at the fixture directory)
    S("discovery_no_editor", [("live_schematic_documents", {})], [], discovery=[]),
    S("discovery_single_instance", [("live_schematic_documents", {})],
      [("GetVersion", lambda r: version()), ("GetOpenDocuments", lambda r: documents(SHEET_DOC))],
      discovery=["api.sock"]),
    S("discovery_pid_endpoint", [("live_pcb_documents", {})],
      [("GetOpenDocuments", lambda r: documents(BOARD_DOC))], discovery=["api-4242.sock"]),
    S("discovery_ambiguous", [("live_schematic_documents", {})], [], discovery=["api.sock", "api-7.sock"],
      second_peer_script=[]),
    S("discovery_pinned_instance_gone", [("live_pcb_documents", {}), ("live_pcb_documents", {})],
      [("GetOpenDocuments", lambda r: documents(BOARD_DOC))], discovery=["api.sock"], stop_before=1),
]


class Peer:
    """A disposable NNG REP0 endpoint answering with a scenario's script."""

    def __init__(self, url, script):
        self.url, self.script = url, list(script)
        self.requests, self.errors = [], []
        self.stop_event = threading.Event()
        self.socket = pynng.Rep0(listen=url, recv_timeout=50, send_timeout=1000)
        self.thread = threading.Thread(target=self.serve, daemon=True)
        self.thread.start()

    def serve(self):
        while not self.stop_event.is_set():
            try:
                raw = self.socket.recv()
            except pynng.Timeout:
                continue
            except pynng.exceptions.Closed:
                return
            try:
                request = envelope.ApiRequest.FromString(raw)
                index = len(self.requests)
                self.requests.append(request)
                kind = request.message.TypeName().rsplit(".", 1)[-1]
                if index < len(self.script) and self.script[index][0] == kind:
                    response = self.script[index][1](request)
                else:
                    response = failure(envelope.AS_BAD_REQUEST, f"fixture: unexpected {kind} at step {index}")
                if response is not DROP:
                    self.socket.send(response.SerializeToString())
            except Exception as error:  # noqa: BLE001 - reported with the scenario
                self.errors.append(repr(error))

    def stop(self):
        self.stop_event.set()
        self.socket.close()
        self.thread.join(timeout=5)


def endpoint_dir(label):
    """A fresh directory for a scenario's endpoints (named pipes on Windows need no directory)."""
    if sys.platform == "win32":
        base = Path(tempfile.gettempdir()) / f"embeint-parity-{os.getpid()}"
        base.mkdir(exist_ok=True)
        return Path(tempfile.mkdtemp(prefix=label[:16] + "-", dir=base))
    return Path(tempfile.mkdtemp(prefix="emb-"))


def normalize(value, ids):
    """Replace generated operation ids (mcp-<uuid>) with <op1>, <op2>... in order of appearance."""
    if isinstance(value, dict):
        return {k: normalize(v, ids) for k, v in value.items()}
    if isinstance(value, list):
        return [normalize(v, ids) for v in value]
    if isinstance(value, str):
        return OP_PATTERN.sub(lambda m: ids.setdefault(m.group(0), f"<op{len(ids) + 1}>"), value)
    return value


def request_log(requests, ids):
    log = []
    for request in requests:
        entry = json_format.MessageToDict(request, preserving_proto_field_name=True)
        if entry.get("header", {}).get("client_name"):
            entry["header"]["client_name"] = "<client>"
        log.append(normalize(entry, ids))
    return log


async def run_scenario(item, make_params):
    """One scenario against one server: (requests KiCad received, tool results)."""
    directory = endpoint_dir(item["name"])
    env = {"KICAD_API_TIMEOUT_MS": FIXTURE_TIMEOUT_MS}
    peers = []
    if "discovery" in item:
        # KiCad's endpoint names under <TMPDIR>/kicad, found by discovery
        kicad_dir = directory / "kicad"
        kicad_dir.mkdir()
        env["TMPDIR"] = str(directory)
        names = item["discovery"]
        for index, name in enumerate(names):
            script = item["script"] if index == 0 else item.get("second_peer_script", [])
            peers.append(Peer("ipc://" + str(kicad_dir / name), script))
    else:
        url = "ipc://" + str(directory / "api.sock")
        env["KICAD_API_SOCKET"] = url
        peers.append(Peer(url, item["script"]))
    between = {}
    if "stop_before" in item:
        between[item["stop_before"]] = peers[0].stop
    try:
        _, _, results = await run_session(make_params(server_env(env)), item["steps"], between)
    finally:
        for peer in peers:
            if not peer.stop_event.is_set():
                peer.stop()
        shutil.rmtree(directory, ignore_errors=True)
    ids = {}
    requests = [request_log(peer.requests, ids) for peer in peers]
    errors = [e for peer in peers for e in peer.errors]
    results = normalize(results, ids)
    text = json.dumps(results)
    if str(directory) in text:
        results = json.loads(text.replace(json.dumps(str(directory))[1:-1], "<dir>"))
    return {"requests": requests, "results": results, "errors": errors}


def results_match(item, python, rust):
    if item.get("compare") == "error_prefix":
        # Pydantic's validation text is not reproduced word for word; the failure and its tool are
        for p, r in zip(python, rust):
            if not (p.get("isError") and r.get("isError")):
                return False
            prefix = p["content"][0]["text"].split(":", 1)[0]
            if not r["content"][0]["text"].startswith(prefix + ": "):
                return False
            if "validation error" not in r["content"][0]["text"]:
                return False
        return len(python) == len(rust)
    return python == rust


def run_fixtures(binary, failures, only=None):
    summary = []
    implementations = servers(binary)
    for item in SCENARIOS:
        if only and not any(o in item["name"] for o in only):
            continue
        outcome = {}
        started = time.monotonic()
        for label, make_params in implementations:
            outcome[label] = asyncio.run(run_scenario(item, make_params))
        python, rust = outcome["python"], outcome["rust"]
        problems = []
        if python["errors"] or rust["errors"]:
            problems.append(f"peer errors {python['errors']} {rust['errors']}")
        if python["requests"] != rust["requests"]:
            problems.append("requests differ")
        if not results_match(item, python["results"], rust["results"]):
            problems.append("results differ")
        expected_requests = sum(1 for _ in item["script"])
        if len(python["requests"][0] if python["requests"] else []) != expected_requests:
            problems.append(f"Python sent {len(python['requests'][0]) if python['requests'] else 0} requests, "
                            f"the fixture scripts {expected_requests}")
        entry = {"scenario": item["name"], "ok": not problems, "problems": problems,
                 "seconds": round(time.monotonic() - started, 2),
                 "requests": len(python["requests"][0]) if python["requests"] else 0,
                 "is_error": [r.get("isError", False) for r in python["results"]]}
        if problems:
            entry["python"], entry["rust"] = python, rust
            failures.append(f"fixture {item['name']}: {', '.join(problems)}")
        summary.append(entry)
        print(f"  {'ok  ' if not problems else 'FAIL'} {item['name']} ({entry['seconds']} s)", flush=True)
    if sys.platform == "win32":
        shutil.rmtree(Path(tempfile.gettempdir()) / f"embeint-parity-{os.getpid()}", ignore_errors=True)
    return summary


# ---- Live comparison ------------------------------------------------------------------------------

def live(binary, failures):
    """Compare both servers against the running KiCad's disposable test project."""
    from e2e_guard import test_document  # refuses anything but the live-demo test project
    import live_parity
    return live_parity.run(binary, failures, servers(binary), run_session, server_env, test_document)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--build", action="store_true", help="build embeint-mcp (release) first")
    parser.add_argument("--binary", type=Path, help="embeint-mcp to test (default: rust/target/release)")
    parser.add_argument("--write-schemas", action="store_true", help="refresh tools.json from the Python server")
    parser.add_argument("--live", action="store_true", help="also compare against a running KiCad")
    parser.add_argument("--only", nargs="*", help="run only the fixtures whose name contains one of these")
    parser.add_argument("--report", type=Path, help="write the detailed results as JSON")
    args = parser.parse_args()
    if args.write_schemas:
        write_schemas()
        return 0
    if args.build:
        build_binary()
    binary = args.binary or binary_path()
    if not binary.exists():
        raise SystemExit(f"{binary} does not exist; run with --build")

    failures, report = [], {}
    print("schema", flush=True)
    report["schema"] = check_schema(binary, failures)
    print("codec and manifest", flush=True)
    report["codec"] = compare_codec(binary, failures)
    print("discovery inventory", flush=True)
    report["discovery"] = compare_inventory(binary, failures)
    print("protocol fixtures", flush=True)
    report["fixtures"] = run_fixtures(binary, failures, args.only)
    if args.live:
        print("live", flush=True)
        report["live"] = live(binary, failures)
    report["failures"] = failures
    if args.report:
        args.report.write_text(json.dumps(report, indent=2), encoding="utf-8")
    fixtures = report["fixtures"]
    codec = report["codec"]
    print(f"\n{len(fixtures) - sum(not f['ok'] for f in fixtures)}/{len(fixtures)} fixtures identical; "
          f"{codec['cases']} codec cases ({codec['accepted']} accepted, {codec['refused']} refused, "
          f"{len(codec['message_differences'])} with different refusal text)")
    for failure_text in failures:
        print("FAIL", failure_text)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
