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
    path.write_text(json.dumps(value, sort_keys=True, indent=2) + "\n", encoding="utf-8", newline="\n")


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
    data = bytearray(gzip.compress(json.dumps(value, sort_keys=True, separators=(",", ":")).encode(),
                                  compresslevel=9, mtime=0))
    # Python 3.11/3.12 delegate mtime=0 to zlib, whose gzip OS byte differs
    # between Windows and Linux. The payload/checksum do not depend on it.
    data[9] = 255
    compressed.write_bytes(data)


def write_cache(path, value):
    """Compress resumable TU records and publish only complete cache entries."""
    compressed = path.with_suffix(path.suffix + ".gz")
    compressed.parent.mkdir(parents=True, exist_ok=True)
    data = bytearray(gzip.compress(json.dumps(value, sort_keys=True, separators=(",", ":")).encode(),
                                  compresslevel=1, mtime=0))
    data[9] = 255
    temporary = compressed.with_suffix(compressed.suffix + ".tmp")
    temporary.write_bytes(data)
    temporary.replace(compressed)


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


def definition_location(record):
    return record["file"], record["start"], record["end"]


def locate_endpoints(result):
    """Attach to each call/override endpoint the body its own TU defines.

    A translation unit can contain at most one definition of a USR (one
    definition rule), and an inline function or destructor must be defined in
    every TU that odr-uses it. A TU-local definition is therefore the exact body
    that TU's call or override refers to. A USR without a local definition (an
    out-of-line function in another TU) gets None: the linker chooses its body,
    which this source-level graph cannot see, so it stays conservative.
    """
    local = {usr: definition_location(record) for usr, record in result["functions"].items()}
    calls = {(caller, local.get(caller), target, local.get(target), bool(virtual))
             for caller, target, virtual in result["calls"]}
    overrides = {(base, local.get(base), derived, local.get(derived))
                 for base, derived in result["overrides"]}
    return calls, overrides


def materialize_definitions(functions, calls, overrides, locations):
    """Keep distinct source bodies sharing a USR across separate executables.

    Their per-configuration calls are conservatively unioned, as for ordinary
    definition variants. Stable copies give every physical body a unit owner.

    Calls are `(caller, caller_location, target, target_location, virtual)` and
    overrides `(base, base_location, derived, derived_location)` from
    `locate_endpoints`. A recorded location selects only the body the reporting
    TU itself defined, so two unrelated classes that happen to share a name in
    separate components do not depend on each other. A None location (or a
    legacy unlocated 3-/2-tuple) reaches every body sharing the USR.
    """
    choices, copies, keys = {}, {}, {}
    for usr, records in sorted(locations.items()):
        primary = functions[usr]
        primary_location = definition_location(primary)
        choices[usr] = [usr]
        keys[usr] = {primary_location: usr}
        for location, record in sorted(records.items()):
            if location == primary_location:
                continue
            key = usr + "@definition:" + digest(json.dumps(location).encode())[:16]
            if key in functions:
                raise ValueError(f"Definition identity collision: {key}")
            functions[key] = dict(record, wx=list(primary["wx"]), clang_usr=usr)
            choices[usr].append(key)
            copies[key] = usr
            keys[usr][location] = key

    def bodies(usr, location):
        if location is None:
            return choices.get(usr, [usr])
        known = keys.get(usr)
        if known is None:
            # Only one body was observed; its location is the primary record.
            return [usr]
        if tuple(location) not in known:
            raise ValueError(f"Call endpoint location was not recorded as a definition: {usr} {location}")
        return [known[tuple(location)]]

    def located_calls():
        for call in calls:
            if len(call) == 3:
                caller, target, virtual = call
                yield caller, None, target, None, virtual
            else:
                yield call

    def located_overrides():
        for override in overrides:
            if len(override) == 2:
                base, derived = override
                yield base, None, derived, None
            else:
                yield override

    expanded_calls = {(a, b, virtual) for caller, caller_at, target, target_at, virtual in located_calls()
                      for a in bodies(caller, caller_at) for b in bodies(target, target_at)}
    expanded_overrides = {(a, b) for base, base_at, derived, derived_at in located_overrides()
                          for a in bodies(base, base_at) for b in bodies(derived, derived_at)}
    return sorted(expanded_calls), sorted(expanded_overrides), copies


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
