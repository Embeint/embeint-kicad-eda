# SPDX-License-Identifier: GPL-3.0-or-later
"""Obstacle-avoiding track router for the live PCB tools.

Pure geometry (shapely + numpy), no KiCad calls: board_client gathers the board into a RouteProblem.
Routes run on a grid over the copper the new track may use: inside the board outline less the edge
clearance, away from every other net's copper by that pair's clearance, and out of keepouts.  Moves
are horizontal, vertical and 45 degrees; a via changes layer where a via fits.  Each finished segment
is checked against the exact geometry, and board_client confirms the result with KiCad's DRC.

Units are millimetres throughout.
"""

from dataclasses import dataclass, field
import heapq
import math

import numpy as np
import shapely
from shapely.geometry import LineString, Point, Polygon, box
from shapely.ops import polygonize, unary_union

SQRT2 = math.sqrt(2.0)
DIRECTIONS = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)]


@dataclass
class Obstacle:
    geometry: object          # shapely geometry of the copper
    net: str = ""             # "" for copper with no net (graphics, antenna shapes): avoided by every net
    kind: str = "copper"      # copper | pad | track | via
    key: str = ""             # pad id, so a route can pass through its own end pads even without a net


@dataclass
class RouteProblem:
    layers: list                                   # copper layer names the board has, top to bottom
    outline: object = None                         # board area (shapely Polygon) or None
    edge_clearance: float = 0.3
    min_clearance: float = 0.2
    obstacles: dict = field(default_factory=dict)  # layer -> [Obstacle]
    track_keepouts: dict = field(default_factory=dict)  # layer -> [geometry]
    via_keepouts: list = field(default_factory=list)    # [geometry] (through vias cross every layer)
    net_clearance: dict = field(default_factory=dict)   # net -> clearance of its net class
    pads: dict = field(default_factory=dict)            # pad id -> {layers: {layer: geometry}, net, position}

    def clearance(self, net_a, net_b):
        """Clearance between two nets: the larger net class clearance, at least the board minimum."""
        return max(self.min_clearance, self.net_clearance.get(net_a, 0.0), self.net_clearance.get(net_b, 0.0))


@dataclass
class Route:
    segments: list            # [(layer, [(x, y), ...])] connected polylines
    vias: list                # [(x, y)]
    length: float
    checked: bool             # every segment passed the exact geometry check
    problems: list = field(default_factory=list)


class NoRoute(RuntimeError):
    pass


# ---- Geometry helpers ---------------------------------------------------------------------------

def arc_points(start, mid, end, segments=24):
    """Points along the circular arc through start, mid and end."""
    (x1, y1), (x2, y2), (x3, y3) = start, mid, end
    d = 2 * (x1 * (y2 - y3) + x2 * (y3 - y1) + x3 * (y1 - y2))
    if abs(d) < 1e-12:
        return [start, end]
    ux = ((x1 ** 2 + y1 ** 2) * (y2 - y3) + (x2 ** 2 + y2 ** 2) * (y3 - y1) + (x3 ** 2 + y3 ** 2) * (y1 - y2)) / d
    uy = ((x1 ** 2 + y1 ** 2) * (x3 - x2) + (x2 ** 2 + y2 ** 2) * (x1 - x3) + (x3 ** 2 + y3 ** 2) * (x2 - x1)) / d
    r = math.hypot(x1 - ux, y1 - uy)
    a1, a2, a3 = (math.atan2(y - uy, x - ux) for x, y in (start, mid, end))

    def ccw(a, b):
        return (b - a) % (2 * math.pi)
    sweep = ccw(a1, a3)
    if ccw(a1, a2) > sweep:          # mid is not on the counter-clockwise way round: go clockwise
        sweep -= 2 * math.pi
    points = [(ux + r * math.cos(a1 + sweep * i / segments), uy + r * math.sin(a1 + sweep * i / segments))
              for i in range(segments + 1)]
    points[0], points[-1] = tuple(start), tuple(end)   # exact, so arcs join the lines they meet
    return points


def outline_from_lines(lines):
    """Board area from Edge.Cuts line work: the largest closed region less the regions inside it."""
    if not lines:
        return None
    merged = shapely.set_precision(unary_union(lines), 1e-4)   # close sub-micron gaps between segments
    polygons = sorted(polygonize(merged), key=lambda p: p.area, reverse=True)
    if not polygons:
        return None
    # polygonize returns a cut-out both as a face of its own and as a hole in the outer face
    outer = Polygon(polygons[0].exterior)
    cutouts = [p for p in polygons[1:] if outer.contains(p.representative_point())]
    return outer.difference(unary_union(cutouts)) if cutouts else outer


def _allowed_region(problem, layer, net, width, region, own, own_keys=()):
    """Where the centre line of a track of this net and width may run on one layer."""
    half = width / 2
    area = box(*region)
    if problem.outline is not None:
        area = area.intersection(problem.outline.buffer(-(problem.edge_clearance + half), join_style="mitre"))
    blocked = []
    for obstacle in problem.obstacles.get(layer, []):
        if obstacle.net and obstacle.net == net or obstacle.key and obstacle.key in own_keys:
            continue
        geometry = obstacle.geometry
        if not geometry.intersects(area.buffer(5)):
            continue
        blocked.append(geometry.buffer(problem.clearance(net, obstacle.net) + half, quad_segs=6))
    for keepout in problem.track_keepouts.get(layer, []):
        blocked.append(keepout.buffer(half, quad_segs=6))
    allowed = area.difference(unary_union(blocked)) if blocked else area
    # The track may always run inside its own pads' copper
    extra = [g.buffer(-half) for g in own.get(layer, [])]
    extra = [g for g in extra if not g.is_empty]
    if extra:
        allowed = allowed.union(unary_union(extra))
    return allowed


def _via_region(problem, net, via_diameter, layers, region, own_keys=()):
    """Where a through via of this net fits: clear on every copper layer and out of via keepouts."""
    r = via_diameter / 2
    area = box(*region)
    if problem.outline is not None:
        area = area.intersection(problem.outline.buffer(-(problem.edge_clearance + r)))
    blocked = [k.buffer(r) for k in problem.via_keepouts]
    for layer in problem.layers:
        for obstacle in problem.obstacles.get(layer, []):
            if obstacle.net and obstacle.net == net or obstacle.key and obstacle.key in own_keys:
                continue
            if obstacle.geometry.intersects(area):
                blocked.append(obstacle.geometry.buffer(problem.clearance(net, obstacle.net) + r, quad_segs=6))
    return area.difference(unary_union(blocked)) if blocked else area


# ---- Search ---------------------------------------------------------------------------------------

def route(problem, net, start, end, start_layers, end_layers, width, layers=None, via=None, grid=0.1,
          via_cost=4.0, margin=6.0, own_pads=None, max_cells=4_000_000, own_keys=()):
    """Route one connection of `net` from start (on one of start_layers) to end (on one of end_layers).

    layers: copper layers the route may use (default: the start and end layers).  via: (diameter,
    drill) to allow layer changes, or None.  own_pads: {layer: [pad geometry]} of this net's pads at
    the ends, inside which the track may always run.  Raises NoRoute if nothing fits.
    """
    layers = list(layers or dict.fromkeys(list(start_layers) + list(end_layers)))
    usable = [layer for layer in layers if layer in problem.layers or not problem.layers]
    if not usable:
        raise NoRoute(f"none of the layers {layers} is a copper layer of this board")
    own_pads = own_pads or {}
    attempts = [margin, margin * 3, None]
    last_error = None
    for pad in attempts:
        region = _search_region(problem, start, end, pad)
        try:
            return _route_in(problem, net, start, end, start_layers, end_layers, width, usable, via, grid,
                             via_cost, region, own_pads, max_cells, set(own_keys))
        except NoRoute as error:
            last_error = error
            if pad is None:
                break
    raise last_error


def _search_region(problem, start, end, margin):
    if margin is None and problem.outline is not None:
        return problem.outline.bounds
    margin = margin if margin is not None else 50.0
    x1, x2 = sorted((start[0], end[0]))
    y1, y2 = sorted((start[1], end[1]))
    region = (x1 - margin, y1 - margin, x2 + margin, y2 + margin)
    if problem.outline is not None:
        bx1, by1, bx2, by2 = problem.outline.bounds
        region = (max(region[0], bx1), max(region[1], by1), min(region[2], bx2), min(region[3], by2))
    return region


def _route_in(problem, net, start, end, start_layers, end_layers, width, layers, via, grid, via_cost, region,
              own_pads, max_cells, own_keys=frozenset()):
    # A large area gets a coarser grid rather than an endless search
    cells = (region[2] - region[0]) * (region[3] - region[1]) / (grid * grid) * len(layers)
    if cells > max_cells:
        grid = grid * math.sqrt(cells / max_cells)
    x0, y0 = start                      # grid aligned on the start point
    i_lo = math.floor((region[0] - x0) / grid)
    i_hi = math.ceil((region[2] - x0) / grid)
    j_lo = math.floor((region[1] - y0) / grid)
    j_hi = math.ceil((region[3] - y0) / grid)
    nx, ny = i_hi - i_lo + 1, j_hi - j_lo + 1
    if nx * ny * len(layers) > max_cells:
        raise NoRoute(f"the search area needs {nx * ny * len(layers)} grid cells; use a coarser grid")
    xs = x0 + (np.arange(nx) + i_lo) * grid
    ys = y0 + (np.arange(ny) + j_lo) * grid
    gx, gy = np.meshgrid(xs, ys, indexing="ij")

    allowed_geometry, masks = {}, []
    for layer in layers:
        geometry = _allowed_region(problem, layer, net, width, region, own_pads, own_keys)
        allowed_geometry[layer] = geometry
        shapely.prepare(geometry)
        masks.append(shapely.contains_xy(geometry, gx, gy) | shapely.intersects_xy(geometry, gx, gy))
    masks = np.array(masks)
    via_mask = None
    if via and len(layers) > 1:
        via_geometry = _via_region(problem, net, via[0], layers, region, own_keys)
        shapely.prepare(via_geometry)
        via_mask = shapely.contains_xy(via_geometry, gx, gy)

    si, sj = -i_lo, -j_lo
    starts = [layers.index(layer) for layer in start_layers if layer in layers]
    if not starts:
        raise NoRoute("the start pad is on none of the routing layers")
    for l in starts:
        masks[l, si, sj] = True

    # Goal: grid nodes near the end from which a straight run to the end point is legal
    goals = {}
    radius = max(2, int(math.ceil(1.5 * width / grid)) + 1)
    ei, ej = int(round((end[0] - x0) / grid)) - i_lo, int(round((end[1] - y0) / grid)) - j_lo
    for layer in end_layers:
        if layer not in layers:
            continue
        l = layers.index(layer)
        for di in range(-radius, radius + 1):
            for dj in range(-radius, radius + 1):
                i, j = ei + di, ej + dj
                if 0 <= i < nx and 0 <= j < ny and masks[l, i, j]:
                    p = (xs[i], ys[j])
                    tail = LineString([p, end]) if math.dist(p, end) > 1e-9 else Point(end)
                    if allowed_geometry[layer].buffer(1e-6).covers(tail):
                        goals[(l, i, j)] = math.dist(p, end)
    if not goals:
        raise NoRoute("no legal approach to the end pad (it is boxed in by other copper or a keepout)")

    def heuristic(i, j):
        dx, dy = abs(i - ei), abs(j - ej)
        return (max(dx, dy) - min(dx, dy) + SQRT2 * min(dx, dy)) * grid

    shape = masks.shape
    cost = np.full(shape, np.inf)
    came = {}
    heading = {}
    heap = []
    for l in starts:
        cost[l, si, sj] = 0.0
        heapq.heappush(heap, (heuristic(si, sj), 0.0, (l, si, sj)))
    turn = 0.3 * grid
    found = None
    while heap:
        _, g, node = heapq.heappop(heap)
        if g > cost[node]:
            continue
        if node in goals:
            found = node
            break
        l, i, j = node
        last = heading.get(node)
        for k, (di, dj) in enumerate(DIRECTIONS):
            ni, nj = i + di, j + dj
            if not (0 <= ni < nx and 0 <= nj < ny) or not masks[l, ni, nj]:
                continue
            if di and dj and not (masks[l, i + di, j] and masks[l, i, j + dj]):
                continue              # no corner cutting past an obstacle
            step = (SQRT2 if di and dj else 1.0) * grid
            if last is not None and last != k:
                step += turn
            nxt = (l, ni, nj)
            ng = g + step
            if ng < cost[nxt]:
                cost[nxt] = ng
                came[nxt] = node
                heading[nxt] = k
                heapq.heappush(heap, (ng + heuristic(ni, nj), ng, nxt))
        if via_mask is not None and via_mask[i, j]:
            for nl in range(len(layers)):
                if nl == l or not masks[nl, i, j]:
                    continue
                nxt = (nl, i, j)
                ng = g + via_cost
                if ng < cost[nxt]:
                    cost[nxt] = ng
                    came[nxt] = node
                    heading.pop(nxt, None)
                    heapq.heappush(heap, (ng + heuristic(i, j), ng, nxt))
    if found is None:
        raise NoRoute("no path: other copper, keepouts or the board edge block every way through")

    nodes = [found]
    while nodes[-1] in came:
        nodes.append(came[nodes[-1]])
    nodes.reverse()
    return _build_route(nodes, xs, ys, layers, start, end, allowed_geometry)


def _build_route(nodes, xs, ys, layers, start, end, allowed_geometry):
    segments, vias = [], []
    current_layer = nodes[0][0]
    points = [tuple(start)]
    for l, i, j in nodes[1:]:
        p = (round(float(xs[i]), 6), round(float(ys[j]), 6))
        if l != current_layer:
            segments.append((layers[current_layer], points))
            vias.append(points[-1])
            current_layer, points = l, [points[-1]]
            continue
        points.append(p)
    if math.dist(points[-1], end) > 1e-9:
        points.append(tuple(end))
    segments.append((layers[current_layer], points))

    route_segments, problems, length = [], [], 0.0
    for layer, pts in segments:
        simplified = simplify(pts)
        if len(simplified) < 2:
            continue
        geometry = allowed_geometry[layer].buffer(1e-6)
        for a, b in zip(simplified, simplified[1:]):
            length += math.dist(a, b)
            if not geometry.covers(LineString([a, b])):
                problems.append({"layer": layer, "from": a, "to": b})
        route_segments.append((layer, simplified))
    return Route(route_segments, vias, round(length, 4), not problems, problems)


def simplify(points):
    """Drop repeated points and points in the middle of straight runs."""
    out = []
    for p in points:
        if out and math.dist(out[-1], p) < 1e-9:
            continue
        out.append(p)
    changed = True
    while changed and len(out) > 2:
        changed = False
        for k in range(1, len(out) - 1):
            (x1, y1), (x2, y2), (x3, y3) = out[k - 1], out[k], out[k + 1]
            if abs((x2 - x1) * (y3 - y1) - (y2 - y1) * (x3 - x1)) < 1e-9:
                del out[k]
                changed = True
                break
    return out


def spanning_order(clusters):
    """Pairs of points (one per cluster) that join all clusters with the shortest total length (Prim).

    clusters: [[(x, y, key), ...], ...].  Returns [((x, y, key), (x, y, key)), ...].
    """
    if len(clusters) < 2:
        return []
    joined = {0}
    pairs = []
    while len(joined) < len(clusters):
        best = None
        for a in joined:
            for b in range(len(clusters)):
                if b in joined:
                    continue
                for p in clusters[a]:
                    for q in clusters[b]:
                        d = math.dist(p[:2], q[:2])
                        if best is None or d < best[0]:
                            best = (d, p, q, b)
        _, p, q, b = best
        pairs.append((p, q))
        joined.add(b)
    return pairs
