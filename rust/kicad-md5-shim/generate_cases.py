#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Write public, deterministic MD5 state-machine requests (LF UTF-8)."""

import argparse
import json
from pathlib import Path
import random

UNIT = "libs.kimath.src.md5_hash.application.pure.p001"


def op(name, id=0, **fields):
    return {"op": name, "id": id, **fields}


def byte_string(length):
    return [(i * 73 + i // 3 + 19) % 256 for i in range(length)]


def cases():
    def case(name, steps):
        return {"unit": UNIT, "case": name, "input": {"steps": steps}}

    for index, text in enumerate(["", "a", "abc", "message digest", "abcdefghijklmnopqrstuvwxyz",
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789", "1234567890" * 8]):
        yield case(f"rfc1321-{index}", [op("new"), op("bytes", data=list(text.encode("ascii"))),
            op("finalize")])

    yield case("repeat-noops", [op("new"), op("bytes", data=[0, 255], repeat=0),
        op("valid", value=True), op("bytes", data=[], repeat=8193), op("finalize")])

    # All split points, including empty leading/trailing updates. The longer
    # strings reach both padding branches after more than one compression block.
    lengths = [*range(66), 119, 120, 127, 128, 129, 255, 256, 257, 1024]
    for length in lengths:
        data = byte_string(length)
        yield case(f"bytes-{length}", [op("new"), op("bytes", data=data), op("finalize")])
        for split in range(length + 1):
            yield case(f"split-{length}-{split}", [
                op("new"), op("bytes", data=data[:split]), op("clone", 1, source=0),
                op("new", 2), op("assign", 2, source=0),
                op("bytes", data=data[split:]), op("bytes", 1, data=data[split:]),
                op("bytes", 2, data=data[split:]), op("finalize"), op("finalize", 1),
                op("finalize", 2), op("compare", other=1), op("compare", other=2),
            ])
        for stride in [1, 2, 3, 7, 31, 55, 56, 63, 64, 65]:
            chunks = [data[index:index + stride] for index in range(0, length, stride)]
            yield case(f"stride-{length}-{stride}", [op("new"),
                *(op("bytes", data=chunk) for chunk in chunks), op("finalize")])

    # Every composition/partition for short inputs, rather than pretending an
    # exponential corpus of all partitions of the long strings is feasible.
    for length in range(2, 9):
        data = byte_string(length)
        for mask in range(1 << (length - 1)):
            cuts = [0, *(i for i in range(1, length) if mask & (1 << (i - 1))), length]
            yield case(f"partition-{length}-{mask}", [op("new"),
                *(op("bytes", data=data[start:end]) for start, end in zip(cuts, cuts[1:])),
                op("finalize")])

    for length in [0, 1, 55, 56, 63, 64, 65, 119, 120, 127, 128, 129, 257]:
        data = byte_string(length)
        for suffix_length in [0, 1, 55, 56, 63, 64, 65, 129]:
            yield case(f"continuation-{length}-{suffix_length}", [
                op("new"), op("bytes", data=data), op("finalize"),
                op("clone", 1, source=0), op("new", 2), op("assign", 2, source=0),
                op("valid", value=False), op("compare", other=1),
                op("bytes", data=byte_string(suffix_length)), op("int", value=-1234567),
                op("finalize"), op("finalize"), op("finalize", 1),
                op("finalize", 2), op("compare", 1, other=2),
                op("assign", source=0), op("init"), op("finalize"),
            ])

    for value in [-2147483648, -2147483647, -65536, -256, -1, 0, 1, 127, 128, 255, 256,
                  65535, 65536, 2147483646, 2147483647]:
        data = list(value.to_bytes(4, "little", signed=True))
        yield case(f"integer-{value}", [op("new"), op("int", value=value), op("finalize"),
            op("new", 1), op("bytes", 1, data=data), op("finalize", 1), op("compare", other=1)])

    yield case("validity-and-equality", [
        op("new"), op("valid", value=True), op("bytes", data=[1, 2, 3]), op("new", 1),
        op("compare", other=1), op("clone", 2, source=0), op("valid", 2, value=False),
        op("compare", other=2), op("finalize"), op("compare", other=1),
        op("assign", 1, source=0), op("valid", 1, value=False), op("compare", other=1),
        op("bytes", data=[]), op("int", value=0), op("inspect"),
        op("init"), op("compare", other=1), op("finalize", 2), op("compare", 2, other=1),
    ])
    yield case("clone-diverges-and-reset", [op("new"), op("bytes", data=byte_string(63)),
        op("clone", 1, source=0), op("bytes", data=[0]), op("bytes", 1, data=[255]),
        op("finalize"), op("finalize", 1), op("compare", other=1),
        op("assign", source=1), op("compare", other=1), op("init", 1),
        op("bytes", 1, data=byte_string(65)), op("finalize", 1), op("compare", other=1)])

    rng = random.Random(0x4CF939)
    for index in range(128):
        data = [rng.randrange(256) for _ in range(rng.randrange(513))]
        steps = [op("new")]
        while data:
            count = rng.randrange(1, 80)
            steps.append(op("bytes", data=data[:count]))
            data = data[count:]
        steps += [op("finalize"), op("valid", value=bool(index % 2)),
                  op("clone", 1, source=0), op("compare", other=1), op("init"), op("finalize")]
        yield case(f"seeded-{index}", steps)

    # Cross the low 32-bit bit counter at 512 MiB using real public Hash calls.
    # No private-state injection or substituted reference algorithm is used.
    yield case("bit-counter-carry", [op("new"),
        op("bytes", data=byte_string(65536), repeat=8191),
        op("bytes", data=byte_string(65472)), op("bytes", data=byte_string(63)),
        op("clone", 1, source=0), op("bytes", data=[0]), op("finalize"),
        op("finalize", 1), op("finalize", 1), op("clone", 2, source=1),
        op("finalize", 1), op("finalize", 2), op("compare", 1, other=2)])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    default = Path(__file__).resolve().parents[2] / "port" / "cases" / f"{UNIT}.jsonl"
    parser.add_argument("--output", type=Path, default=default)
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    count = 0
    with args.output.open("w", encoding="utf-8", newline="\n") as stream:
        for value in cases():
            stream.write(json.dumps(value, separators=(",", ":")) + "\n")
            count += 1
    print(f"Wrote {count} public generated cases to {args.output}")


if __name__ == "__main__":
    main()
