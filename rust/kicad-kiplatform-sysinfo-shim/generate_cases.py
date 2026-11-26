#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fixed-seed public corpus and refusal controls for
libs.kiplatform.os.unix.application.pure.p001.

Writes port/cases/<unit>.jsonl (requests only; every one is inside the reference's
defined domain, with every output-argument field initialized before any
observation) and port/oracle/fixtures/kiplatform-unix-p001-controls.jsonl
(malformed requests both adapters must refuse, with the expected refusal). It never
computes a reference reply. Inputs are boundary values and a splitmix64 stream with
a fixed seed. `--check` verifies the committed files.
"""
import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
UNIT = "libs.kiplatform.os.unix.application.pure.p001"
CASES = ROOT / "port/cases" / (UNIT + ".jsonl")
CONTROLS = ROOT / "port/oracle/fixtures/kiplatform-unix-p001-controls.jsonl"
EXPECTED = ROOT / "rust/kicad-kiplatform-sysinfo-shim/expected-replies.json"
I64_MIN, I64_MAX, U32_MAX, U64_MAX = -(1 << 63), (1 << 63) - 1, (1 << 32) - 1, (1 << 64) - 1
MAX_ENTRIES, MAX_STEPS, MAX_TEXT_BYTES = 64, 64, 4096
STEPS = ("memory", "cpu", "gpu", "reconstruct")
# Python cannot serialize the JSON number -0; this token is replaced after encoding.
NEG0 = "\x00neg0"


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

    def u32(self):
        return self.choice((0, 1, U32_MAX, self.word() >> 32))

    def u64(self):
        return self.choice((0, 1, U64_MAX, self.word()))

    def i64(self):
        return self.choice((I64_MIN, -1, 0, I64_MAX, self.word() - (1 << 63)))

    def text(self):
        return self.bytes(self.choice((0, 1, 15, 16, 23, 64, self.below(300)))).hex()


def memory(usage=0, value=0):
    return {"usage": usage, "total_physical": value, "free_physical": value,
            "total_paging": value, "free_paging": value, "total_virtual": value,
            "free_virtual": value}


def cpu(name="", manufacturer="", cores=0, logical=0):
    return {"name": name, "manufacturer": manufacturer, "number_cores": cores,
            "number_logical": logical}


def gpu(name="", size=0, driver="", manufacturer=""):
    return {"name": name, "memory_size": size, "driver_version": driver,
            "manufacturer": manufacturer}


def sysinfo(mem, cpus, gpus, steps):
    return {"op": "sysinfo", "memory": mem, "cpus": cpus, "gpus": gpus, "steps": list(steps)}


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


def random_memory(stream):
    return {"usage": stream.u32(), "total_physical": stream.u64(), "free_physical": stream.u64(),
            "total_paging": stream.u64(), "free_paging": stream.u64(),
            "total_virtual": stream.u64(), "free_virtual": stream.u64()}


def build():
    corpus = Corpus()
    corpus.case("driver", {"op": "valid_3dconnexion_driver_version"})
    corpus.case("driver-repeat", {"op": "valid_3dconnexion_driver_version"})

    # Lifecycle alone: construction and destruction with no query, then every
    # single query and every pair, on empty and on populated fixtures.
    populated = (memory(7, 123456789),
                 [cpu("41", "", 4, 8), cpu("", "00ff", U32_MAX, 0)],
                 [gpu("6e756c00", I64_MIN, "80c3", ""), gpu("", I64_MAX, "", "ffffffff")])
    for label, (mem, cpus, gpus) in (("empty", (memory(), [], [])), ("populated", populated)):
        corpus.case(f"lifecycle-{label}", sysinfo(mem, cpus, gpus, ()))
        for first in STEPS:
            corpus.case(f"single-{label}", sysinfo(mem, cpus, gpus, (first,)))
            for second in STEPS:
                corpus.case(f"pair-{label}", sysinfo(mem, cpus, gpus, (first, second)))

    # Field boundaries.
    for usage, value in ((0, 0), (U32_MAX, U64_MAX), (1, 1), (U32_MAX - 1, U64_MAX - 1)):
        corpus.case("memory-bounds", sysinfo(memory(usage, value), [], [], ("memory",)))
    corpus.case("memory-distinct", sysinfo(
        {"usage": 1, "total_physical": 2, "free_physical": 3, "total_paging": 4,
         "free_paging": 5, "total_virtual": 6, "free_virtual": U64_MAX},
        [], [], ("memory", "memory")))
    for cores, logical in ((0, 0), (U32_MAX, U32_MAX), (1, U32_MAX), (U32_MAX, 1)):
        corpus.case("cpu-bounds", sysinfo(memory(), [cpu("", "", cores, logical)], [], ("cpu",)))
    for size in (I64_MIN, I64_MIN + 1, -1, 0, 1, I64_MAX - 1, I64_MAX):
        corpus.case("gpu-bounds", sysinfo(memory(), [], [gpu("", size, "", "")], ("gpu",)))
    # "-0" is a defined zero in both decoders (nlohmann stores it as a signed zero).
    corpus.case("negative-zero", sysinfo(memory(NEG0, NEG0), [cpu("", "", NEG0, NEG0)],
                                         [gpu("", NEG0, "", "")], ("memory", "cpu", "gpu")))

    # std::string contents: NUL, non-UTF-8, every byte value and the length bound
    # (short and long string representations alike).
    every = bytes(range(256)).hex()
    longest = (b"\x00\xff" * (MAX_TEXT_BYTES // 2)).hex()
    texts = ("", "00", "0000", "ff", "c328", "f09f9880", "ed a080".replace(" ", ""),
             every, "41" * 15, "41" * 16, "42" * 23, longest)
    for text in texts:
        corpus.case("text", sysinfo(memory(), [cpu(text, text[::-1] if len(text) % 4 == 0 else "",
                                                   1, 2)],
                                    [gpu(text, 3, text, text)], ("cpu", "gpu", "reconstruct",
                                                                 "cpu", "gpu")))

    # Entry-count bounds.
    many_cpus = [cpu(f"{i:02x}", "", i, MAX_ENTRIES - i) for i in range(MAX_ENTRIES)]
    many_gpus = [gpu("", i - 32, f"{i:02x}" * (i % 5), "") for i in range(MAX_ENTRIES)]
    corpus.case("entries-max", sysinfo(memory(1, 2), many_cpus, many_gpus, ("cpu", "gpu")))
    corpus.case("entries-max-other-only", sysinfo(memory(), many_cpus, [], ("gpu", "memory")))

    # Step-count bound and repetition, with reconstruction between queries.
    corpus.case("steps-max", sysinfo(*populated, [STEPS[i % 4] for i in range(MAX_STEPS)]))
    corpus.case("steps-reconstruct-only", sysinfo(*populated, ["reconstruct"] * 8))
    corpus.case("steps-repeat-each", sysinfo(*populated, ["memory"] * 5 + ["cpu"] * 5 + ["gpu"] * 5))

    # Fixed-seed fixtures.
    stream = Stream(0x5953494E464F2D31)
    for _ in range(48):
        cpus = [cpu(stream.text(), stream.text(), stream.u32(), stream.u32())
                for _ in range(stream.choice((0, 1, 2, 5, stream.below(MAX_ENTRIES + 1))))]
        gpus = [gpu(stream.text(), stream.i64(), stream.text(), stream.text())
                for _ in range(stream.choice((0, 1, 3, stream.below(MAX_ENTRIES + 1))))]
        steps = [stream.choice(STEPS) for _ in range(stream.choice((0, 1, 4, stream.below(MAX_STEPS + 1))))]
        corpus.case("seeded", sysinfo(random_memory(stream), cpus, gpus, steps))

    build_controls(corpus)
    return corpus


def build_controls(corpus):
    refuse = corpus.refuse
    ok_cpu, ok_gpu = cpu("41", "", 1, 2), gpu("", 3, "", "")
    base = lambda **overrides: {**sysinfo(memory(), [ok_cpu], [ok_gpu], ("cpu",)), **overrides}

    refuse("op", {}, "expected string: op")
    refuse("op", {"op": 1}, "expected string: op")
    refuse("op", {"op": None}, "expected string: op")
    refuse("op", {"op": "Valid3DConnexionDriverVersion"}, "unknown operation")
    refuse("op", {"op": "GetMemoryInfo"}, "unknown operation")
    refuse("op", {"op": ""}, "unknown operation")
    refuse("driver-keys", {"op": "valid_3dconnexion_driver_version", "steps": []},
           "unexpected or missing field")

    top = base()
    for key in ("memory", "cpus", "gpus", "steps"):
        missing = dict(top)
        del missing[key]
        refuse("keys", missing, "unexpected or missing field")
    refuse("keys", {**top, "extra": 1}, "unexpected or missing field")

    # Memory: shape, then each field in decoding order.
    refuse("memory", base(memory=[]), "unexpected or missing field: memory")
    refuse("memory", base(memory=None), "unexpected or missing field: memory")
    missing = memory()
    del missing["free_virtual"]
    refuse("memory", base(memory=missing), "unexpected or missing field: memory")
    refuse("memory", base(memory={**memory(), "Usage": 0}), "unexpected or missing field: memory")
    for field, bad in (("usage", U32_MAX + 1), ("usage", -1), ("usage", 1.0), ("usage", "1"),
                       ("total_physical", U64_MAX + 1), ("free_physical", -1),
                       ("total_paging", True), ("free_paging", None), ("total_virtual", 0.5),
                       ("free_virtual", 1e30), ("free_virtual", -(1 << 63) - 1)):
        refuse("memory-field", base(memory={**memory(), field: bad}),
               f"integer out of range: memory.{field}")
    # The first bad field in decoding order is the one reported.
    refuse("memory-order", base(memory={**memory(), "usage": -1, "free_virtual": -1}),
           "integer out of range: memory.usage")

    # CPU and GPU arrays.
    for name in ("cpus", "gpus"):
        refuse(name, base(**{name: {}}), f"expected array: {name}")
        refuse(name, base(**{name: "[]"}), f"expected array: {name}")
        refuse(name, base(**{name: [ok_cpu if name == "cpus" else ok_gpu] * (MAX_ENTRIES + 1)}),
               f"too many entries: {name}")
        refuse(name, base(**{name: [1]}), f"unexpected or missing field: {name}")
        wrong = ok_gpu if name == "cpus" else ok_cpu
        refuse(name, base(**{name: [wrong]}), f"unexpected or missing field: {name}")
    for field, bad, message in (
            ("name", 1, "expected string: cpus.name"),
            ("name", "4", "invalid hex: cpus.name"),
            ("name", "4G", "invalid hex: cpus.name"),
            ("name", "4A", "invalid hex: cpus.name"),
            ("name", "41" * (MAX_TEXT_BYTES + 1), "invalid hex: cpus.name"),
            ("manufacturer", None, "expected string: cpus.manufacturer"),
            ("manufacturer", " 41", "invalid hex: cpus.manufacturer"),
            ("number_cores", -1, "integer out of range: cpus.number_cores"),
            ("number_cores", U32_MAX + 1, "integer out of range: cpus.number_cores"),
            ("number_logical", 2.5, "integer out of range: cpus.number_logical")):
        refuse("cpu-field", base(cpus=[ok_cpu, {**ok_cpu, field: bad}]), message)
    for field, bad, message in (
            ("name", [], "expected string: gpus.name"),
            ("memory_size", I64_MAX + 1, "integer out of range: gpus.memory_size"),
            ("memory_size", I64_MIN - 1, "integer out of range: gpus.memory_size"),
            ("memory_size", U64_MAX, "integer out of range: gpus.memory_size"),
            ("memory_size", "0", "integer out of range: gpus.memory_size"),
            ("driver_version", "abc", "invalid hex: gpus.driver_version"),
            ("manufacturer", "0x00", "invalid hex: gpus.manufacturer")):
        refuse("gpu-field", base(gpus=[{**ok_gpu, field: bad}]), message)
    # Memory is decoded before the arrays, CPUs before GPUs, GPUs before steps.
    refuse("order", base(memory={**memory(), "usage": -1}, cpus=7, gpus=7, steps=7),
           "integer out of range: memory.usage")
    refuse("order", base(cpus=7, gpus=7, steps=7), "expected array: cpus")
    refuse("order", base(gpus=7, steps=7), "expected array: gpus")

    # Steps.
    refuse("steps", base(steps={}), "expected array: steps")
    refuse("steps", base(steps="cpu"), "expected array: steps")
    refuse("steps", base(steps=["cpu"] * (MAX_STEPS + 1)), "too many entries: steps")
    for bad in ("CPU", "", "destroy", 1, None, ["cpu"]):
        refuse("step", base(steps=["cpu", bad]), "unknown step")


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
    for path, text in outputs().items():
        if args.check:
            if not path.exists() or path.read_bytes() != text.encode("utf-8"):
                stale.append(str(path.relative_to(ROOT)))
        else:
            path.write_bytes(text.encode("utf-8"))
    if stale:
        raise SystemExit("stale generated files: " + ", ".join(stale))


if __name__ == "__main__":
    main()
