#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fixed-seed public corpus and refusal controls for
libs.kiplatform.include.kiplatform.application.pure.p001.

Writes port/cases/<unit>.jsonl (requests only; every one is inside the defined domain of
the header code it reaches: aggregates are value-initialized or have default member
initializers, MAPPED_FILE states are ones the pinned Unix constructor and readIntoBuffer
can leave) and port/oracle/fixtures/kiplatform-include-p001-controls.jsonl (malformed
requests both adapters must refuse, with the expected refusal). It never computes a
reference reply. Inputs are boundary values and a splitmix64 stream with a fixed seed.
`--check` verifies the committed files.
"""
import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
UNIT = "libs.kiplatform.include.kiplatform.application.pure.p001"
CASES = ROOT / "port/cases" / (UNIT + ".jsonl")
CONTROLS = ROOT / "port/oracle/fixtures/kiplatform-include-p001-controls.jsonl"
EXPECTED = ROOT / "rust/kicad-kiplatform-include-shim/expected-replies.json"
I32_MIN, I32_MAX, U32_MAX = -(1 << 31), (1 << 31) - 1, (1 << 32) - 1
U64_MAX = (1 << 64) - 1
MAX_UNITS, MAX_BYTES, MAX_ENTRIES, HANDLER_SLOTS = 4096, 4096, 64, 8
# Python cannot serialize the JSON number -0; this token is replaced after encoding.
NEG0 = "\x00neg0"
FUNCTIONS = ("atomic_rename", "open_unique_sibling_temp_file", "commit_temp_file",
             "atomic_write_file", "store_secret_async", "get_native_window_handles")
STRING_COUNT = {"atomic_rename": 2, "open_unique_sibling_temp_file": 2, "commit_temp_file": 2,
                "atomic_write_file": 1, "store_secret_async": 3, "get_native_window_handles": 0}
HAS_RETURNED = {"atomic_rename", "commit_temp_file", "atomic_write_file"}

# IEEE-754 doubles as bit patterns: signed zeros, subnormal and normal limits,
# infinities, quiet and signalling NaNs with payloads.
DOUBLES = ("0000000000000000", "8000000000000000", "3ff0000000000000", "bff0000000000000",
           "0000000000000001", "800fffffffffffff", "0010000000000000", "7fefffffffffffff",
           "ffefffffffffffff", "7ff0000000000000", "fff0000000000000", "7ff8000000000000",
           "fff8000000000123", "7ff0000000000001", "7ff4000000000000", "fff7ffffffffffff",
           "3fb999999999999a", "400921fb54442d18")
UNITS = (0, 1, -1, 0x41, 0x7F, 0x80, 0xFF, 0x100, 0xD800, 0xDBFF, 0xDC00, 0xDFFF, 0xFFFD,
         0xFFFF, 0x10000, 0x10FFFF, 0x110000, I32_MIN, I32_MAX)


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

    def i32(self):
        return self.choice((I32_MIN, -1, 0, 1, I32_MAX, (self.word() >> 32) - (1 << 31)))

    def unit(self):
        return self.choice((self.choice(UNITS), self.below(0x80), self.below(0x110000),
                            (self.word() >> 32) - (1 << 31)))

    def units(self):
        return [self.unit() for _ in range(self.choice((0, 1, 2, 7, 64, self.below(300))))]

    def bytes(self):
        n = self.choice((0, 1, 2, 15, 16, 255, 256, self.below(1024)))
        return bytes(self.below(256) for _ in range(n)).hex()

    def double(self):
        return self.choice((self.choice(DOUBLES), f"{self.word():016x}"))

    def gesture(self):
        return gesture(self.double(), self.double(), self.double(), self.i32(), self.i32())


def gesture(pan_x, pan_y, zoom, x, y):
    return {"pan_x": pan_x, "pan_y": pan_y, "zoom_factor": zoom, "zoom_anchor": [x, y]}


def text(value):
    return [ord(c) for c in value]


def default_call(function, strings, explicit, returned=False, data=""):
    payload = {"op": "default_arguments", "function": function, "explicit": explicit}
    if STRING_COUNT[function]:
        payload["strings"] = strings
    if function == "atomic_write_file":
        payload["data"] = data
    if function in HAS_RETURNED:
        payload["returned"] = returned
    return payload


class Corpus:
    def __init__(self):
        self.cases = []
        self.controls = []

    def case(self, name, payload):
        self.cases.append({"unit": UNIT, "case": f"{name}-{len(self.cases):05d}", "input": payload})

    def refuse(self, name, payload, message):
        self.controls.append({
            "request": {"unit": UNIT, "case": f"refuse-{name}-{len(self.controls):05d}",
                        "input": payload},
            "error": {"kind": "invalid_input", "message": message}})


def build():
    corpus = Corpus()
    case = corpus.case

    # Constants, enumerators and initialization of every header aggregate.
    case("constants", {"op": "constants"})
    case("value-init", {"op": "value_init"})
    case("constants-repeat", {"op": "constants"})
    case("value-init-repeat", {"op": "value_init"})

    # TARGET_ATTRS: brace initialization with and without the defaulted member, and a copy.
    for value in (0, 1, 2, 0x7FFFFFFF, 0x80000000, U32_MAX - 1, U32_MAX, NEG0):
        case("target-attrs", {"op": "target_attrs", "value": value})
        for captured in (False, True):
            case("target-attrs", {"op": "target_attrs", "value": value, "captured": captured})

    # PROXY_CONFIG: default construction, member assignment and the implicit copy.
    case("proxy-config", {"op": "proxy_config", "host": [], "username": [], "password": []})
    case("proxy-config", {"op": "proxy_config", "host": text("proxy.example.org:3128"),
                          "username": text("user"), "password": text("päss\u0000word")})
    case("proxy-config", {"op": "proxy_config", "host": list(UNITS), "username": [NEG0, 0],
                          "password": list(reversed(UNITS))})
    case("proxy-config", {"op": "proxy_config", "host": [0x41] * MAX_UNITS,
                          "username": [I32_MIN] * MAX_UNITS, "password": [I32_MAX] * MAX_UNITS})
    for count in (1, 15, 16, 23, 24, 31, 32):
        case("proxy-config-length", {"op": "proxy_config", "host": [0x61] * count,
                                     "username": [0x62] * (count - 1), "password": [0] * count})

    # TOUCHPAD_GESTURE_CALLBACK: empty and set functions, every special double, anchor bounds.
    zero = gesture(*DOUBLES[:3], 0, 0)
    for kind in ("set", "empty"):
        case("touchpad-callback", {"op": "touchpad_callback", "callback": kind, "gestures": []})
        case("touchpad-callback", {"op": "touchpad_callback", "callback": kind, "gestures": [zero]})
        case("touchpad-callback", {"op": "touchpad_callback", "callback": kind,
                                   "gestures": [gesture(d, d, d, I32_MIN, I32_MAX) for d in DOUBLES]})
    for index, bits in enumerate(DOUBLES):
        rotated = DOUBLES[(index + 1) % len(DOUBLES)], DOUBLES[(index + 2) % len(DOUBLES)]
        case("touchpad-callback-double", {"op": "touchpad_callback", "callback": "set",
                                          "gestures": [gesture(bits, *rotated, index - 9, 9 - index)]})
    for x, y in ((I32_MIN, I32_MIN), (I32_MAX, I32_MAX), (I32_MIN, I32_MAX), (-1, 1), (NEG0, NEG0)):
        case("touchpad-callback-anchor", {"op": "touchpad_callback", "callback": "set",
                                          "gestures": [gesture(*DOUBLES[:3], x, y)]})
    case("touchpad-callback-max", {"op": "touchpad_callback", "callback": "set",
                                   "gestures": [gesture(DOUBLES[i % len(DOUBLES)],
                                                        f"{i:016x}", DOUBLES[-1 - i % len(DOUBLES)],
                                                        i, -i) for i in range(MAX_ENTRIES)]})
    case("touchpad-callback-max-empty", {"op": "touchpad_callback", "callback": "empty",
                                         "gestures": [zero] * MAX_ENTRIES})

    # TOUCHPAD_GESTURE_HANDLER: destruction through owners of the base class.
    def step(action, slot):
        return {"action": action, "slot": slot}

    case("touchpad-handlers", {"op": "touchpad_handlers", "steps": []})
    case("touchpad-handlers", {"op": "touchpad_handlers", "steps": [step("release", 0)]})
    case("touchpad-handlers", {"op": "touchpad_handlers", "steps": [step("create", 0)]})
    case("touchpad-handlers", {"op": "touchpad_handlers",
                               "steps": [step("create", 0), step("release", 0)]})
    case("touchpad-handlers", {"op": "touchpad_handlers",
                               "steps": [step("create", 3), step("create", 3), step("create", 3)]})
    case("touchpad-handlers", {"op": "touchpad_handlers",
                               "steps": [step("create", s) for s in reversed(range(HANDLER_SLOTS))]})
    case("touchpad-handlers", {"op": "touchpad_handlers",
                               "steps": [step("create", s) for s in range(HANDLER_SLOTS)]
                               + [step("release", s) for s in (5, 0, 7, 0)]})
    case("touchpad-handlers", {"op": "touchpad_handlers",
                               "steps": [step("create", NEG0), step("release", NEG0)]})
    case("touchpad-handlers-max", {"op": "touchpad_handlers",
                                   "steps": [step("create" if i % 3 else "release", (i * 5) % 8)
                                             for i in range(MAX_ENTRIES)]})

    # MAPPED_FILE: Data() and Size() in each state the Unix constructor can leave.
    every = bytes(range(256)).hex()
    case("mapped-file", {"op": "mapped_file", "state": "empty_file", "bytes": ""})
    case("mapped-file", {"op": "mapped_file", "state": "fallback", "bytes": ""})
    for data in ("00", "ff", "0000", every, "41" * MAX_BYTES, (b"\x00\xff" * (MAX_BYTES // 2)).hex()):
        for state in ("mapped", "fallback"):
            case("mapped-file", {"op": "mapped_file", "state": state, "bytes": data})

    # Default arguments: each defaulted parameter omitted and passed explicitly.
    sample = [text("/tmp/a"), text("/tmp/b"), text("wéb")]
    for function in FUNCTIONS:
        strings = sample[:STRING_COUNT[function]]
        for explicit in (False, True):
            for returned in ((False, True) if function in HAS_RETURNED else (False,)):
                case("default-arguments", default_call(function, strings, explicit, returned,
                                                       "00ff" if function == "atomic_write_file" else ""))
    for function in FUNCTIONS[:5]:
        count = STRING_COUNT[function]
        for strings in ([[]] * count, [list(UNITS)] * count, [[0x5A] * MAX_UNITS] * count,
                        [[NEG0, I32_MIN], [I32_MAX], [0]][:count]):
            case("default-arguments-strings", default_call(function, strings, False, True,
                                                           every if function == "atomic_write_file" else ""))
    for data in ("", "00", every, "ab" * MAX_BYTES):
        for explicit in (False, True):
            case("default-arguments-data", default_call("atomic_write_file", [text("t")], explicit,
                                                        explicit, data))

    # Fixed-seed requests across every operation.
    stream = Stream(0x4B49504C54464F52)
    for _ in range(96):
        op = stream.choice(("target_attrs", "proxy_config", "touchpad_callback",
                            "touchpad_handlers", "mapped_file", "default_arguments"))
        if op == "target_attrs":
            payload = {"op": op, "value": stream.word() >> 32}
            if stream.below(3):
                payload["captured"] = bool(stream.below(2))
        elif op == "proxy_config":
            payload = {"op": op, "host": stream.units(), "username": stream.units(),
                       "password": stream.units()}
        elif op == "touchpad_callback":
            payload = {"op": op, "callback": stream.choice(("set", "set", "empty")),
                       "gestures": [stream.gesture() for _ in range(stream.below(MAX_ENTRIES + 1))]}
        elif op == "touchpad_handlers":
            payload = {"op": op, "steps": [step(stream.choice(("create", "create", "release")),
                                                stream.below(HANDLER_SLOTS))
                                           for _ in range(stream.below(MAX_ENTRIES + 1))]}
        elif op == "mapped_file":
            state = stream.choice(("empty_file", "mapped", "fallback"))
            data = "" if state == "empty_file" else stream.bytes()
            if state == "mapped" and not data:
                data = "5a"
            payload = {"op": op, "state": state, "bytes": data}
        else:
            function = stream.choice(FUNCTIONS)
            payload = default_call(function, [stream.units() for _ in range(STRING_COUNT[function])],
                                   bool(stream.below(2)), bool(stream.below(2)), stream.bytes())
        case(f"seeded-{op.replace('_', '-')}", payload)

    build_controls(corpus)
    return corpus


def build_controls(corpus):
    refuse = corpus.refuse
    ok_gesture = gesture(*DOUBLES[:3], 1, 2)

    refuse("op", {}, "expected string: op")
    refuse("op", {"op": 1}, "expected string: op")
    refuse("op", {"op": None}, "expected string: op")
    refuse("op", {"op": "Constants"}, "unknown operation")
    refuse("op", {"op": ""}, "unknown operation")
    refuse("op", {"op": "file_lock"}, "unknown operation")
    for op in ("constants", "value_init"):
        refuse("keys", {"op": op, "extra": 1}, "unexpected or missing field")

    # target_attrs
    refuse("target-attrs", {"op": "target_attrs"}, "unexpected or missing field")
    refuse("target-attrs", {"op": "target_attrs", "value": 0, "extra": 1}, "unexpected or missing field")
    refuse("target-attrs", {"op": "target_attrs", "captured": True}, "unexpected or missing field")
    refuse("target-attrs", {"op": "target_attrs", "value": 0, "captured": True, "extra": 1},
           "unexpected or missing field")
    for bad in (-1, U32_MAX + 1, U64_MAX, 1.0, 0.5, "1", None, True, 1e30):
        refuse("target-attrs-value", {"op": "target_attrs", "value": bad}, "integer out of range: value")
    for bad in (0, 1, "true", None, []):
        refuse("target-attrs-captured", {"op": "target_attrs", "value": 0, "captured": bad},
               "expected boolean: captured")
    refuse("target-attrs-order", {"op": "target_attrs", "value": -1, "captured": 0},
           "integer out of range: value")

    # proxy_config
    proxy = {"op": "proxy_config", "host": [], "username": [], "password": []}
    for key in ("host", "username", "password"):
        missing = dict(proxy)
        del missing[key]
        refuse("proxy-keys", missing, "unexpected or missing field")
        refuse("proxy-field", {**proxy, key: "abc"}, f"expected array: {key}")
        refuse("proxy-field", {**proxy, key: {}}, f"expected array: {key}")
        refuse("proxy-field", {**proxy, key: [0] * (MAX_UNITS + 1)}, f"too many entries: {key}")
        for bad in (I32_MAX + 1, I32_MIN - 1, 1.5, "a", None, [65]):
            refuse("proxy-unit", {**proxy, key: [65, bad]}, f"integer out of range: {key}")
    refuse("proxy-keys", {**proxy, "extra": []}, "unexpected or missing field")
    refuse("proxy-order", {**proxy, "host": [I32_MAX + 1], "username": 1, "password": 1},
           "integer out of range: host")
    refuse("proxy-order", {**proxy, "username": 1, "password": 1}, "expected array: username")

    # touchpad_callback
    callback = {"op": "touchpad_callback", "callback": "set", "gestures": [ok_gesture]}
    refuse("callback-keys", {"op": "touchpad_callback", "callback": "set"}, "unexpected or missing field")
    refuse("callback-keys", {**callback, "extra": 1}, "unexpected or missing field")
    for bad, message in ((None, "expected string: callback"), (1, "expected string: callback"),
                         ("Set", "unknown callback"), ("", "unknown callback"),
                         ("null", "unknown callback")):
        refuse("callback-kind", {**callback, "callback": bad}, message)
    refuse("callback-order", {**callback, "callback": "x", "gestures": 1}, "unknown callback")
    refuse("callback-gestures", {**callback, "gestures": {}}, "expected array: gestures")
    refuse("callback-gestures", {**callback, "gestures": [ok_gesture] * (MAX_ENTRIES + 1)},
           "too many entries: gestures")
    refuse("callback-gestures", {**callback, "gestures": [1]}, "unexpected or missing field: gestures")
    missing = dict(ok_gesture)
    del missing["zoom_anchor"]
    refuse("callback-gestures", {**callback, "gestures": [missing]}, "unexpected or missing field: gestures")
    refuse("callback-gestures", {**callback, "gestures": [{**ok_gesture, "panX": DOUBLES[0]}]},
           "unexpected or missing field: gestures")
    for field in ("pan_x", "pan_y", "zoom_factor"):
        for bad in ("3FF0000000000000", "3ff000000000000", "3ff00000000000000", "0x3ff0000000000",
                    1.0, None, " 3ff000000000000"):
            refuse("callback-double", {**callback, "gestures": [ok_gesture, {**ok_gesture, field: bad}]},
                   f"invalid double: gestures.{field}")
    for bad in ([1], [1, 2, 3], {}, None, "1,2"):
        refuse("callback-anchor", {**callback, "gestures": [{**ok_gesture, "zoom_anchor": bad}]},
               "expected point: gestures.zoom_anchor")
    for bad in ([I32_MAX + 1, 0], [0, I32_MIN - 1], [0.5, 0], [0, "1"], [None, 0]):
        refuse("callback-anchor", {**callback, "gestures": [{**ok_gesture, "zoom_anchor": bad}]},
               "integer out of range: gestures.zoom_anchor")
    refuse("callback-gesture-order", {**callback, "gestures": [{**ok_gesture, "pan_y": 1, "zoom_anchor": 1}]},
           "invalid double: gestures.pan_y")

    # touchpad_handlers
    handlers = {"op": "touchpad_handlers", "steps": [{"action": "create", "slot": 0}]}
    refuse("handlers-keys", {"op": "touchpad_handlers"}, "unexpected or missing field")
    refuse("handlers-keys", {**handlers, "extra": 1}, "unexpected or missing field")
    refuse("handlers-steps", {**handlers, "steps": "create"}, "expected array: steps")
    refuse("handlers-steps", {**handlers, "steps": [{"action": "create", "slot": 0}] * (MAX_ENTRIES + 1)},
           "too many entries: steps")
    refuse("handlers-steps", {**handlers, "steps": [[]]}, "unexpected or missing field: steps")
    refuse("handlers-steps", {**handlers, "steps": [{"action": "create"}]}, "unexpected or missing field: steps")
    refuse("handlers-steps", {**handlers, "steps": [{"action": "create", "slot": 0, "id": 0}]},
           "unexpected or missing field: steps")
    for bad, message in ((1, "expected string: action"), (None, "expected string: action"),
                         ("destroy", "unknown action"), ("Create", "unknown action")):
        refuse("handlers-action", {**handlers, "steps": [{"action": bad, "slot": 0}]}, message)
    for bad in (-1, HANDLER_SLOTS, U64_MAX, 0.0, "0", None):
        refuse("handlers-slot", {**handlers, "steps": [{"action": "release", "slot": bad}]},
               "integer out of range: steps.slot")
    refuse("handlers-order", {**handlers, "steps": [{"action": "x", "slot": -1}]}, "unknown action")

    # mapped_file
    mapped = {"op": "mapped_file", "state": "mapped", "bytes": "00"}
    refuse("mapped-keys", {"op": "mapped_file", "state": "mapped"}, "unexpected or missing field")
    refuse("mapped-keys", {**mapped, "size": 1}, "unexpected or missing field")
    for bad, message in ((1, "expected string: state"), ("Mapped", "unknown state"),
                         ("error", "unknown state"), ("", "unknown state")):
        refuse("mapped-state", {**mapped, "state": bad}, message)
    for bad in (None, 0, "0", "0g", "0A", "00 ", "41" * (MAX_BYTES + 1)):
        refuse("mapped-bytes", {**mapped, "bytes": bad}, "invalid hex: bytes")
    refuse("mapped-domain", {**mapped, "state": "empty_file", "bytes": "00"}, "empty_file state has no bytes")
    refuse("mapped-domain", {**mapped, "bytes": ""}, "mapped state needs at least one byte")
    refuse("mapped-order", {**mapped, "state": "x", "bytes": 1}, "unknown state")
    refuse("mapped-order", {**mapped, "state": "empty_file", "bytes": "0"}, "invalid hex: bytes")

    # default_arguments
    refuse("default-function", {"op": "default_arguments"}, "expected string: function")
    refuse("default-function", {"op": "default_arguments", "function": 1}, "expected string: function")
    for bad in ("AtomicRename", "", "make_sibling_temp_path"):
        refuse("default-function", {"op": "default_arguments", "function": bad, "explicit": True},
               "unknown function")
    for function in FUNCTIONS:
        good = default_call(function, [[0x61]] * STRING_COUNT[function], True, True, "00")
        refuse("default-keys", {**good, "extra": 1}, "unexpected or missing field")
        for key in good:
            if key not in ("op", "function"):
                missing = dict(good)
                del missing[key]
                refuse("default-keys", missing, "unexpected or missing field")
        refuse("default-explicit", {**good, "explicit": 1}, "expected boolean: explicit")
        if function in HAS_RETURNED:
            refuse("default-returned", {**good, "returned": "true"}, "expected boolean: returned")
            refuse("default-order", {**good, "explicit": None, "returned": None}, "expected boolean: explicit")
        count = STRING_COUNT[function]
        if count:
            refuse("default-strings", {**good, "strings": "a"}, "expected array: strings")
            refuse("default-strings", {**good, "strings": [[]] * (count + 1)}, "too many entries: strings")
            refuse("default-strings", {**good, "strings": [[]] * (count - 1)}, "wrong string count")
            refuse("default-strings", {**good, "strings": [[]] * (count - 1) + [[I32_MAX + 1]]},
                   "integer out of range: strings")
            refuse("default-strings", {**good, "strings": [[]] * (count - 1) + [{}]}, "expected array: strings")
            refuse("default-strings", {**good, "strings": [[]] * (count - 1) + [[0] * (MAX_UNITS + 1)]},
                   "too many entries: strings")
            refuse("default-order", {**good, "strings": [[]] * (count - 1) + [[-(1 << 40)]],
                                     "explicit": None}, "integer out of range: strings")
        if function == "atomic_write_file":
            for bad in ("0", "zz", None, "ab" * (MAX_BYTES + 1)):
                refuse("default-data", {**good, "data": bad}, "invalid hex: data")
            refuse("default-order", {**good, "data": "0", "explicit": None}, "invalid hex: data")
            refuse("default-order", {**good, "strings": [[0.5]], "data": "0"}, "integer out of range: strings")


def lines(records):
    token = json.dumps(NEG0)
    return "".join(json.dumps(r, separators=(",", ":"), ensure_ascii=False).replace(token, "-0")
                   + "\n" for r in records)


def outputs():
    corpus = build()
    expected = {"cases": len(corpus.cases), "compared": len(corpus.cases), "deferred": 0,
                "controls": len(corpus.controls)}
    return {CASES: lines(corpus.cases), CONTROLS: lines(corpus.controls),
            EXPECTED: json.dumps(expected, indent=2) + "\n"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    stale = []
    for path, content in outputs().items():
        if args.check:
            if not path.exists() or path.read_bytes() != content.encode("utf-8"):
                stale.append(str(path.relative_to(ROOT)))
        else:
            path.write_bytes(content.encode("utf-8"))
    if stale:
        raise SystemExit("stale generated files: " + ", ".join(stale))


if __name__ == "__main__":
    main()
