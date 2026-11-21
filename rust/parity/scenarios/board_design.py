# SPDX-License-Identifier: GPL-3.0-or-later
"""Parity scenarios for the board design tools (tools/board_design.rs, board_design.rs,
board_router.rs and board_rf.rs against board_design.py, board_router.py and board_rf.py).

Rules, custom rules, net classes, stackup, impedance, RF nets, checks and keepouts are compared
exactly, and so are the routes and path checks wherever the two routers give the same result: the
fixture boards keep grid points clear of the clearance boundaries, and the Rust router follows
GEOS's own polygons for the convex clearance zones (pads, vias, tracks) and for round joins, so every
route and path check here comes out identical except board_design_route_tight.  That scenario is
built to differ (a grid point 3 um inside a non-convex pad's clearance, which GEOS's 6-segment
fillet leaves outside) and is compared by behaviour, as rust/MCP_PARITY_PLAN.md (Decisions) allows:

- routes (compare=route_behaviour; live_pcb_route, live_pcb_route_net): the same success or refusal
  (and error text), the same pads joined on the same layers with the same number of vias, a length
  within the larger of one grid step and 2 % (ROUTE_LENGTH_TOLERANCE), geometry_checked and the DRC
  result the same; the requests may differ only in the coordinates of the tracks and vias a route
  creates (their number may differ; their kinds, layers, nets and widths must agree), and the DRC
  verification and cleanup requests must be the same kinds.  One grid step is the most a route moves
  when a grid point near a clearance boundary falls the other way; 2 % covers a detour that changes
  side round a small obstacle.
- path checks (path_behaviour, for live_pcb_check_path and the check in live_pcb_add_tracks; ready
  for a board where they diverge, exercised by _self_test): the same success or refusal, the same
  conflicts in the same order (kind, net, needed clearance), and gaps within GAP_TOLERANCE_MM (1 um).
  A multi-point path is buffered with geo's round joins, which can sit a fraction of a micrometre
  from GEOS's (0.15 um at most for a 0.25 mm track) and whose distances may differ in the last bits;
  1 um is ten times the rounding of gap_mm (0.1 um) and far below any clearance.

Fixture items have ids that are not version-4 UUIDs, so scenarios that create items
(generated_ids=True) compare the servers' own ids by order of appearance and the fixture ids exactly.
"""

import json

ANY = "type.googleapis.com/kiapi.board.types."
U1 = "f1000000-0000-0000-0000-000000000001"
R1 = "f1000000-0000-0000-0000-000000000002"
J1 = "f1000000-0000-0000-0000-000000000003"
GAP_TOLERANCE_MM = 0.001
ROUTE_LENGTH_TOLERANCE = 0.02


def nm(mm):
    return str(round(mm * 1_000_000))


def vec(x, y):
    return {"x_nm": nm(x), "y_nm": nm(y)}


# ---- Behavioural comparisons ------------------------------------------------------------------

def _text(result):
    return result["content"][0]["text"] if result.get("content") else ""


def _json(result):
    try:
        return json.loads(_text(result))
    except ValueError:
        return None


def _same_errors(python, rust):
    """Both servers fail or succeed on every step; failures with the same text."""
    problems = []
    if len(python["results"]) != len(rust["results"]):
        return ["different number of results"]
    for index, (p, r) in enumerate(zip(python["results"], rust["results"])):
        if bool(p.get("isError")) != bool(r.get("isError")):
            problems.append(f"step {index}: one server failed: {_text(p)[:200]} | {_text(r)[:200]}")
        elif p.get("isError") and _text(p) != _text(r):
            problems.append(f"step {index}: errors differ: {_text(p)[:200]} | {_text(r)[:200]}")
    return problems


def _conflicts_agree(p, r, where):
    problems = []
    if p.get("clear") != r.get("clear") or len(p["conflicts"]) != len(r["conflicts"]):
        return [f"{where}: conflicts differ: {p} | {r}"]
    for a, b in zip(p["conflicts"], r["conflicts"]):
        key = ("kind", "net", "needed_mm")
        if any(a.get(k) != b.get(k) for k in key) or set(a) != set(b):
            problems.append(f"{where}: conflict differs: {a} | {b}")
        elif "gap_mm" in a and abs(a["gap_mm"] - b["gap_mm"]) > GAP_TOLERANCE_MM:
            problems.append(f"{where}: gap differs beyond {GAP_TOLERANCE_MM} mm: {a} | {b}")
    return problems


def _conflicts_in_error(text):
    """The conflicts list in add_tracks' refusal, as Python's repr."""
    import ast
    start = text.index("copper: ") + len("copper: ")
    end = text.index(" (use live_pcb_route")
    return ast.literal_eval(text[start:end])


def path_behaviour(python, rust):
    """check_path / add_tracks: same refusals and conflicts, gaps within GAP_TOLERANCE_MM."""
    problems = _same_errors(python, rust)
    if problems:
        # add_tracks refusals carry the conflicts: compare those by behaviour too
        for index, (p, r) in enumerate(zip(python["results"], rust["results"])):
            if p.get("isError") and r.get("isError") and "conflicts with other copper" in _text(p):
                try:
                    problems = [x for x in problems if not x.startswith(f"step {index}:")]
                    problems += _conflicts_agree({"clear": False, "conflicts": _conflicts_in_error(_text(p))},
                                                 {"clear": False, "conflicts": _conflicts_in_error(_text(r))},
                                                 f"step {index}")
                except (ValueError, SyntaxError):
                    problems.append(f"step {index}: unreadable refusal")
        if problems:
            return problems
    for index, (p, r) in enumerate(zip(python["results"], rust["results"])):
        if not p.get("isError") and "conflicts" in (_json(p) or {}):
            problems += _conflicts_agree(_json(p), _json(r), f"step {index}")
        elif not p.get("isError") and _json(p) != _json(r):
            problems.append(f"step {index}: results differ")
    if python["requests"] != rust["requests"]:
        problems.append("requests differ")
    return problems


def _route_summary(result):
    """What two routes must share: pads, layers, vias, checks; and the length."""
    segments = result.get("segments")
    summary = {k: v for k, v in result.items() if k not in ("segments", "vias", "length_mm", "items", "legs")}
    if segments is not None:
        summary["layers"] = [s["layer"] for s in segments]
        summary["ends"] = [segments[0]["path_mm"][0], segments[-1]["path_mm"][-1]]
        summary["vias"] = len(result["vias"])
    if "legs" in result:
        summary["legs"] = [{k: v for k, v in leg.items() if k != "length_mm"} for leg in result["legs"]]
    return summary


def _lengths(result):
    if "legs" in result:
        return [leg["length_mm"] for leg in result["legs"]]
    return [result["length_mm"]] if "length_mm" in result else []


def _request_kinds(requests):
    """The requests with created tracks and vias reduced to their kind, layer, net and width."""
    out = []
    for request in requests:
        message = dict(request.get("message", {}))
        if message.get("@type", "").endswith(".CreateItems"):
            message["items"] = sorted(json.dumps({k: item.get(k) for k in ("@type", "layer", "net", "width")},
                                                 sort_keys=True) for item in message.get("items", []))
            message["items"] = sorted(set(message["items"]))
        if message.get("@type", "").endswith(".DeleteItems"):
            message["item_ids"] = len(message.get("item_ids", []))
        out.append({**request, "message": message})
    return out


def route_behaviour(grid=0.1):
    def compare(python, rust):
        problems = _same_errors(python, rust)
        for index, (p, r) in enumerate(zip(python["results"], rust["results"])):
            if p.get("isError") or r.get("isError"):
                continue
            pj, rj = _json(p), _json(r)
            if _route_summary(pj) != _route_summary(rj):
                problems.append(f"step {index}: routes differ: {_route_summary(pj)} | {_route_summary(rj)}")
            for a, b in zip(_lengths(pj), _lengths(rj)):
                if abs(a - b) > max(grid, ROUTE_LENGTH_TOLERANCE * a):
                    problems.append(f"step {index}: lengths differ beyond tolerance: {a} | {b}")
        pr, rr = python["requests"], rust["requests"]
        if len(pr) != len(rr) or any(len(a) != len(b) for a, b in zip(pr, rr)):
            problems.append("different requests")
        elif [_request_kinds(a) for a in pr] != [_request_kinds(b) for b in rr]:
            problems.append("requests differ beyond the route coordinates")
        return problems
    return compare


def _self_test():
    """The behavioural comparisons accept differences within their tolerances and refuse the rest
    (no path check diverges in the fixtures, so this is what exercises path_behaviour)."""
    def outcome(*values, requests=()):
        return {"requests": [list(requests)], "written": {}, "errors": [], "results": [
            {"content": [{"type": "text", "text": v if isinstance(v, str) else json.dumps(v)}],
             "isError": isinstance(v, str)} for v in values]}

    def check(gap, kind="pad"):
        return {"clear": False, "conflicts": [{"kind": kind, "net": "GND", "gap_mm": gap, "needed_mm": 0.25}]}

    refusal = ("Error executing tool live_pcb_add_tracks: the path conflicts with other copper: "
               "[{'kind': 'pad', 'net': 'GND', 'gap_mm': %s, 'needed_mm': 0.25}] (use live_pcb_route to "
               "find a clear way, or check=false to draw it anyway)")
    assert not path_behaviour(outcome(check(0.1002)), outcome(check(0.1009)))
    assert path_behaviour(outcome(check(0.1002)), outcome(check(0.1013)))
    assert path_behaviour(outcome(check(0.1002)), outcome(check(0.1002, "track")))
    assert path_behaviour(outcome(check(0.1002)), outcome({"clear": True, "conflicts": []}))
    assert not path_behaviour(outcome(refusal % 0.1002), outcome(refusal % 0.1006))
    assert path_behaviour(outcome(refusal % 0.1002), outcome(refusal % 0.2))
    segments = [{"layer": "F.Cu", "path_mm": [[10.0, 15.0], [30.0, 15.0]]}]
    routed = {"net": "SIG", "width_mm": 0.25, "length_mm": 20.0, "vias": [], "segments": segments,
              "items": ["<id1>"], "geometry_checked": True}
    longer = dict(routed, length_mm=20.3, items=["<id1>", "<id2>"])
    assert not route_behaviour()(outcome(routed), outcome(longer))
    assert route_behaviour()(outcome(routed), outcome(dict(routed, length_mm=21.0)))
    assert route_behaviour()(outcome(routed), outcome(dict(routed, vias=[[20.0, 15.0]])))
    assert route_behaviour()(outcome(routed), outcome("Error executing tool live_pcb_route: no path"))


# ---- Scenarios ---------------------------------------------------------------------------------

def scenarios(h):
    from google.protobuf import empty_pb2, json_format

    from board_client import bc, board_tasks, bt, project, tasks

    editor, envelope, types = h.editor, h.envelope, h.types
    reply, parse, failure, S = h.reply, h.parse, h.failure, h.S
    B = h.BOARD_DOC
    BEGIN, END_OK, DROP_OK = h.BEGIN, h.END_OK, h.DROP_OK

    def ok(kind):
        return (kind, lambda r: reply(empty_pb2.Empty()))

    def items(*values, status="IRS_OK"):
        return lambda r: reply(parse(editor.GetItemsResponse, {"status": status, "items": list(values)}))

    def get_items(*values):
        return ("GetItems", items(*values))

    def text(value):
        return {"text": {"text": {"text": value}}}

    def footprint(fid, ref, x, y, layer="BL_F_Cu", definition_items=()):
        return {"@type": ANY + "FootprintInstance", "id": {"value": fid}, "position": vec(x, y), "layer": layer,
                "reference_field": text(ref), "definition": {"items": list(definition_items)}}

    def pad(parent, number, net, x, y, kind="PT_SMD", layers=("BL_F_Cu",), drill=None):
        stack = {"layers": list(layers)}
        if drill:
            stack["drill"] = {"diameter": {"x_nm": nm(drill), "y_nm": nm(drill)}}
        return {"@type": ANY + "Pad", "id": {"value": f"b1000000-0000-0000-0000-{parent[-4:]}{int(number):08d}"},
                "number": str(number), "net": {"name": net}, "type": kind, "parent": {"value": parent},
                "position": vec(x, y), "pad_stack": stack}

    def pad_id(p):
        return p["id"]["value"]

    def rect(cx, cy, w, hgt):
        return {"outline": {"closed": True, "nodes": [{"point": vec(cx - w / 2, cy - hgt / 2)},
                                                      {"point": vec(cx + w / 2, cy - hgt / 2)},
                                                      {"point": vec(cx + w / 2, cy + hgt / 2)},
                                                      {"point": vec(cx - w / 2, cy + hgt / 2)}]}}

    def pad_shapes(shapes):
        """GetPadShapeAsPolygon: each requested pad's polygon (shapes: pad id -> PolygonWithHoles)."""
        def respond(request):
            query = bc.GetPadShapeAsPolygon()
            request.message.Unpack(query)
            ids = [p.value for p in query.pads]
            return reply(parse(bc.PadShapeAsPolygonResponse, {"pads": [{"value": i} for i in ids],
                                                               "polygons": [shapes[i] for i in ids]}))
        return ("GetPadShapeAsPolygon", respond)

    def segment(x1, y1, x2, y2, layer="BL_Edge_Cuts", width=0.05, net=None, fid="e1000000-0000-0000-0000-000000000000"):
        shape = {"@type": ANY + "BoardGraphicShape", "id": {"value": fid}, "layer": layer,
                 "shape": {"segment": {"start": vec(x1, y1), "end": vec(x2, y2)},
                           "attributes": {"stroke": {"width": {"value_nm": nm(width)}}}}}
        if net:
            shape["net"] = {"name": net}
        return shape

    def outline(x1, y1, x2, y2):
        return [segment(x1, y1, x2, y1), segment(x2, y1, x2, y2), segment(x2, y2, x1, y2), segment(x1, y2, x1, y1)]

    def track(tid, x1, y1, x2, y2, net, layer="BL_F_Cu", width=0.25):
        return {"@type": ANY + "Track", "id": {"value": tid}, "start": vec(x1, y1), "end": vec(x2, y2),
                "width": {"value_nm": nm(width)}, "layer": layer, "net": {"name": net}}

    def via(vid, x, y, net, size=0.6):
        return {"@type": ANY + "Via", "id": {"value": vid}, "position": vec(x, y), "net": {"name": net},
                "type": "VT_THROUGH",
                "pad_stack": {"copper_layers": [{"layer": "BL_F_Cu", "size": {"x_nm": nm(size), "y_nm": nm(size)}}]}}

    def keepout_zone(zid, x1, y1, x2, y2, layers=("BL_F_Cu", "BL_B_Cu"), tracks=True, vias=True):
        return {"@type": ANY + "Zone", "id": {"value": zid}, "type": "ZT_RULE_AREA", "layers": list(layers),
                "outline": {"polygons": [{"outline": {"closed": True, "nodes": [
                    {"point": vec(x1, y1)}, {"point": vec(x2, y1)}, {"point": vec(x2, y2)}, {"point": vec(x1, y2)}]}}]},
                "rule_area_settings": {"keepout_tracks": tracks, "keepout_vias": vias, "keepout_copper": True}}

    def enabled(count=2):
        return ("GetBoardEnabledLayers", lambda r: reply(bc.BoardEnabledLayersResponse(copper_layer_count=count)))

    rules_value = {
        "constraints": {"min_clearance": {"value_nm": "200000"}, "min_track_width": {"value_nm": "127000"},
                        "copper_edge_clearance": {"value_nm": "300000"}, "min_via_size": {"value_nm": "500000"},
                        "min_through_drill": {"value_nm": "254000"}, "hole_to_hole_min": {"value_nm": "250000"},
                        "silk_clearance": {"value_nm": "12345"}, "min_groove_width": {"value_nm": "333333"}},
        "predefined_sizes": {"tracks": [{"width": {"value_nm": "250000"}}, {"width": {"value_nm": "123456"}}],
                             "vias": [{"diameter": {"value_nm": "600000"}, "drill": {"value_nm": "300000"}}]},
        "solder_mask_paste": {"mask_expansion": {"value_nm": "51000"}, "mask_min_width": {"value_nm": "100000"}},
        "severities": [{"rule_type": "DRCET_CLEARANCE", "severity": "RS_ERROR"},
                       {"rule_type": "DRCET_SILK_CLEARANCE", "severity": "RS_WARNING"},
                       {"rule_type": "DRCET_CLEARANCE", "severity": "RS_IGNORE"}]}

    def design_rules(value=None):
        return ("GetBoardDesignRules", lambda r: reply(parse(bc.BoardDesignRulesResponse,
                                                             {"rules": rules_value if value is None else value})))

    def echo_rules():
        def respond(request):
            query = bc.SetBoardDesignRules()
            request.message.Unpack(query)
            return reply(bc.BoardDesignRulesResponse(rules=query.rules))
        return ("SetBoardDesignRules", respond)

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

    def all_created(record=None):
        def respond(request):
            create = editor.CreateItems()
            request.message.Unpack(create)
            result = editor.CreateItemsResponse(status=types.IRS_OK, header=create.header)
            for packed in create.items:
                entry = result.created_items.add(item=packed)
                entry.status.code = editor.ISC_OK
            if record is not None:
                record[:] = [json_format.MessageToDict(p)["id"]["value"] for p in create.items]
            return reply(result, revision="fx-2")
        return ("CreateItems", respond)

    def deleted():
        def respond(request):
            edit = editor.DeleteItems()
            request.message.Unpack(edit)
            response = editor.DeleteItemsResponse(header=edit.header, status=types.IRS_OK)
            for item_id in edit.item_ids:
                response.deleted_items.add(id=item_id, status=editor.IDS_OK)
            return reply(response, revision="fx-5")
        return ("DeleteItems", respond)

    commit = [BEGIN, all_created(), END_OK]

    def drc(violations=(), state="TS_COMPLETED", error="", record=None):
        script = [("StartDrcTask", lambda r: reply(tasks.StartTaskResponse(token=tasks.TaskToken(value="drc-r")))),
                  ("GetTaskStatus", lambda r: reply(tasks.TaskStatusResponse(state=tasks.TaskState.Value(state),
                                                                             error_message=error)))]
        if state == "TS_COMPLETED":
            def respond(request):
                found = []
                for v in violations:
                    v = dict(v)
                    if v.get("items") == "NEW" and record:
                        v["items"] = [{"value": record[0]}]
                    found.append(v)
                return reply(parse(board_tasks.DrcTaskResult, {"violations": found}))
            script.append(("GetDrcTaskResult", respond))
        return script

    stackup_2 = {"layers": [
        {"type": "BSLT_SILKSCREEN", "layer": "BL_F_SilkS", "color": {"r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0}},
        {"type": "BSLT_SOLDERMASK", "layer": "BL_F_Mask", "thickness": {"value_nm": "10000"},
         "soldermask": {"epsilon_r": 3.3, "material_name": "LPI"}},
        {"type": "BSLT_COPPER", "layer": "BL_F_Cu", "thickness": {"value_nm": "35000"}},
        {"type": "BSLT_DIELECTRIC", "thickness": {"value_nm": "1510000"},
         "dielectric": {"type": "BSDT_CORE", "layer": [{"thickness": {"value_nm": "1510000"}, "epsilon_r": 4.5,
                                                        "loss_tangent": 0.02, "material_name": "FR4"}]}},
        {"type": "BSLT_COPPER", "layer": "BL_B_Cu", "thickness": {"value_nm": "35000"}}],
        "finish": {"type_name": "ENIG"}, "impedance": {"is_controlled": True}}
    stackup_4 = {"layers": [
        {"type": "BSLT_COPPER", "layer": "BL_F_Cu", "thickness": {"value_nm": "35000"}},
        {"type": "BSLT_DIELECTRIC", "thickness": {"value_nm": "200000"},
         "dielectric": {"type": "BSDT_PREPREG", "layer": [{"thickness": {"value_nm": "120000"}, "epsilon_r": 4.2},
                                                          {"thickness": {"value_nm": "80000"}, "epsilon_r": 0.0}]}},
        {"type": "BSLT_COPPER", "layer": "BL_In1_Cu", "thickness": {"value_nm": "17500"}},
        {"type": "BSLT_DIELECTRIC", "thickness": {"value_nm": "1000000"}},
        {"type": "BSLT_COPPER", "layer": "BL_In2_Cu", "thickness": {"value_nm": "17500"}},
        {"type": "BSLT_DIELECTRIC", "thickness": {"value_nm": "200000"},
         "dielectric": {"type": "BSDT_PREPREG", "layer": [{"thickness": {"value_nm": "200000"}, "epsilon_r": 4.2}]}},
        {"type": "BSLT_COPPER", "layer": "BL_B_Cu", "thickness": {"value_nm": "0"}}]}

    def stackup(value):
        return ("GetBoardStackup", lambda r: reply(parse(bc.BoardStackupResponse, {"stackup": value})))

    def echo_stackup(value=None):
        def respond(request):
            query = bc.UpdateBoardStackup()
            request.message.Unpack(query)
            return reply(bc.BoardStackupResponse(stackup=query.stackup) if value is None
                         else parse(bc.BoardStackupResponse, {"stackup": value}))
        return ("UpdateBoardStackup", respond)

    # ---- Net classes and custom rules
    netclass_default = {"name": "Default", "priority": 2147483647, "board": {
        "clearance": {"value_nm": "200000"}, "track_width": {"value_nm": "250000"},
        "via_stack": {"copper_layers": [{"layer": "BL_F_Cu", "size": {"x_nm": "600000", "y_nm": "600000"}}],
                      "drill": {"diameter": {"x_nm": "300000", "y_nm": "300000"}}}}}
    netclass_power = {"name": "Power", "board": {"track_width": {"value_nm": "500000"},
                                                 "diff_pair_track_width": {"value_nm": "123456"},
                                                 "diff_pair_gap": {"value_nm": "100000"}}}

    def get_netclasses(*listing):
        return ("GetNetClasses", lambda r: reply(parse(project.NetClassesResponse,
                                                       {"net_classes": list(listing)})))

    def get_assignments(assign=None, patterns=None):
        return ("GetNetClassAssignments", lambda r: reply(parse(project.NetClassAssignmentsResponse, {
            "assignments": [{"net": n, "netclasses": c} for n, c in (assign or {}).items()],
            "pattern_assignments": [{"pattern": p, "netclass": c} for p, c in (patterns or {}).items()]})))

    custom_existing = [
        {"name": "RF clearance", "condition": "A.NetClass == 'RF'", "severity": "RS_WARNING", "layer_mode": "CRLM_OUTER",
         "constraints": [{"type": "CRCT_CLEARANCE", "numeric": {"min": 500000, "max": 2000000}}]},
        {"name": "No vias", "condition": "A.insideArea('ANT')", "single_layer": "BL_B_Cu",
         "constraints": [{"type": "CRCT_DISALLOW", "disallow": {"types": ["CRDT_THROUGH_VIAS", "CRDT_MICRO_VIAS"]}},
                         {"type": "CRCT_ASSERTION", "assertion_expression": "A.Type == 'Pad'"},
                         {"type": "CRCT_ZONE_CONNECTION", "zone_connection": "ZCS_FULL"}]},
        {"name": "Old", "condition": "", "constraints": [{"type": 999}]}]

    def get_custom(rules=None, status="CRS_VALID", error=""):
        return ("GetCustomDesignRules", lambda r: reply(parse(bc.CustomRulesResponse, {
            "status": status, "rules": custom_existing if rules is None else rules, "error_text": error})))

    def set_custom(status="CRS_VALID", error=""):
        def respond(request):
            query = bc.SetCustomDesignRules()
            request.message.Unpack(query)
            response = bc.CustomRulesResponse(status=bc.CustomRulesStatus.Value(status), error_text=error)
            response.rules.extend(query.rules)
            return reply(response)
        return ("SetCustomDesignRules", respond)

    # ---- A board for checks and routes: 40 x 30 mm, U1 and R1 with SIG and GND pads
    fp_u1 = footprint(U1, "U1", 10, 15)
    fp_r1 = footprint(R1, "R1", 30, 15)
    footprints = get_items(fp_u1, fp_r1)
    u1_1, u1_2 = pad(U1, 1, "SIG", 10, 15), pad(U1, 2, "GND", 10, 17.5)
    r1_1, r1_2 = pad(R1, 1, "SIG", 30, 15), pad(R1, 2, "GND", 30, 17.5)
    basic_pads = [u1_1, u1_2, r1_1, r1_2]
    shapes_by_id = {pad_id(p): rect(*(int(p["position"][k]) / 1e6 for k in ("x_nm", "y_nm")), 1.0, 1.0)
                    for p in basic_pads}
    gnd_wall = track("a1000000-0000-0000-0000-000000000001", 20, 9, 20, 25, "GND")
    gnd_via = via("a1000000-0000-0000-0000-000000000002", 15, 21, "GND")
    gnd_class = {"name": "GND", "board": {"clearance": {"value_nm": "250000"}}}

    def problem(pads=None, tracks=(), arcs=(), vias=(), zones=(), extra_shapes=(), shapes=None,
                pad_layers=("F.Cu",), copper=2, rules=None, fps=None, net_classes=None):
        """The requests of route_problem: layers, rules, footprints, shapes, pads (and their shapes per
        layer), tracks, arcs, vias, zones, nets and their classes."""
        pads = basic_pads if pads is None else pads
        all_shapes = dict(shapes_by_id)
        all_shapes.update(shapes or {})
        script = [enabled(copper), design_rules(rules), fps or footprints,
                  get_items(*outline(0, 0, 40, 30), *extra_shapes), get_items(*pads)]
        script += [pad_shapes(all_shapes) for _ in pad_layers]
        script += [get_items(*tracks), get_items(*arcs), get_items(*vias), get_items(*zones),
                   nets_reply("SIG", "GND", "")]
        script.append(classes(**(net_classes if net_classes is not None else {"GND": gnd_class})))
        return script

    def find(*pads_list):
        """find_pad: footprints, then pads."""
        return [footprints, get_items(*(pads_list or basic_pads))]

    route = {"board_document": B, "from_reference": "U1", "from_pad": "1", "to_reference": "R1", "to_pad": "1"}
    no_class = classes()

    result = [
        # ---- Design rules
        S("board_design_rules_read", [("live_pcb_design_rules", {"board_document": B}),
                                      ("live_pcb_design_rules", {"board_document": B, "severities": {}})],
          [design_rules(), design_rules({})]),
        S("board_design_rules_set", [("live_pcb_design_rules", {
            "board_document": B, "min_clearance": 0.15, "min_track_width": "0.1", "hole_to_hole": 0.25,
            "min_microvia_drill": 0.1, "silk_clearance": 0, "track_widths_mm": [0.2, 0.3333333],
            "via_sizes_mm": [[0.6, 0.3], [0.8, "0.4"]],
            "severities": {"silk_clearance": "error", "unconnected_items": "ignore", "clearance": "warning",
                           "CLEARANCE": "error"}})],
          [design_rules(), echo_rules()]),
        S("board_design_rules_partial", [("live_pcb_design_rules", {"board_document": B, "track_widths_mm": []}),
                                         ("live_pcb_design_rules", {"board_document": B, "via_sizes_mm": []})],
          [design_rules(), echo_rules(), design_rules(), echo_rules()]),
        S("board_design_rules_refusals", [("live_pcb_design_rules", dict({"board_document": B}, **args)) for args in (
            {"severities": {"bogus": "error"}}, {"severities": {"clearance": "loud"}},
            {"severities": {"clearance": ["error"]}}, {"severities": {"clearance": None}},
            {"via_sizes_mm": [[0.3, 0.6]]}, {"via_sizes_mm": [[0.6]]}, {"via_sizes_mm": [[0.6, 0.3, 0.1]]},
            {"min_clearance": 20000}, {"track_widths_mm": [0.2, 1e5]})],
          [design_rules()] * 9),
        S("board_design_rules_kicad_errors", [("live_pcb_design_rules", {"board_document": B}),
                                              ("live_pcb_design_rules", {"board_document": B, "min_drill": 0.3})],
          [("GetBoardDesignRules", lambda r: failure(envelope.AS_BAD_REQUEST, "no board")), design_rules(),
           ("SetBoardDesignRules", lambda r: failure(envelope.AS_BUSY, "dialog open"))]),
        S("board_design_rules_validation", [("live_pcb_design_rules", {"board_document": B, "min_clearance": "x"}),
                                            ("live_pcb_design_rules", {"board_document": B, "via_sizes_mm": [1]})],
          [], compare="error_prefix"),
        S("board_design_document_refused", [
            ("live_pcb_design_rules", {"board_document": h.SHEET_DOC}),
            ("live_pcb_route", dict(route, board_document=dict(B, board_filename=""))),
            ("live_pcb_check_path", {"board_document": {"type": "DOCTYPE_PCB"}, "points": [[0, 0], [1, 1]]})], []),
        # ---- Custom rules
        S("board_design_custom_rules_get", [("live_pcb_custom_rules", {"board_document": B}),
                                            ("live_pcb_custom_rules", {"board_document": B})],
          [get_custom(error="line 3: unknown keyword"), get_custom(rules=[], status="CRS_NONE")]),
        S("board_design_custom_rules_add", [("live_pcb_custom_rules", {"board_document": B, "action": "add", "rules": [
            {"name": "RF clearance", "condition": "A.NetClass == 'RF'", "layer": "inner", "severity": "error",
             "comments": "keep the feed clear", "constraints": [{"type": "clearance", "min": 0.5, "max": 2},
                                                                {"type": "track_width", "opt": 0.3}]},
            {"name": "Edge", "layer": "B.Cu", "constraints": [{"type": "disallow", "items": ["tracks", "through_vias"]},
                                                               {"type": "zone_connection", "style": "thermal"},
                                                               {"type": "assertion", "expression": "A.x > 0"},
                                                               {"type": "Disallow", "min": 1},
                                                               {"type": "disallow"}]}]})],
          [get_custom(), set_custom()]),
        S("board_design_custom_rules_set_remove", [
            ("live_pcb_custom_rules", {"board_document": B, "action": "set", "rules": [
                {"name": "Raw", "condition": "A.Type == 'Via'",
                 "constraints": [{"type": "CRCT_HOLE_SIZE", "numeric": {"min": 300000}}], "severity": "RS_ERROR"},
                {"name": "Plain"}]}),
            ("live_pcb_custom_rules", {"board_document": B, "action": "remove", "names": ["Old", "RF clearance"]}),
            ("live_pcb_custom_rules", {"board_document": B, "action": "set"}),
            ("live_pcb_custom_rules", {"board_document": B, "action": "add", "rules": [{"name": "X"}]})],
          [get_custom(), set_custom(), get_custom(), set_custom(), get_custom(), set_custom(),
           get_custom(), set_custom(status="CRS_INVALID", error="unknown constraint at line 2")]),
        S("board_design_custom_rules_refusals", [("live_pcb_custom_rules", dict({"board_document": B}, **args))
                                                 for args in (
            {"action": "replace"}, {"action": "remove", "names": ["Nope", "Old", "Also"]},
            {"action": "add", "rules": [{"condition": "x"}]},
            {"action": "add", "rules": [{"name": 5}]},
            {"action": "add", "rules": [{"name": "x", "condition": ["a"]}]},
            {"action": "add", "rules": [{"name": "x", "comments": 7}]},
            {"action": "add", "rules": [{"name": "x", "constraints": ["clearance"]}]},
            {"action": "add", "rules": [{"name": "x", "constraints": {"type": "clearance"}}]},
            {"action": "add", "rules": [{"name": "x", "constraints": 3}]},
            {"action": "add", "rules": [{"name": "x", "constraints": [{"min": 1}]}]},
            {"action": "add", "rules": [{"name": "x", "constraints": [{"type": "nope"}]}]},
            {"action": "add", "rules": [{"name": "x", "constraints": [{"type": None}]}]},
            {"action": "add", "rules": [{"name": "x", "layer": "Nope.Cu"}]},
            {"action": "add", "rules": [{"name": "x", "layer": 3}]},
            {"action": "add", "rules": [{"name": "x", "layer": ["F.Cu"]}]},
            {"action": "add", "rules": [{"name": "x", "severity": "loud"}]},
            {"action": "add", "rules": [{"name": "x", "severity": 2}]},
            {"action": "add", "rules": [{"name": "x", "constraints": [{"type": "disallow", "items": ["bogus"]}]}]},
            {"action": "add", "rules": [{"name": "x", "constraints": [{"type": "disallow", "items": "tracks"}]}]},
            {"action": "add", "rules": [{"name": "x", "constraints": [{"type": "disallow", "items": [3]}]}]},
            {"action": "add", "rules": [{"name": "x", "constraints": [{"type": "assertion"}]}]},
            {"action": "add", "rules": [{"name": "x", "constraints": [{"type": "assertion", "expression": 1}]}]},
            {"action": "add", "rules": [{"name": "x", "constraints": [{"type": "zone_connection", "style": "x"}]}]},
            {"action": "add", "rules": [{"name": "x", "constraints": [{"type": "zone_connection"}]}]},
            {"action": "add", "rules": [{"name": "x", "constraints": [{"type": "clearance", "min": "1"}]}]},
            {"action": "add", "rules": [{"name": "x", "constraints": [{"type": "clearance", "max": 2500}]}]},
            {"action": "add", "rules": [{"name": "x", "constraints": [{"type": "clearance", "min": 2e4}]}]},
            {"action": "add", "rules": [{"name": "x", "constraints": [{"type": "CRCT_CLEARANCE", "bogus": 1}]}]},
            {"action": "add", "rules": [{"name": "x", "constraints": [5, {"type": "CRCT_CLEARANCE"}]}]})],
          [get_custom()] * 29),
        # ---- Net classes
        S("board_design_netclasses_read", [("live_pcb_netclasses", {"board_document": B}),
                                           ("live_pcb_netclasses", {"board_document": B, "classes": [],
                                                                    "assign": {}, "patterns": {}})],
          [get_netclasses(netclass_default, netclass_power), get_assignments({"/VBUS": ["Power"], "/X": []},
                                                                             {"/USB_D*": "USB90"}),
           get_netclasses(), get_assignments()]),
        S("board_design_netclasses_set", [("live_pcb_netclasses", {"board_document": B, "classes": [
            {"name": "Default", "track_width": 0.2, "clearance": 0.15, "via_diameter": 0.7, "priority": 5},
            {"name": "RF", "track_width": 0.35, "via_drill": 0.25, "diff_pair_width": 0.2, "diff_pair_gap": 0.1,
             "diff_pair_via_gap": 0.3},
            {"name": "Power", "via_diameter": 0.9, "via_drill": 0.45, "clearance": None},
            {"name": None}], "assign": {"/RF_OUT": "RF", "/ANT": "RF"}, "patterns": {"/PWR*": "Power", "/X*": None}})],
          [get_netclasses(netclass_default, netclass_power, dict(netclass_power, priority=1)), ok("SetNetClasses"),
           ok("SetNetClassAssignments"), get_netclasses(netclass_default), get_assignments()]),
        S("board_design_netclasses_paths", [
            ("live_pcb_netclasses", {"board_document": dict(B, project={"name": "p", "path": "C:\\fixture\\p"}),
                                     "assign": {"A": "Default"}}),
            ("live_pcb_netclasses", {"board_document": dict(B, project={"name": "p", "path": "/home/p/"})}),
            ("live_pcb_netclasses", {"board_document": dict(B, project={"name": "p", "path": "\\\\srv\\p/x"})})],
          [ok("SetNetClassAssignments"), get_netclasses(), get_assignments(), get_netclasses(), get_assignments(),
           get_netclasses(), get_assignments()]),
        S("board_design_netclasses_refusals", [("live_pcb_netclasses", dict({"board_document": B}, **args))
                                               for args in (
            {"classes": [{"track_width": 0.2}]}, {"classes": [{"name": 3}]}, {"classes": [{"name": ["x"]}]},
            {"classes": [{"name": "A", "priority": 1.5}]}, {"classes": [{"name": "A", "priority": True}]},
            {"classes": [{"name": "A", "priority": "3"}]}, {"classes": [{"name": "A", "priority": 2 ** 40}]},
            {"classes": [{"name": "A", "via_drill": "x"}]}, {"classes": [{"name": "A", "clearance": 1e5}]},
            {"assign": {"N": 3}}, {"assign": {"N": None}}, {"patterns": {"P": ["x"]}})],
          [get_netclasses()] * 9),
        # ---- Stackup
        S("board_design_stackup", [("live_pcb_stackup", {"board_document": B}),
                                   ("live_pcb_stackup", {"board_document": B})],
          [stackup(stackup_2), stackup(stackup_4)]),
        S("board_design_set_stackup", [
            ("live_pcb_set_stackup", {"board_document": B, "copper_layers": 4, "copper_thickness_mm": {"F.Cu": 0.035,
                                                                                                       "B.Cu": 0},
                                      "dielectrics": [{"type": "prepreg", "thickness": 0.2, "epsilon_r": 4.2},
                                                      {"thickness": 1.0, "material": "FR4"},
                                                      {"type": "prepreg", "thickness": 0.2, "loss_tangent": 0.02}],
                                      "soldermask_color": "#00ff00", "silkscreen_color": "#FFFFFF",
                                      "finish": "ENIG", "impedance_controlled": True}),
            ("live_pcb_set_stackup", {"board_document": B, "copper_thickness_mm": 0.07, "board_thickness_mm": 1.0})],
          [stackup(stackup_2), echo_stackup(stackup_4), stackup(stackup_4),
           stackup(stackup_2), echo_stackup(), stackup(stackup_2), echo_stackup(), stackup(stackup_2)]),
        S("board_design_set_stackup_refusals", [
            ("live_pcb_set_stackup", {"board_document": B, "copper_layers": 3}),
            ("live_pcb_set_stackup", {"board_document": B, "dielectrics": [{}, {}]}),
            ("live_pcb_set_stackup", {"board_document": B, "board_thickness_mm": 0.05}),
            ("live_pcb_set_stackup", {"board_document": B, "copper_layers": 4})],
          [stackup(stackup_2), stackup(stackup_2), stackup(stackup_2), echo_stackup(), stackup(stackup_2),
           stackup(stackup_2), ("UpdateBoardStackup", lambda r: failure(envelope.AS_BAD_REQUEST,
                                                                        "In1.Cu still has items"))]),
        # ---- Impedance and RF nets
        S("board_design_impedance_width", [
            ("live_pcb_impedance_width", {"board_document": B}),
            ("live_pcb_impedance_width", {"board_document": B, "impedance_ohm": 90, "layer": "B.Cu"}),
            ("live_pcb_impedance_width", {"board_document": B, "structure": "cpwg", "gap_mm": 0.2}),
            ("live_pcb_impedance_width", {"board_document": B, "layer": "In1.Cu", "structure": "cpwg",
                                          "gap_mm": 0.15, "reference_layer": "In2.Cu"}),
            ("live_pcb_impedance_width", {"board_document": B, "layer": "F.Cu", "reference_layer": "In2.Cu"})],
          [stackup(stackup_2), stackup(stackup_2), stackup(stackup_2), stackup(stackup_4), stackup(stackup_4)]),
        S("board_design_impedance_refusals", [
            ("live_pcb_impedance_width", {"board_document": B, "layer": "In1.Cu"}),
            ("live_pcb_impedance_width", {"board_document": B, "impedance_ohm": 500}),
            ("live_pcb_impedance_width", {"board_document": B, "impedance_ohm": 1}),
            ("live_pcb_impedance_width", {"board_document": B, "structure": "stripline"}),
            ("live_pcb_impedance_width", {"board_document": B, "structure": "cpwg"}),
            ("live_pcb_impedance_width", {"board_document": B, "structure": "cpwg", "gap_mm": 0}),
            ("live_pcb_impedance_width", {"board_document": B, "reference_layer": "In9.Cu"}),
            ("live_pcb_impedance_width", {"board_document": B, "layer": "B.Cu", "reference_layer": "B.Cu"}),
            ("live_pcb_impedance_width", {"board_document": B, "structure": "cpwg", "layer": "In3.Cu",
                                          "gap_mm": 0.1})],
          [stackup(stackup_2)] * 8),
        S("board_design_rf_net", [
            ("live_pcb_rf_net", {"board_document": B, "nets": ["/RF_OUT", "/ANT", "/RF_OUT"]}),
            ("live_pcb_rf_net", {"board_document": B, "nets": ["/ANT"], "netclass": "RF50", "structure": "cpwg",
                                 "gap_mm": 0.2, "keep_others_away_mm": 0.5}),
            ("live_pcb_rf_net", {"board_document": B, "nets": [], "width_mm": 0.3, "clearance_mm": 0.4})],
          [stackup(stackup_2), get_netclasses(netclass_default), ok("SetNetClasses"), ok("SetNetClassAssignments"),
           get_netclasses(netclass_default, {"name": "RF", "board": {"track_width": {"value_nm": "2870000"}}}),
           get_assignments({"/RF_OUT": ["RF"]}),
           stackup(stackup_2), get_netclasses(), ok("SetNetClasses"), ok("SetNetClassAssignments"),
           get_netclasses(netclass_default), get_assignments(), get_custom(), set_custom(),
           get_netclasses(), ok("SetNetClasses"), get_netclasses(netclass_default), get_assignments()]),
        S("board_design_rf_net_refused", [("live_pcb_rf_net", {"board_document": B, "nets": ["/RF"],
                                                               "layer": "In1.Cu"})], []),
        S("board_design_rf_net_validation", [("live_pcb_rf_net", {"board_document": B}),
                                             ("live_pcb_rf_net", {"board_document": B, "nets": "/RF"})], [],
          compare="error_prefix"),
        # ---- Checks
        S("board_design_check", [("live_pcb_check", {"board_document": B}),
                                 ("live_pcb_check", {"board_document": B, "categories": ["clearance", "schematic",
                                                                                         "clearance"]}),
                                 ("live_pcb_check", {"board_document": B, "categories": [],
                                                     "schematic_parity": True})],
          drc([{"error_type": "DRCET_CLEARANCE", "severity": "RS_ERROR", "description": "Clearance",
                "items": [{"value": U1}]},
               {"error_type": "DRCET_SILK_CLEARANCE", "severity": "RS_WARNING", "description": "Silk"},
               {"error_type": "DRCET_UNCONNECTED_ITEMS", "severity": "RS_ERROR", "description": "Unrouted"},
               {"error_type": "DRCET_MISSING_FOOTPRINT", "severity": "RS_WARNING", "description": "Missing"},
               {"error_type": "DRCET_GENERIC_WARNING", "severity": "RS_WARNING", "description": "Other"}])
          + drc([{"error_type": "DRCET_UNCONNECTED_ITEMS", "severity": "RS_ERROR"}]) + drc()),
        S("board_design_check_many", [("live_pcb_check", {"board_document": B})],
          drc([{"error_type": "DRCET_CLEARANCE", "severity": "RS_ERROR", "description": f"C{i}"}
               for i in range(150)] + [{"error_type": "DRCET_GENERIC_WARNING", "severity": "RS_WARNING",
                                        "description": f"G{i}"} for i in range(60)])),
        S("board_design_check_refusals", [("live_pcb_check", {"board_document": B, "categories": ["copper", "x"]}),
                                          ("live_pcb_check", {"board_document": B})],
          drc(state="TS_FAILED", error="zones not filled")),
        # ---- Keepouts
        S("board_design_keepout_points", [
            ("live_pcb_add_keepout", {"board_document": B, "points": [[0, 0], [5, 0], [5, 5]], "name": "ant"}),
            ("live_pcb_add_keepout", {"board_document": B, "points": [[0, 0], [5, 0], [5, 5]], "layers": ["F.Cu"],
                                      "tracks": False, "pads": True, "footprints": True, "copper": False})],
          [enabled(4)] + commit + commit, generated_ids=True),
        S("board_design_keepout_around", [
            ("live_pcb_add_keepout", {"board_document": B, "around_reference": "J1", "margin_mm": 0.5}),
            ("live_pcb_add_keepout", {"board_document": B, "around_reference": "J1", "layers": [], "vias": False}),
            ("live_pcb_add_keepout", {"board_document": B, "around_reference": "U1", "layers": ["F.Cu"]})],
          [get_items(footprint(J1, "J1", 20, 10, layer="BL_B_Cu", definition_items=[
              segment(18, 8, 22, 8, layer="BL_B_CrtYd", width=0.05),
              segment(18, 8, 21.3, 12.7, layer="BL_B_CrtYd", width=0.05),
              {"@type": ANY + "BoardGraphicShape", "layer": "BL_B_CrtYd",
               "shape": {"circle": {"center": vec(20, 10), "radius_point": vec(21.2, 10.4)},
                         "attributes": {"stroke": {"width": {"value_nm": "50000"}}}}},
              segment(0, 0, 50, 50, layer="BL_F_CrtYd", width=0.05),
              {"@type": ANY + "Zone", "type": "ZT_RULE_AREA"}])),
           enabled(2)] + commit
          + [get_items(footprint(J1, "J1", 20, 10, layer="BL_B_Cu", definition_items=[
              {"@type": ANY + "BoardGraphicShape", "layer": "BL_B_CrtYd",
               "shape": {"rectangle": {"top_left": vec(17.5, 8.25), "bottom_right": vec(22.5, 11.75)},
                         "attributes": {"stroke": {"width": {"value_nm": "50000"}},
                                        "fill": {"fill_type": "GFT_UNFILLED"}}}}]))] + commit
          + [footprints, ("GetBoundingBox", lambda r: reply(parse(editor.GetBoundingBoxResponse, {
              "boxes": [{"position": vec(9.2, 14.1), "size": vec(1.6, 1.85)}]})))] + commit,
          generated_ids=True),
        S("board_design_keepout_refusals", [
            ("live_pcb_add_keepout", {"board_document": B}),
            ("live_pcb_add_keepout", {"board_document": B, "points": [], "around_reference": ""}),
            ("live_pcb_add_keepout", {"board_document": B, "around_reference": "Q1"}),
            ("live_pcb_add_keepout", {"board_document": B, "around_reference": "U1"}),
            ("live_pcb_add_keepout", {"board_document": B, "around_reference": "U1", "layers": ["Nope"]}),
            ("live_pcb_add_keepout", {"board_document": B, "points": [[0, 0], [1]], "layers": ["F.Cu"]})],
          [footprints, footprints, ("GetBoundingBox", lambda r: reply(editor.GetBoundingBoxResponse())),
           footprints, ("GetBoundingBox", lambda r: reply(parse(editor.GetBoundingBoxResponse, {
               "boxes": [{"position": vec(9, 14), "size": vec(2, 2)}]})))], generated_ids=True),
        S("board_design_keepout_commit_lost", [("live_pcb_add_keepout", {"board_document": B, "around_reference": "U1",
                                                                         "layers": ["F.Cu", "B.Cu"]})],
          [footprints, ("GetBoundingBox", lambda r: reply(parse(editor.GetBoundingBoxResponse, {
              "boxes": [{"position": vec(9, 14), "size": vec(2, 2)}]}))),
           BEGIN, all_created(), ("EndCommit", lambda r: h.DROP),
           ("GetOperationStatus", h.status("OPS_COMMITTED", revision_after="fx-6"))], generated_ids=True),
        # ---- Path checks
        S("board_design_check_path", [
            ("live_pcb_check_path", {"board_document": B, "points": [[10, 15], [10, 5], [30, 5], [30, 15]],
                                     "net": "SIG"}),
            ("live_pcb_check_path", {"board_document": B, "points": [[10, 15], [30, 15]], "net": "SIG"}),
            ("live_pcb_check_path", {"board_document": B, "points": [[5, 17.5], [15, 17.5]], "width_mm": 0.3}),
            ("live_pcb_check_path", {"board_document": B, "points": [[1, 1], [45, 1]], "layer": "B.Cu"}),
            ("live_pcb_check_path", {"board_document": B, "points": [], "layer": "In1.Cu"}),
            ("live_pcb_check_path", {"board_document": B, "points": [[12, 20], [18, 20]], "width_mm": 0,
                                     "net": "GND"})],
          problem(tracks=[gnd_wall], vias=[gnd_via]) * 6),
        S("board_design_check_path_keepouts", [
            ("live_pcb_check_path", {"board_document": B, "points": [[12, 2], [28, 2]], "net": "SIG"}),
            ("live_pcb_check_path", {"board_document": B, "points": [[12, 2], [28, 2]], "layer": "B.Cu"})],
          problem(zones=[keepout_zone("c1000000-0000-0000-0000-000000000001", 18, 1, 22, 4)],
                  fps=get_items(fp_u1, fp_r1, footprint(J1, "J1", 35, 25, definition_items=[
                      keepout_zone("c1000000-0000-0000-0000-000000000002", 33, 23, 37, 27, layers=["BL_F_Cu"]),
                      segment(34, 2, 34, 4, layer="BL_F_Cu", width=0.3, net="ANT")]))) * 2),
        S("board_design_check_path_refusals", [
            ("live_pcb_check_path", {"board_document": B, "points": [[1, 1]]}),
            ("live_pcb_check_path", {"board_document": B, "points": [[1], [2]]}),
            ("live_pcb_check_path", {"board_document": B, "points": [[1, 1], [2]]})],
          problem() * 3),
        S("board_design_check_path_validation", [("live_pcb_check_path", {"board_document": B, "points": [1, 2]})],
          [], compare="error_prefix"),
        # ---- Tracks with the check
        S("board_design_add_tracks", [
            ("live_pcb_add_tracks", {"board_document": B, "points": [[10, 15], [10, 5], [30, 5], [30, 15]],
                                     "net": "SIG"}),
            ("live_pcb_add_tracks", {"board_document": B, "points": [[10, 15], [30, 15]], "net": "SIG",
                                     "width_mm": 0.3}),
            ("live_pcb_add_tracks", {"board_document": B, "points": [[10, 15], [30, 15]], "check": False})],
          [classes(SIG={"name": "Default", "board": {"track_width": {"value_nm": "200000"}}})]
          + problem(tracks=[gnd_wall]) + [classes(SIG={"name": "Default", "board": {"track_width": {
              "value_nm": "200000"}}})] + commit
          + problem(tracks=[gnd_wall]) + commit, generated_ids=True),
        S("board_design_add_tracks_refusals", [
            ("live_pcb_add_tracks", {"board_document": B, "points": [[1, 1]]}),
            ("live_pcb_add_tracks", {"board_document": B, "points": [[1, 1], [2, 2]], "layer": "Nope"}),
            ("live_pcb_add_tracks", {"board_document": B, "points": [[1, 1], [2, 2]], "layer": "F.SilkS",
                                     "check": False})],
          problem() + problem(), generated_ids=True),
        # ---- Routes
        S("board_design_route_plain", [("live_pcb_route", dict(route, avoid=False, layers=["B.Cu", "F.Cu"]))],
          [footprints, get_items(*basic_pads), footprints, get_items(*basic_pads), footprints, no_class] + commit,
          generated_ids=True),
        S("board_design_route_around", [("live_pcb_route", route)],
          problem(tracks=[gnd_wall]) + find() + find() + [no_class] + commit + drc(), generated_ids=True),
    ]

    # ---- A busier board: rounded outline with cut-outs, a keepout, a mounting hole, an arc, a blind
    # via, a PTH connector with copper and a rule area in its footprint, net class clearances
    def arc_shape(x1, y1, xm, ym, x2, y2, layer="BL_Edge_Cuts"):
        return {"@type": ANY + "BoardGraphicShape", "layer": layer,
                "shape": {"arc": {"start": vec(x1, y1), "mid": vec(xm, ym), "end": vec(x2, y2)},
                          "attributes": {"stroke": {"width": {"value_nm": "50000"}}}}}

    s2 = 2 * (1 - 0.5 ** 0.5)
    rounded = [segment(2, 0, 38, 0), segment(40, 2, 40, 28), segment(38, 30, 2, 30), segment(0, 28, 0, 2),
               arc_shape(38, 0, 40 - s2, s2, 40, 2), arc_shape(40, 28, 40 - s2, 30 - s2, 38, 30),
               arc_shape(2, 30, s2, 30 - s2, 0, 28), arc_shape(0, 2, s2, s2, 2, 0),
               {"@type": ANY + "BoardGraphicShape", "layer": "BL_Edge_Cuts",
                "shape": {"rectangle": {"top_left": vec(25, 20), "bottom_right": vec(28, 24)},
                          "attributes": {"stroke": {"width": {"value_nm": "50000"}}}}},
               {"@type": ANY + "BoardGraphicShape", "layer": "BL_Edge_Cuts",
                "shape": {"circle": {"center": vec(5, 25), "radius_point": vec(6, 25)},
                          "attributes": {"stroke": {"width": {"value_nm": "50000"}}}}}]
    hole = pad(J1, 9, "", 15, 15, kind="PT_NPTH", layers=(), drill=1.0)
    j1_1 = pad(J1, 1, "", 35, 5, kind="PT_PTH", layers=("BL_F_Cu", "BL_B_Cu"), drill=0.8)
    busy_pads = basic_pads + [hole, j1_1]
    busy_shapes = {pad_id(hole): rect(15, 15, 1.0, 1.0), pad_id(j1_1): rect(35, 5, 1.6, 1.6)}
    fp_j1 = footprint(J1, "J1", 35, 5, definition_items=[
        segment(33, 8, 37, 8, layer="BL_F_Cu", width=0.4, net="ANT"),
        keepout_zone("c1000000-0000-0000-0000-000000000003", 32, 1, 38, 3, layers=["BL_F_Cu"], vias=False)])
    blind = {"@type": ANY + "Via", "id": {"value": "a1000000-0000-0000-0000-000000000009"}, "position": vec(12, 10),
             "net": {"name": "GND"}, "type": "VT_BLIND_BURIED",
             "pad_stack": {"drill": {"start_layer": "BL_F_Cu", "end_layer": "BL_B_Cu"}}}
    gnd_arc = {"@type": ANY + "Arc", "id": {"value": "a1000000-0000-0000-0000-000000000008"}, "layer": "BL_F_Cu",
               "start": vec(24, 10), "mid": vec(26, 12), "end": vec(28, 10), "width": {"value_nm": "250000"},
               "net": {"name": "GND"}}
    busy = dict(pads=busy_pads, shapes=busy_shapes, pad_layers=("F.Cu", "B.Cu"), extra_shapes=[],
                fps=get_items(fp_u1, fp_r1, fp_j1), tracks=[], arcs=[gnd_arc], vias=[blind],
                zones=[keepout_zone("c1000000-0000-0000-0000-000000000004", 18, 13, 22, 17)],
                net_classes={"GND": gnd_class, "SIG": {"name": "Signal", "board": {"clearance": {"value_nm": "150000"}}}})

    def busy_problem():
        script = problem(**busy)
        # the outline: the rounded rectangle and its cut-outs instead of the plain one
        script[3] = get_items(*rounded)
        return script

    def find_busy():
        return [get_items(fp_u1, fp_r1, fp_j1), get_items(*busy_pads)]

    class_sizes = classes(SIG={"name": "Signal", "board": {"track_width": {"value_nm": "300000"}, "via_stack": {
        "copper_layers": [{"size": {"x_nm": "700000"}}], "drill": {"diameter": {"x_nm": "350000"}}}}})
    record = []
    pth_end = pad(R1, 1, "SIG", 30, 15, kind="PT_PTH", layers=("BL_F_Cu", "BL_B_Cu"), drill=0.6)
    wall = track("a1000000-0000-0000-0000-000000000005", 20, -1, 20, 31, "GND")
    j1_sig = pad(J1, 3, "SIG", 20, 5)
    sig3_pads = basic_pads + [j1_sig]
    sig3_shapes = {pad_id(j1_sig): rect(20, 5, 1.0, 1.0)}

    def connected(*ids):
        return ("GetConnectedItems", lambda r: reply(parse(editor.GetItemsResponse, {
            "status": "IRS_OK", "items": [{"@type": ANY + "Pad", "id": {"value": i}} for i in ids]
            + [{"@type": ANY + "Track", "id": {"value": "a1000000-0000-0000-0000-00000000000a"}}]})))

    renamed = get_items(footprint(U1, "U9", 10, 15), fp_r1, footprint(J1, "J1", 20, 5))

    result += [
        S("board_design_route_detour", [("live_pcb_route", dict(route, vias=False)),
                                        ("live_pcb_route", dict(route, vias=False, width_mm=0.4, grid_mm=0.2,
                                                                verify=False))],
          problem(tracks=[gnd_wall]) + find() + find() + [no_class] + commit + drc()
          + problem(tracks=[gnd_wall]) + find() + find() + [no_class] + commit, generated_ids=True),
        S("board_design_route_one_via", [("live_pcb_route", route)],
          problem(pads=[u1_1, pth_end], tracks=[wall], pad_layers=("F.Cu", "B.Cu"))
          + find(u1_1, pth_end) + find(u1_1, pth_end) + [class_sizes] + commit + drc(), generated_ids=True),
        S("board_design_route_busy", [("live_pcb_route", dict(route, vias=False)),
                                      ("live_pcb_route", dict(route, to_reference="J1", to_pad="1", verify=False))],
          busy_problem() + find_busy() + find_busy() + [no_class] + commit + drc()
          + busy_problem() + find_busy() + find_busy() + [no_class] + commit, generated_ids=True),
        S("board_design_route_refusals", [
            ("live_pcb_route", dict(route, to_pad="2")),
            ("live_pcb_route", dict(route, layers=["X9.Cu"])),
            ("live_pcb_route", dict(route, layers=["B.Cu"], vias=False)),
            ("live_pcb_route", dict(route, to_reference="Q1"))],
          problem() + find() + find(u1_1, u1_2, r1_1, r1_2)
          + problem() + find() + find() + [no_class]
          + problem() + find() + find() + [no_class]
          + problem() + find() + [footprints], generated_ids=True),
        S("board_design_route_boxed_in", [("live_pcb_route", dict(route, vias=False, grid_mm=0.25))],
          problem(extra_shapes=[{"@type": ANY + "BoardGraphicShape", "layer": "BL_F_Cu", "net": {"name": "GND"},
                                 "shape": {"rectangle": {"top_left": vec(8, 13), "bottom_right": vec(12, 17)},
                                           "attributes": {"stroke": {"width": {"value_nm": "300000"}},
                                                          "fill": {"fill_type": "GFT_UNFILLED"}}}}])
          + find() + find() + [no_class], generated_ids=True),
        S("board_design_route_drc_rejected", [("live_pcb_route", route)],
          problem(tracks=[gnd_wall]) + find() + find() + [no_class, BEGIN, all_created(record), END_OK]
          + drc([{"error_type": "DRCET_CLEARANCE", "severity": "RS_ERROR", "description": "Clearance (0.1 mm)",
                  "items": "NEW"},
                 {"error_type": "DRCET_UNCONNECTED_ITEMS", "severity": "RS_ERROR", "description": "Unrouted",
                  "items": "NEW"},
                 {"error_type": "DRCET_TRACK_WIDTH", "severity": "RS_ERROR", "description": "Width",
                  "items": [{"value": U1}]}], record=record)
          + [BEGIN, deleted(), END_OK], generated_ids=True),
        S("board_design_route_drc_failed", [("live_pcb_route", route)],
          problem(tracks=[gnd_wall]) + find() + find() + [no_class] + commit
          + drc(state="TS_FAILED", error="zone fill failed"), generated_ids=True),
        S("board_design_route_cleanup_failed", [("live_pcb_route", route)],
          problem(tracks=[gnd_wall]) + find() + find() + [no_class, BEGIN, all_created(record), END_OK]
          + drc([{"error_type": "DRCET_SHORTING_ITEMS", "severity": "RS_ERROR", "description": "Short",
                  "items": "NEW"}], record=record)
          + [("BeginCommit", lambda r: failure(envelope.AS_BUSY, "a tool is active"))], generated_ids=True),
        S("board_design_route_commit_lost", [("live_pcb_route", dict(route, verify=False))],
          problem(tracks=[gnd_wall]) + find() + find() + [no_class, BEGIN, all_created(),
                                                          ("EndCommit", lambda r: h.DROP),
                                                          ("GetOperationStatus",
                                                           h.status("OPS_COMMITTED", revision_after="fx-7"))],
          generated_ids=True),
        S("board_design_route_rejected_by_kicad", [("live_pcb_route", route)],
          problem() + find() + find() + [no_class, BEGIN, h.created(editor.ISC_INVALID_DATA, message="bad track"),
                                         DROP_OK], generated_ids=True),
        S("board_design_route_net", [("live_pcb_route_net", {"board_document": B, "net": "SIG"}),
                                     ("live_pcb_route_net", {"board_document": B, "net": "GND", "vias": False,
                                                             "layers": ["F.Cu"], "verify": False})],
          problem(pads=sig3_pads, shapes=sig3_shapes)
          + [connected(pad_id(u1_1)), connected(pad_id(r1_1)), connected(), no_class,
             get_items(fp_u1, fp_r1, footprint(J1, "J1", 20, 5))] + commit + drc()
          + problem(pads=sig3_pads, shapes=sig3_shapes, fps=renamed)
          + [connected(pad_id(u1_2)), connected(), no_class] + commit, generated_ids=True),
        S("board_design_route_net_connected", [("live_pcb_route_net", {"board_document": B, "net": "SIG"}),
                                               ("live_pcb_route_net", {"board_document": B, "net": "VCC"}),
                                               ("live_pcb_route_net", {"board_document": B, "net": ""})],
          problem() + [connected(pad_id(r1_1))] + problem()
          + problem(pads=[u1_1, pad("ffff", 1, "", 1, 1)], shapes={pad_id(pad("ffff", 1, "", 1, 1)): rect(1, 1, .5, .5)}),
          generated_ids=True),
    ]

    # ---- Tight: a W-shaped GND pad (not convex) whose tooth tip is 0.3734 mm from the grid point
    # (20, 15) on the straight route.  The clearance is 0.375 mm: GEOS's 6-segment fillet round the
    # tip leaves that point outside (its chord is 3 um inside the circle), the Rust router (which
    # uses the true distance for shapes that are not convex) blocks it and steps round one cell.
    # Compared by behaviour (route_behaviour); the other route scenarios come out identical.
    Q1 = "f1000000-0000-0000-0000-000000000004"
    tooth = [(19.086858, 13.43657), (20.0, 14.6266), (21.19003, 13.713458), (21.99003, 14.0266),
             (22.79003, 13.713458), (22.79003, 12.43657), (19.086858, 12.43657)]
    q1 = pad(Q1, 1, "GND", 21, 13.5)
    tight_pads = basic_pads + [q1]
    tight_fps = get_items(fp_u1, fp_r1, footprint(Q1, "Q1", 21, 13.5))
    tight_shape = {pad_id(q1): {"outline": {"closed": True, "nodes": [{"point": vec(x, y)} for x, y in tooth]}}}
    result += [
        S("board_design_route_tight", [("live_pcb_route", dict(route, vias=False)),
                                       ("live_pcb_route", dict(route, vias=False, grid_mm=0.05, verify=False))],
          problem(pads=tight_pads, shapes=tight_shape, fps=tight_fps)
          + [tight_fps, get_items(*tight_pads), tight_fps, get_items(*tight_pads), no_class] + commit + drc()
          + problem(pads=tight_pads, shapes=tight_shape, fps=tight_fps)
          + [tight_fps, get_items(*tight_pads), tight_fps, get_items(*tight_pads), no_class] + commit,
          generated_ids=True, compare=route_behaviour()),
    ]

    # ---- Gaps at a round join: a pad corner just off the corner of a two-segment path, between the
    # join's arc vertices, the gap stepping across the rounding of gap_mm (geo places the join's arc
    # vertices as GEOS does, so these come out identical)
    import math as _math

    def join_pad(step):
        phi = _math.radians(47.8125)
        d = 0.125 + 0.1 + step * 0.000023
        cx, cy = 15 + d * _math.cos(phi), 10 + d * _math.sin(phi)
        corner_pad = pad("f2000000-0000-0000-0000-000000000001", 7, "GND", cx + 0.25, cy + 0.25)
        return corner_pad, {pad_id(corner_pad): {"outline": {"closed": True, "nodes": [
            {"point": vec(cx, cy)}, {"point": vec(cx + 0.5, cy)}, {"point": vec(cx + 0.5, cy + 0.5)},
            {"point": vec(cx, cy + 0.5)}]}}}

    gap_steps, gap_script = [], []
    for step in range(10):
        corner_pad, corner_shape = join_pad(step)
        gap_steps.append(("live_pcb_check_path", {"board_document": B, "points": [[10, 10], [15, 10], [15, 5]],
                                                  "net": "SIG"}))
        gap_script += problem(pads=basic_pads + [corner_pad], shapes=corner_shape)
    result += [S("board_design_check_path_join", gap_steps, gap_script)]

    # ---- Reading the board: KiCad errors, a lost read, values the protocol does not know
    def with_reply(script, index, respond):
        script = list(script)
        script[index] = (script[index][0], respond)
        return script[:index + 1]

    base = problem()
    odd_pad = dict(pad(R1, 5, "SIG", 30, 20), type=99)
    odd_shape = segment(1, 1, 2, 2, layer=999)
    four = problem(pads=[pad(U1, 1, "SIG", 10, 15, kind="PT_PTH", drill=0.6),
                         pad(R1, 1, "SIG", 30, 15, kind="PT_PTH", drill=0.6)],
                   pad_layers=("F.Cu", "In1.Cu", "In2.Cu", "B.Cu"), copper=4,
                   tracks=[track("a1000000-0000-0000-0000-00000000000b", 20, 8, 20, 22, "GND", layer="BL_In1_Cu")],
                   vias=[{"@type": ANY + "Via", "id": {"value": "a1000000-0000-0000-0000-00000000000c"},
                          "position": vec(20, 15), "net": {"name": "GND"}, "type": "VT_BLIND_BURIED",
                          "pad_stack": {"copper_layers": [{"size": {"x_nm": "800000"}}],
                                        "drill": {"start_layer": "BL_In2_Cu", "end_layer": "BL_In1_Cu"}}}])
    four_pads = [pad(U1, 1, "SIG", 10, 15, kind="PT_PTH", drill=0.6), pad(R1, 1, "SIG", 30, 15, kind="PT_PTH",
                                                                           drill=0.6)]
    result += [
        S("board_design_problem_errors", [("live_pcb_route", route)] * 4 + [("live_pcb_check_path", {
            "board_document": B, "points": [[1, 1], [2, 2]]})] * 2,
          with_reply(base, 0, lambda r: failure(envelope.AS_BUSY, "a dialog is open"))
          + with_reply(base, 1, lambda r: failure(envelope.AS_BAD_REQUEST, "no rules"))
          + with_reply(base, 2, items(status="IRS_DOCUMENT_NOT_FOUND"))
          + with_reply(base, 5, lambda r: h.DROP)
          + problem(pads=basic_pads + [odd_pad])[:5]
          + problem(extra_shapes=[odd_shape])[:4]),
        S("board_design_route_pad_unknown", [("live_pcb_route", route)],
          problem(pads=[u1_1, u1_2, r1_2]) + find() + find(), generated_ids=True),
        S("board_design_route_four_layers", [("live_pcb_route", dict(route, layers=["In1.Cu", "In2.Cu"]))],
          four + find(*four_pads) + find(*four_pads) + [no_class] + commit + drc(), generated_ids=True),
        S("board_design_add_tracks_commit_lost", [("live_pcb_add_tracks", {
            "board_document": B, "points": [[10, 15], [10, 5]], "net": "SIG", "width_mm": 0.3})],
          problem() + [BEGIN, all_created(), ("EndCommit", lambda r: h.DROP),
                       ("GetOperationStatus", h.status("OPS_ROLLED_BACK", message="lease expired"))],
          generated_ids=True),
    ]
    _self_test()
    return result
