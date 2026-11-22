#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Write the fixed-seed corpus for libs.kimath.include.application.pure.p001.

Only generated public data is used. Every case is a successful operation in the
pinned reference's defined domain; malformed requests live in guards.requests.jsonl.
Run from the repository root: python3 rust/kicad-hash-shim/generate_cases.py
"""
import itertools
import json
import pathlib
import random
import struct

UNIT = "libs.kimath.include.application.pure.p001"
ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = ROOT / "port" / "cases" / f"{UNIT}.jsonl"
SEED = 0x4B1C4D_2026

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


def main():
    corpus = Corpus()
    std_hash_cases(corpus)
    combine_cases(corpus)
    hash128_cases(corpus)
    mmh3_cases(corpus)
    free_function_cases(corpus)
    md5_cases(corpus)
    transform_cases(corpus)
    OUT.write_text("\n".join(corpus.lines) + "\n", encoding="utf-8", newline="\n")
    print(f"{len(corpus.lines)} cases -> {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
