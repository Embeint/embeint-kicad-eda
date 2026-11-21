# SPDX-License-Identifier: GPL-3.0-or-later
"""Deterministic graph operations; edges point from callers to dependencies."""
from collections import defaultdict, deque
import hashlib
import gzip
import json
import re

PIN = "4cf939778df04d2b747bb2c4714fc5014a7565c7"
CLASSES = ("pure", "thin-replacement", "UI")
THIN = {"wxString", "wxChar", "wxUniChar", "wxFileName", "wxPoint", "wxSize",
        "wxRect", "wxRealPoint", "wxLongLong", "wxULongLong", "wxDateTime",
        "wxArrayString", "wxScopedCharBuffer", "wxCharBuffer", "wxWCharBuffer",
        "wxConfig", "wxConfigBase", "wxFileConfig", "wxRegConfig", "wxVariant",
        "wxInputStream", "wxOutputStream", "wxMemoryInputStream", "wxMemoryOutputStream",
        "wxFileInputStream", "wxFileOutputStream", "wxFFileInputStream", "wxFFileOutputStream",
        "wxFile", "wxFFile", "wxDir", "wxTextFile", "wxTextInputStream", "wxTextOutputStream",
        "wxStringTokenizer", "wxStringIterator", "wxMBConv", "wxMBConvUTF8"}


def wx_class(names):
    names = set(names)
    return "UI" if names - THIN else "thin-replacement" if names else "pure"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, sort_keys=True, indent=2) + "\n", encoding="utf-8")


def read_json(path):
    """Accept the human-readable path or its deterministic gzip companion."""
    if not path.exists() and path.with_suffix(path.suffix + ".gz").exists():
        path = path.with_suffix(path.suffix + ".gz")
    if path.suffix == ".gz":
        return json.loads(gzip.decompress(path.read_bytes()))
    return json.loads(path.read_text(encoding="utf-8"))


def write_graph(path, value):
    """Keep the full graph below repository blob limits, with reproducible gzip."""
    path.parent.mkdir(parents=True, exist_ok=True)
    compressed = path.with_suffix(path.suffix + ".gz")
    compressed.write_bytes(gzip.compress(json.dumps(value, sort_keys=True, separators=(",", ":")).encode(), mtime=0))


def expand_virtual(calls, overrides):
    """Expand transitively, including overrides with no observed instantiation."""
    children = defaultdict(set)
    for base, derived in overrides:
        children[base].add(derived)
    out = set()
    for caller, target, virtual in calls:
        out.add((caller, target, "virtual" if virtual else "static"))
        if virtual:
            seen, pending = set(), [target]
            while pending:
                node = pending.pop()
                if node in seen:
                    continue
                seen.add(node)
                for derived in sorted(children[node]):
                    out.add((caller, derived, "override"))
                    pending.append(derived)
    return sorted(out)


def classify(functions, edges):
    """Propagate wx coupling through every call, before dropping cycle edges."""
    rank = {k: CLASSES.index(wx_class(v.get("wx", []))) for k, v in functions.items()}
    reverse = defaultdict(set)
    for caller, target, *_ in edges:
        reverse[target].add(caller)
    pending = deque(sorted(rank))
    while pending:
        target = pending.popleft()
        for caller in sorted(reverse[target]):
            if caller in rank and rank[caller] < rank.get(target, 0):
                rank[caller] = rank[target]
                pending.append(caller)
    for key, value in functions.items():
        value["wx_direct"] = wx_class(value.get("wx", []))
        value["wx_class"] = CLASSES[rank[key]]


def weighted_dag(nodes, weights):
    """Greedy weighted feedback arc heuristic, retaining strongest calls first.

    This is a heuristic, not a minimum feedback-arc solution. A call is dropped
    only when its insertion closes a cycle; every dropped weight is retained.
    """
    adj = {n: set() for n in nodes}
    dropped = []
    for (caller, target), weight in sorted(weights.items(), key=lambda e: (-e[1], e[0])):
        pending, seen = [target], set()
        cyclic = caller == target
        while pending and not cyclic:
            node = pending.pop()
            if node == caller:
                cyclic = True
                break
            if node not in seen:
                seen.add(node)
                pending.extend(adj[node] - seen)
        if cyclic:
            dropped.append({"caller": caller, "callee": target, "weight": weight})
        else:
            adj[caller].add(target)
    return adj, sorted(dropped, key=lambda e: (e["caller"], e["callee"]))


def layers(adj):
    reverse, remaining = defaultdict(set), {}
    for node, deps in adj.items():
        remaining[node] = len(deps)
        for dep in deps:
            reverse[dep].add(node)
    result = {n: 0 for n, count in remaining.items() if not count}
    queue = deque(sorted(result))
    while queue:
        dep = queue.popleft()
        for caller in sorted(reverse[dep]):
            result[caller] = max(result.get(caller, 0), result[dep] + 1)
            remaining[caller] -= 1
            if not remaining[caller]:
                queue.append(caller)
    if any(remaining.values()):
        raise ValueError("hard dependency cycle")
    return result


def identifier(path):
    return re.sub(r"[^a-z0-9_]+", ".", path.lower()).strip(".")
