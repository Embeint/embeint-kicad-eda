#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Write the fixed-seed corpus for libs.kimath.include.application.pure.p001.

Only generated public data is used. Every corpus case is a successful operation
in the pinned reference's defined domain. Malformed requests are in check_guards.py.

The first 1315 requests are byte-identical to the original corpus (its RNG stream
is unchanged). Fifteen of them read MMH3 block bytes the reference never wrote, so
they are undefined in C++; they are kept, unchanged, in
fixtures/mmh3-unwritten-reads.requests.jsonl as permanent refusal regressions and
are not corpus cases (port/findings/kimath-header-hash-mmh3-unwritten-reads.md).
Later additions use separate seeds so they never perturb the earlier streams:
ADDITION_SEED for the first review's additions and BEZIER_SEED for the second
review's (the owned bezier_curves.h members and locale groupings of any length).

BEZIER<int> requests whose reference evaluation is undefined (a signed int
overflow or a NaN converted to int) are refusals, so they are written to
fixtures/bezier-int-undefined.{requests,replies}.jsonl for check_guards.py. The
classification below follows bezier_curves.h and vector2d.h at the pin; the oracle
adapter and the Rust port each decide definedness independently, and check_guards.py
and the difftest require all three to agree.

Run from the repository root: python3 rust/kicad-hash-shim/generate_cases.py
"""
import itertools
import json
import math
import pathlib
import random
import struct

UNIT = "libs.kimath.include.application.pure.p001"
ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = ROOT / "port" / "cases" / f"{UNIT}.jsonl"
FIXTURES = ROOT / "rust" / "kicad-hash-shim" / "fixtures"
FIXTURE = FIXTURES / "mmh3-unwritten-reads.requests.jsonl"
BEZIER_REQUESTS = FIXTURES / "bezier-int-undefined.requests.jsonl"
BEZIER_REPLIES = FIXTURES / "bezier-int-undefined.replies.jsonl"
SEED = 0x4B1C4D_2026
ADDITION_SEED = 0x4B1C4D_2027
BEZIER_SEED = 0x4B1C4D_2028

# Original cases whose reference execution would read never-written block bytes
# (first read: hashTail via digest, or getblock64). Source-derived, and equal to the
# independent review's defined-domain analysis of head 8c54103a7d.
UNWRITTEN_READ_CASES = [
    "mmh3-data-v1-after-int-1", "mmh3-data-v1-after-int-4", "mmh3-random-031", "mmh3-random-036",
    "mmh3-random-039", "mmh3-random-053", "mmh3-random-063", "mmh3-random-068", "mmh3-random-114",
    "mmh3-random-115", "mmh3-random-125", "mmh3-random-132", "mmh3-random-137", "mmh3-random-139",
    "mmh3-random-143",
]

RANGES = {
    "i8": (-(1 << 7), (1 << 7) - 1), "u8": (0, (1 << 8) - 1),
    "i16": (-(1 << 15), (1 << 15) - 1), "u16": (0, (1 << 16) - 1),
    "i32": (-(1 << 31), (1 << 31) - 1), "u32": (0, (1 << 32) - 1),
    "i64": (-(1 << 63), (1 << 63) - 1), "u64": (0, (1 << 64) - 1),
}
TYPES = ["bool", *RANGES, "f32", "f64", "string"]
THREE_TYPES = ["bool", "i32", "u64", "f64", "string"]
U64 = (1 << 64) - 1
U32 = (1 << 32) - 1

F64_SPECIAL = [0x0000000000000000, 0x8000000000000000, 0x3FF0000000000000, 0xBFF0000000000000,
               0x0000000000000001, 0x800FFFFFFFFFFFFF, 0x0010000000000000, 0x7FEFFFFFFFFFFFFF,
               0x7FF0000000000000, 0xFFF0000000000000, 0x7FF8000000000000, 0xFFF8000000000000,
               0x7FF0000000000001, 0x7FF4000000000000, 0x3FB999999999999A, 0x4059000000000000]
F32_SPECIAL = [0x00000000, 0x80000000, 0x3F800000, 0xBF800000, 0x00000001, 0x807FFFFF,
               0x00800000, 0x7F7FFFFF, 0x7F800000, 0xFF800000, 0x7FC00000, 0xFFC00000,
               0x7F800001, 0x7FA00000, 0x3DCCCCCD]


class Corpus:
    def __init__(self):
        self.rng = random.Random(SEED)
        self.lines = []
        self.ids = set()

    def add(self, case, value):
        if case in self.ids:
            raise ValueError(f"duplicate case {case}")
        self.ids.add(case)
        self.lines.append(json.dumps({"unit": UNIT, "case": case, "input": value},
                                     separators=(",", ":")))

    def bytes(self, count):
        return bytes(self.rng.getrandbits(8) for _ in range(count))

    def value(self, kind, mode="random"):
        rng = self.rng
        if kind == "bool":
            return {"type": "bool", "value": rng.random() < 0.5 if mode == "random" else mode == "max"}
        if kind in RANGES:
            low, high = RANGES[kind]
            number = {"min": low, "max": high, "zero": 0}.get(mode)
            if number is None:
                number = rng.randint(low, high) if rng.random() < 0.7 else rng.randint(-300, 300)
                number = min(max(number, low), high)
            return {"type": kind, "value": number}
        if kind == "f64":
            bits = rng.choice(F64_SPECIAL) if mode == "special" else struct.unpack(
                "<Q", struct.pack("<d", rng.uniform(-1e6, 1e6)))[0]
            return {"type": "f64", "bits": f"{bits:016x}"}
        if kind == "f32":
            bits = rng.choice(F32_SPECIAL) if mode == "special" else struct.unpack(
                "<I", struct.pack("<f", rng.uniform(-1e4, 1e4)))[0]
            return {"type": "f32", "bits": f"{bits:08x}"}
        length = rng.choice([0, 1, 3, 7, 8, 9, 15, 16, 17, 31, 64]) if mode == "random" else 0
        return {"type": "string", "hex": self.bytes(length).hex()}


def std_hash_cases(corpus):
    for kind, (low, high) in RANGES.items():
        values = [{"type": kind, "value": v} for v in sorted({low, high, 0, 1, max(low, -1), high - 1})]
        values += [corpus.value(kind) for _ in range(10)]
        corpus.add(f"std-hash-{kind}", {"op": "std_hash", "values": values})
    corpus.add("std-hash-bool", {"op": "std_hash", "values": [
        {"type": "bool", "value": False}, {"type": "bool", "value": True}]})
    corpus.add("std-hash-f64-special", {"op": "std_hash", "values": [
        {"type": "f64", "bits": f"{bits:016x}"} for bits in F64_SPECIAL]})
    corpus.add("std-hash-f32-special", {"op": "std_hash", "values": [
        {"type": "f32", "bits": f"{bits:08x}"} for bits in F32_SPECIAL]})
    for index in range(4):
        corpus.add(f"std-hash-f64-random-{index}", {"op": "std_hash", "values": [
            {"type": "f64", "bits": f"{corpus.rng.getrandbits(64):016x}"} for _ in range(16)]})
        corpus.add(f"std-hash-f32-random-{index}", {"op": "std_hash", "values": [
            {"type": "f32", "bits": f"{corpus.rng.getrandbits(32):08x}"} for _ in range(16)]})
    # Every _Hash_bytes tail length 0..7 over several aligned word counts.
    for words in range(4):
        corpus.add(f"std-hash-string-words-{words}", {"op": "std_hash", "values": [
            {"type": "string", "hex": corpus.bytes(words * 8 + tail).hex()} for tail in range(8)]})
    corpus.add("std-hash-string-text", {"op": "std_hash", "values": [
        {"type": "string", "hex": text.encode().hex()}
        for text in ["", "a", "abc", "KiCad", "F.Cu", "GND", "Hello, World!", "é€"]]})
    corpus.add("std-hash-string-high-bytes", {"op": "std_hash", "values": [
        {"type": "string", "hex": bytes([b] * n).hex()} for b in (0x80, 0xFF, 0x00) for n in (1, 7, 8, 9)]})
    corpus.add("std-hash-string-4096", {"op": "std_hash", "values": [
        {"type": "string", "hex": corpus.bytes(4096).hex()}]})
    corpus.add("std-hash-empty", {"op": "std_hash", "values": []})
    corpus.add("std-hash-64-mixed", {"op": "std_hash", "values": [
        corpus.value(corpus.rng.choice(TYPES)) for _ in range(64)]})


def combine_cases(corpus):
    rng = corpus.rng
    seeds = [0, 1, 0xa82de1c0, U64, 1 << 63, (1 << 63) - 1]
    for index, seed in enumerate(seeds + [rng.getrandbits(64) for _ in range(4)]):
        corpus.add(f"hash-combine-0-{index}", {"op": "hash_combine", "seed": seed, "values": []})
    corpus.add("hash-val-0", {"op": "hash_val", "values": []})
    for kind in TYPES:
        for index, mode in enumerate(["min", "max", "zero", "random", "random"]
                                     if kind in RANGES else ["random", "random", "special", "special"]
                                     if kind in ("f32", "f64") else ["min", "max"]
                                     if kind == "bool" else ["random", "random", "zero"]):
            value = corpus.value(kind, mode)
            corpus.add(f"hash-combine-1-{kind}-{index}", {
                "op": "hash_combine", "seed": rng.choice(seeds + [rng.getrandbits(64)]), "values": [value]})
            corpus.add(f"hash-val-1-{kind}-{index}", {"op": "hash_val", "values": [value]})
    for first, second in itertools.product(TYPES, repeat=2):
        values = [corpus.value(first), corpus.value(second)]
        corpus.add(f"hash-combine-2-{first}-{second}", {
            "op": "hash_combine", "seed": rng.getrandbits(64), "values": values})
        corpus.add(f"hash-val-2-{first}-{second}", {"op": "hash_val", "values": values})
    for kinds in itertools.product(THREE_TYPES, repeat=3):
        values = [corpus.value(kind) for kind in kinds]
        name = "-".join(kinds)
        corpus.add(f"hash-combine-3-{name}", {
            "op": "hash_combine", "seed": rng.getrandbits(64), "values": values})
        corpus.add(f"hash-val-3-{name}", {"op": "hash_val", "values": values})
    # KiCad call-site shapes: coordinate pairs, angles, flags and text in sequence.
    points = [{"type": "i32", "value": rng.randint(-(1 << 31), (1 << 31) - 1)} for _ in range(2)]
    corpus.add("hash-combine-2-coordinate-pair", {"op": "hash_combine", "seed": 0, "values": points})
    corpus.add("hash-combine-each-pad-like", {"op": "hash_combine_each", "seed": 0, "values": [
        {"type": "i32", "value": 1}, {"type": "i32", "value": 250000}, {"type": "i32", "value": -250000},
        {"type": "f64", "bits": "4056800000000000"}, {"type": "bool", "value": True},
        {"type": "string", "hex": b"Net-(U1-Pad1)".hex()}, {"type": "u64", "value": 7}]})
    for index in range(6):
        count = [1, 8, 32, 100, 255, 256][index]
        corpus.add(f"hash-combine-each-random-{index}", {
            "op": "hash_combine_each", "seed": rng.getrandbits(64),
            "values": [corpus.value(rng.choice(TYPES)) for _ in range(count)]})
    corpus.add("hash-combine-each-max-seed-zeros", {
        "op": "hash_combine_each", "seed": U64, "values": [{"type": "u64", "value": 0}] * 16})


def hash128_cases(corpus):
    rng = corpus.rng
    zero = "00" * 16
    pairs = [("zero-zero", zero, zero), ("ff-ff", "ff" * 16, "ff" * 16), ("zero-ff", zero, "ff" * 16)]
    base = corpus.bytes(16)
    pairs.append(("equal-random", base.hex(), base.hex()))
    for position in (0, 7, 8, 15):
        other = bytearray(base)
        other[position] ^= 0x01
        pairs.append((f"differ-byte-{position}", base.hex(), bytes(other).hex()))
    pairs.append(("leading-zero-words", ("01" + "00" * 7 + "00" * 7 + "0a"), zero))
    for index in range(12):
        pairs.append((f"random-{index}", corpus.bytes(16).hex(), corpus.bytes(16).hex()))
    for name, a, b in pairs:
        corpus.add(f"hash128-{name}", {"op": "hash128", "a": a, "b": b})


def mmh3(corpus, name, steps):
    corpus.add(f"mmh3-{name}", {"op": "mmh3", "steps": steps})


def mmh3_cases(corpus):
    rng = corpus.rng
    digest = {"op": "digest"}
    for name, new in [("default", {"op": "new"}), ("seed-0", {"op": "new", "seed": 0}),
                      ("seed-1", {"op": "new", "seed": 1}), ("seed-max", {"op": "new", "seed": U32}),
                      ("seed-3d-cache", {"op": "new", "seed": 0xA1B2C3D4})]:
        mmh3(corpus, f"empty-{name}", [new, digest])
    # Every tail length and block count through single addData calls.
    for length in range(0, 50):
        mmh3(corpus, f"data-length-{length}", [
            {"op": "new", "seed": rng.getrandbits(32)}, {"op": "data", "hex": corpus.bytes(length).hex()}, digest])
    for length in range(0, 41):
        mmh3(corpus, f"data-v1-length-{length}", [
            {"op": "new", "seed": rng.getrandbits(32)}, {"op": "data_v1", "hex": corpus.bytes(length).hex()}, digest])
    text = corpus.bytes(97)
    for split in (1, 2, 3, 5, 7, 13, 15, 16, 17, 31, 32, 33):
        steps = [{"op": "new", "seed": 0xA1B2C3D4}]
        steps += [{"op": "data", "hex": text[i:i + split].hex()} for i in range(0, len(text), split)]
        mmh3(corpus, f"stream-split-{split}", steps + [digest])
    mmh3(corpus, "stream-whole", [{"op": "new", "seed": 0xA1B2C3D4}, {"op": "data", "hex": text.hex()}, digest])
    mmh3(corpus, "pending-then-empty", [{"op": "new"}, {"op": "data", "hex": "0102030405"},
                                        {"op": "data", "hex": ""}, digest])
    mmh3(corpus, "pending-completes-block-exactly", [{"op": "new"}, {"op": "data", "hex": corpus.bytes(5).hex()},
                                                     {"op": "data", "hex": corpus.bytes(11).hex()}, digest])
    mmh3(corpus, "pending-completes-then-tail", [{"op": "new"}, {"op": "data", "hex": corpus.bytes(9).hex()},
                                                 {"op": "data", "hex": corpus.bytes(30).hex()}, digest])
    for name, op in [("string", "string"), ("chars", "chars")]:
        mmh3(corpus, f"{name}-text", [{"op": "new", "seed": 0x4B1C4D},
                                      {"op": op, "hex": b"embedded file contents".hex()}, digest])
        mmh3(corpus, f"{name}-empty", [{"op": "new"}, {"op": op, "hex": ""}, digest])
    for count in range(1, 9):
        mmh3(corpus, f"ints-{count}", [{"op": "new"}] + [
            {"op": "int", "value": rng.randint(-(1 << 31), (1 << 31) - 1)} for _ in range(count)] + [digest])
    mmh3(corpus, "int-extremes", [{"op": "new"}, {"op": "int", "value": -(1 << 31)},
                                  {"op": "int", "value": (1 << 31) - 1}, {"op": "int", "value": 0},
                                  {"op": "int", "value": -1}, digest])
    # add( int32_t ) after a partial byte block: slot ( len % 16 ) / 4, no hash at len 17.
    for offset in (1, 2, 3, 5, 12, 13, 14, 15):
        mmh3(corpus, f"int-after-{offset}-bytes", [
            {"op": "new"}, {"op": "data", "hex": corpus.bytes(offset).hex()},
            {"op": "int", "value": rng.randint(-(1 << 31), (1 << 31) - 1)},
            {"op": "data", "hex": corpus.bytes(rng.randint(0, 20)).hex()}, digest])
    for offset in (1, 4, 8, 13):
        mmh3(corpus, f"data-v1-after-{offset}-bytes", [
            {"op": "new"}, {"op": "data", "hex": corpus.bytes(offset).hex()},
            {"op": "data_v1", "hex": corpus.bytes(rng.randint(1, 20)).hex()}, digest])
        mmh3(corpus, f"data-v1-after-int-{offset}", [
            {"op": "new"}, {"op": "int", "value": offset}, {"op": "data_v1", "hex": corpus.bytes(offset).hex()},
            digest])
    mmh3(corpus, "digest-twice", [{"op": "new"}, {"op": "data", "hex": corpus.bytes(21).hex()}, digest, digest])
    mmh3(corpus, "digest-then-continue", [{"op": "new"}, {"op": "data", "hex": corpus.bytes(7).hex()}, digest,
                                          {"op": "data", "hex": corpus.bytes(12).hex()}, digest])
    mmh3(corpus, "reset-keeps-blocks", [{"op": "new"}, {"op": "data", "hex": corpus.bytes(23).hex()},
                                        {"op": "reset"}, digest, {"op": "reset", "seed": 99}, digest,
                                        {"op": "int", "value": 5}, digest])
    mmh3(corpus, "reset-default-vs-zero", [{"op": "new", "seed": 7}, {"op": "reset"}, digest,
                                           {"op": "reset", "seed": 0}, digest])
    mmh3(corpus, "renew-mid-stream", [{"op": "new"}, {"op": "data", "hex": corpus.bytes(9).hex()},
                                      {"op": "new", "seed": 3}, digest])
    # Larger streams within the byte budget.
    mmh3(corpus, "large-64k-x16", [{"op": "new", "seed": 0xA1B2C3D4},
                                   {"op": "data", "hex": corpus.bytes(65536).hex(), "repeat": 16}, digest])
    mmh3(corpus, "large-17b-x4096", [{"op": "new"}, {"op": "data", "hex": corpus.bytes(17).hex(), "repeat": 4096},
                                     digest])
    mmh3(corpus, "large-v1-15b-x4096", [{"op": "new"}, {"op": "data_v1", "hex": corpus.bytes(15).hex(),
                                                         "repeat": 4096}, digest])
    # uint32_t len wrap, reached through state injection (a 4 GiB stream otherwise).
    for name, length, steps in [
            ("data-16", U32 - 15, [{"op": "data", "hex": corpus.bytes(16).hex()}]),
            ("data-1-pending-15", U32, [{"op": "data", "hex": "ab"}]),
            ("int", U32 - 3, [{"op": "int", "value": -2}]),
            ("data-v1-3", U32 - 3, [{"op": "data_v1", "hex": "010203"}]),
            ("data-40", U32 - 7, [{"op": "data", "hex": corpus.bytes(40).hex()}])]:
        mmh3(corpus, f"len-wrap-{name}", [
            {"op": "new"}, {"op": "state", "h1": rng.getrandbits(64), "h2": rng.getrandbits(64),
                            "blocks": corpus.bytes(16).hex(), "len": length}] + steps + [digest])
    # The private helpers, called directly on injected states.
    for index in range(16):
        length = rng.choice([0, 1, 7, 8, 9, 15, 16, 31, U32, rng.getrandbits(32)])
        mmh3(corpus, f"private-helpers-{index}", [
            {"op": "new"}, {"op": "state", "h1": rng.getrandbits(64), "h2": rng.getrandbits(64),
                            "blocks": corpus.bytes(16).hex(), "len": length},
            {"op": "getblock64", "index": 0}, {"op": "getblock64", "index": 1},
            {"op": "hash_block"}, {"op": "hash_tail"}, {"op": "hash_final"}])
    for tail in range(16):
        mmh3(corpus, f"hash-tail-len-{tail}", [
            {"op": "new"}, {"op": "state", "h1": rng.getrandbits(64), "h2": rng.getrandbits(64),
                            "blocks": corpus.bytes(16).hex(), "len": 32 + tail}, {"op": "hash_tail"}])
    # Fixed-seed random operation sequences.
    for index in range(150):
        steps = [{"op": "new", "seed": rng.getrandbits(32)} if rng.random() < 0.7 else {"op": "new"}]
        for _ in range(rng.randint(1, 24)):
            choice = rng.random()
            if choice < 0.35:
                steps.append({"op": "data", "hex": corpus.bytes(rng.randint(0, 40)).hex()})
            elif choice < 0.45:
                steps.append({"op": "data_v1", "hex": corpus.bytes(rng.randint(0, 20)).hex()})
            elif choice < 0.52:
                steps.append({"op": rng.choice(["string", "chars"]), "hex": corpus.bytes(rng.randint(0, 33)).hex()})
            elif choice < 0.75:
                steps.append({"op": "int", "value": rng.randint(-(1 << 31), (1 << 31) - 1)})
            elif choice < 0.9:
                steps.append(digest)
            elif choice < 0.95:
                steps.append({"op": "reset", "seed": rng.getrandbits(32)} if rng.random() < 0.5 else {"op": "reset"})
            else:
                steps.append({"op": "getblock64", "index": rng.randint(0, 1)})
        steps.append(digest)
        mmh3(corpus, f"random-{index:03d}", steps)


def free_function_cases(corpus):
    rng = corpus.rng
    xs = [0, 1, U64, 1 << 63, 0x87c37b91114253d5] + [rng.getrandbits(64) for _ in range(3)]
    for r in range(1, 64):
        for index, x in enumerate(xs if r in (1, 27, 31, 33, 63) else xs[:2] + xs[-1:]):
            corpus.add(f"rotl-{r}-{index}", {"op": "rotl", "x": x, "r": r})
    for index, k in enumerate([0, 1, U64, 1 << 63, 0xff51afd7ed558ccd] + [rng.getrandbits(64) for _ in range(8)]):
        corpus.add(f"fmix64-{index}", {"op": "fmix64", "k": k})


def md5_cases(corpus):
    new = {"op": "new"}
    sequences = {
        "fresh-invalid": [new, {"op": "inspect"}],
        "set-valid-toggles": [new, {"op": "set_valid", "value": True}, {"op": "set_valid", "value": True},
                              {"op": "set_valid", "value": False}, {"op": "set_valid", "value": False}],
        "finalize-sets-valid": [new, {"op": "bytes", "hex": b"abc".hex()}, {"op": "finalize"}],
        "set-invalid-keeps-digest": [new, {"op": "bytes", "hex": b"abc".hex()}, {"op": "finalize"},
                                     {"op": "set_valid", "value": False}, {"op": "inspect"}],
        "init-clears-valid": [new, {"op": "set_valid", "value": True}, {"op": "init"}],
        "hash-keeps-validity": [new, {"op": "set_valid", "value": True}, {"op": "bytes", "hex": "00ff"},
                                {"op": "int", "value": -7}, {"op": "bytes", "hex": ""}],
        "copy-preserves-valid": [new, {"op": "set_valid", "value": True}, {"op": "copy"},
                                 {"op": "set_valid", "value": False}, {"op": "copy"}],
        "assign-preserves-valid": [new, {"op": "finalize"}, {"op": "assign"}, {"op": "set_valid", "value": False},
                                   {"op": "assign"}],
        "valid-without-digest": [new, {"op": "set_valid", "value": True}, {"op": "bytes", "hex": b"x".hex()},
                                 {"op": "finalize"}, {"op": "init"}, {"op": "set_valid", "value": True}],
    }
    for name, steps in sequences.items():
        corpus.add(f"md5-valid-{name}", {"op": "md5_valid", "steps": steps})


def transform_cases(corpus):
    rng = corpus.rng
    ident = [1, 0, 0, 1]
    pairs = [("default-default", None, None), ("default-explicit-identity", None, ident),
             ("explicit-identity-default", ident, None),
             ("mirror-x", None, [1, 0, 0, -1]), ("mirror-y", None, [-1, 0, 0, 1]),
             ("rotate-90-vs-270", [0, 1, -1, 0], [0, -1, 1, 0]), ("rotate-180", [-1, 0, 0, -1], None),
             ("extremes-equal", [-(1 << 31), (1 << 31) - 1, 0, -1], [-(1 << 31), (1 << 31) - 1, 0, -1])]
    for member in range(4):
        other = list(ident)
        other[member] += 1
        pairs.append((f"differ-member-{member}", ident, other))
    for index in range(12):
        a = [rng.randint(-2, 2) for _ in range(4)]
        b = list(a) if index % 3 == 0 else [rng.randint(-2, 2) for _ in range(4)]
        pairs.append((f"random-small-{index}", a, b))
    for index in range(6):
        pairs.append((f"random-wide-{index}", [rng.randint(-(1 << 31), (1 << 31) - 1) for _ in range(4)],
                      [rng.randint(-(1 << 31), (1 << 31) - 1) for _ in range(4)]))
    for name, a, b in pairs:
        corpus.add(f"transform-{name}", {"op": "transform", "a": a, "b": b})


def locale_cases(corpus):
    """HASH_128::ToString under a caller global locale (numpunct<char> grouping)."""
    rng = corpus.rng
    reviewer = "f0debc9a78563412" + "3412000000000000"
    groupings = [
        ("reviewer-3-underscore", 0x5F, "03", reviewer),
        ("empty-grouping", 0x2C, "", reviewer),
        ("every-digit", 0x2E, "01", corpus.bytes(16).hex()),
        ("one-two-zero", 0x2E, "010200", "efcdab0000000000" + "00" * 8),
        ("repeat-four-non-ascii-sep", 0xC3, "04", "ff" * 8 + "01" + "00" * 7),
        ("first-zero", 0x5F, "0002", reviewer),
        ("first-negative", 0x5F, "8002", reviewer),
        ("first-ff", 0x5F, "ff02", reviewer),
        ("first-char-max", 0x5F, "7f02", reviewer),
        ("later-char-max", 0x5F, "027f", reviewer),
        ("later-negative", 0x5F, "0380", reviewer),
        ("group-wider-than-digits", 0x27, "11", reviewer),
        ("group-equals-word", 0x27, "10", "ff" * 16),
        ("nul-separator", 0x00, "02", reviewer),
        ("zero-words", 0x5F, "01", "00" * 16),
        ("max-length-grouping", 0x2D, "0102" * 32, corpus.bytes(16).hex()),
    ]
    for name, sep, grouping, value in groupings:
        corpus.add(f"hash128-locale-{name}", {"op": "hash128", "a": value, "b": corpus.bytes(16).hex(),
                                              "locale": {"thousands_sep": sep, "grouping": grouping}})
    for index in range(12):
        grouping = bytes(rng.choice([1, 2, 3, 4, 5, 7, 16, 0, 0x7F, 0x80, 0xFF])
                         for _ in range(rng.randint(1, 6))).hex()
        corpus.add(f"hash128-locale-random-{index}", {
            "op": "hash128", "a": corpus.bytes(16).hex(), "b": corpus.bytes(16).hex(),
            "locale": {"thousands_sep": rng.randint(0, 255), "grouping": grouping}})


def addition_cases(corpus):
    """Cases added after the first review, from ADDITION_SEED."""
    corpus.rng = random.Random(ADDITION_SEED)
    locale_cases(corpus)
    base = corpus.bytes(16)
    for position in [p for p in range(16) if p not in (0, 7, 8, 15)]:
        other = bytearray(base)
        other[position] ^= 0x80
        corpus.add(f"hash128-differ-byte-{position}", {"op": "hash128", "a": base.hex(), "b": bytes(other).hex()})
    digest = {"op": "digest"}
    # Defined mixes of add( int32_t ) and addDataV1: every byte read was written.
    mmh3(corpus, "v1-then-int-defined", [{"op": "new"}, {"op": "data_v1", "hex": corpus.bytes(5).hex()},
                                          {"op": "int", "value": 9}, {"op": "int", "value": -9}, digest])
    # int writes 0..3 (len 4); addDataV1 of 8 bytes rewrites 0..7, pads 8..11 and
    # counts len 16, so digest reads no tail and word 0 is written.
    mmh3(corpus, "int-then-v1-aligned-defined", [{"op": "new"}, {"op": "int", "value": 3},
                                                  {"op": "data_v1", "hex": corpus.bytes(8).hex()},
                                                  {"op": "getblock64", "index": 0}, digest])
    mmh3(corpus, "unwritten-bytes-after-ints", [{"op": "new", "seed": 5}, {"op": "int", "value": 1},
                                                 {"op": "int", "value": 2}, {"op": "reset"},
                                                 {"op": "getblock64", "index": 0}, digest])
    mmh3(corpus, "unwritten-then-fully-written", [{"op": "new"}, {"op": "data_v1", "hex": "aa"},
                                                   {"op": "data", "hex": corpus.bytes(16).hex()},
                                                   {"op": "getblock64", "index": 1}, {"op": "hash_block"},
                                                   {"op": "hash_tail"}, digest])


# --- BEZIER<T> (bezier_curves.h:89-197) and BEZIER_POLY( std::vector<VECTOR2D> ) ---

INT_MIN = -(1 << 31)
INT_MAX = (1 << 31) - 1
OVERFLOW = "int component overflow"
NAN_TO_INT = "NaN component converted to int"


class Undefined(Exception):
    """A BEZIER<int> evaluation the reference leaves undefined."""


def f64_bits(value):
    return struct.unpack("<Q", struct.pack("<d", value))[0]


def bits_text(bits):
    return f"{bits:016x}"


def from_bits(bits):
    return struct.unpack("<d", struct.pack("<Q", bits))[0]


def int_store(point):
    """VECTOR2<int>( const VECTOR2<double>& ): clamp to [INT_MIN, INT_MAX], truncate."""
    result = []
    for value in point:
        clamped = float(INT_MIN) if value < INT_MIN else float(INT_MAX) if INT_MAX < value else value
        if math.isnan(clamped):
            raise Undefined(NAN_TO_INT)
        result.append(int(clamped))
    return tuple(result)


def int_lerp(a, b, t):
    """a + aT * ( b - a ) for VECTOR2<int> a and b, stored as VECTOR2<int>."""
    difference = (b[0] - a[0], b[1] - a[1])
    if not all(INT_MIN <= d <= INT_MAX for d in difference):
        raise Undefined(OVERFLOW)
    return int_store((float(a[0]) + t * float(difference[0]), float(a[1]) + t * float(difference[1])))


def int_point_at(curve, t):
    """PointAt for int: the double sum stored as VECTOR2<int> (only NaN is undefined)."""
    t2 = t * t
    t3 = t2 * t
    t_m1 = 1.0 - t
    t_m1_2 = t_m1 * t_m1
    t_m1_3 = t_m1_2 * t_m1
    s, c1, c2, e = curve
    point = tuple(t_m1_3 * float(s[i]) + 3.0 * t * t_m1_2 * float(c1[i]) + 3.0 * t2 * t_m1 * float(c2[i])
                  + t3 * float(e[i]) for i in range(2))
    return int_store(point)


def int_split(curve, t):
    s, c1, c2, e = curve
    start_c1 = int_lerp(s, c1, t)
    c1_c2 = int_lerp(c1, c2, t)
    c2_end = int_lerp(c2, e, t)
    left_c2 = int_lerp(start_c1, c1_c2, t)
    right_c1 = int_lerp(c1_c2, c2_end, t)
    shared = int_lerp(left_c2, right_c1, t)
    return (s, start_c1, left_c2, shared), (shared, right_c1, c2_end, e)


def unit_clamp(t):
    return 0.0 if t < 0.0 else 1.0 if t > 1.0 else t


def ieee_div(a, b):
    """IEEE double division (Python raises for a zero divisor); NaN bits are not modelled."""
    if b != 0.0 or math.isnan(b):
        return a / b
    if math.isnan(a) or a == 0.0:
        return math.nan
    return math.copysign(math.inf, a) * math.copysign(1.0, b)


def int_sub_curve(curve, t0, t1):
    t0, t1 = unit_clamp(t0), unit_clamp(t1)
    if t1 <= t0:
        return int_point_at(curve, t0)
    if t0 == 0.0 and t1 == 1.0:
        return curve
    if t0 == 0.0:
        return int_split(curve, t1)[0]
    if t1 == 1.0:
        return int_split(curve, t0)[1]
    return int_split(int_split(curve, t1)[0], ieee_div(t0, t1))[1]


def int_refusal(curve, call, params):
    """None when the BEZIER<int> call is defined, else the refusal message."""
    try:
        if call == "point_at":
            int_point_at(curve, from_bits(params["t"]))
        elif call == "split":
            int_split(curve, from_bits(params["t"]))
        elif call == "sub_curve":
            int_sub_curve(curve, from_bits(params["t0"]), from_bits(params["t1"]))
    except Undefined as error:
        return str(error)
    return None


# Bit patterns: signed zeros, +-1, a subnormal, the largest finite, infinities, quiet and
# signalling NaNs with payloads and both signs, and ordinary fractions.
F64_POINT_SPECIAL = [0x0000000000000000, 0x8000000000000000, 0x3FF0000000000000, 0xBFF0000000000000,
                     0x0000000000000001, 0x7FEFFFFFFFFFFFFF, 0xFFEFFFFFFFFFFFFF, 0x7FF0000000000000,
                     0xFFF0000000000000, 0x7FF8000000000001, 0xFFF80000000000A5, 0x7FF0000000000005,
                     0x3FB999999999999A, 0x4059000000000000, 0x41CDCD6500000000]
T_SPECIAL = [0.0, -0.0, 1.0, 0.5, 0.25, 0.75, 1.0 / 3.0, 2.0 / 3.0, -0.5, 1.5, 2.0, -1.0,
             1e-300, -1e-300, 5e-324, 1.0 - 2.0 ** -53, 1.0 + 2.0 ** -52, 1e10, -1e10, 1e200, -1e200,
             math.inf, -math.inf]
T_NAN_BITS = [0x7FF8000000000000, 0xFFF8000000000000, 0x7FF80000000000C3, 0x7FF0000000000009]


def bezier_request(kind, points, call, **params):
    value = {"op": "bezier", "type": kind, "points": [list(point) for point in points], "call": call}
    for name, bits in params.items():
        value[name] = bits_text(bits)
    return value


def f64_points(points):
    return [[bits_text(f64_bits(x)), bits_text(f64_bits(y))] for x, y in points]


def f64_raw_points(points):
    return [[bits_text(x), bits_text(y)] for x, y in points]


def bezier_cases(corpus, refusals):
    """Every owned BEZIER branch for both instantiations, plus BEZIER_POLY."""
    rng = corpus.rng

    def add(name, kind, points, call, curve=None, **params):
        value = bezier_request(kind, points, call, **params)
        message = int_refusal(curve, call, params) if kind == "i32" else None
        if message is None:
            corpus.add(f"bezier-{name}", value)
        else:
            refusals.append((f"bezier-{name}", value, message))

    def t_bits():
        choice = rng.random()
        if choice < 0.45:
            return f64_bits(rng.random())
        if choice < 0.75:
            return f64_bits(rng.choice(T_SPECIAL))
        if choice < 0.85:
            return rng.choice(T_NAN_BITS)
        return f64_bits(rng.uniform(-3.0, 4.0))

    # Fixed curves for double: an S curve, a straight line, a point, and EDA_SHAPE-like
    # nanometre coordinates converted to double.
    f64_curves = {
        "s-curve": [(0.0, 0.0), (1.0, 2.0), (3.0, -2.0), (4.0, 0.0)],
        "line": [(-5.0, -5.0), (-1.0, -1.0), (1.0, 1.0), (5.0, 5.0)],
        "point": [(2.5, -7.25)] * 4,
        "nanometres": [(1000000.0, 2500000.0), (1500000.0, 4000000.0), (3000000.0, 4000000.0),
                       (3500000.0, 2500000.0)],
        "fractions": [(0.1, 0.2), (0.3, -0.7), (1e-3, 123.456), (-9.875, 1e9)],
    }
    for name, points in f64_curves.items():
        encoded = f64_points(points)
        add(f"f64-{name}-new", "f64", encoded, "new")
        for index, t in enumerate(T_SPECIAL):
            add(f"f64-{name}-point-at-{index}", "f64", encoded, "point_at", t=f64_bits(t))
            add(f"f64-{name}-split-{index}", "f64", encoded, "split", t=f64_bits(t))
        for index, bits in enumerate(T_NAN_BITS):
            add(f"f64-{name}-point-at-nan-{index}", "f64", encoded, "point_at", t=bits)
            add(f"f64-{name}-split-nan-{index}", "f64", encoded, "split", t=bits)

    # SubCurve: every clamp and branch, including -0.0, equal and reversed ranges and NaNs.
    sub_ranges = [(0.0, 1.0), (-0.0, 1.0), (-1.0, 2.0), (0.0, 0.5), (-3.0, 0.5), (0.5, 1.0), (0.5, 7.0),
                  (0.25, 0.75), (0.1, 0.9), (0.5, 0.5), (0.75, 0.25), (1.5, 2.0), (-1.0, -2.0),
                  (0.0, 0.0), (1.0, 1.0), (-0.0, 0.0), (0.0, -0.0), (1.0, 0.0), (1e-300, 1.0),
                  (0.0, 1.0 - 2.0 ** -53), (5e-324, 1.0 - 2.0 ** -53), (1.0 / 3.0, 2.0 / 3.0),
                  (-math.inf, math.inf), (math.inf, -math.inf), (0.3, math.inf)]
    nan = from_bits(0x7FF80000000000C3)
    nan_ranges = [(nan, 0.5), (0.5, nan), (nan, nan), (0.0, nan), (nan, 1.0), (nan, 0.0), (1.0, nan)]
    for name, points in f64_curves.items():
        encoded = f64_points(points)
        for index, (t0, t1) in enumerate(sub_ranges + nan_ranges):
            add(f"f64-{name}-sub-curve-{index}", "f64", encoded, "sub_curve", t0=f64_bits(t0), t1=f64_bits(t1))
    # Distinct NaN payloads meeting in one expression: aT against a coordinate, and two
    # coordinates (the left operand's payload is the platform's result).
    nan_points = [
        [(0x7FF8000000000011, 0x3FF0000000000000), (0x4000000000000000, 0x7FF8000000000022),
         (0x4008000000000000, 0xFFF8000000000033), (0x7FF0000000000044, 0x4010000000000000)],
        [(0xFFF8000000000055, 0xFFF8000000000066), (0x7FF8000000000077, 0x7FF8000000000088),
         (0x0000000000000000, 0x8000000000000000), (0x7FF0000000000000, 0xFFF0000000000000)],
    ]
    for curve_index, points in enumerate(nan_points):
        encoded = f64_raw_points(points)
        add(f"f64-nan-curve-{curve_index}-new", "f64", encoded, "new")
        for index, bits in enumerate([f64_bits(0.5), f64_bits(0.0), f64_bits(1.0), f64_bits(2.0)] + T_NAN_BITS):
            add(f"f64-nan-curve-{curve_index}-point-at-{index}", "f64", encoded, "point_at", t=bits)
            add(f"f64-nan-curve-{curve_index}-split-{index}", "f64", encoded, "split", t=bits)
        for index, (t0, t1) in enumerate([(0.25, 0.75), (0.0, 0.5), (0.5, 1.0), (0.6, 0.4), (nan, 0.5)]):
            add(f"f64-nan-curve-{curve_index}-sub-curve-{index}", "f64", encoded, "sub_curve",
                t0=f64_bits(t0), t1=f64_bits(t1))
    # Constructor: every special bit pattern stored unchanged.
    for index in range(0, len(F64_POINT_SPECIAL), 2):
        values = (F64_POINT_SPECIAL * 2)[index:index + 8]
        add(f"f64-new-special-{index}", "f64", f64_raw_points(list(zip(values[0::2], values[1::2]))), "new")
    # Overflow to infinity and inf - inf inside the double evaluation.
    big = [(1e308, -1e308), (-1e308, 1e308), (1e308, 1e308), (-1e308, -1e308)]
    for index, t in enumerate([0.5, 2.0, -1.0, 1e10]):
        add(f"f64-overflow-point-at-{index}", "f64", f64_points(big), "point_at", t=f64_bits(t))
        add(f"f64-overflow-split-{index}", "f64", f64_points(big), "split", t=f64_bits(t))
    # Fixed-seed random double curves and calls.
    for index in range(120):
        if index % 4 == 3:
            points = [(rng.choice(F64_POINT_SPECIAL), rng.choice(F64_POINT_SPECIAL)) for _ in range(4)]
            encoded = f64_raw_points(points)
        else:
            scale = rng.choice([1.0, 1e3, 1e6, 1e9, 1e-6])
            encoded = f64_points([(rng.uniform(-scale, scale), rng.uniform(-scale, scale)) for _ in range(4)])
        call = rng.choice(["point_at", "split", "sub_curve"])
        if call == "sub_curve":
            add(f"f64-random-{index:03d}", "f64", encoded, call, t0=t_bits(), t1=t_bits())
        else:
            add(f"f64-random-{index:03d}", "f64", encoded, call, t=t_bits())

    # BEZIER<int>: promotion of int components to double, truncation toward zero, saturation at
    # INT_MIN/INT_MAX, and the undefined overflows and NaN conversions (refused).
    i32_curves = {
        "s-curve": [(0, 0), (1, 2), (3, -2), (4, 0)],
        "odd": [(-7, 7), (3, -3), (-1, 11), (9, -5)],
        "nanometres": [(1000000, 2500000), (1500000, 4000000), (3000000, 4000000), (3500000, 2500000)],
        "negative": [(-1000001, -2500001), (-1500003, -4000005), (-3000007, -4000009), (-3500011, -2500013)],
        "point": [(123456789, -987654321)] * 4,
        "extremes": [(INT_MIN, INT_MAX), (INT_MAX, INT_MIN), (INT_MIN, INT_MIN), (INT_MAX, INT_MAX)],
        "near-extremes": [(INT_MAX - 1, INT_MIN + 1), (INT_MAX, INT_MAX - 2), (INT_MIN, INT_MIN + 3),
                          (INT_MAX - 4, INT_MIN)],
        "half-range": [(-(1 << 30), 1 << 30), ((1 << 30) - 1, -(1 << 30)), (0, 0), (1 << 30, (1 << 30) - 1)],
        "saturating": [(-1, -1), (1, 1), (-1, -1), (1, 1)],
        "large": [(2000000000, -2000000000), (-2000000000, 2000000000), (2000000000, 2000000000),
                  (-2000000000, -2000000000)],
    }
    for name, curve in i32_curves.items():
        encoded = [list(point) for point in curve]
        add(f"i32-{name}-new", "i32", encoded, "new", curve)
        for index, t in enumerate(T_SPECIAL):
            add(f"i32-{name}-point-at-{index}", "i32", encoded, "point_at", curve, t=f64_bits(t))
            add(f"i32-{name}-split-{index}", "i32", encoded, "split", curve, t=f64_bits(t))
        for index, bits in enumerate(T_NAN_BITS[:2]):
            add(f"i32-{name}-point-at-nan-{index}", "i32", encoded, "point_at", curve, t=bits)
            add(f"i32-{name}-split-nan-{index}", "i32", encoded, "split", curve, t=bits)
        for index, (t0, t1) in enumerate(sub_ranges + nan_ranges):
            add(f"i32-{name}-sub-curve-{index}", "i32", encoded, "sub_curve", curve,
                t0=f64_bits(t0), t1=f64_bits(t1))
    for index in range(160):
        bound = rng.choice([10, 1000, 1000000, 1 << 30, INT_MAX])
        curve = [(rng.randint(max(-bound, INT_MIN), bound), rng.randint(max(-bound, INT_MIN), bound))
                 for _ in range(4)]
        encoded = [list(point) for point in curve]
        call = rng.choice(["point_at", "split", "sub_curve"])
        if call == "sub_curve":
            add(f"i32-random-{index:03d}", "i32", encoded, call, curve, t0=t_bits(), t1=t_bits())
        else:
            add(f"i32-random-{index:03d}", "i32", encoded, call, curve, t=t_bits())

    # BEZIER_POLY( const std::vector<VECTOR2D>& ): any count, every bit pattern copied.
    polys = {
        "empty": [],
        "one": [(1.0, 2.0)],
        "quadratic": [(0.0, 0.0), (5.0, 10.0), (10.0, 0.0)],
        "cubic": [(0.0, 0.0), (1.0, 2.0), (3.0, -2.0), (4.0, 0.0)],
        "five": [(float(i), float(-i)) for i in range(5)],
    }
    for name, points in polys.items():
        corpus.add(f"bezier-poly-{name}", {"op": "bezier_poly", "points": f64_points(points)})
    corpus.add("bezier-poly-special", {"op": "bezier_poly", "points": f64_raw_points(
        list(zip(F64_POINT_SPECIAL, reversed(F64_POINT_SPECIAL))))})
    corpus.add("bezier-poly-4096", {"op": "bezier_poly", "points": [
        [bits_text(rng.getrandbits(64)), bits_text(rng.getrandbits(64))] for _ in range(4096)]})
    for index in range(6):
        corpus.add(f"bezier-poly-random-{index}", {"op": "bezier_poly", "points": f64_points(
            [(rng.uniform(-1e6, 1e6), rng.uniform(-1e6, 1e6)) for _ in range(rng.randint(0, 12))])})


def long_grouping_cases(corpus):
    """numpunct groupings longer than 64 bytes; only one byte per digit is ever used."""
    reviewer = "f0debc9a78563412" + "3412000000000000"
    for name, grouping in [("grouping-65-bytes", "03" * 65), ("grouping-100-ones", "01" * 100),
                           ("grouping-4096-bytes", "02" * 4096),
                           ("grouping-long-then-invalid", "01" * 15 + "00" + "05" * 200)]:
        corpus.add(f"hash128-locale-{name}", {"op": "hash128", "a": reviewer, "b": corpus.bytes(16).hex(),
                                              "locale": {"thousands_sep": 0x5F, "grouping": grouping}})


def second_review_cases(corpus):
    """Cases added after the second review, from BEZIER_SEED; returns the BEZIER<int> refusals."""
    corpus.rng = random.Random(BEZIER_SEED)
    refusals = []
    long_grouping_cases(corpus)
    bezier_cases(corpus, refusals)
    return refusals


def main():
    corpus = Corpus()
    std_hash_cases(corpus)
    combine_cases(corpus)
    hash128_cases(corpus)
    mmh3_cases(corpus)
    free_function_cases(corpus)
    md5_cases(corpus)
    transform_cases(corpus)
    if len(corpus.lines) != 1315:
        raise ValueError("the original corpus stream changed")
    refused = set(UNWRITTEN_READ_CASES)
    fixture = [line for line in corpus.lines if json.loads(line)["case"] in refused]
    if len(fixture) != len(refused):
        raise ValueError("missing unwritten-read regression case")
    corpus.lines = [line for line in corpus.lines if json.loads(line)["case"] not in refused]
    addition_cases(corpus)
    refusals = second_review_cases(corpus)
    OUT.write_text("\n".join(corpus.lines) + "\n", encoding="utf-8", newline="\n")
    FIXTURES.mkdir(parents=True, exist_ok=True)
    FIXTURE.write_text("\n".join(fixture) + "\n", encoding="utf-8", newline="\n")
    requests = [json.dumps({"unit": UNIT, "case": case, "input": value}, separators=(",", ":"))
                for case, value, _ in refusals]
    replies = [json.dumps({"unit": UNIT, "case": case, "error": {"kind": "invalid_input", "message": message}},
                          separators=(",", ":")) for case, _, message in refusals]
    BEZIER_REQUESTS.write_text("\n".join(requests) + "\n", encoding="utf-8", newline="\n")
    BEZIER_REPLIES.write_text("\n".join(replies) + "\n", encoding="utf-8", newline="\n")
    print(f"{len(corpus.lines)} cases -> {OUT.relative_to(ROOT)}; "
          f"{len(fixture)} refusal regressions -> {FIXTURE.relative_to(ROOT)}; "
          f"{len(refusals)} BEZIER<int> refusals -> {BEZIER_REQUESTS.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
