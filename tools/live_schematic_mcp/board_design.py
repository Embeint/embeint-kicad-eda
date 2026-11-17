# SPDX-License-Identifier: GPL-3.0-or-later
"""Board design rules, net classes, stackup, RF widths, checks and obstacle-avoiding routing.

Mixed into LiveSchematic next to BoardTools.  Distances are millimetres; KiCad stores nanometres.
"""

import math
from pathlib import Path
import sys

from google.protobuf import json_format
from shapely.geometry import LineString, Point, Polygon
from shapely.ops import unary_union

sys.path.insert(0, str(Path(__file__).resolve().parent / "_generated"))
from board import board_commands_pb2 as bc            # noqa: E402
from board import board_pb2 as board_pb               # noqa: E402
from board import board_rules_pb2 as rules_pb         # noqa: E402
from board import board_types_pb2 as bt               # noqa: E402
from common.commands import editor_commands_pb2 as editor  # noqa: E402
from common.commands import project_commands_pb2 as project_cmds  # noqa: E402
from common.types import base_types_pb2 as types      # noqa: E402
from common.types import enums_pb2 as enums           # noqa: E402
from common.types import project_settings_pb2 as ps   # noqa: E402

import board_rf                                       # noqa: E402
import board_router as router                         # noqa: E402
from board_client import _dict, _mm, _nm, _vec, is_copper, layer, layer_name, make_track, make_via, make_zone  # noqa: E402,E501

# DRC violation types by what they are about (names as board_drc reports them)
CHECK_CATEGORIES = {
    "clearance": {"clearance", "shorting_items", "tracks_crossing", "hole_clearance", "edge_clearance", "creepage",
                  "copper_sliver", "zones_intersect", "drilled_holes_too_close", "drilled_holes_colocated"},
    "keepout": {"allowed_items"},
    "silkscreen": {"silk_clearance", "silk_mask_clearance", "silk_edge_clearance", "text_height", "text_thickness",
                   "line_width", "mirrored_text_on_front_layer", "nonmirrored_text_on_back_layer",
                   "text_on_edgecuts"},
    "courtyard": {"overlapping_footprints", "missing_courtyard", "malformed_courtyard", "pth_in_courtyard",
                  "npth_in_courtyard"},
    "connectivity": {"unconnected_items", "dangling_track", "dangling_via", "isolated_copper", "starved_thermal",
                     "net_conflict"},
    "manufacturing": {"track_width", "connection_width", "annular_width", "drill_out_of_range", "via_diameter",
                      "padstack", "padstack_invalid", "microvia_drill_out_of_range", "soldermask_bridge",
                      "track_angle", "track_segment_length", "track_on_post_machined_layer",
                      "track_not_centered_on_via", "invalid_outline", "disabled_layer_item"},
    "schematic": {"missing_footprint", "extra_footprint", "duplicate_footprint", "schematic_parity",
                  "schematic_fields_parity", "footprint_filters", "lib_footprint_issues", "lib_footprint_mismatch",
                  "footprint_type_mismatch", "footprint"},
}
ROUTE_VIOLATIONS = CHECK_CATEGORIES["clearance"] | CHECK_CATEGORIES["keepout"]

RULE_FIELDS = {  # MinimumConstraints field -> tool argument (mm)
    "min_clearance": "min_clearance", "min_track_width": "min_track_width",
    "min_connection_width": "min_connection_width", "min_via_annular_width": "min_annular_ring",
    "min_via_size": "min_via_diameter", "min_through_drill": "min_drill", "min_microvia_size": "min_microvia_diameter",
    "min_microvia_drill": "min_microvia_drill", "copper_edge_clearance": "copper_edge_clearance",
    "hole_clearance": "hole_clearance", "hole_to_hole_min": "hole_to_hole", "silk_clearance": "silk_clearance",
    "min_silk_text_height": "min_silk_text_height", "min_silk_text_thickness": "min_silk_text_thickness",
    "min_groove_width": "min_groove_width",
}
SEVERITIES = {"error": types.RS_ERROR, "warning": types.RS_WARNING, "ignore": types.RS_IGNORE}


def _poly_line(line):
    points = []
    for node in line.nodes:
        if node.HasField("arc"):
            pts = router.arc_points(tuple(_mm(node.arc.start)), tuple(_mm(node.arc.mid)), tuple(_mm(node.arc.end)), 12)
            points.extend(pts if not points else pts[1:])
        else:
            points.append(tuple(_mm(node.point)))
    return points


def polygon_from_proto(polygon):
    """shapely Polygon from a kiapi PolygonWithHoles."""
    outline = _poly_line(polygon.outline)
    if len(outline) < 3:
        return None
    holes = [h for h in (_poly_line(hole) for hole in polygon.holes) if len(h) >= 3]
    shape = Polygon(outline, holes)
    return shape if shape.is_valid else shape.buffer(0)


def polyset_geometry(polyset):
    shapes = [s for s in (polygon_from_proto(p) for p in polyset.polygons) if s is not None and not s.is_empty]
    return unary_union(shapes) if shapes else None


def shape_geometry(shape, as_outline=False):
    """Copper area of a BoardGraphicShape (a stroke, or a filled area), or None."""
    g = shape.shape
    width = max(g.attributes.stroke.width.value_nm / 1e6, 0.0)
    filled = g.attributes.fill.fill_type in (types.GFT_FILLED,) and not as_outline
    if g.HasField("segment"):
        line = LineString([_mm(g.segment.start), _mm(g.segment.end)])
        return line.buffer(width / 2) if width else line
    if g.HasField("arc"):
        line = LineString(router.arc_points(tuple(_mm(g.arc.start)), tuple(_mm(g.arc.mid)), tuple(_mm(g.arc.end))))
        return line.buffer(width / 2) if width else line
    if g.HasField("rectangle"):
        (x1, y1), (x2, y2) = _mm(g.rectangle.top_left), _mm(g.rectangle.bottom_right)
        area = Polygon([(x1, y1), (x2, y1), (x2, y2), (x1, y2)])
        return area.buffer(width / 2) if filled else area.exterior.buffer(max(width / 2, 1e-4))
    if g.HasField("circle"):
        centre = _mm(g.circle.center)
        radius = math.dist(centre, _mm(g.circle.radius_point))
        disc = Point(centre).buffer(radius)
        return disc.buffer(width / 2) if filled else disc.exterior.buffer(max(width / 2, 1e-4))
    if g.HasField("polygon"):
        area = polyset_geometry(g.polygon)
        if area is None:
            return None
        return area.buffer(width / 2) if filled else area.boundary.buffer(max(width / 2, 1e-4))
    return None


def edge_lines(shape):
    """Edge.Cuts line work of a shape, for the board outline."""
    g = shape.shape
    if g.HasField("segment"):
        return [LineString([_mm(g.segment.start), _mm(g.segment.end)])]
    if g.HasField("arc"):
        return [LineString(router.arc_points(tuple(_mm(g.arc.start)), tuple(_mm(g.arc.mid)), tuple(_mm(g.arc.end))))]
    if g.HasField("rectangle"):
        (x1, y1), (x2, y2) = _mm(g.rectangle.top_left), _mm(g.rectangle.bottom_right)
        return [LineString([(x1, y1), (x2, y1), (x2, y2), (x1, y2), (x1, y1)])]
    if g.HasField("circle"):
        centre = _mm(g.circle.center)
        return [Point(centre).buffer(math.dist(centre, _mm(g.circle.radius_point)), quad_segs=32).exterior]
    if g.HasField("polygon"):
        area = polyset_geometry(g.polygon)
        if area is None:
            return []
        parts = getattr(area, "geoms", [area])
        return [LineString(p.exterior.coords) for p in parts]
    return []


def copper_layer_names(count):
    return ["F.Cu"] + [f"In{i}.Cu" for i in range(1, count - 1)] + ["B.Cu"]


class BoardDesignTools:
    """Rules, net classes, stackup, RF, checks and routing; LiveSchematic provides send/transaction."""

    # ---- Reading the board for the router ------------------------------------------------------

    def board_copper_layers(self, doc):
        count = self.send(bc.GetBoardEnabledLayers(board=doc), bc.BoardEnabledLayersResponse).copper_layer_count
        return copper_layer_names(count or 2)

    def route_problem(self, doc):
        """Everything the router avoids, read from the live board."""
        copper = self.board_copper_layers(doc)
        rules = self.send(bc.GetBoardDesignRules(board=doc), bc.BoardDesignRulesResponse).rules
        problem = router.RouteProblem(
            layers=copper,
            min_clearance=rules.constraints.min_clearance.value_nm / 1e6 or 0.2,
            edge_clearance=rules.constraints.copper_edge_clearance.value_nm / 1e6 or 0.3)
        footprints = self.board_unpacked(doc, "footprints")

        def add(layer_value_name, geometry, net, kind):
            if geometry is not None and not geometry.is_empty and layer_value_name in copper:
                problem.obstacles.setdefault(layer_value_name, []).append(router.Obstacle(geometry, net, kind))

        # Board outline
        lines = []
        for shape in self.board_unpacked(doc, "shapes"):
            if shape.layer == bt.BL_Edge_Cuts:
                lines += edge_lines(shape)
            elif is_copper(shape.layer):
                add(layer_name(shape.layer), shape_geometry(shape), shape.net.name, "copper")
        problem.outline = router.outline_from_lines(lines)

        # Pads (exact shapes per layer)
        pads = self.board_unpacked(doc, "pads")
        by_layer = {}
        for pad in pads:
            present = [layer_name(v) for v in pad.pad_stack.layers if is_copper(v)]
            if pad.type in (bt.PT_PTH, bt.PT_NPTH) or bt.BL_UNDEFINED in pad.pad_stack.layers:
                present = list(copper) if pad.type == bt.PT_PTH else present
            for name in present:
                if name in copper:
                    by_layer.setdefault(name, []).append(pad)
            problem.pads[pad.id.value] = {"net": pad.net.name, "position": tuple(_mm(pad.position)), "layers": {},
                                          "number": pad.number, "parent": pad.parent.value,
                                          "type": bt.PadType.Name(pad.type)}
        for name, layer_pads in by_layer.items():
            request = bc.GetPadShapeAsPolygon(board=doc, layer=layer(name))
            request.pads.extend(types.KIID(value=p.id.value) for p in layer_pads)
            response = self.send(request, bc.PadShapeAsPolygonResponse)
            for pid, polygon in zip(response.pads, response.polygons):
                geometry = polygon_from_proto(polygon)
                if geometry is None:
                    continue
                info = problem.pads.get(pid.value)
                if info is None:
                    continue
                info["layers"][name] = geometry
                problem.obstacles.setdefault(name, []).append(router.Obstacle(geometry, info["net"], "pad",
                                                                              pid.value))
            # NPTH holes have no copper but still need clearance: use the drill
        for pad in pads:
            if pad.type == bt.PT_NPTH and pad.pad_stack.drill.diameter.x_nm:
                hole = Point(_mm(pad.position)).buffer(pad.pad_stack.drill.diameter.x_nm / 2e6)
                for name in copper:
                    add(name, hole, "", "hole")

        # Tracks, arcs and vias
        for track in self.board_unpacked(doc, "tracks"):
            line = LineString([_mm(track.start), _mm(track.end)])
            add(layer_name(track.layer), line.buffer(track.width.value_nm / 2e6), track.net.name, "track")
        for arc in self.board_unpacked(doc, "arcs"):
            line = LineString(router.arc_points(tuple(_mm(arc.start)), tuple(_mm(arc.mid)), tuple(_mm(arc.end))))
            add(layer_name(arc.layer), line.buffer(arc.width.value_nm / 2e6), arc.net.name, "track")
        for via in self.board_unpacked(doc, "vias"):
            stack = via.pad_stack
            diameter = (stack.copper_layers[0].size.x_nm if stack.copper_layers else 600000) / 1e6
            span = copper
            if via.type != bt.VT_THROUGH and stack.drill.start_layer and stack.drill.end_layer:
                ends = sorted(copper.index(layer_name(v)) for v in (stack.drill.start_layer, stack.drill.end_layer)
                              if layer_name(v) in copper)
                if len(ends) == 2:
                    span = copper[ends[0]:ends[1] + 1]
            disc = Point(_mm(via.position)).buffer(diameter / 2)
            for name in span:
                add(name, disc, via.net.name, "via")

        # Keepouts (board level and inside footprints, e.g. antenna modules) and footprint copper shapes
        zones = list(self.board_unpacked(doc, "zones"))
        for fp in footprints:
            for packed in fp.definition.items:
                zone, shape = bt.Zone(), bt.BoardGraphicShape()
                if packed.Is(bt.Zone.DESCRIPTOR) and packed.Unpack(zone):
                    zones.append(zone)
                elif packed.Is(bt.BoardGraphicShape.DESCRIPTOR) and packed.Unpack(shape) and is_copper(shape.layer):
                    add(layer_name(shape.layer), shape_geometry(shape), shape.net.name, "copper")
        for zone in zones:
            if zone.type != bt.ZT_RULE_AREA:
                continue
            area = polyset_geometry(zone.outline)
            if area is None:
                continue
            settings = zone.rule_area_settings
            names = [layer_name(v) for v in zone.layers if is_copper(v)] or list(copper)
            if settings.keepout_tracks:
                for name in names:
                    problem.track_keepouts.setdefault(name, []).append(area)
            if settings.keepout_vias:
                problem.via_keepouts.append(area)

        # Net class clearances
        nets = self.send(bc.GetNets(board=doc), bc.NetsResponse).nets
        if nets:
            request = bc.GetNetClassForNets()
            request.net.extend(nets)
            classes = self.send(request, bc.NetClassForNetsResponse).classes
            for name, netclass in classes.items():
                if netclass.HasField("board") and netclass.board.HasField("clearance"):
                    problem.net_clearance[name] = netclass.board.clearance.value_nm / 1e6
        return problem

    # ---- Routing -------------------------------------------------------------------------------

    def _pad_layers(self, problem, pad_id):
        return list(problem.pads[pad_id]["layers"]) or list(problem.layers[:1])

    def _find_pad_id(self, problem, doc, reference, number):
        pad = self.find_pad(doc, reference, number)
        return pad.id.value

    def _route_items(self, problem, net, start_id, end_id, width, layers, via, grid):
        start, end = problem.pads[start_id]["position"], problem.pads[end_id]["position"]
        own = {}
        for pid in (start_id, end_id):
            for name, geometry in problem.pads[pid]["layers"].items():
                own.setdefault(name, []).append(geometry)
        start_layers, end_layers = self._pad_layers(problem, start_id), self._pad_layers(problem, end_id)
        if layers:
            start_layers = [n for n in start_layers if n in layers] or start_layers
            end_layers = [n for n in end_layers if n in layers] or end_layers
        try:
            found = router.route(problem, net, start, end, start_layers, end_layers, width, layers, via, grid,
                                 own_pads=own, own_keys={start_id, end_id})
        except router.NoRoute:
            if grid <= 0.03:
                raise
            found = router.route(problem, net, start, end, start_layers, end_layers, width, layers, via, grid / 2,
                                 own_pads=own, own_keys={start_id, end_id})
        items = []
        for name, points in found.segments:
            items += [make_track(a, b, width, layer(name), net) for a, b in zip(points, points[1:])]
        for position in found.vias:
            items.append(make_via(position, net, via[0], via[1]))
        return found, items

    def _net_sizes(self, doc, net, width, via_diameter, via_drill):
        sizes = (self.netclass_widths(doc, net) or {}) if net else {}
        width = width or sizes.get("track_width") or 0.25
        via = (via_diameter or sizes.get("via_diameter") or 0.6, via_drill or sizes.get("via_drill") or 0.3)
        return width, via

    def _verify(self, doc, new_ids):
        """KiCad's DRC on the result: clearance, short, crossing and keepout violations of the new items."""
        report = self.board_drc(doc, parity=False, refill=False)
        new_ids = set(new_ids)
        bad = [v for v in report["violations"] if v["type"] in ROUTE_VIOLATIONS and new_ids & set(v["items"])]
        return bad

    def board_route_avoiding(self, doc, from_ref, from_pad, to_ref, to_pad, layers=None, width=None, vias=True,
                             via_diameter=None, via_drill=None, grid=0.1, verify=True):
        """Route between two pads around other nets' copper, keepouts and the board edge, as one undo step;
        with verify, KiCad's DRC checks the new copper and the route is undone if it breaks a rule."""
        problem = self.route_problem(doc)
        start_id = self._find_pad_id(problem, doc, from_ref, from_pad)
        end_id = self._find_pad_id(problem, doc, to_ref, to_pad)
        a, b = problem.pads[start_id]["net"], problem.pads[end_id]["net"]
        if a and b and a != b:
            raise ValueError(f"{from_ref}.{from_pad} is on {a} but {to_ref}.{to_pad} is on {b}")
        net = a or b
        width, via = self._net_sizes(doc, net, width, via_diameter, via_drill)
        allowed = list(layers) if layers else (list(problem.layers) if vias else None)
        found, items = self._route_items(problem, net, start_id, end_id, width, allowed, via if vias else None, grid)
        self.mutate(doc, items, description=f"MCP: route {from_ref}.{from_pad} to {to_ref}.{to_pad}")
        result = {"net": net, "width_mm": width, "length_mm": found.length, "vias": [list(v) for v in found.vias],
                  "segments": [{"layer": n, "path_mm": [list(p) for p in pts]} for n, pts in found.segments],
                  "items": [i.id.value for i in items], "geometry_checked": found.checked}
        if verify:
            bad = self._verify(doc, result["items"])
            if bad:
                self.undo(doc)
                raise RuntimeError(f"route undone: KiCad's DRC found {len(bad)} problem(s): "
                                   + "; ".join(v["description"] for v in bad[:5]))
            result["drc_clean"] = True
        return result

    def board_route_net(self, doc, net, layers=None, width=None, vias=True, via_diameter=None, via_drill=None,
                        grid=0.1, verify=True):
        """Connect every pad of a net that is not yet connected (shortest joins first), as one undo step."""
        problem = self.route_problem(doc)
        net_pads = [pid for pid, info in problem.pads.items() if info["net"] == net]
        if len(net_pads) < 2:
            raise ValueError(f"net {net!r} has {len(net_pads)} pad(s) on the board")
        clusters, seen = [], set()
        for pid in net_pads:
            if pid in seen:
                continue
            request = bc.GetConnectedItems(header=types.ItemHeader(document=doc), types=[enums.KOT_PCB_PAD])
            request.items.add(value=pid)
            connected = {pid}
            for packed in self.send(request, editor.GetItemsResponse).items:
                pad = bt.Pad()
                if packed.Unpack(pad) and pad.id.value in problem.pads:
                    connected.add(pad.id.value)
            connected &= set(net_pads)
            seen |= connected
            clusters.append([(*problem.pads[p]["position"], p) for p in sorted(connected)])
        if len(clusters) < 2:
            return {"net": net, "already_connected": True, "items": []}
        width, via = self._net_sizes(doc, net, width, via_diameter, via_drill)
        allowed = list(layers) if layers else (list(problem.layers) if vias else None)
        legs, items = [], []
        for p, q in router.spanning_order(clusters):
            found, leg_items = self._route_items(problem, net, p[2], q[2], width, allowed, via if vias else None,
                                                 grid)
            items += leg_items
            label = [f"{self._pad_label(doc, problem, x[2])}" for x in (p, q)]
            legs.append({"from": label[0], "to": label[1], "length_mm": found.length, "vias": len(found.vias)})
        if len(items) > 200:
            raise ValueError("this net needs more than 200 items; route it in parts")
        self.mutate(doc, items, description=f"MCP: route net {net}")
        result = {"net": net, "width_mm": width, "legs": legs, "items": [i.id.value for i in items]}
        if verify:
            bad = self._verify(doc, result["items"])
            if bad:
                self.undo(doc)
                raise RuntimeError(f"net route undone: KiCad's DRC found {len(bad)} problem(s): "
                                   + "; ".join(v["description"] for v in bad[:5]))
            result["drc_clean"] = True
        return result

    def _pad_label(self, doc, problem, pad_id):
        info = problem.pads[pad_id]
        if not hasattr(self, "_ref_cache"):
            self._ref_cache = {}
        if info["parent"] not in self._ref_cache:
            self._ref_cache = {fp.id.value: fp.reference_field.text.text.text
                               for fp in self.board_unpacked(doc, "footprints")}
        return f"{self._ref_cache.get(info['parent'], '?')}.{info['number']}"

    def board_clear_path(self, doc, net, points, layer_value_name="F.Cu", width=0.25):
        """Does a proposed track (points in mm) keep clear of other nets, keepouts and the edge?"""
        problem = self.route_problem(doc)
        line = LineString(points).buffer(width / 2)
        hits = []
        for obstacle in problem.obstacles.get(layer_value_name, []):
            if obstacle.net and obstacle.net == net:
                continue
            gap = line.distance(obstacle.geometry)
            need = problem.clearance(net, obstacle.net)
            if gap < need - 1e-6:
                hits.append({"kind": obstacle.kind, "net": obstacle.net, "gap_mm": round(gap, 4),
                             "needed_mm": need})
        for keepout in problem.track_keepouts.get(layer_value_name, []):
            if line.intersects(keepout):
                hits.append({"kind": "keepout"})
        if problem.outline is not None and line.distance(problem.outline.exterior) < problem.edge_clearance - 1e-6 \
                or problem.outline is not None and not problem.outline.contains(line):
            hits.append({"kind": "board edge", "needed_mm": problem.edge_clearance})
        return {"clear": not hits, "conflicts": hits[:50]}

    # ---- Checks --------------------------------------------------------------------------------

    def board_check(self, doc, categories=None, parity=False, refill=True):
        """DRC grouped by category: clearance, keepout, silkscreen, courtyard, connectivity, manufacturing,
        schematic (needs parity).  categories limits the report; None reports them all."""
        wanted = list(categories or CHECK_CATEGORIES)
        unknown = [c for c in wanted if c not in CHECK_CATEGORIES]
        if unknown:
            raise ValueError(f"unknown categories {unknown}; use {sorted(CHECK_CATEGORIES)}")
        report = self.board_drc(doc, parity=parity or "schematic" in (categories or []), refill=refill)
        grouped = {c: [] for c in wanted}
        other = []
        for violation in report["violations"]:
            category = next((c for c, kinds in CHECK_CATEGORIES.items() if violation["type"] in kinds), None)
            if category in grouped:
                grouped[category].append(violation)
            elif category is None:
                other.append(violation)
        summary = {c: len(v) for c, v in grouped.items()}
        return {"clean": not any(grouped.values()), "counts": summary,
                "violations": {c: v[:50] for c, v in grouped.items() if v},
                **({"other": other[:20]} if other and categories is None else {})}

    # ---- Design rules --------------------------------------------------------------------------

    def board_rules(self, doc, track_widths=None, via_sizes=None, severities=None, **minimums):
        """Built-in design rules: minimums (mm, see RULE_FIELDS), the predefined track widths and via
        sizes offered in the editor, and DRC severities ({type: error|warning|ignore})."""
        response = self.send(bc.GetBoardDesignRules(board=doc), bc.BoardDesignRulesResponse)
        rules = response.rules
        by_argument = {v: k for k, v in RULE_FIELDS.items()}
        changed = False
        for argument, value in minimums.items():
            if value is None:
                continue
            if argument not in by_argument:
                raise ValueError(f"unknown rule {argument}; use {sorted(by_argument)}")
            getattr(rules.constraints, by_argument[argument]).value_nm = _nm(value)
            changed = True
        if track_widths is not None:
            del rules.predefined_sizes.tracks[:]
            for w in track_widths:
                rules.predefined_sizes.tracks.add().width.value_nm = _nm(w)
            changed = True
        if via_sizes is not None:
            del rules.predefined_sizes.vias[:]
            for diameter, drill in via_sizes:
                if drill >= diameter:
                    raise ValueError("a via's drill must be smaller than its diameter")
                v = rules.predefined_sizes.vias.add()
                v.diameter.value_nm, v.drill.value_nm = _nm(diameter), _nm(drill)
            changed = True
        if severities:
            current = {s.rule_type: s for s in rules.severities}
            for name, level in severities.items():
                key = "DRCET_" + name.upper()
                if key not in rules_pb.DrcErrorType.keys():
                    raise ValueError(f"unknown DRC check {name}")
                if level not in SEVERITIES:
                    raise ValueError("severity must be error, warning or ignore")
                value = rules_pb.DrcErrorType.Value(key)
                if value in current:
                    current[value].severity = SEVERITIES[level]
                else:
                    rules.severities.add(rule_type=value, severity=SEVERITIES[level])
            changed = True
        if changed:
            response = self.send(bc.SetBoardDesignRules(board=doc, rules=rules), bc.BoardDesignRulesResponse)
        return rules_summary(response.rules)

    def board_custom_rules(self, doc, action="get", rules=None, names=None):
        """Custom (.kicad_dru) rules: get, add (replaces rules of the same name), remove (names), set (all)."""
        current = self.send(bc.GetCustomDesignRules(board=doc), bc.CustomRulesResponse)
        if action == "get":
            return custom_rules_summary(current)
        existing = list(current.rules)
        if action == "remove":
            wanted = set(names or [])
            missing = wanted - {r.name for r in existing}
            if missing:
                raise ValueError(f"no custom rules named {sorted(missing)}")
            final = [r for r in existing if r.name not in wanted]
        elif action in ("add", "set"):
            new = [custom_rule(r) for r in (rules or [])]
            if action == "add":
                replaced = {r.name for r in new}
                final = [r for r in existing if r.name not in replaced] + new
            else:
                final = new
        else:
            raise ValueError("action must be get, add, remove or set")
        request = bc.SetCustomDesignRules(board=doc)
        request.rules.extend(final)
        response = self.send(request, bc.CustomRulesResponse)
        if response.status == bc.CRS_INVALID:
            raise RuntimeError(f"KiCad rejected the custom rules: {response.error_text}")
        return custom_rules_summary(response)

    # ---- Net classes (default widths, clearances and via sizes) --------------------------------

    def board_netclasses(self, doc, classes=None, assign=None, patterns=None):
        """Read net classes; classes=[{name, clearance, track_width, via_diameter, via_drill, diff_pair_width,
        diff_pair_gap, priority}] (mm) creates or changes them ("Default" sets the board defaults);
        assign={net: class} and patterns={"/RF*": class} assign nets."""
        # The net class commands match the project path with its trailing separator; board listings omit it
        project = types.ProjectSpecifier()
        project.CopyFrom(doc.project)
        if project.path and not project.path.endswith("/"):
            project.path += "/"
        if classes:
            existing = {c.name: c for c in self.send(project_cmds.GetNetClasses(project=project),
                                                      project_cmds.NetClassesResponse).net_classes}
            changed = []
            for spec in classes:
                name = spec["name"]
                netclass = ps.NetClass()
                netclass.CopyFrom(existing.get(name, ps.NetClass(name=name)))
                board = netclass.board
                for key, attr in (("clearance", "clearance"), ("track_width", "track_width"),
                                  ("diff_pair_width", "diff_pair_track_width"), ("diff_pair_gap", "diff_pair_gap"),
                                  ("diff_pair_via_gap", "diff_pair_via_gap")):
                    if spec.get(key) is not None:
                        getattr(board, attr).value_nm = _nm(spec[key])
                if spec.get("via_diameter") is not None or spec.get("via_drill") is not None:
                    stack = board.via_stack
                    if spec.get("via_diameter") is not None:
                        if not stack.copper_layers:
                            stack.copper_layers.add(layer=bt.BL_F_Cu, shape=bt.PSS_CIRCLE)
                        stack.copper_layers[0].size.CopyFrom(_vec(spec["via_diameter"], spec["via_diameter"]))
                    if spec.get("via_drill") is not None:
                        stack.drill.diameter.CopyFrom(_vec(spec["via_drill"], spec["via_drill"]))
                if spec.get("priority") is not None:
                    netclass.priority = spec["priority"]
                changed.append(netclass)
            self.send(project_cmds.SetNetClasses(project=project, net_classes=changed,
                                                 merge_mode=types.MMM_MERGE), empty())
        if assign or patterns:
            request = project_cmds.SetNetClassAssignments(project=project, merge_mode=types.MMM_MERGE)
            for net, name in (assign or {}).items():
                request.assignments.add(net=net, netclasses=[name])
            for pattern, name in (patterns or {}).items():
                request.pattern_assignments.add(pattern=pattern, netclass=name)
            self.send(request, empty())
        listing = self.send(project_cmds.GetNetClasses(project=project), project_cmds.NetClassesResponse)
        assignments = self.send(project_cmds.GetNetClassAssignments(project=project),
                                project_cmds.NetClassAssignmentsResponse)
        return {"classes": [netclass_summary(c) for c in listing.net_classes],
                "assignments": {a.net: list(a.netclasses) for a in assignments.assignments},
                "patterns": {p.pattern: p.netclass for p in assignments.pattern_assignments}}

    # ---- Stackup -------------------------------------------------------------------------------

    def board_stackup_layers(self, doc):
        """The stackup top to bottom in mm (the form board_rf.dielectric_under reads)."""
        stackup = self.send(bc.GetBoardStackup(board=doc), bc.BoardStackupResponse).stackup
        layers = []
        for entry in stackup.layers:
            kind = board_pb.BoardStackupLayerType.Name(entry.type).removeprefix("BSLT_").lower()
            item = {"type": kind, "thickness": round(entry.thickness.value_nm / 1e6, 4)}
            if kind != "dielectric":
                item["layer"] = layer_name(entry.layer)
            if entry.material_name:
                item["material"] = entry.material_name
            if entry.HasField("color"):
                c = entry.color
                item["color"] = "#%02X%02X%02X" % (round(c.r * 255), round(c.g * 255), round(c.b * 255))
            if entry.HasField("dielectric"):
                item["dielectric"] = board_pb.BoardStackupDielectricType.Name(entry.dielectric.type) \
                    .removeprefix("BSDT_").lower()
                item["sublayers"] = [{"thickness": round(p.thickness.value_nm / 1e6, 4), "epsilon_r": p.epsilon_r,
                                      "loss_tangent": p.loss_tangent, "material": p.material_name}
                                     for p in entry.dielectric.layer]
            if entry.HasField("soldermask"):
                item.update(epsilon_r=entry.soldermask.epsilon_r, material=entry.soldermask.material_name)
            layers.append(item)
        thickness = sum(entry["thickness"] for entry in layers if entry["type"] in ("copper", "dielectric"))
        return {"layers": layers, "board_thickness_mm": round(thickness, 4),
                "finish": stackup.finish.type_name, "impedance_controlled": stackup.impedance.is_controlled}

    def board_set_stackup(self, doc, copper_layers=None, copper_thickness=None, dielectrics=None,
                          board_thickness=None, soldermask_color=None, silkscreen_color=None, finish=None,
                          impedance_controlled=None, soldermask_thickness=None):
        """Change the stackup.  copper_thickness: mm for every copper layer or {layer: mm}.  dielectrics:
        [{type: core|prepreg, thickness, epsilon_r, loss_tangent, material}] top to bottom (one fewer
        than the copper layers).  board_thickness scales the dielectrics to reach it."""
        current = self.board_stackup_layers(doc)
        count = copper_layers or sum(1 for entry in current["layers"] if entry["type"] == "copper")
        if count < 2 or count % 2:
            raise ValueError("copper_layers must be an even number, at least 2")
        names = copper_layer_names(count)
        if dielectrics is not None and len(dielectrics) != count - 1:
            raise ValueError(f"{count} copper layers need {count - 1} dielectrics, top to bottom")
        stackup = board_pb.BoardStackup()

        def colour(entry, value):
            if value:
                value = value.lstrip("#")
                entry.color.r, entry.color.g, entry.color.b = (int(value[i:i + 2], 16) / 255 for i in (0, 2, 4))
                entry.color.a = 1.0

        for side in ("F", "B"):
            silk = stackup.layers.add(type=board_pb.BSLT_SILKSCREEN, layer=layer(f"{side}.SilkS"))
            colour(silk, silkscreen_color)
            mask = stackup.layers.add(type=board_pb.BSLT_SOLDERMASK, layer=layer(f"{side}.Mask"))
            colour(mask, soldermask_color)
            if soldermask_thickness:
                mask.soldermask.thickness.value_nm = _nm(soldermask_thickness)
        for index, name in enumerate(names):
            entry = stackup.layers.add(type=board_pb.BSLT_COPPER, layer=layer(name))
            thickness = copper_thickness.get(name) if isinstance(copper_thickness, dict) else copper_thickness
            if thickness:
                entry.thickness.value_nm = _nm(thickness)
            if index < count - 1:
                d = stackup.layers.add(type=board_pb.BSLT_DIELECTRIC)
                spec = (dielectrics or [None] * (count - 1))[index]
                if spec:
                    d.dielectric.type = {"core": board_pb.BSDT_CORE, "prepreg": board_pb.BSDT_PREPREG}.get(
                        spec.get("type", "core"), board_pb.BSDT_CORE)
                    props = d.dielectric.layer.add()
                    if spec.get("thickness"):
                        props.thickness.value_nm = _nm(spec["thickness"])
                    props.epsilon_r = spec.get("epsilon_r", 0) or 0
                    props.loss_tangent = spec.get("loss_tangent", 0) or 0
                    props.material_name = spec.get("material", "") or ""
        if finish:
            stackup.finish.type_name = finish
        if impedance_controlled is not None:
            stackup.impedance.is_controlled = impedance_controlled
        self.send(bc.UpdateBoardStackup(board=doc, stackup=stackup), bc.BoardStackupResponse)
        result = self.board_stackup_layers(doc)
        if board_thickness:
            fixed = sum(e["thickness"] for e in result["layers"] if e["type"] == "copper")
            dielectric_total = sum(e["thickness"] for e in result["layers"] if e["type"] == "dielectric")
            target = board_thickness - fixed
            if target <= 0 or dielectric_total <= 0:
                raise ValueError("board_thickness is thinner than the copper alone")
            scale = target / dielectric_total
            scaled = board_pb.BoardStackup()
            for entry in result["layers"]:
                if entry["type"] != "dielectric":
                    continue
                d = scaled.layers.add(type=board_pb.BSLT_DIELECTRIC)
                d.dielectric.type = board_pb.BoardStackupDielectricType.Value(
                    "BSDT_" + (entry.get("dielectric") or "core").upper())
                for sub in entry.get("sublayers") or [{"thickness": entry["thickness"]}]:
                    d.dielectric.layer.add().thickness.value_nm = _nm(sub["thickness"] * scale)
            for name in names:
                scaled.layers.add(type=board_pb.BSLT_COPPER, layer=layer(name))
            self.send(bc.UpdateBoardStackup(board=doc, stackup=scaled), bc.BoardStackupResponse)
            result = self.board_stackup_layers(doc)
        return result

    # ---- RF and controlled impedance -------------------------------------------------------------

    def board_impedance_width(self, doc, impedance=50.0, signal_layer="F.Cu", structure="microstrip", gap=None,
                              reference_layer=None):
        """Track width for an impedance on this board's stackup."""
        if structure == "microstrip" and signal_layer not in ("F.Cu", "B.Cu"):
            raise ValueError("microstrip is an outer-layer structure; use F.Cu or B.Cu")
        stack = self.board_stackup_layers(doc)["layers"]
        d = board_rf.dielectric_under(stack, signal_layer, reference_layer)
        width = board_rf.width_for(impedance, d["height"], d["epsilon_r"], d["copper_thickness"], structure, gap)
        z = (board_rf.microstrip(width, d["height"], d["epsilon_r"], d["copper_thickness"]) if structure == "microstrip"
             else board_rf.cpwg(width, gap, d["height"], d["epsilon_r"]))
        return {"width_mm": round(width, 3), "impedance_ohm": round(z[0], 2), "effective_er": round(z[1], 3),
                "structure": structure, **({"gap_mm": gap} if gap else {}), "signal_layer": signal_layer,
                "reference_layer": d["reference_layer"], "dielectric_height_mm": round(d["height"], 4),
                "epsilon_r": round(d["epsilon_r"], 3), "copper_thickness_mm": d["copper_thickness"],
                "note": "Soldermask lowers a surface trace by about 1-3 ohm; have the board house confirm."}

    def board_rf_net(self, doc, nets, netclass="RF", impedance=50.0, width=None, structure="microstrip", gap=None,
                     clearance=None, signal_layer="F.Cu", via_diameter=None, via_drill=None, keep_others_away=None):
        """Set up RF / antenna nets: a net class with the impedance-controlled width (from the stackup unless
        width is given) and clearance (the CPWG gap, or 3x the width), assigned to the nets, plus a
        custom rule keeping other nets keep_others_away mm from them."""
        calc = None
        if width is None:
            calc = self.board_impedance_width(doc, impedance, signal_layer, structure, gap)
            width = calc["width_mm"]
        clearance = clearance if clearance is not None else (gap if structure == "cpwg" and gap else round(3 * width, 3))
        spec = {"name": netclass, "track_width": width, "clearance": clearance}
        if via_diameter:
            spec["via_diameter"] = via_diameter
        if via_drill:
            spec["via_drill"] = via_drill
        classes = self.board_netclasses(doc, classes=[spec], assign={net: netclass for net in nets})
        result = {"netclass": netclass, "nets": list(nets), "track_width_mm": width, "clearance_mm": clearance,
                  **({"impedance": calc} if calc else {}),
                  "class": next((c for c in classes["classes"] if c["name"] == netclass), None)}
        if keep_others_away:
            rule = {"name": f"{netclass} isolation", "condition": f"A.NetClass == '{netclass}' && B.NetClass != '{netclass}'",
                    "constraints": [{"type": "clearance", "min": keep_others_away}]}
            result["custom_rules"] = self.board_custom_rules(doc, "add", [rule])["rules"]
        return result

    def board_footprint_keepout(self, doc, reference, margin=1.0, layers=None, tracks=True, vias=True, pads=False,
                                copper=True, footprints=False, side_only=False):
        """Keepout rule area around a footprint (its courtyard, or its outline, plus margin), e.g. under and
        around a chip antenna, on all copper layers unless layers are given."""
        fp = self.board_footprint(doc, reference)
        area = None
        courtyard = bt.BL_B_CrtYd if fp.layer == bt.BL_B_Cu else bt.BL_F_CrtYd
        for packed in fp.definition.items:
            shape = bt.BoardGraphicShape()
            if packed.Is(bt.BoardGraphicShape.DESCRIPTOR) and packed.Unpack(shape) and shape.layer == courtyard:
                g = shape_geometry(shape, as_outline=False)
                if g is not None:
                    area = g if area is None else area.union(g)
        if area is None:
            box = self.send(editor.GetBoundingBox(header=types.ItemHeader(document=doc),
                                                  items=[types.KIID(value=fp.id.value)]),
                            editor.GetBoundingBoxResponse)
            if not box.boxes:
                raise ValueError(f"{reference} has no courtyard and no size")
            b = box.boxes[0]
            x, y = _mm(b.position)
            w, h = _mm(b.size)
            area = Polygon([(x, y), (x + w, y), (x + w, y + h), (x, y + h)])
        x1, y1, x2, y2 = area.bounds
        points = [(x1 - margin, y1 - margin), (x2 + margin, y1 - margin), (x2 + margin, y2 + margin),
                  (x1 - margin, y2 + margin)]
        if layers is None:
            names = self.board_copper_layers(doc)
            if side_only:
                names = ["B.Cu" if fp.layer == bt.BL_B_Cu else "F.Cu"]
        else:
            names = list(layers)
        rule = {"keepout_tracks": tracks, "keepout_vias": vias, "keepout_pads": pads, "keepout_copper": copper,
                "keepout_footprints": footprints}
        zone = make_zone(points, names, name=f"{reference} keepout", rule_area=rule)
        self.mutate(doc, [zone], description=f"MCP: keepout around {reference}")
        return {"rule_area": zone.id.value, "around": reference, "box_mm": [round(v, 4) for v in
                                                                          (points[0] + points[2])],
                "layers": names}


def empty():
    from google.protobuf import empty_pb2
    return empty_pb2.Empty


def rules_summary(rules):
    c = rules.constraints
    out = {argument: round(getattr(c, field).value_nm / 1e6, 4) for field, argument in RULE_FIELDS.items()}
    out["track_widths_mm"] = [round(t.width.value_nm / 1e6, 4) for t in rules.predefined_sizes.tracks]
    out["via_sizes_mm"] = [[round(v.diameter.value_nm / 1e6, 4), round(v.drill.value_nm / 1e6, 4)]
                           for v in rules.predefined_sizes.vias]
    out["severities"] = {rules_pb.DrcErrorType.Name(s.rule_type).removeprefix("DRCET_").lower():
                         types.RuleSeverity.Name(s.severity).removeprefix("RS_").lower() for s in rules.severities}
    out["soldermask"] = {"expansion_mm": rules.solder_mask_paste.mask_expansion.value_nm / 1e6,
                         "min_web_mm": rules.solder_mask_paste.mask_min_width.value_nm / 1e6}
    return out


def netclass_summary(netclass):
    b = netclass.board
    out = {"name": netclass.name}
    for key, attr in (("clearance_mm", "clearance"), ("track_width_mm", "track_width"),
                      ("diff_pair_width_mm", "diff_pair_track_width"), ("diff_pair_gap_mm", "diff_pair_gap")):
        if b.HasField(attr):
            out[key] = round(getattr(b, attr).value_nm / 1e6, 4)
    if b.HasField("via_stack"):
        if b.via_stack.copper_layers:
            out["via_diameter_mm"] = round(b.via_stack.copper_layers[0].size.x_nm / 1e6, 4)
        if b.via_stack.drill.diameter.x_nm:
            out["via_drill_mm"] = round(b.via_stack.drill.diameter.x_nm / 1e6, 4)
    if netclass.HasField("priority"):
        out["priority"] = netclass.priority
    return out


def custom_rule(spec):
    """A CustomRule from {name, condition, constraints: [{type, min, opt, max (mm) | items | expression}],
    layer: outer|inner|<layer>, severity: error|warning|ignore}, or protobuf JSON (with "constraints"
    whose type starts with CRCT_)."""
    constraints = spec.get("constraints", [])
    if any(str(c.get("type", "")).startswith("CRCT_") for c in constraints):
        return json_format.ParseDict(spec, rules_pb.CustomRule())
    rule = rules_pb.CustomRule(name=spec["name"], condition=spec.get("condition", ""))
    if spec.get("comments"):
        rule.comments = spec["comments"]
    where = spec.get("layer")
    if where in ("outer", "inner"):
        rule.layer_mode = rules_pb.CRLM_OUTER if where == "outer" else rules_pb.CRLM_INNER
    elif where:
        rule.single_layer = layer(where)
    if spec.get("severity"):
        rule.severity = SEVERITIES[spec["severity"]]
    for c in constraints:
        key = "CRCT_" + c["type"].upper()
        if key not in rules_pb.CustomRuleConstraintType.keys():
            raise ValueError(f"unknown constraint {c['type']}")
        constraint = rule.constraints.add(type=rules_pb.CustomRuleConstraintType.Value(key))
        if c["type"] == "disallow":
            for item in c.get("items", []):
                constraint.disallow.types.append(rules_pb.CustomRuleDisallowType.Value("CRDT_" + item.upper()))
        elif c["type"] == "assertion":
            constraint.assertion_expression = c["expression"]
        elif c["type"] == "zone_connection":
            constraint.zone_connection = {"solid": bt.ZCS_FULL, "thermal": bt.ZCS_THERMAL, "none": bt.ZCS_NONE}[
                c["style"]]
        else:
            for bound in ("min", "opt", "max"):
                if c.get(bound) is not None:
                    setattr(constraint.numeric, bound, _nm(c[bound]))
    return rule


def custom_rules_summary(response):
    rules = []
    for rule in response.rules:
        entry = {"name": rule.name, "condition": rule.condition, "constraints": []}
        for c in rule.constraints:
            kind = rules_pb.CustomRuleConstraintType.Name(c.type).removeprefix("CRCT_").lower()
            item = {"type": kind}
            if c.HasField("numeric"):
                for bound in ("min", "opt", "max"):
                    if c.numeric.HasField(bound):
                        item[bound] = getattr(c.numeric, bound) / 1e6
            elif c.HasField("disallow"):
                item["items"] = [rules_pb.CustomRuleDisallowType.Name(t).removeprefix("CRDT_").lower()
                                 for t in c.disallow.types]
            elif c.assertion_expression:
                item["expression"] = c.assertion_expression
            entry["constraints"].append(item)
        if rule.HasField("layer_mode"):
            entry["layer"] = rules_pb.CustomRuleLayerMode.Name(rule.layer_mode).removeprefix("CRLM_").lower()
        elif rule.HasField("single_layer"):
            entry["layer"] = layer_name(rule.single_layer)
        if rule.severity:
            entry["severity"] = types.RuleSeverity.Name(rule.severity).removeprefix("RS_").lower()
        rules.append(entry)
    return {"status": bc.CustomRulesStatus.Name(response.status).removeprefix("CRS_").lower(), "rules": rules,
            **({"error": response.error_text} if response.error_text else {})}
