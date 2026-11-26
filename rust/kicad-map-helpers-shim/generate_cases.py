#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fixed-seed public corpus and refusal controls for
libs.core.include.core.application.thin.replacement.p001 (map_helpers.h).

Writes port/cases/<unit>.jsonl (requests only, all inside the reference's defined
domain), port/oracle/fixtures/core-map-helpers-controls.jsonl (malformed requests
both adapters must refuse, with the refusal) and expected-replies.json (counts). It
never computes a reference reply. Inputs come from the pinned callers' maps
(common/string_utils.cpp, pcbnew/dialogs/dialog_export_step.cpp, the LTspice and
EasyEDA Pro importers' attribute keys), the boundaries of wx 3.2.4's strict UTF-8
decoder (tableUtf8Lengths, continuation checks, truncation, the NUL terminator) and
the signed wchar_t order, plus a splitmix64 stream with a fixed seed. `--check`
verifies the committed files.
"""
import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
UNIT = "libs.core.include.core.application.thin.replacement.p001"
CASES = ROOT / "port/cases" / (UNIT + ".jsonl")
CONTROLS = ROOT / "port/oracle/fixtures/core-map-helpers-controls.jsonl"
EXPECTED = ROOT / "rust/kicad-map-helpers-shim/expected-replies.json"
I32_MIN, I32_MAX = -(1 << 31), (1 << 31) - 1
MAX_ENTRIES, MAX_WIDE, MAX_BYTES = 256, 4096, 4096
SEED = 0x6D61705F68656C70  # "map_help"


class Stream:
    """splitmix64: stable across Python versions, unlike random.Random's helpers."""

    def __init__(self, seed):
        self.state = seed

    def word(self):
        self.state = (self.state + 0x9E3779B97F4A7C15) % (1 << 64)
        z = self.state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) % (1 << 64)
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) % (1 << 64)
        return z ^ (z >> 31)

    def below(self, n):
        return self.word() % n

    def choice(self, items):
        return items[self.below(len(items))]


def wide(text):
    """Units of a wxS() literal on the 32-bit wchar_t build."""
    return [ord(c) for c in text]


def utf8(text):
    return list(text.encode("utf-8"))


def encode(code, overlong=0):
    """Bytes the strict decoder turns into `code` (at most 0x13FFFF): the shortest
    form, or a longer (overlong) three- or four-byte form, which it also accepts.
    Two-byte overlongs are not produced: their C0/C1 lead bytes are rejected."""
    natural = 1 if code < 0x80 else 2 if code < 0x800 else 3 if code < 0x10000 else 4
    length = natural + overlong
    if natural == 1 and length == 2:
        length = 3
    length = min(length, 4)
    if length == 1:
        return [code]
    tail = []
    value = code
    for _ in range(length - 1):
        tail.append(0x80 | (value & 0x3F))
        value >>= 6
    lead = {2: 0xC0, 3: 0xE0, 4: 0xF0}[length]
    assert value <= {2: 0x1F, 3: 0x0F, 4: 0x07}[length]
    return [lead | value] + tail[::-1]


# Keys and values from the pinned callers.
REPLACEMENTS = [("quot", '"'), ("apos", "'"), ("amp", "&"), ("lt", "<"), ("gt", ">")]
EXTENSIONS = [("step", 0), ("stp", 0), ("glb", 1), ("xao", 2), ("brep", 3), ("ply", 4),
              ("stl", 5), ("stpz", 6), ("u3d", 7), ("pdf", 8)]
LTSPICE = [("INSTNAME", "R1"), ("TYPE", "res"), ("PREFIX", "X"), ("VALUE", "10k"),
           ("VALUE2", ""), ("MODELFILE", "opamp.lib"), ("SPICELINE", "Rser=1"),
           ("PinName", "IN+"), ("SpiceOrder", "1")]
EASYEDA = [("Value", "100nF"), ("Name", "C1"), ("Footprint", "C0603"), ("Device", "")]
# Wide units at the order and representation boundaries of signed wchar_t.
UNIT_POOL = [0, 1, 0x41, 0x61, 0x7F, 0x80, 0xFF, 0x7FF, 0x800, 0xD7FF, 0xD800, 0xDBFF,
             0xDC00, 0xDFFF, 0xE000, 0xFFFD, 0xFFFF, 0x10000, 0x10FFFF, 0x110000,
             0x13FFFF, 0x200000, I32_MAX, -1, -2, I32_MIN, I32_MIN + 1]


class Corpus:
    def __init__(self):
        self.cases = []
        self.controls = []
        self.counts = {}
        self.stream = Stream(SEED)

    def add(self, group, inp):
        n = self.counts.get(group, 0)
        self.counts[group] = n + 1
        self.cases.append({"unit": UNIT, "case": f"{group}-{n:05d}", "input": inp})

    def refuse(self, group, inp, message):
        n = self.counts.get("refuse-" + group, 0)
        self.counts["refuse-" + group] = n + 1
        self.controls.append({
            "request": {"unit": UNIT, "case": f"refuse-{group}-{n:05d}", "input": inp},
            "error": {"kind": "invalid_input", "message": message},
        })

    # Random material -----------------------------------------------------------
    def random_units(self, max_len=6):
        s = self.stream
        out = []
        for _ in range(s.below(max_len + 1)):
            kind = s.below(4)
            if kind == 0:
                out.append(s.choice(UNIT_POOL))
            elif kind == 1:
                out.append(0x61 + s.below(3))  # short alphabet: frequent prefixes and hits
            elif kind == 2:
                out.append(s.below(0x140000))
            else:
                out.append(s.below(1 << 32) + I32_MIN)
        return out

    def random_int(self):
        s = self.stream
        return s.choice([0, 1, -1, 2, 7, I32_MIN, I32_MAX, I32_MIN + 1, I32_MAX - 1,
                         s.below(1 << 32) + I32_MIN, s.below(16) - 8])

    def random_item(self, kind):
        return self.random_int() if kind == "int" else self.random_units()

    def random_map(self, key, value, size=None):
        s = self.stream
        n = s.below(9) if size is None else size
        entries = [[self.random_item(key), self.random_item(value)] for _ in range(n)]
        # Duplicate keys: emplace keeps the first value.
        if entries and size is None and s.below(3) == 0:
            entries.append([entries[s.below(len(entries))][0], self.random_item(value)])
        return entries

    def random_utf8(self):
        """Bytes mixing valid forms, accepted overlong/surrogate forms and errors."""
        s = self.stream
        out = []
        for _ in range(s.below(8)):
            kind = s.below(10)
            if kind < 3:
                out += [0x20 + s.below(0x5F)]
            elif kind < 6:
                code = s.choice([s.below(0x800), s.below(0x10000), 0x10000 + s.below(0x30000),
                                 0xD800 + s.below(0x800), 0x110000 + s.below(0x30000)])
                out += encode(code)
            elif kind == 6:
                out += encode(s.below(0x800), overlong=1 + s.below(2) if s.below(2) else 1)
            elif kind == 7:
                out.append(s.choice([0x80, 0xBF, 0xC0, 0xC1, 0xF5, 0xFF]))
            elif kind == 8:
                seq = encode(0x800 + s.below(0xF000))
                out += seq[: 1 + s.below(len(seq) - 1)]  # truncated sequence
            else:
                out.append(0)  # embedded NUL: the callee stops here
        return out


def key_candidates(c, entries, kind):
    """A hit on an existing key, or a near miss, or a random key."""
    s = c.stream
    if entries and s.below(2) == 0:
        key = entries[s.below(len(entries))][0]
        if kind == "wx" and s.below(3) == 0:
            key = key + [0] if s.below(2) else key[:-1]  # prefix / extension near-miss
        return key
    return c.random_item(kind)


def cstr_for(c, units):
    """A const char* argument for a wxString key: its UTF-8 (possibly overlong)
    encoding when the decoder can produce it, otherwise random bytes."""
    s = c.stream
    if all(0 < u <= 0x13FFFF for u in units):
        out = []
        for u in units:
            out += encode(u, overlong=1 if (u < 0x800 and s.below(4) == 0) else 0)
        return out
    return c.random_utf8()


def utf8_boundaries(c):
    c.add("from_utf8", {"op": "from_utf8", "text": None})
    c.add("from_utf8", {"op": "from_utf8", "text": []})
    c.add("from_utf8", {"op": "from_utf8", "text": utf8("Hello, wxString")})
    # Every lead byte alone and followed by three continuation bytes.
    for lead in range(1, 256):
        c.add("from_utf8", {"op": "from_utf8", "text": [lead]})
        c.add("from_utf8", {"op": "from_utf8", "text": [lead, 0x80, 0x80, 0x80]})
        c.add("from_utf8", {"op": "from_utf8", "text": [lead, 0xBF, 0xBF, 0xBF, 0x41]})
    edges = [[0xC2, 0x80], [0xDF, 0xBF], [0xE0, 0x80, 0x80], [0xE0, 0x9F, 0xBF],
             [0xE0, 0xA0, 0x80], [0xED, 0x9F, 0xBF], [0xED, 0xA0, 0x80], [0xED, 0xBF, 0xBF],
             [0xEE, 0x80, 0x80], [0xEF, 0xBF, 0xBF], [0xF0, 0x80, 0x80, 0x80],
             [0xF0, 0x8F, 0xBF, 0xBF], [0xF0, 0x90, 0x80, 0x80], [0xF4, 0x8F, 0xBF, 0xBF],
             [0xF4, 0x90, 0x80, 0x80], [0xF4, 0xBF, 0xBF, 0xBF], [0xF1, 0x80, 0x80, 0x80]]
    for seq in edges:
        c.add("from_utf8", {"op": "from_utf8", "text": seq})
        c.add("from_utf8", {"op": "from_utf8", "text": [0x41] + seq + [0x42]})
        for cut in range(1, len(seq)):
            c.add("from_utf8", {"op": "from_utf8", "text": seq[:cut]})
            c.add("from_utf8", {"op": "from_utf8", "text": seq[:cut] + [0x41]})
            c.add("from_utf8", {"op": "from_utf8", "text": seq[:cut] + [0]})
        for pos in range(1, len(seq)):
            for bad in (0x00, 0x7F, 0xC0, 0xFF):
                broken = list(seq)
                broken[pos] = bad
                c.add("from_utf8", {"op": "from_utf8", "text": broken})
    for text in ([0x61, 0x62, 0, 0x63], [0, 0x61], [0x61, 0, 0xFF], [0xFF, 0, 0x61],
                 [0xE0, 0x80, 0x80], [0xE0, 0x80, 0x80, 0x61], [0xF0, 0x80, 0x80, 0x80, 0]):
        c.add("from_utf8", {"op": "from_utf8", "text": text})
    for text in ("Résistance", "抵抗器", "😀 smile", "Ω≈µ", " ", "﻿BOM"):
        c.add("from_utf8", {"op": "from_utf8", "text": utf8(text)})
    # Size boundaries of the adapter (not of wx).
    c.add("from_utf8", {"op": "from_utf8", "text": [0x61] * MAX_BYTES})
    c.add("from_utf8", {"op": "from_utf8", "text": [0xF0, 0x9F, 0x98, 0x80] * (MAX_BYTES // 4)})
    c.add("from_utf8", {"op": "from_utf8", "text": [0x61] * (MAX_BYTES - 1) + [0xC3]})
    for _ in range(300):
        c.add("from_utf8", {"op": "from_utf8", "text": c.random_utf8()})


def caller_cases(c):
    repl = [[wide(k), wide(v)] for k, v in REPLACEMENTS]
    for code in ["quot", "apos", "amp", "lt", "gt", "nbsp", "", "QUOT", "lt;"]:
        c.add("caller-string_utils", {"op": "get_opt_wx", "value_type": "wx", "map": repl,
                                      "key": wide(code)})
    ext = [[wide(k), v] for k, v in EXTENSIONS]
    for code in [k for k, _ in EXTENSIONS] + ["STEP", "wrl", "", "st"]:
        c.add("caller-export_step", {"op": "get_opt_wx", "value_type": "int", "map": ext,
                                     "key": wide(code)})
    job = [[i, i] for i in range(9)]
    for key in [-1, 0, 4, 8, 9, I32_MAX]:
        c.add("caller-export_step", {"op": "get_opt", "key_type": "int", "value_type": "int",
                                     "map": job, "key": key})
    lt = [[wide(k), wide(v)] for k, v in LTSPICE]
    for key, defval in [("INSTNAME", "_"), ("PinName", None), ("TYPE", None), ("PREFIX", None),
                        ("VALUE2", None), ("SpiceOrder", None), ("MISSING", "_"), ("MISSING", None),
                        ("instname", "_")]:
        inp = {"op": "get_def_wx", "map": lt, "key": wide(key)}
        if defval is not None:
            inp["defval"] = wide(defval)
        c.add("caller-ltspice", inp)
    ee = [[wide(k), wide(v)] for k, v in EASYEDA]
    for key in ["Value", "Name", "Device", "Missing"]:
        c.add("caller-easyedapro", {"op": "get_def_wx", "map": ee, "key": wide(key), "defval": []})
        c.add("caller-easyedapro", {"op": "get_opt_wx", "value_type": "wx", "map": ee,
                                    "key": wide(key)})
        c.add("caller-easyedapro", {"op": "get_def_wx", "map": ee, "key": wide(key),
                                    "defval": wide("{" + key + "!}")})
    # The const char* overloads with literal keys as plain C++ callers would pass them.
    for key in ["TYPE", "PinName", "MISSING", ""]:
        c.add("caller-cstr", {"op": "get_opt_cstr", "value_type": "wx", "map": lt, "key": utf8(key)})
        c.add("caller-cstr", {"op": "get_def_cstr", "map": lt, "key": utf8(key), "defval": utf8("?")})
        c.add("caller-cstr", {"op": "get_def_cstr_wx", "map": lt, "key": utf8(key),
                              "defval": wide("?")})


def cstr_boundaries(c):
    """Null, empty, invalid and embedded-NUL const char* keys and defaults, against
    maps with and without an empty-string key (which all of them convert to)."""
    base = [[wide("a"), wide("A")], [wide("ab"), wide("AB")], [[0xE9], wide("e-acute")],
            [[0xD800], wide("surrogate")], [[0x13FFFF], wide("max")], [[0, 0x78], wide("nul-x")],
            [[0x61, 0], wide("a-nul")]]
    with_empty = base + [[[], wide("EMPTY")]]
    keys = [None, [], [0x61], [0x61, 0x62], [0x61, 0, 0x62], [0, 0x61], [0xC3, 0xA9],
            [0xE0, 0x83, 0xA9], [0xF0, 0x80, 0x83, 0xA9], [0xED, 0xA0, 0x80], [0xF4, 0xBF, 0xBF, 0xBF],
            [0xE0, 0x80, 0x80, 0x78], [0x61, 0xE0, 0x80, 0x80], [0xFF], [0xC3], [0x61, 0xC3],
            [0xC0, 0x80], [0x80], [0xF5, 0x80, 0x80, 0x80]]
    defvals = [None, [], [0x64], [0xFF], [0x64, 0, 0x65], [0xE0, 0x80, 0x80], [0xC3, 0xA9]]
    for entries in (base, with_empty):
        for key in keys:
            for value_type, mapped in (("wx", entries), ("int", [[k, i] for i, (k, _) in enumerate(entries)])):
                c.add("cstr-get_opt", {"op": "get_opt_cstr", "value_type": value_type,
                                       "map": mapped, "key": key})
            for defval in defvals:
                c.add("cstr-get_def", {"op": "get_def_cstr", "map": entries, "key": key,
                                       "defval": defval})
            c.add("cstr-get_def_wx", {"op": "get_def_cstr_wx", "map": entries, "key": key,
                                      "defval": wide("dflt")})
            c.add("cstr-get_def_wx", {"op": "get_def_cstr_wx", "map": entries, "key": key,
                                      "defval": [0, -1, 0x7FFFFFFF]})


def generic_cases(c):
    types = [("int", "int"), ("int", "wx"), ("wx", "int"), ("wx", "wx")]
    for key, value in types:
        # Empty maps, size boundaries and order.
        c.add("map_order", {"op": "map_order", "key_type": key, "value_type": value, "map": []})
        for _ in range(40):
            c.add("map_order", {"op": "map_order", "key_type": key, "value_type": value,
                                "map": c.random_map(key, value)})
        c.add("map_order", {"op": "map_order", "key_type": key, "value_type": value,
                            "map": c.random_map(key, value, size=MAX_ENTRIES)})
        for _ in range(60):
            entries = c.random_map(key, value)
            c.add("get_opt", {"op": "get_opt", "key_type": key, "value_type": value,
                              "map": entries, "key": key_candidates(c, entries, key)})
            entries = c.random_map(key, value)
            inp = {"op": "get_def", "key_type": key, "value_type": value, "map": entries,
                   "key": key_candidates(c, entries, key)}
            if c.stream.below(2):
                inp["defval"] = c.random_item(value)
            c.add("get_def", inp)
    # Signed order: keys whose unsigned order differs.
    signed = [[[-1], 1], [[0], 2], [[I32_MIN], 3], [[I32_MAX], 4], [[0x80000000 - 1, 0], 5],
              [[], 6], [[0, 0], 7], [[-1, -1], 8]]
    c.add("map_order", {"op": "map_order", "key_type": "wx", "value_type": "int", "map": signed})
    c.add("map_order", {"op": "map_order", "key_type": "int", "value_type": "int",
                        "map": [[I32_MAX, 1], [I32_MIN, 2], [0, 3], [-1, 4], [1, 5], [0, 6]]})
    for value in ("int", "wx"):
        for _ in range(80):
            entries = c.random_map("wx", value)
            c.add("get_opt_wx", {"op": "get_opt_wx", "value_type": value, "map": entries,
                                 "key": key_candidates(c, entries, "wx")})
        for _ in range(80):
            entries = c.random_map("wx", value)
            key = key_candidates(c, entries, "wx")
            c.add("get_opt_cstr", {"op": "get_opt_cstr", "value_type": value, "map": entries,
                                   "key": cstr_for(c, key)})
    for _ in range(80):
        entries = c.random_map("wx", "wx")
        key = key_candidates(c, entries, "wx")
        c.add("get_def_cstr", {"op": "get_def_cstr", "map": entries, "key": cstr_for(c, key),
                               "defval": c.stream.choice([None, c.random_utf8()])})
        entries = c.random_map("wx", "wx")
        key = key_candidates(c, entries, "wx")
        c.add("get_def_cstr_wx", {"op": "get_def_cstr_wx", "map": entries, "key": cstr_for(c, key),
                                  "defval": c.random_units()})
        entries = c.random_map("wx", "wx")
        inp = {"op": "get_def_wx", "map": entries, "key": key_candidates(c, entries, "wx")}
        if c.stream.below(2):
            inp["defval"] = c.random_units()
        c.add("get_def_wx", inp)
    # Long keys and values at the adapter's bounds.
    long_key = [0x61] * MAX_WIDE
    c.add("get_opt_wx", {"op": "get_opt_wx", "value_type": "wx",
                         "map": [[long_key, long_key], [long_key[:-1], [1]]], "key": long_key})
    c.add("get_opt_cstr", {"op": "get_opt_cstr", "value_type": "int",
                           "map": [[long_key, 1]], "key": [0x61] * MAX_BYTES})
    c.add("get_def_wx", {"op": "get_def_wx", "map": [[[0x61], long_key]], "key": [0x62],
                         "defval": [-1] * MAX_WIDE})


def controls(c):
    wx_map = [[wide("k"), wide("v")]]
    good = {
        "from_utf8": {"op": "from_utf8", "text": []},
        "map_order": {"op": "map_order", "key_type": "int", "value_type": "int", "map": []},
        "get_opt_wx": {"op": "get_opt_wx", "value_type": "wx", "map": wx_map, "key": []},
        "get_opt_cstr": {"op": "get_opt_cstr", "value_type": "wx", "map": wx_map, "key": None},
        "get_opt": {"op": "get_opt", "key_type": "int", "value_type": "int", "map": [], "key": 0},
        "get_def_cstr": {"op": "get_def_cstr", "map": wx_map, "key": None, "defval": None},
        "get_def_cstr_wx": {"op": "get_def_cstr_wx", "map": wx_map, "key": None, "defval": []},
        "get_def_wx": {"op": "get_def_wx", "map": wx_map, "key": []},
        "get_def": {"op": "get_def", "key_type": "int", "value_type": "int", "map": [], "key": 0},
    }
    c.refuse("op", {}, "expected string: op")
    c.refuse("op", {"op": 1}, "expected string: op")
    c.refuse("op", {"op": None}, "expected string: op")
    c.refuse("op", {"op": "get_value"}, "unknown op")
    c.refuse("op", {"op": "GET_OPT"}, "unknown op")
    for op, inp in good.items():
        extra = dict(inp, extra=1)
        c.refuse("fields", extra, "unexpected or missing field")
        for field in inp:
            if field != "op":
                missing = {k: v for k, v in inp.items() if k != field}
                c.refuse("fields", missing, "unexpected or missing field")
        if "key_type" in inp:
            for bad in ("wxString", 1, None, "INT"):
                c.refuse("type", dict(inp, key_type=bad), "expected type: key_type")
        if "value_type" in inp:
            for bad in ("double", 0, None):
                c.refuse("type", dict(inp, value_type=bad), "expected type: value_type")
        if "map" in inp:
            for bad in ({}, None, 1, [[1, 2]] * (MAX_ENTRIES + 1)):
                c.refuse("map", dict(inp, map=bad), "expected map")
            for bad in ([[1]], [[1, 2, 3]], [1], [{}]):
                c.refuse("map", dict(inp, map=bad), "expected map entry")
    for op in ("get_opt", "get_def", "map_order"):
        base = good[op]
        for bad in (I32_MAX + 1, I32_MIN - 1, 1.0, 1.5, "1", True, None, 1e2, 18446744073709551616):
            c.refuse("int", dict(base, map=[[bad, 0]]), "expected int")
            c.refuse("int", dict(base, map=[[0, bad]]), "expected int")
            if op != "map_order":
                c.refuse("int", dict(base, key=bad), "expected int")
        if op == "get_def":
            c.refuse("int", dict(base, defval=2.0), "expected int")
    for op in ("get_opt_wx", "get_def_wx", "get_def_cstr_wx"):
        base = good[op]
        for bad in ("k", None, 1, [I32_MAX + 1], [I32_MIN - 1], [0.5], [True], [None],
                    [0] * (MAX_WIDE + 1)):
            c.refuse("wide", dict(base, map=[[bad, []]]), "expected wide string")
            c.refuse("wide", dict(base, map=[[[], bad]]), "expected wide string")
            if op == "get_opt_wx":
                c.refuse("wide", dict(base, key=bad), "expected wide string")
            else:
                c.refuse("wide", dict(base, defval=bad), "expected wide string")
    for op, field in (("from_utf8", "text"), ("get_opt_cstr", "key"), ("get_def_cstr", "key"),
                      ("get_def_cstr", "defval"), ("get_def_cstr_wx", "key")):
        for bad in ("abc", 1, {}, [256], [-1], [1.0], [True], [None], [0] * (MAX_BYTES + 1)):
            c.refuse("bytes", dict(good[op], **{field: bad}), "expected bytes")
    # Validation order: the map is parsed before the key, the key before the default.
    c.refuse("order", {"op": "get_def_cstr", "map": [[1]], "key": 1, "defval": 1}, "expected map entry")
    c.refuse("order", {"op": "get_def_cstr", "map": wx_map, "key": 1, "defval": []}, "expected bytes")
    c.refuse("order", {"op": "get_def", "key_type": "int", "value_type": "wx", "map": [], "key": "x",
                       "defval": 1}, "expected int")
    c.refuse("order", {"op": "get_def", "key_type": "int", "value_type": "wx", "map": [], "key": 0,
                       "defval": 1}, "expected wide string")
    c.refuse("order", {"op": "get_opt", "key_type": "x", "value_type": "y", "map": 1, "key": 1},
             "expected type: key_type")
    c.refuse("order", {"op": "map_order", "key_type": "wx", "value_type": "wx", "map": [[[], 1]]},
             "expected wide string")


def dumps(value):
    return json.dumps(value, separators=(",", ":"), ensure_ascii=False)


def build():
    c = Corpus()
    utf8_boundaries(c)
    caller_cases(c)
    cstr_boundaries(c)
    generic_cases(c)
    controls(c)
    ids = [case["case"] for case in c.cases]
    assert len(ids) == len(set(ids))
    cases = "".join(dumps(case) + "\n" for case in c.cases)
    fixtures = "".join(dumps(control) + "\n" for control in c.controls)
    expected = json.dumps({"cases": len(c.cases), "compared": len(c.cases), "deferred": 0,
                           "controls": len(c.controls)}, indent=1) + "\n"
    return {CASES: cases, CONTROLS: fixtures, EXPECTED: expected}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify the committed files")
    args = parser.parse_args()
    outputs = build()
    if args.check:
        stale = [str(p.relative_to(ROOT)) for p, text in outputs.items()
                 if not p.exists() or p.read_bytes() != text.encode("utf-8")]
        if stale:
            raise SystemExit(f"stale generated files: {stale}")
        print("generated corpus is current")
        return
    for path, text in outputs.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(text.encode("utf-8"))
        print(f"wrote {path.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
