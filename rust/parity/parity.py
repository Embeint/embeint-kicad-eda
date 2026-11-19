#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Parity harness: symbol_layout.layout_box (Python) against embeint-layout (native and WASM).

Every request in cases.json, plus a fixed seeded corpus, goes through three implementations of
the same JSON boundary:

  python   the reference adapter below around tools/live_schematic_mcp/symbol_layout.py
  native   rust/embeint-layout-cli (embeint-layout binary), one request per line
  wasm     rust/embeint-web built for wasm32-unknown-unknown, run by Node.js (run_wasm.mjs)

Responses are compared structurally, including key order, the int/float distinction and each
float's exact value; any difference fails the run.  --bench also times each implementation.
Only the standard library is needed.

  python rust/parity/parity.py --build                  build with cargo, compare, skip WASM if no node
  python rust/parity/parity.py --build --require-wasm --bench 200 --report report.json
"""

import argparse
import json
import os
import random
import re
import shutil
import statistics
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
RUST = HERE.parent
REPO = RUST.parent
sys.path.insert(0, str(REPO / "tools" / "live_schematic_mcp"))

from symbol_layout import layout_box  # noqa: E402  (the authoritative implementation)

SEED = 20261119
REQUEST_FIELDS = ("pins", "arrange", "group_gap")


# ---- Python reference adapter: the JSON boundary around layout_box --------------------------------

def _error(kind, message):
    return {"error": {"type": kind, "message": message}}


def _reject_constant(name):
    raise ValueError(f"{name} is not JSON")


def reference(text):
    """The JSON response for one JSON request, as embeint-layout defines the boundary.

    The request checks mirror the MCP tool's typed parameters (pins: list of objects, arrange: str,
    group_gap: int); they live here, not in symbol_layout.py, and match embeint-layout's
    parse_request message for message.
    """
    try:
        request = json.loads(text, parse_constant=_reject_constant)
    except (ValueError, RecursionError):
        return _error("RequestError", "request is not valid JSON")
    if not isinstance(request, dict):
        return _error("RequestError", "request must be a JSON object")
    for key in request:
        if key not in REQUEST_FIELDS:
            return _error("RequestError", f"unknown request field: {key}")
    pins = request.get("pins")
    if not isinstance(pins, list) or not all(isinstance(pin, dict) for pin in pins):
        return _error("RequestError", "pins must be an array of objects")
    arrange = request.get("arrange", "auto")
    if not isinstance(arrange, str):
        return _error("RequestError", "arrange must be a string")
    group_gap = request.get("group_gap", 2)
    if isinstance(group_gap, bool) or not isinstance(group_gap, int):
        return _error("RequestError", "group_gap must be an integer")
    try:
        return layout_box(pins, arrange=arrange, group_gap=group_gap)
    except (ValueError, KeyError, TypeError) as exc:
        message = str(exc)
        if isinstance(exc, TypeError):
            # Python 3.14 words this "cannot use 'list' as a dict key (unhashable type: 'list')".
            found = re.search(r"unhashable type: '\w+'", message)
            message = found.group(0) if found else message
        return _error(type(exc).__name__, message)


def reference_text(text):
    return json.dumps(reference(text), ensure_ascii=False)


# ---- Requests ------------------------------------------------------------------------------------

NAMES = ["VIN", "VOUT", "EN", "~{SHDN}", "/EN", "ADJ", "FB", "PG", "~{PGOOD}", "GND", "PGND", "AGND", "GND2",
         "EP", "VSS", "VCC", "VDD", "SW", "BST", "SS", "MODE", "SDA", "SCL", "MOSI", "MISO", "SCK", "~{CS}",
         "~{RESET}", "XTAL1", "XTAL2", "NC", "DNC", "N.C.", "nc", "", "IO_LONG_SIGNAL_NAME", "µC_VDD", "Ω",
         "D+", "D-", "!INT", "#WR", "!PG", "#FB", "!NC", "0V", "PAD", "SENSE", "BYPASS"]
TYPES = ["input", "output", "bidirectional", "tri_state", "passive", "free", "unspecified", "power_in",
         "power_out", "open_collector", "open_emitter", "no_connect"]


def generated_requests(count, seed=SEED):
    """A fixed pseudo-random corpus: mostly valid pin lists, some with the errors the cases cover."""
    rng = random.Random(seed)
    requests = []
    for index in range(count):
        pins = []
        for n in range(rng.randint(1, 48)):
            if rng.random() < 0.06:
                pins.append({"gap": True, "side": rng.choice(["left", "right", "top", "bottom"])})
                continue
            pin = {"number": str(n + 1) if rng.random() < 0.995 else str(rng.randint(1, 3)),
                   "name": rng.choice(NAMES)}
            if rng.random() < 0.9:
                pin["type"] = rng.choice(TYPES)
            if rng.random() < 0.3:
                pin["side"] = rng.choice(["left", "right", "top", "bottom", ""])
            if rng.random() < 0.1:
                pin["hidden"] = rng.choice([True, False])
            if rng.random() < 0.1:
                pin["shape"] = rng.choice(["SPS_INVERTED", "SPS_CLOCK"])
            if rng.random() < 0.1:
                pin["unit"] = rng.randint(1, 4)
            if rng.random() < 0.002:
                pin["type"] = "magic"
            pins.append(pin)
        rng.shuffle(pins)
        request = {"pins": pins}
        if rng.random() < 0.3:
            request["arrange"] = rng.choice(["auto", "as_given"])
        if rng.random() < 0.4:
            request["group_gap"] = rng.randint(0, 6) if rng.random() < 0.95 else 7
        requests.append((f"generated_{index:04d}", request))
    return requests


def large_request(count=256):
    """A big MCU-like symbol: supplies, grounds, many IOs, top/bottom pins and NC pins."""
    pins = []
    for n in range(1, count + 1):
        if n % 32 == 1:
            pin = {"name": "VDD", "type": "power_in"}
        elif n % 32 == 2:
            pin = {"name": "GND", "type": "power_in"}
        elif n % 40 == 3:
            pin = {"name": "NC", "type": "passive"}
        elif n % 50 == 4:
            pin = {"name": f"TOP{n}", "type": "input", "side": "top"}
        else:
            pin = {"name": f"P{n // 16}_{n % 16}", "type": "bidirectional"}
        pins.append({"number": str(n), **pin})
    return {"pins": pins}


def load_cases(generated):
    cases = []
    for case in json.loads((HERE / "cases.json").read_text(encoding="utf-8")):
        text = case["raw"] if "raw" in case else json.dumps(case["request"], ensure_ascii=False)
        cases.append({"name": case["name"], "group": case.get("group", "fixed"), "text": text})
    cases.append({"name": "large_256_pins", "group": "fixed", "text": json.dumps(large_request())})
    for name, request in generated_requests(generated):
        cases.append({"name": name, "group": "generated", "text": json.dumps(request, ensure_ascii=False)})
    for case in cases:
        if "\n" in case["text"] or "\r" in case["text"]:
            raise SystemExit(f"{case['name']}: requests are sent one per line and must not contain newlines")
    return cases


# ---- Comparison ----------------------------------------------------------------------------------

class _Object(tuple):
    """A decoded JSON object as its ordered (key, value) pairs."""


def canonical(text):
    """A comparable form of a JSON response: key order, int vs float and exact float values count."""
    def walk(value):
        if isinstance(value, _Object):
            return ("object", tuple((key, walk(item)) for key, item in value))
        if isinstance(value, list):
            return ("array", tuple(walk(item) for item in value))
        if isinstance(value, bool) or value is None or isinstance(value, str):
            return (type(value).__name__, value)
        if isinstance(value, int):
            return ("int", value)
        return ("float", repr(value))
    return walk(json.loads(text, object_pairs_hook=_Object))


def first_difference(a, b, path="$"):
    if a == b:
        return None
    if a[0] != b[0] or a[0] not in ("object", "array"):
        return f"{path}: {a!r} != {b!r}"
    if a[0] == "array":
        for index, (x, y) in enumerate(zip(a[1], b[1])):
            found = first_difference(x, y, f"{path}[{index}]")
            if found:
                return found
        return f"{path}: length {len(a[1])} != {len(b[1])}"
    keys_a, keys_b = [k for k, _ in a[1]], [k for k, _ in b[1]]
    if keys_a != keys_b:
        return f"{path}: keys {keys_a} != {keys_b}"
    for (key, x), (_, y) in zip(a[1], b[1]):
        found = first_difference(x, y, f"{path}.{key}")
        if found:
            return found
    return f"{path}: differs"


# ---- Implementations -----------------------------------------------------------------------------

def run_lines(command, cases, extra=()):
    payload = "".join(case["text"] + "\n" for case in cases).encode("utf-8")
    start = time.perf_counter()
    completed = subprocess.run([*command, *extra], input=payload, capture_output=True, check=False)
    elapsed = time.perf_counter() - start
    if completed.returncode != 0:
        raise RuntimeError(f"{command[0]} failed ({completed.returncode}): {completed.stderr.decode(errors='replace')}")
    # Split on newlines only: str.splitlines() would also split at U+2028 or U+0085 in a pin name.
    lines = completed.stdout.decode("utf-8").split("\n")
    lines = lines[:-1] if lines and not lines[-1] else lines
    if len(lines) != len(cases):
        raise RuntimeError(f"{command[0]} returned {len(lines)} responses for {len(cases)} requests")
    return lines, elapsed


def bench_python(cases, iterations, layout_only=False):
    """Best of 5 rounds of N calls per case, in ns per call.  layout_only times layout_box on
    already-decoded pins, as the MCP calls it, instead of the whole JSON boundary."""
    results = []
    for case in cases:
        request = json.loads(case["text"])
        call = ((lambda: layout_box(request["pins"], arrange=request.get("arrange", "auto"),
                                    group_gap=request.get("group_gap", 2)))
                if layout_only else (lambda: reference_text(case["text"])))
        best = float("inf")
        for _ in range(5):
            start = time.perf_counter_ns()
            for _ in range(iterations):
                call()
            best = min(best, (time.perf_counter_ns() - start) / iterations)
        results.append(best)
    return results


def cargo_target_dir(cargo):
    metadata = subprocess.run([cargo, "metadata", "--format-version", "1", "--no-deps"], cwd=RUST,
                              capture_output=True, text=True, check=True)
    return Path(json.loads(metadata.stdout)["target_directory"])


def build(cargo):
    subprocess.run([cargo, "build", "--release", "--locked", "-p", "embeint-layout-cli"], cwd=RUST, check=True)
    wasm = subprocess.run([cargo, "build", "--release", "--locked", "-p", "embeint-web",
                           "--target", "wasm32-unknown-unknown"], cwd=RUST, check=False)
    if wasm.returncode != 0:
        print("WASM build failed (is the wasm32-unknown-unknown target installed?)", file=sys.stderr)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--build", action="store_true", help="build the native runner and WASM module first")
    parser.add_argument("--cargo", default=os.environ.get("CARGO", "cargo"))
    parser.add_argument("--node", default=os.environ.get("NODE", "node"))
    parser.add_argument("--native", type=Path, help="embeint-layout binary (default: cargo's release build)")
    parser.add_argument("--wasm", type=Path, help="embeint_web.wasm (default: cargo's wasm32 release build)")
    parser.add_argument("--require-wasm", action="store_true", help="fail if the WASM module or node is missing")
    parser.add_argument("--generated", type=int, default=500, help="seeded corpus size (default 500)")
    parser.add_argument("--bench", type=int, default=0, metavar="N", help="time N calls per fixed valid case")
    parser.add_argument("--report", type=Path, help="write a JSON report here")
    args = parser.parse_args()

    if args.build:
        build(args.cargo)
    native, wasm = args.native, args.wasm
    if native is None or wasm is None:
        target = cargo_target_dir(args.cargo)
        exe = ".exe" if os.name == "nt" else ""
        native = native or target / "release" / f"embeint-layout{exe}"
        wasm = wasm or target / "wasm32-unknown-unknown" / "release" / "embeint_web.wasm"
    if not native.is_file():
        raise SystemExit(f"native runner not found: {native} (use --build)")
    node = shutil.which(args.node)
    implementations = {"native": [str(native)]}
    if node and wasm.is_file():
        implementations["wasm"] = [node, str(HERE / "run_wasm.mjs"), str(wasm)]
    elif args.require_wasm:
        raise SystemExit(f"WASM run required but {'node' if not node else wasm} is missing")
    else:
        print(f"skipping WASM: {'node not found' if not node else f'{wasm} not built'}")

    cases = load_cases(args.generated)
    start = time.perf_counter()
    expected = [reference_text(case["text"]) for case in cases]
    python_seconds = time.perf_counter() - start
    expected_canonical = [canonical(text) for text in expected]

    report = {"python": sys.version.split()[0], "platform": sys.platform, "cases": len(cases),
              "fixed": sum(c["group"] != "generated" for c in cases),
              "errors": sum("error" in json.loads(text) for text in expected),
              "corpus_seconds": {"python": round(python_seconds, 4)}, "mismatches": {}, "bench": {}}
    failed = False
    for label, command in implementations.items():
        lines, seconds = run_lines(command, cases)
        report["corpus_seconds"][label] = round(seconds, 4)
        mismatches = []
        for case, want, got in zip(cases, expected_canonical, lines):
            try:
                difference = first_difference(want, canonical(got))
            except ValueError as exc:
                difference = f"invalid JSON from {label}: {exc}"
            if difference:
                mismatches.append({"case": case["name"], "difference": difference, "request": case["text"]})
        report["mismatches"][label] = mismatches
        failed |= bool(mismatches)
        print(f"{label}: {len(cases) - len(mismatches)}/{len(cases)} identical to Python, "
              f"corpus in {seconds * 1000:.1f} ms (process start included)")
        for mismatch in mismatches[:10]:
            print(f"  MISMATCH {mismatch['case']}: {mismatch['difference']}")
    print(f"python: {len(cases)} requests in {python_seconds * 1000:.1f} ms in-process "
          f"({report['errors']} are error responses, {report['fixed']} fixed cases)")

    if args.bench:
        bench_cases = [c for c, text in zip(cases, expected)
                       if c["group"] != "generated" and "error" not in json.loads(text)]
        timings = {"python": bench_python(bench_cases, args.bench)}
        for label, command in implementations.items():
            lines, _ = run_lines(command, bench_cases, ("--bench", str(args.bench)))
            timings[label] = [json.loads(line)["ns_per_call"] for line in lines]
        largest = next(i for i, c in enumerate(bench_cases) if c["name"] == "large_256_pins")
        print(f"\nper call (JSON in, JSON out), best of 5 x {args.bench}, {len(bench_cases)} valid fixed cases:")
        print(f"  {'':8} {'median us':>10} {'max us':>10} {'256 pins us':>12} {'speed-up':>9}")
        python_median = statistics.median(timings["python"])
        for label, values in timings.items():
            median = statistics.median(values)
            report["bench"][label] = {"median_us": round(median / 1000, 2), "max_us": round(max(values) / 1000, 2),
                                      "large_256_pins_us": round(values[largest] / 1000, 2),
                                      "per_case_us": {c["name"]: round(v / 1000, 2)
                                                      for c, v in zip(bench_cases, values)}}
            print(f"  {label:8} {median / 1000:10.2f} {max(values) / 1000:10.2f} {values[largest] / 1000:12.2f} "
                  f"{python_median / median:8.1f}x")
        layout_only = bench_python(bench_cases, args.bench, layout_only=True)
        report["bench"]["python_layout_only"] = {
            "median_us": round(statistics.median(layout_only) / 1000, 2),
            "max_us": round(max(layout_only) / 1000, 2),
            "large_256_pins_us": round(layout_only[largest] / 1000, 2)}
        print(f"  (python layout_box alone, pins already decoded as in the MCP: median "
              f"{statistics.median(layout_only) / 1000:.2f} us, 256 pins {layout_only[largest] / 1000:.2f} us)")

    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=1, ensure_ascii=False), encoding="utf-8")
    print("PARITY FAILED" if failed else "parity: all implementations identical")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
