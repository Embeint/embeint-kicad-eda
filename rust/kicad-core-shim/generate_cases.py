#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fixed-seed public corpus and refusal controls for libs.core.application.pure.p001.

Writes port/cases/<unit>.jsonl (requests only; every one is inside the reference's
defined domain) and port/oracle/fixtures/core-p001-controls.jsonl (requests that
both adapters must refuse, with the expected refusal). It never computes a
reference reply. Inputs are built from the upstream QA vectors
(qa/tests/common/test_base64.cpp, test_observable.cpp), boundary values and a
splitmix64 stream with a fixed seed. `--check` verifies the committed files.
"""
import argparse
import base64
import json
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[2]
UNIT = "libs.core.application.pure.p001"
CASES = ROOT / "port/cases" / (UNIT + ".jsonl")
CONTROLS = ROOT / "port/oracle/fixtures/core-p001-controls.jsonl"
I64_MIN, I64_MAX, U64_MAX = -(1 << 63), (1 << 63) - 1, (1 << 64) - 1


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

    def bytes(self, n):
        return bytes(self.below(256) for _ in range(n))


def hexs(data):
    return bytes(data).hex()


def bits(value):
    return struct.pack(">d", value).hex()


class Corpus:
    def __init__(self):
        self.cases = []
        self.controls = []
        self.counts = {}

    def add(self, group, inp):
        n = self.counts.get(group, 0)
        self.counts[group] = n + 1
        self.cases.append({"unit": UNIT, "case": f"{group}-{n:05d}", "input": inp})

    def refuse(self, group, inp, message):
        n = self.counts.get("refuse-" + group, 0)
        self.counts["refuse-" + group] = n + 1
        request = {"unit": UNIT, "case": f"refuse-{group}-{n:05d}", "input": inp}
        self.controls.append({"request": request, "error": {"kind": "invalid_input", "message": message}})


def terminates(text):
    """The reference loop's terminating domain (see the adapters)."""
    raw = text.encode()
    for i, c in enumerate(raw):
        digit = 48 <= c <= 57
        dash = c == 45 and i + 1 < len(raw) and 48 <= raw[i + 1] <= 57
        if not (digit or dash or c == 46):
            return False
    return True


def lengths(c, rng):
    values = list(range(0, 66)) + [(1 << 32) + d for d in range(-3, 4)]
    values += [(1 << 63) + d for d in range(-3, 4)] + [U64_MAX - d for d in range(0, 8)]
    values += [rng.word() for _ in range(256)]
    for op in ("encode_length", "decode_length"):
        for value in values:
            c.add(op, {"op": op, "len": value})


def encodes(c, rng):
    qa = []
    for length in range(0, 64):
        qa.append(bytes((i * 37 + 11) % 256 for i in range(length)))
    for data in qa:
        c.add("encode", {"op": "encode", "input": hexs(data)})
    for b in range(256):
        c.add("encode", {"op": "encode", "input": hexs([b])})
        c.add("encode", {"op": "encode", "input": hexs([b, 255 - b])})
        c.add("encode", {"op": "encode", "input": hexs([b, b, (b * 7) % 256])})
    for _ in range(192):
        c.add("encode", {"op": "encode", "input": hexs(rng.bytes(rng.below(300)))})
    for _ in range(32):
        c.add("encode", {"op": "encode", "input": hexs(rng.bytes(rng.below(20))),
                         "output": hexs(rng.bytes(1 + rng.below(9)))})
    block = bytes(range(256)) * 256  # 65536 bytes, 65536 % 3 == 1
    for repeat in (1, 2, 3, 16):  # remainders 1, 2, 0 and the 1 MiB maximum
        c.add("encode", {"op": "encode", "input": hexs(block), "repeat": repeat})
    c.add("encode", {"op": "encode", "input": "", "repeat": 65536})
    c.add("encode", {"op": "encode", "input": "", "output": "00ff"})


def decodes(c, rng):
    for text in ["", "TWFu", "TWE", "TQ", "TWE=", "TQ==", "aGVsbG8gd29ybGQ"]:
        c.add("decode", {"op": "decode", "input": hexs(text.encode())})
    # QA round trip inputs, unpadded and padded, with up to five extra '='.
    for length in range(0, 64):
        data = bytes((i * 37 + 11) % 256 for i in range(length))
        padded = base64.b64encode(data)
        bare = padded.rstrip(b"=")
        for pad in range(0, 6):
            c.add("decode", {"op": "decode", "input": hexs(bare + b"=" * pad)})
        c.add("decode", {"op": "decode", "input": hexs(padded)})
    for text in [b"=", b"==", b"====", b"=" * 9, b"TQ==TQ", b"T=Q=", b"=TQ", b"TQ=a", b"TWFu=TWFu",
                 b"T", b"TWFuT", b"TWFuTWFuT", b"TR", b"TWF", b"TWG", b"TQ=", b"-_", b"QUJD-_-_",
                 b"\xc1\xc2\xc3\xc4", b"\xff\xff\xff\xff", b"\x00\x00", b"QUJD\nQUJD", b" QUJD"]:
        c.add("decode", {"op": "decode", "input": hexs(text)})
    # Every byte value in every position of a full block and of two- and three-byte tails.
    for position in range(4):
        for b in range(256):
            block = bytearray(b"QUJD")
            block[position] = b
            c.add("decode", {"op": "decode", "input": hexs(block)})
    for tail in (b"QU", b"QUJ"):
        for position in range(len(tail)):
            for b in range(256):
                text = bytearray(b"QUJD" + tail)
                text[4 + position] = b
                c.add("decode", {"op": "decode", "input": hexs(text)})
    alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/="
    for _ in range(256):
        n = rng.below(40)
        if rng.below(2):
            text = bytes(rng.choice(alphabet) for _ in range(n))
        else:
            text = rng.bytes(n)
        c.add("decode", {"op": "decode", "input": hexs(text)})
    for _ in range(32):
        c.add("decode", {"op": "decode", "input": hexs(base64.b64encode(rng.bytes(rng.below(30)))),
                         "output": hexs(rng.bytes(1 + rng.below(9)))})
    c.add("decode", {"op": "decode", "input": hexs(b"QUJDRUZHSElKS0xN"), "repeat": 65536})
    c.add("decode", {"op": "decode", "input": hexs(b"QUJD" * 16383 + b"QUJ")})
    c.add("decode", {"op": "decode", "input": hexs(b"QUJD" * 16382 + b"QUJDQ"), "output": "01"})


def versions(c, rng):
    texts = ["", "0", "1", "1.2", "1.2.3", "1.2.3.4", "10.7.2", "8.0.0", "8.0", "9.99.123",
             "007", "007.000.010", "0.0.0", "1..2", ".1", "1.", "..", ".", "...", "1...2",
             "-1", "1-2", "1-2-3", "1.-2", "-0", "-00", "-0.-0", "2147483647", "2147483648",
             "-2147483648", "-2147483649", "4294967296", "99999999999999999999",
             "1.99999999999999999999.3", "0" * 1000 + "1", "9" * 1000, "-" + "9" * 1000,
             "1" * 65536, "1." * 4096, "." * 8192, "00000000000000000000002147483647",
             "-00000000000000000000002147483648", "2147483647.2147483648.-2147483648.-2147483649"]
    pieces = ["0", "1", "7", "12", "007", "65535", "2147483647", "2147483648", "-2147483648",
              "-1", "-0", "99999999999", ".", "..", ""]
    for _ in range(320):
        text = "".join(rng.choice(pieces) + rng.choice(["", ".", "-", ".."]) for _ in range(rng.below(6)))
        if terminates(text):
            texts.append(text)
    for text in texts:
        c.add("parse_version", {"op": "parse_version", "text": text})
    pairs = [("10.7.2", "10.7.2"), ("10.7.2", "10.7.3"), ("10.7.2", "10.8"), ("10.7.2", "10.7"),
             ("10.7", "10.7.2"), ("1.2.0", "1.2"), ("1.2", "1.2.0"), ("", ""), ("", "1"), ("1", ""),
             ("1", "1"), ("2", "10"), ("10", "2"), ("-1", "0"), ("0", "-1"), ("1.2147483648", "1.0"),
             ("1.0", "1.2147483648"), ("1..2", "1.0.2"), ("1.0.2", "1..2"), (".1", "0.1"), ("1.", "1")]
    short = [t for t in texts if len(t) <= 64]
    for _ in range(320):
        pairs.append((rng.choice(short), rng.choice(short)))
    for a, b in pairs:
        c.add("compare_versions", {"op": "compare_versions", "a": a, "b": b})


def clocks(c, rng):
    sec_max, sec_min = I64_MAX // 1_000_000, -(-I64_MIN // 1_000_000)
    values = [(0, 0), (0, 999), (0, 1000), (0, 999_999_999), (1, 0), (1, 999_999_999), (-1, 0),
              (-1, 999_999_999), (123456, 789012345), (sec_max, 0), (sec_max, 775_807_999),
              (sec_min, 0), (sec_min, 999_999_999), (sec_max - 1, 999_999_999)]
    for _ in range(256):
        values.append(((rng.word() % (2 * sec_max)) - sec_max, rng.below(1_000_000_000)))
    for sec, nsec in values:
        c.add("running_micro_secs", {"op": "running_micro_secs", "tv_sec": sec, "tv_nsec": nsec})


def probes(c, rng):
    deltas = ["0000000000000000", "8000000000000000", bits(0.0625), bits(0.1875), bits(0.0005),
              bits(1e21), bits(1e300), "7fefffffffffffff", "0000000000000001", "8000000000000001",
              bits(1.0005), bits(2.0625), bits(123.4565), bits(-2.5e-4), bits(-0.0004),
              bits(0.9995), bits(999999.9995), "7ff0000000000000", "fff0000000000000",
              "7ff8000000000000", "fff8000000000000", "7ff0000000000001"]
    while len(deltas) < 300:
        word = rng.word()
        if (word >> 52) & 0x7FF != 0x7FF:
            deltas.append(f"{word:016x}")
    names = ["", "a", "x" * 29, "y" * 30, "z" * 31, "w" * 200, "café", "測量" * 5,
             "nul\u0000byte", "tab\tline\n"]

    def timer(name, delta):
        return {"name": name, "timestamp": 0, "timer": True, "delta": delta}

    c.add("latency_probe", {"op": "latency_probe", "name": "", "start": 0, "checkpoints": []})
    for name in names:
        c.add("latency_probe", {"op": "latency_probe", "name": name, "start": 5, "checkpoints": []})
        c.add("latency_probe", {"op": "latency_probe", "name": "probe", "start": 0,
                                "checkpoints": [timer(name, bits(1.5))]})
    for start in range(0, len(deltas), 6):
        group = [timer(rng.choice(names), d) for d in deltas[start:start + 6]]
        c.add("latency_probe", {"op": "latency_probe", "name": "timers", "start": rng.word() - (1 << 63),
                                "checkpoints": group})
    # Checkpoint lines call LATENCY_PROBE::delta_ms (header unit): Rust defers these.
    stamps = [0, 1, 999, 1000, 1499, 1500, 1_000_000, 123_456_789, -5, I64_MAX, I64_MIN]
    for i in range(24):
        start = rng.choice([0, 1_000, -1_000, 1 << 40])
        cps, prev = [], start
        for j in range(1 + rng.below(4)):
            if rng.below(3) == 0:
                cps.append(timer(f"t{j}", rng.choice(deltas)))
                continue
            stamp = prev + rng.choice(stamps[:9]) if i % 2 else start + rng.choice(stamps[:9]) * (j + 1)
            cps.append({"name": f"c{j}", "timestamp": stamp, "timer": False, "delta": "0000000000000000"})
            prev = stamp
        c.add("latency_probe", {"op": "latency_probe", "name": f"mixed{i}", "start": start, "checkpoints": cps})
    c.add("latency_probe", {"op": "latency_probe", "name": "edge", "start": 0, "checkpoints": [
        {"name": "max", "timestamp": I64_MAX, "timer": False, "delta": "0000000000000000"}]})
    c.add("latency_probe", {"op": "latency_probe", "name": "edge", "start": -1, "checkpoints": [
        {"name": "min", "timestamp": I64_MIN, "timer": False, "delta": "0000000000000000"}]})


# ---- observable -------------------------------------------------------------------------------

def step(op, **fields):
    return {"op": op, **fields}


def notify(obs, event, reactions=None, ignore=False, ignored=None):
    value = {"op": "notify_ignore" if ignore else "notify", "obs": obs, "event": event,
             "reactions": [{"observer": o, "steps": s} for o, s in (reactions or {}).items()]}
    if ignore:
        value["ignore"] = ignored
    return value


def scenarios():
    s = []
    # qa/tests/common/test_observable.cpp: MoveAssignedLinkLeavesOldObservable.
    s.append([step("new", obs=0), step("new", obs=1), step("subscribe", obs=0, observer=0, link=0),
              step("subscribe", obs=1, observer=0, link=1), step("link_assign", link=0, from_=1),
              step("link_drop", link=1)])
    # Notification order and managed links.
    s.append([step("new", obs=0)] + [step("subscribe", obs=0, observer=o, link=o) for o in (2, 0, 1)]
             + [notify(0, 7), step("link_reset", link=0), notify(0, 8), step("link_drop", link=2), notify(0, 9)])
    # Unmanaged subscribers: Notify releases an unshared, unlinked list.
    s.append([step("new", obs=0), step("subscribe_unmanaged", obs=0, observer=3),
              step("subscribe_unmanaged", obs=0, observer=4), notify(0, 1), notify(0, 2),
              step("unsubscribe", obs=0, observer=3, guard=True), step("subscribe_unmanaged", obs=0, observer=3),
              step("unsubscribe", obs=0, observer=3), step("unsubscribe", obs=0, observer=3)])
    # Removal during notification: later (skipped), self, earlier; collected afterwards.
    s.append([step("new", obs=0)] + [step("subscribe", obs=0, observer=o, link=o) for o in range(4)]
             + [notify(0, 1, {0: [step("unsubscribe", obs=0, observer=2)],
                              1: [step("unsubscribe", obs=0, observer=1), step("unsubscribe", obs=0, observer=0)],
                              3: [step("link_reset", link=3)]}), notify(0, 2)])
    # Link operations inside callbacks.
    s.append([step("new", obs=0), step("new", obs=1)]
             + [step("subscribe", obs=0, observer=o, link=o) for o in range(3)]
             + [step("subscribe", obs=1, observer=5, link=5),
                notify(0, 3, {0: [step("link_assign", link=1, from_=5), step("link_drop", link=2)],
                              1: [step("link_reset", link=0)]}), notify(0, 4), notify(1, 5)])
    # Nested notification of the same observable and of a shared copy.
    s.append([step("new", obs=0), step("subscribe", obs=0, observer=0, link=0),
              step("subscribe", obs=0, observer=1, link=1), step("copy", obs=1, from_=0),
              notify(0, 1, {0: [notify(0, 2, {1: [step("unsubscribe", obs=0, observer=0)]}),
                                notify(1, 3)]}), notify(1, 4)])
    # NotifyIgnore never leaves the iteration: removals stay null and are never collected.
    s.append([step("new", obs=0)] + [step("subscribe", obs=0, observer=o, link=o) for o in range(3)]
             + [notify(0, 1, ignore=True, ignored=1), notify(0, 2, ignore=True, ignored=None),
                step("unsubscribe", obs=0, observer=0), notify(0, 3), step("subscribe_unmanaged", obs=0, observer=6),
                step("link_reset", link=1), notify(0, 4, ignore=True, ignored=7)])
    # Copy construction from no list (new shared list) and from an owned list (marked shared).
    s.append([step("new", obs=0), step("copy", obs=1, from_=0), step("subscribe_unmanaged", obs=1, observer=2),
              notify(0, 1), step("new", obs=2), step("subscribe_unmanaged", obs=2, observer=3),
              step("copy", obs=3, from_=2), step("drop", obs=2), notify(3, 2), step("copy", obs=4, from_=3),
              step("drop", obs=3), step("drop", obs=4), step("drop", obs=0)])
    # Duplicate and null observers.
    s.append([step("new", obs=0), step("subscribe_unmanaged", obs=0, observer=1),
              step("subscribe_unmanaged", obs=0, observer=None), step("subscribe", obs=0, observer=1, link=0),
              step("subscribe", obs=0, observer=None, link=1), step("unsubscribe", obs=0, observer=1),
              notify(0, 1), step("unsubscribe", obs=0, observer=None), step("link_reset", link=1),
              step("link_reset", link=0), notify(0, 2)])
    # LINK( token, observer ) with a null token and with an unsubscribed observer.
    s.append([step("new", obs=0), step("link_token", link=0, obs=0, observer=1), step("link_reset", link=0),
              step("subscribe_unmanaged", obs=0, observer=1), step("link_token", link=1, obs=0, observer=2),
              step("link_token", link=2, obs=0, observer=1), step("link_drop", link=1), notify(0, 1),
              step("link_drop", link=2), step("link_new", link=3), step("link_reset", link=3)])
    # on_observers_empty with links still holding the list; resets on the orphan list.
    s.append([step("new", obs=0), step("subscribe", obs=0, observer=0, link=0),
              step("subscribe", obs=0, observer=1, link=1), step("on_observers_empty", obs=0),
              notify(0, 1), step("link_reset", link=0), step("subscribe", obs=0, observer=2, link=2),
              step("drop", obs=0), step("link_drop", link=1), step("link_reset", link=2)])
    # Unpaired leave_iteration wraps the unsigned count; enter wraps it back.
    s.append([step("new", obs=0), step("enter_iteration", obs=0), step("leave_iteration", obs=0),
              step("subscribe", obs=0, observer=0, link=0), step("leave_iteration", obs=0),
              step("unsubscribe", obs=0, observer=0), step("enter_iteration", obs=0), step("enter_iteration", obs=0),
              step("unsubscribe", obs=0, observer=0), step("leave_iteration", obs=0),
              step("leave_iteration", obs=0), step("on_observers_empty", obs=0), step("leave_iteration", obs=0)])
    # Moves: the moved-from link keeps its observer value; self-assignment changes nothing.
    s.append([step("new", obs=0), step("subscribe", obs=0, observer=4, link=0), step("link_move", link=1, from_=0),
              step("link_reset", link=0), step("link_assign", link=1, from_=1), step("link_assign", link=0, from_=1),
              step("link_drop", link=1), step("link_drop", link=0), notify(0, 1)])
    # Adds and copies during notification where defined; guarded adds to the active list.
    s.append([step("new", obs=0), step("new", obs=1), step("new", obs=2),
              step("subscribe", obs=0, observer=0, link=0), step("subscribe", obs=2, observer=1, link=1),
              step("copy", obs=3, from_=2),
              notify(0, 1, {0: [step("subscribe_unmanaged", obs=1, observer=5),
                                step("subscribe_unmanaged", obs=0, observer=6, guard=True),
                                step("subscribe_unmanaged", obs=1, observer=7, guard=True),
                                notify(2, 2, {1: [step("subscribe_unmanaged", obs=3, observer=2, guard=True),
                                                  step("unsubscribe", obs=1, observer=5),
                                                  step("unsubscribe", obs=1, observer=5, guard=True)]})]}),
              notify(1, 3), notify(2, 5, {1: [step("unsubscribe", obs=3, observer=1), notify(3, 6)]})])
    # Drop another observable sharing the list during notification.
    s.append([step("new", obs=0), step("subscribe", obs=0, observer=0, link=0), step("copy", obs=1, from_=0),
              step("subscribe", obs=1, observer=1, link=1),
              notify(0, 1, {0: [step("unsubscribe", obs=1, observer=1)]}), step("drop", obs=1), notify(0, 2)])
    # Maximum nesting depth: the innermost notify is a step at depth 8.
    inner = []
    for depth in range(9, 0, -1):
        inner = [notify(0, depth, {0: inner})]
    s.append([step("new", obs=0), step("subscribe", obs=0, observer=0, link=0)] + inner + [notify(0, 0)])
    # Every slot.
    s.append([step("new", obs=k) for k in range(8)]
             + [step("subscribe", obs=k % 8, observer=k % 8, link=k) for k in range(16)]
             + [notify(k, k) for k in range(8)] + [step("link_drop", link=k) for k in range(16)]
             + [step("drop", obs=k) for k in range(8)])
    return s


def fix_from(value):
    if isinstance(value, list):
        return [fix_from(v) for v in value]
    if isinstance(value, dict):
        return {("from" if k == "from_" else k): fix_from(v) for k, v in value.items()}
    return value


def random_script(rng):
    """Random scripts; tracks only slot occupancy (script bookkeeping, not reference state)."""
    obs_present = [False] * 8
    link_present = [False] * 16
    steps = []

    def observer():
        return None if rng.below(10) == 0 else rng.below(8)

    def present(flags):
        return [i for i, f in enumerate(flags) if f]

    def absent(flags):
        return [i for i, f in enumerate(flags) if not f]

    def reaction_steps(depth):
        out = []
        for _ in range(rng.below(3) + (1 if depth == 1 else 0)):
            kind = rng.below(6)
            observables, links = present(obs_present), present(link_present)
            if kind == 0 and observables:
                out.append(step("unsubscribe", obs=rng.choice(observables), observer=observer(), guard=True))
            elif kind == 1 and links:
                out.append(step("link_reset", link=rng.choice(links)))
            elif kind == 2 and len(links) >= 1:
                out.append(step("link_assign", link=rng.choice(links), from_=rng.choice(links)))
            elif kind == 3 and observables and depth < 3:
                out.append(nested(rng.choice(observables), depth))
            elif kind == 4 and observables and rng.below(3) == 0:
                out.append(step("subscribe_unmanaged", obs=rng.choice(observables), observer=observer(), guard=True))
            elif kind == 5 and observables:
                out.append(step("unsubscribe", obs=rng.choice(observables), observer=observer(), guard=True))
        return out

    def nested(obs, depth):
        reactions = {}
        for o in sorted({rng.below(8) for _ in range(rng.below(3))}):
            reactions[o] = reaction_steps(depth + 1)
        ignore = rng.below(4) == 0
        return notify(obs, rng.below(2001) - 1000, reactions, ignore, observer() if ignore else None)

    for _ in range(4 + rng.below(20)):
        kind = rng.below(14)
        observables, links = present(obs_present), present(link_present)
        free_obs, free_links = absent(obs_present), absent(link_present)
        if kind <= 1 and free_obs:
            k = rng.choice(free_obs)
            if observables and rng.below(2):
                steps.append(step("copy", obs=k, from_=rng.choice(observables)))
            else:
                steps.append(step("new", obs=k))
            obs_present[k] = True
        elif kind == 2 and observables and rng.below(3) == 0:
            k = rng.choice(observables)
            steps.append(step("drop", obs=k))
            obs_present[k] = False
        elif kind <= 4 and observables and free_links:
            l = rng.choice(free_links)
            steps.append(step("subscribe", obs=rng.choice(observables), observer=observer(), link=l))
            link_present[l] = True
        elif kind == 5 and observables:
            steps.append(step("subscribe_unmanaged", obs=rng.choice(observables), observer=observer()))
        elif kind == 6 and observables:
            steps.append(step("unsubscribe", obs=rng.choice(observables), observer=observer(), guard=True))
        elif kind <= 8 and observables:
            steps.append(nested(rng.choice(observables), 0))
        elif kind == 9 and links:
            l = rng.choice(links)
            if rng.below(2):
                steps.append(step("link_drop", link=l))
                link_present[l] = False
            else:
                steps.append(step("link_reset", link=l))
        elif kind == 10 and links:
            steps.append(step("link_assign", link=rng.choice(links), from_=rng.choice(links)))
        elif kind == 11 and links and free_links:
            l = rng.choice(free_links)
            steps.append(step("link_move", link=l, from_=rng.choice(links)))
            link_present[l] = True
        elif kind == 12 and observables and free_links:
            l = rng.choice(free_links)
            steps.append(step("link_token", link=l, obs=rng.choice(observables), observer=observer()))
            link_present[l] = True
        elif kind == 13 and observables:
            steps.append(step(rng.choice(["enter_iteration", "leave_iteration", "on_observers_empty"]),
                              obs=rng.choice(observables)))
    return steps


def observables(c, rng):
    for script in scenarios():
        c.add("observable", {"op": "observable", "steps": fix_from(script)})
    for _ in range(400):
        c.add("observable", {"op": "observable", "steps": fix_from(random_script(rng))})


# ---- refusal controls ---------------------------------------------------------------------------

def refusals(c):
    for value, message in [(-1, "len"), (1 << 64, "len"), (1.0, "len"), ("1", "len"), (None, "len"), (True, "len")]:
        c.refuse("length", {"op": "encode_length", "len": value}, "integer out of range: " + message)
    for inp, message in [
            ({"op": "encode", "input": "0"}, "invalid hex: input"),
            ({"op": "encode", "input": "0A"}, "invalid hex: input"),
            ({"op": "encode", "input": "zz"}, "invalid hex: input"),
            ({"op": "decode", "input": 5}, "invalid hex: input"),
            ({"op": "decode", "input": "00", "output": "0"}, "invalid hex: output"),
            ({"op": "encode", "input": "00" * 65537}, "byte array too large: input"),
            ({"op": "encode", "input": "00", "repeat": 0}, "integer out of range: repeat"),
            ({"op": "encode", "input": "00", "repeat": 65537}, "integer out of range: repeat"),
            ({"op": "encode", "input": "00" * 17, "repeat": 65536}, "input too large"),
            ({"op": "decode", "input": "00" * 65536, "repeat": 17}, "input too large"),
            ({"op": "encode"}, "unexpected or missing field"),
            ({"op": "encode", "input": "", "extra": 1}, "unexpected or missing field"),
            ({}, "expected string: op"), ({"op": 3}, "expected string: op"), ({"op": "base64"}, "unknown operation")]:
        c.refuse("base64", inp, message)
    for text in ["a", "1.a", "-", "1-", "+1", " 1", "1 ", "1,2", "1_2", "1.2-rc1", "10.0.1-rc1", "\u0000",
                 "café", "--1", "1.-", "-.1", "1.2.3 "]:
        c.refuse("version", {"op": "parse_version", "text": text}, "version string does not terminate")
    c.refuse("version", {"op": "compare_versions", "a": "1", "b": "x"}, "version string does not terminate")
    c.refuse("version", {"op": "compare_versions", "a": "x", "b": "1"}, "version string does not terminate")
    c.refuse("version", {"op": "parse_version", "text": "1" * 65537}, "string too long: text")
    c.refuse("version", {"op": "parse_version", "text": 1}, "expected string: text")
    c.refuse("version", {"op": "compare_versions", "a": "1"}, "unexpected or missing field")
    sec_max, sec_min = I64_MAX // 1_000_000, -(-I64_MIN // 1_000_000)
    for sec, nsec, message in [(sec_max + 1, 0, "signed overflow"), (sec_max, 775_808_000, "signed overflow"),
                               (sec_min - 1, 999_999_999, "signed overflow"), (I64_MAX, 0, "signed overflow"),
                               (I64_MIN, 0, "signed overflow"), (0, -1, "invalid timespec: tv_nsec"),
                               (0, 1_000_000_000, "invalid timespec: tv_nsec"),
                               (0, I64_MIN, "invalid timespec: tv_nsec"), (1 << 63, 0, "integer out of range: tv_sec"),
                               (0.5, 0, "integer out of range: tv_sec")]:
        c.refuse("clock", {"op": "running_micro_secs", "tv_sec": sec, "tv_nsec": nsec}, message)
    zero = "0000000000000000"
    for inp, message in [
            ({"name": "p", "start": I64_MIN, "checkpoints": [{"name": "c", "timestamp": I64_MAX, "timer": False, "delta": zero}]},
             "time point difference overflows"),
            ({"name": "p", "start": 0, "checkpoints": [
                {"name": "c", "timestamp": I64_MIN, "timer": False, "delta": zero},
                {"name": "d", "timestamp": I64_MAX, "timer": False, "delta": zero}]}, "time point difference overflows"),
            ({"name": "p", "start": 1, "checkpoints": [{"name": "c", "timestamp": I64_MIN, "timer": False, "delta": zero}]},
             "time point difference overflows"),
            ({"name": "p", "start": 0, "checkpoints": [{"name": "c", "timestamp": 0, "timer": 1, "delta": zero}]},
             "expected boolean: timer"),
            ({"name": "p", "start": 0, "checkpoints": [{"name": "c", "timestamp": 0, "timer": True, "delta": "0"}]},
             "invalid bits: delta"),
            ({"name": "p", "start": 0, "checkpoints": [{"name": "c", "timestamp": 0, "timer": True, "delta": "7FF0000000000000"}]},
             "invalid bits: delta"),
            ({"name": "p", "start": 0, "checkpoints": [{"name": "c", "timer": True, "delta": zero}]},
             "unexpected or missing field"),
            ({"name": "p", "start": 0, "checkpoints": {}}, "expected array: checkpoints"),
            ({"name": "p", "start": 0, "checkpoints": [
                {"name": "c", "timestamp": 0, "timer": True, "delta": zero}] * 257}, "too many checkpoints"),
            ({"name": "p" * 65537, "start": 0, "checkpoints": []}, "string too long: name"),
            ({"name": "p", "start": 1.5, "checkpoints": []}, "integer out of range: start")]:
        c.refuse("probe", {"op": "latency_probe", **inp}, message)

    def obs_refuse(steps, message):
        c.refuse("observable", {"op": "observable", "steps": fix_from(steps)}, message)

    base = [step("new", obs=0)]
    sub = base + [step("subscribe", obs=0, observer=0, link=0)]
    # Undefined in the reference: null impl_ dereference and adds to an active range-for.
    obs_refuse(base + [step("unsubscribe", obs=0, observer=0)], "unsubscribe without impl")
    obs_refuse(base + [step("subscribe_unmanaged", obs=0, observer=0), notify(0, 1),
                       step("unsubscribe", obs=0, observer=0)], "unsubscribe without impl")
    obs_refuse(sub + [step("on_observers_empty", obs=0), step("unsubscribe", obs=0, observer=0)],
               "unsubscribe without impl")
    obs_refuse(sub + [notify(0, 1, {0: [step("subscribe_unmanaged", obs=0, observer=1)]})],
               "subscribe during active iteration")
    obs_refuse(sub + [notify(0, 1, {0: [step("subscribe", obs=0, observer=1, link=1)]})],
               "subscribe during active iteration")
    obs_refuse(sub + [step("copy", obs=1, from_=0), notify(1, 1, {0: [step("subscribe_unmanaged", obs=0, observer=1)]})],
               "subscribe during active iteration")
    obs_refuse(sub + [step("new", obs=1), step("subscribe", obs=1, observer=1, link=1),
                      notify(0, 1, {0: [notify(1, 2, {1: [step("subscribe_unmanaged", obs=0, observer=2)]})]})],
               "subscribe during active iteration")
    obs_refuse(sub + [notify(0, 1, {0: [step("drop", obs=0)]})], "drop during own notification")
    obs_refuse(sub + [notify(0, 1, {0: [step("leave_iteration", obs=0)]})], "base operation inside notification")
    obs_refuse(sub + [notify(0, 1, {0: [step("on_observers_empty", obs=0)]})], "base operation inside notification")
    obs_refuse(sub + [notify(0, 1, {0: [step("enter_iteration", obs=0)]})], "base operation inside notification")
    # Harness bounds and script errors.
    deep = []
    for depth in range(10, 0, -1):
        deep = [notify(0, depth, {0: deep})]
    obs_refuse(sub + deep, "nesting too deep")
    obs_refuse(base + [step("link_new", link=0)] * 2048, "too many steps")
    many = base + [step("subscribe_unmanaged", obs=0, observer=0) for _ in range(40)] + [step("copy", obs=1, from_=0)]
    blow = []
    for depth in range(4, 0, -1):
        blow = [notify(0, depth, {0: blow})]
    obs_refuse(many + blow, "execution budget exceeded")
    obs_refuse(sub + [notify(0, 1, {0: [], 1: []}),
                      {"op": "notify", "obs": 0, "event": 1, "reactions": [{"observer": 0, "steps": []},
                                                                            {"observer": 0, "steps": []}]}],
               "duplicate reaction")
    obs_refuse(sub + [{"op": "notify", "obs": 0, "event": 1, "reactions": [{"observer": 0, "steps": [], "x": 1}]}],
               "unexpected or missing field")
    obs_refuse(sub + [{"op": "notify", "obs": 0, "event": 1, "reactions": {}}], "expected array: reactions")
    obs_refuse(sub + [{"op": "notify", "obs": 0, "event": 1, "reactions": [{"observer": 0, "steps": 1}]}],
               "expected array: steps")
    obs_refuse(sub + [notify(0, 1000001)], "integer out of range: event")
    obs_refuse(sub + [step("subscribe", obs=0, observer=8, link=1)], "integer out of range: observer")
    obs_refuse(sub + [step("subscribe", obs=0, observer=0, link=16)], "integer out of range: link")
    obs_refuse(sub + [step("subscribe", obs=8, observer=0, link=1)], "integer out of range: obs")
    obs_refuse(sub + [step("subscribe", obs=0, observer=0, link=1, guard=1)], "expected boolean: guard")
    obs_refuse(sub + [{"op": "notify_ignore", "obs": 0, "event": 1, "ignore": -1, "reactions": []}],
               "integer out of range: ignore")
    obs_refuse(sub + [{"op": "notify_ignore", "obs": 0, "event": 1, "reactions": []}], "unexpected or missing field")
    obs_refuse(sub + [step("frobnicate", obs=0)], "unknown operation")
    obs_refuse(sub + [{"obs": 0}], "expected string: op")
    obs_refuse(sub + [5], "expected string: op")
    obs_refuse([step("new", obs=0), step("new", obs=0)], "observable slot occupied")
    obs_refuse([step("copy", obs=0, from_=1)], "observable slot empty")
    obs_refuse(base + [step("copy", obs=0, from_=0)], "observable slot occupied")
    obs_refuse([step("drop", obs=0)], "observable slot empty")
    obs_refuse([notify(0, 1)], "observable slot empty")
    obs_refuse([step("unsubscribe", obs=0, observer=0)], "observable slot empty")
    obs_refuse([step("enter_iteration", obs=0)], "observable slot empty")
    obs_refuse(sub + [step("subscribe", obs=0, observer=1, link=0)], "link slot occupied")
    obs_refuse(sub + [step("link_new", link=0)], "link slot occupied")
    obs_refuse(sub + [step("link_move", link=0, from_=0)], "link slot occupied")
    obs_refuse(base + [step("link_move", link=0, from_=1)], "link slot empty")
    obs_refuse(base + [step("link_assign", link=0, from_=0)], "link slot empty")
    obs_refuse(sub + [step("link_assign", link=0, from_=1)], "link slot empty")
    obs_refuse(base + [step("link_reset", link=0)], "link slot empty")
    obs_refuse(base + [step("link_drop", link=0)], "link slot empty")
    obs_refuse(base + [step("link_token", link=0, obs=1, observer=0)], "observable slot empty")
    c.refuse("observable", {"op": "observable", "steps": {}}, "expected array: steps")
    c.refuse("observable", {"op": "observable"}, "unexpected or missing field")


def generate():
    c = Corpus()
    rng = Stream(0x6B69636164636F72)  # "kicadcor"
    lengths(c, rng)
    encodes(c, rng)
    decodes(c, rng)
    versions(c, rng)
    clocks(c, rng)
    probes(c, rng)
    observables(c, rng)
    refusals(c)

    def encode(items):
        return "".join(json.dumps(r, separators=(",", ":"), ensure_ascii=False, allow_nan=False) + "\n"
                       for r in items)
    return encode(c.cases), encode(c.controls), c.counts


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    data, controls, counts = generate()
    if args.check:
        if CASES.read_text(encoding="utf-8") != data or CONTROLS.read_text(encoding="utf-8") != controls:
            raise SystemExit("corpus differs from deterministic generator")
    else:
        CASES.write_text(data, encoding="utf-8")
        CONTROLS.write_text(controls, encoding="utf-8")
    cases = sum(v for k, v in counts.items() if not k.startswith("refuse-"))
    print(json.dumps({"cases": cases, "controls": len(controls.splitlines()), **counts}, sort_keys=True))
