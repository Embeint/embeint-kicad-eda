# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline contracts for whole-design verification: hierarchical netlist, page text overlaps and
kicad-cli runs in KiCad's launch environment.  No running KiCad is required."""

import asyncio
import os
from pathlib import Path

import pytest

import kicad_cli
from client import base, editor, types
from client import commands as sch_cmds
from test_client import FakeClient, _page_box, doc, page_reply

posix_only = pytest.mark.skipif(os.name != "posix", reason="Linux/WSL library paths and shell scripts")


ROOT, CHILD, REUSED_A, REUSED_B = "root-0000", "child-1111", "reuse-aaaa", "reuse-bbbb"


def sheet_path(*ids, human="/"):
    return types.SheetPath(path=[types.KIID(value=i) for i in ids], path_human_readable=human)


def hierarchy_reply():
    """Root R1; child Filter with R2 and C1; one sheet file used twice (U1 and U2 share pin UUIDs)."""
    paths = {
        (ROOT,): ("/", [("R1", [("r1-p1", "1"), ("r1-p2", "2")])]),
        (ROOT, CHILD): ("/Filter/", [("R2", [("r2-p1", "1"), ("r2-p2", "2")]), ("C1", [("c1-p1", "1")])]),
        (ROOT, REUSED_A): ("/Chan_A/", [("U1", [("u-p1", "1")])]),
        (ROOT, REUSED_B): ("/Chan_B/", [("U2", [("u-p1", "1")])]),
    }

    def reply(request, response_type):
        name = type(request).__name__
        if name == "GetSchematicHierarchy":
            response = sch_cmds.SchematicHierarchyResponse()
            root = response.top_level_sheets.add(name="Root", path=sheet_path(ROOT, human="/"))
            for sheet, human in ((CHILD, "/Filter/"), (REUSED_A, "/Chan_A/"), (REUSED_B, "/Chan_B/")):
                root.children.add(name=human.strip("/"), path=sheet_path(ROOT, sheet, human=human))
            return response
        if name == "GetSymbolPins":
            key = tuple(k.value for k in request.document.sheet_path.path)
            response = sch_cmds.SymbolPinsResponse()
            for reference, pins in paths[key][1]:
                symbol = response.symbols.add(reference=reference)
                for pin_id, number in pins:
                    symbol.pins.add(id=types.KIID(value=pin_id), number=number)
            return response
        if name == "GetSchematicNetlist":
            response = sch_cmds.SchematicNetlistResponse()
            net = response.nets.add(name="/Filter/SIGNAL")
            net.sheets.add(path=sheet_path(ROOT, human="/"),
                           items=[types.KIID(value="r1-p2"), types.KIID(value="root-wire")])
            net.sheets.add(path=sheet_path(ROOT, CHILD, human="/Filter/"),
                           items=[types.KIID(value="r2-p1"), types.KIID(value="c1-p1"), types.KIID(value="hlabel")])
            out = response.nets.add(name="/Filter/OUT")
            out.sheets.add(path=sheet_path(ROOT, CHILD, human="/Filter/"), items=[types.KIID(value="r2-p2")])
            for sheet, net_name in ((REUSED_A, "/Chan_A/IN"), (REUSED_B, "/Chan_B/IN")):
                reused = response.nets.add(name=net_name)
                reused.sheets.add(path=sheet_path(ROOT, sheet), items=[types.KIID(value="u-p1")])
            return response
        raise AssertionError(f"unexpected request {name}")
    return reply


def root_doc():
    spec = doc()
    spec.sheet_path.CopyFrom(sheet_path(ROOT))
    return spec


def test_hierarchy_netlist_lists_pins_on_every_sheet_with_their_sheet_path():
    client = FakeClient(hierarchy_reply())
    nets = client.netlist_pins(root_doc())
    assert nets["/Filter/SIGNAL"] == {
        "pins": ["C1.1", "R1.2", "R2.1"], "items": 5,
        "sheets": [{"path": "/", "pins": ["R1.2"]}, {"path": "/Filter/", "pins": ["C1.1", "R2.1"]}]}
    assert nets["/Filter/OUT"]["pins"] == ["R2.2"]
    # One sheet file placed twice: the shared pin UUID resolves to each instance's own reference
    assert nets["/Chan_A/IN"]["sheets"] == [{"path": "/Chan_A/", "pins": ["U1.1"]}]
    assert nets["/Chan_B/IN"]["sheets"] == [{"path": "/Chan_B/", "pins": ["U2.1"]}]
    pin_requests = [r for r in client.requests if type(r).__name__ == "GetSymbolPins"]
    assert len(pin_requests) == 4, "one pin read per sheet instance"


def test_hierarchy_netlist_does_not_depend_on_the_requested_sheet():
    child = root_doc()
    child.sheet_path.CopyFrom(sheet_path(ROOT, CHILD, human="/Filter/"))
    assert FakeClient(hierarchy_reply()).netlist_pins(child) == FakeClient(hierarchy_reply()).netlist_pins(root_doc())


def test_sheet_scope_keeps_the_original_sheet_only_reply():
    client = FakeClient(hierarchy_reply())
    nets = client.netlist_pins(root_doc(), scope="sheet")
    assert nets["/Filter/SIGNAL"] == {"pins": ["R1.2"], "items": 5}
    assert nets["/Filter/OUT"] == {"pins": [], "items": 1}
    assert not any(type(r).__name__ == "GetSchematicHierarchy" for r in client.requests)
    with pytest.raises(ValueError, match="scope"):
        client.netlist_pins(root_doc(), scope="project")


def test_netlist_tool_defaults_to_the_whole_hierarchy(monkeypatch):
    import server
    from google.protobuf import json_format

    fake = FakeClient(hierarchy_reply())
    monkeypatch.setattr(server, "client", fake)
    spec = json_format.MessageToDict(root_doc())
    whole = server.live_schematic_netlist(spec)
    assert whole["/Filter/SIGNAL"]["pins"] == ["C1.1", "R1.2", "R2.1"]
    assert server.live_schematic_netlist(spec, scope="sheet")["/Filter/SIGNAL"]["pins"] == ["R1.2"]


# ---- Page overlaps ------------------------------------------------------------------------------

def overlap_reply(checked=True, count=None, pairs=2):
    others = page_reply()

    def reply(request, response_type):
        response = others(request, response_type)
        if type(request).__name__ != "GetPageLayout":
            return response
        response.overlaps_checked = checked
        for index in range(pairs if checked else 0):
            entry = response.overlaps.add(overlap=_page_box(30, 30 + index, 32, 31 + index))
            entry.first.CopyFrom(editor.PageLayoutItem(id=types.KIID(value=f"r{index}"), type="Symbol", name="R1",
                                                       part="Value", box=_page_box(28, 29, 34, 32)))
            entry.second.CopyFrom(editor.PageLayoutItem(id=types.KIID(value=f"l{index}"), type="Label", name="SDA",
                                                        box=_page_box(30, 30, 40, 33)))
        response.overlap_count = pairs if count is None else count
        return response
    return reply


def test_page_reports_overlap_pairs_without_changing_fits():
    page = FakeClient(overlap_reply()).page_layout(doc())
    assert page["fits"] and page["problems"] == []
    assert page["overlap_free"] is False and len(page["overlaps"]) == 2
    assert page["overlaps"][0] == {
        "first": {"id": "r0", "type": "Symbol", "name": "R1", "part": "Value", "box_mm": [28.0, 29.0, 34.0, 32.0]},
        "second": {"id": "l0", "type": "Label", "name": "SDA", "box_mm": [30.0, 30.0, 40.0, 33.0]},
        "overlap_mm": [30.0, 30.0, 32.0, 31.0]}
    assert "overlaps_not_listed" not in page and "overlap_check" not in page


def test_page_counts_overlaps_beyond_the_listed_pairs():
    page = FakeClient(overlap_reply(count=250, pairs=2)).page_layout(doc())
    assert page["overlap_free"] is False and page["overlaps_not_listed"] == 248


def test_page_is_overlap_free_only_when_the_editor_checked():
    clean = FakeClient(overlap_reply(pairs=0)).page_layout(doc())
    assert clean["overlap_free"] is True and clean["overlaps"] == []
    older = FakeClient(overlap_reply(checked=False)).page_layout(doc())
    assert older["overlap_free"] is None and older["overlaps"] == []
    assert "not available" in older["overlap_check"] and older["fits"]


def test_board_overlap_keeps_its_layer():
    others = overlap_reply()

    def reply(request, response_type):
        response = others(request, response_type)
        if type(request).__name__ == "GetPageLayout":
            response.overlaps[0].layer = "F.Silkscreen"
        return response

    page = FakeClient(reply).page_layout(doc())
    assert page["overlaps"][0]["layer"] == "F.Silkscreen" and "layer" not in page["overlaps"][1]


# ---- kicad-cli environment ----------------------------------------------------------------------

def test_frozen_mcp_does_not_pass_its_private_libraries_to_kicad():
    frozen = {"LD_LIBRARY_PATH": "/opt/embeint-kicad/libexec/mcp/_internal:/opt/embeint-kicad/lib",
              "LD_LIBRARY_PATH_ORIG": "/opt/embeint-kicad/lib", "HOME": "/home/u"}
    assert kicad_cli.child_environment(frozen, frozen=True) == {"LD_LIBRARY_PATH": "/opt/embeint-kicad/lib",
                                                                "HOME": "/home/u"}
    assert "LD_LIBRARY_PATH" not in kicad_cli.child_environment({"LD_LIBRARY_PATH": "/mei"}, frozen=True)
    assert kicad_cli.child_environment(frozen, frozen=False) == frozen


def test_kicad_reported_environment_takes_precedence(tmp_path):
    reported = {"KICAD_CONFIG_HOME": "/home/u/.config/embeint-kicad", "LD_LIBRARY_PATH": "/opt/embeint-kicad/lib",
                "KICAD10_SYMBOL_DIR": "/usr/share/kicad/symbols", "TMPDIR": "/run/user/1000/embeint-kicad-1000"}
    env, description = kicad_cli.cli_environment(tmp_path / "bin/kicad-cli", reported,
                                                 environ={"KICAD_CONFIG_HOME": "/elsewhere", "PATH": "/usr/bin"},
                                                 frozen=False)
    assert env == {**reported, "PATH": "/usr/bin"}
    assert description == {"source": "kicad", "variables": sorted(reported)}


@posix_only
def test_development_layout_matches_run_kicad_wsl(tmp_path):
    root = tmp_path / "kicad-live"
    for folder in ("install/bin", "install/lib", "config", "share/symbols", "share/footprints"):
        (root / folder).mkdir(parents=True)
    cli = root / "install/bin/kicad-cli"
    cli.write_text("")
    stock = tmp_path / "stock"
    (stock / "3dmodels").mkdir(parents=True)
    env, description = kicad_cli.cli_environment(cli, None, environ={"KICAD_LIBRARY_ROOT": str(stock)},
                                                 frozen=False)
    assert description["source"] == "install layout"
    assert env["LD_LIBRARY_PATH"].split(":")[0] == str((root / "install/lib").resolve())
    assert env["KICAD_CONFIG_HOME"] == str((root / "config").resolve())
    assert env["KICAD10_SYMBOL_DIR"] == env["KICAD11_SYMBOL_DIR"] == str((root / "share/symbols").resolve())
    assert env["KICAD10_FOOTPRINT_DIR"] == str((root / "share/footprints").resolve())
    assert env["KICAD10_3DMODEL_DIR"] == str(stock / "3dmodels")
    assert {"KICAD_CONFIG_HOME", "LD_LIBRARY_PATH", "KICAD10_SYMBOL_DIR"} <= set(description["variables"])


@posix_only
def test_installed_layout_matches_the_package_environment(tmp_path):
    prefix = tmp_path / "opt/embeint-kicad"
    for folder in ("bin", "lib", "libexec"):
        (prefix / folder).mkdir(parents=True)
    (prefix / "libexec/environment.sh").write_text("")
    cli = prefix / "bin/kicad-cli"
    cli.write_text("")
    env, _ = kicad_cli.cli_environment(cli, None, environ={"HOME": "/home/u"}, frozen=False)
    assert env["KICAD_CONFIG_HOME"] == "/home/u/.config/embeint-kicad"
    assert env["LD_LIBRARY_PATH"] == str((prefix / "lib").resolve())
    assert not any(name.startswith("KICAD10_") for name in env), "stock paths come from the package's tables"
    env, _ = kicad_cli.cli_environment(cli, None, environ={"HOME": "/home/u", "XDG_CONFIG_HOME": "/cfg",
                                                           "KICAD_CONFIG_HOME": "/chosen"}, frozen=False)
    assert env["KICAD_CONFIG_HOME"] == "/chosen", "a configuration home already set is kept"


def test_default_cli_finds_the_package_beside_a_frozen_mcp(tmp_path):
    prefix = tmp_path / "opt/embeint-kicad"
    (prefix / "bin").mkdir(parents=True)
    (prefix / "libexec/mcp").mkdir(parents=True)
    (prefix / "bin/kicad-cli").write_text("")
    found = kicad_cli.default_cli({"HOME": str(tmp_path)}, executable=prefix / "libexec/mcp/embeint-kicad-mcp")
    assert Path(found) == (prefix / "bin/kicad-cli").resolve()
    assert kicad_cli.default_cli({"HOME": str(tmp_path / "nobody")}) is None
    explicit = tmp_path / "custom-cli"
    explicit.write_text("")
    assert kicad_cli.default_cli({"KICAD_CLI": str(explicit), "HOME": str(tmp_path)}) == str(explicit)


@pytest.mark.parametrize("args", [[], ["rm", "-rf", "/"], ["sch", 3], ["sch", "erc\0"], "sch erc"])
def test_cli_arguments_are_refused_before_starting_a_process(args):
    with pytest.raises(ValueError):
        kicad_cli.cli_arguments(args)


@posix_only
def test_cli_windows_paths_are_translated_on_wsl():
    assert kicad_cli.cli_arguments(["sch", "erc", "-o", "C:\\out\\erc.json", "D:/p/a.kicad_sch"]) == [
        "sch", "erc", "-o", "/mnt/c/out/erc.json", "/mnt/d/p/a.kicad_sch"]


def environment_reply(cli_path, variables=None, unhandled=False):
    def reply(request, response_type):
        name = type(request).__name__
        if name == "GetKiCadBinaryPath":
            assert request.binary_name == "kicad-cli"
            return base.PathResponse(path=str(cli_path))
        if name == "GetKiCadEnvironment":
            if unhandled:
                raise RuntimeError("AS_UNHANDLED: ")
            return base.KiCadEnvironmentResponse(variables=variables or {})
        raise AssertionError(f"unexpected request {name}")
    return reply


def shell_script(path, body):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("#!/bin/sh\n" + body)
    path.chmod(0o755)
    return path


@posix_only
def test_run_kicad_cli_uses_the_environment_kicad_reports(tmp_path):
    cli = shell_script(tmp_path / "bin/kicad-cli",
                       'echo "config=$KICAD_CONFIG_HOME args=$*"\necho warn >&2\nexit 3\n')
    client = FakeClient(environment_reply(cli, {"KICAD_CONFIG_HOME": "/launcher/config"}))
    result = client.run_kicad_cli(["sch", "erc", "C:\\p\\a.kicad_sch"], timeout_s=30)
    assert result["returncode"] == 3 and result["stderr"] == "warn\n"
    assert result["stdout"] == "config=/launcher/config args=sch erc /mnt/c/p/a.kicad_sch\n"
    assert result["environment"] == {"source": "kicad", "variables": ["KICAD_CONFIG_HOME"]}
    assert result["command"][0] == str(cli)


@posix_only
def test_older_kicad_falls_back_to_the_install_layout(tmp_path, monkeypatch):
    monkeypatch.delenv("KICAD_CONFIG_HOME", raising=False)
    cli = shell_script(tmp_path / "kicad-live/install/bin/kicad-cli", 'echo "$KICAD_CONFIG_HOME"\n')
    (tmp_path / "kicad-live/config").mkdir()
    result = FakeClient(environment_reply(cli, unhandled=True)).run_kicad_cli(["version"], timeout_s=30)
    assert result["environment"]["source"] == "install layout"
    assert result["stdout"].strip() == str((tmp_path / "kicad-live/config").resolve())


def test_unreachable_kicad_uses_the_located_cli_or_says_what_to_set(monkeypatch, tmp_path):
    def unreachable(request, response_type):
        raise RuntimeError("KiCad socket selection is default_missing")

    monkeypatch.setattr(kicad_cli, "default_cli", lambda: None)
    with pytest.raises(RuntimeError, match="KICAD_CLI"):
        FakeClient(unreachable).kicad_cli_launch()
    cli = tmp_path / "kicad-cli"
    cli.write_text("")
    monkeypatch.setattr(kicad_cli, "default_cli", lambda: str(cli))
    path, env, description = FakeClient(unreachable).kicad_cli_launch()
    assert path == str(cli) and description["source"] == "install layout"
    assert "not reachable" in description["note"]
    with pytest.raises(ValueError):
        FakeClient(unreachable).run_kicad_cli(["bash"])


def test_cli_tool_is_registered_with_a_list_argument():
    import server

    tools = {tool.name: tool for tool in asyncio.run(server.mcp.list_tools())}
    schema = tools["live_kicad_cli"].inputSchema
    assert schema["properties"]["args"]["type"] == "array" and schema["required"] == ["args"]


@posix_only
def test_package_ships_a_kicad_cli_wrapper_with_the_launcher_environment(tmp_path):
    from qualification.package_linux import ENVIRONMENT, stage_launchers

    stage_launchers(tmp_path)
    wrapper = tmp_path / "usr/bin/embeint-kicad-cli"
    assert wrapper.read_text().splitlines() == [
        "#!/bin/sh", "set -eu", ". /opt/embeint-kicad/libexec/environment.sh",
        'exec /opt/embeint-kicad/bin/kicad-cli "$@"']
    assert os.access(wrapper, os.X_OK)
    assert (tmp_path / "opt/embeint-kicad/libexec/environment.sh").read_text() == ENVIRONMENT
    for name in ("embeint-kicad", "embeint-kicad-mcp"):
        assert ". /opt/embeint-kicad/libexec/environment.sh" in (tmp_path / "usr/bin" / name).read_text()


def test_installed_check_runs_cli_through_the_mcp_and_the_wrapper():
    import json
    from types import SimpleNamespace
    from qualification import check_install

    calls, executed = [], []

    class Peer:
        async def call_tool(self, name, arguments):
            calls.append((name, arguments["args"][:3]))
            return SimpleNamespace(isError=False, structuredContent={
                "returncode": 0, "stdout": "", "stderr": "",
                "environment": {"source": "kicad", "variables": ["KICAD_CONFIG_HOME", "LD_LIBRARY_PATH"]}})

    class Runtime:
        def __init__(self, violations):
            self.violations = violations

        def execute(self, *args):
            executed.append(args)
            if args[-1] == "/tmp/installed-erc.json":
                return json.dumps({"sheets": [{"violations": [{"type": t} for t in self.violations]}]})
            if args[-1] == "/tmp/installed.net":
                return '(node (ref "R1") (pin "2")) (node (ref "R2") (pin "2"))'
            return "10.99.0\n"

    result = asyncio.run(check_install.check_cli(Runtime(["pin_not_connected"]), Peer()))
    assert [c[1][:2] for c in calls] == [["sch", "erc"], ["sch", "export"]]
    assert result["wrapper_version"] == "10.99.0" and ("/usr/bin/embeint-kicad-cli", "version") in executed
    with pytest.raises(AssertionError):
        asyncio.run(check_install.check_cli(Runtime(["lib_symbol_issues"]), Peer()))
