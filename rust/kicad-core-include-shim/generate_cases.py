#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fixed-seed public corpus and refusal controls for libs.core.include.core.application.pure.p001.

Writes port/cases/<unit>.jsonl (requests only; every one is inside the reference's
defined domain) and port/oracle/fixtures/core-include-p001-controls.jsonl (requests
that both adapters must refuse, with the expected refusal). It never computes a
reference reply. Inputs come from the upstream QA tests
(qa/tests/common/test_union_find.cpp, test_filter_kruskal.cpp: the same std::mt19937
seeds and trial parameters), boundary values and a splitmix64 stream with a fixed
seed. `--check` verifies the committed files.
"""
import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
UNIT = "libs.core.include.core.application.pure.p001"
CASES = ROOT / "port/cases" / (UNIT + ".jsonl")
CONTROLS = ROOT / "port/oracle/fixtures/core-include-p001-controls.jsonl"
ADVERSARY = ROOT / "rust/kicad-core-include-shim/adversary-weights.json"
I32_MIN, I32_MAX = -(1 << 31), (1 << 31) - 1
I64_MIN, I64_MAX, U64_MAX = -(1 << 63), (1 << 63) - 1, (1 << 64) - 1
U32_MAX = (1 << 32) - 1
CLOCK_LIMIT = (1 << 62) - 1


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


class Mt19937:
    """std::mt19937 (init_genrand seeding), counting outputs so a C++ or Rust adapter
    can reproduce a QA trial's draws with discard()."""

    def __init__(self, seed):
        self.mt = [0] * 624
        self.mt[0] = seed & 0xFFFFFFFF
        for i in range(1, 624):
            prev = self.mt[i - 1]
            self.mt[i] = (1812433253 * (prev ^ (prev >> 30)) + i) & 0xFFFFFFFF
        self.index = 624
        self.drawn = 0

    def __call__(self):
        if self.index >= 624:
            for i in range(624):
                y = (self.mt[i] & 0x80000000) | (self.mt[(i + 1) % 624] & 0x7FFFFFFF)
                nxt = self.mt[(i + 397) % 624] ^ (y >> 1)
                if y & 1:
                    nxt ^= 0x9908B0DF
                self.mt[i] = nxt
            self.index = 0
        y = self.mt[self.index]
        self.index += 1
        self.drawn += 1
        y ^= y >> 11
        y ^= (y << 7) & 0x9D2C5680
        y ^= (y << 15) & 0xEFC60000
        return y ^ (y >> 18)

    def skip(self, n):
        for _ in range(n):
            self()


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


# ---- MINOPTMAX ----------------------------------------------------------------------------------

def minoptmaxes(c, rng):
    for kind, low, high in (("int", I32_MIN, I32_MAX), ("long long", I64_MIN, I64_MAX)):
        edges = [low, low + 1, -2, -1, 0, 1, 2, 100, high - 1, high]
        probes = sorted(set(edges + [-100, 50, 150, 1000]))
        defaults = [low, -5, 0, 7, 99, 101, high]

        def add(sets):
            c.add("minoptmax", {"op": "minoptmax", "type": kind, "sets": sets,
                                "probes": probes, "defaults": defaults})

        add([])
        for which in ("min", "max", "opt"):
            for value in edges:
                add([{"set": which, "value": value}])
        orders = [("min", "opt", "max"), ("min", "max", "opt"), ("opt", "min", "max"),
                  ("opt", "max", "min"), ("max", "min", "opt"), ("max", "opt", "min")]
        triples = [(0, 50, 100), (100, 50, 0), (10, 5, 20), (10, 25, 20), (7, 7, 7),
                   (low, 0, high), (high, low, low), (-100, -150, -50)]
        for order in orders:
            for triple in triples:
                values = dict(zip(("min", "opt", "max"), triple))
                add([{"set": which, "value": values[which]} for which in order])
        # Pairs leave one value unset; repeated setters overwrite.
        for a, b in (("min", "max"), ("min", "opt"), ("opt", "max")):
            for x, y in ((0, 100), (100, 0), (-1, -1)):
                add([{"set": a, "value": x}, {"set": b, "value": y}])
        add([{"set": "min", "value": 5}, {"set": "min", "value": -5}, {"set": "opt", "value": 1},
             {"set": "opt", "value": 2}])
        for _ in range(24):
            sets = [{"set": rng.choice(["min", "max", "opt"]),
                     "value": rng.choice(edges + [rng.below(2001) - 1000])}
                    for _ in range(1 + rng.below(5))]
            add(sets)


# ---- KI_UNION_FIND ------------------------------------------------------------------------------

def union_finds(c, rng):
    def add(count, steps):
        c.add("union_find", {"op": "union_find", "count": count, "steps": steps})

    def unite(a, b):
        return {"op": "unite", "a": a, "b": b}

    # qa/tests/common/test_union_find.cpp: DegenerateSizes.
    add(0, [{"op": "size"}, {"op": "component_count"}])
    add(1, [unite(0, 0), {"op": "find", "x": 0}, {"op": "component_count"}])
    add(2, [unite(1, 0), unite(0, 1), {"op": "find", "x": 1}, {"op": "component_count"}])
    # ResetClearsState.
    add(10, [unite(0, i) for i in range(1, 10)] + [{"op": "component_count"}, {"op": "reset", "count": 5},
                                                   {"op": "size"}, {"op": "component_count"}]
        + [{"op": "find", "x": i} for i in range(5)])
    # Chains in both directions, stars, and the reverse chain of ReverseChainUnderContention.
    for n in (8, 64, 1000):
        add(n, [unite(i, i + 1) for i in range(n - 1)] + [{"op": "find", "x": n - 1}])
        add(n, [unite(i + 1, i) for i in reversed(range(n - 1))] + [{"op": "find_compress", "x": n - 1}])
        add(n, [unite(n - 1, i) for i in range(n - 1)] + [{"op": "connected", "a": 0, "b": n - 1}])
    # Index truncation to 32 bits: 2^32 + k is index k.
    add(6, [unite((1 << 32) + 5, 3), {"op": "find", "x": (2 << 32) + 5}, {"op": "find_compress", "x": (7 << 32) + 4},
            {"op": "connected", "a": U64_MAX - U32_MAX + 3, "b": 5}])
    # Counts above 2^32 - 1 throw std::length_error (constructor and Reset).
    for count in (1 << 32, (1 << 32) + 1, 1 << 40, U64_MAX):
        add(count, [])
        add(3, [unite(2, 1), {"op": "reset", "count": count}, {"op": "size"}, {"op": "component_count"},
                {"op": "find", "x": 2}])
    add(4, [unite(3, 2), {"op": "reset", "count": 0}, {"op": "size"}, {"op": "reset", "count": 8192},
            {"op": "component_count"}])
    # Random scripts (RootIsMinimumOfComponent / CompressionPreservesPartition shapes).
    for n in (1, 2, 3, 5, 8, 17, 64, 257, 1000, 4096, 8192):
        for _ in range(4 if n < 1000 else 2):
            steps = []
            for _ in range(min(4 * n, 6000)):
                kind = rng.below(10)
                if kind < 5:
                    steps.append(unite(rng.below(n), rng.below(n)))
                elif kind < 7:
                    steps.append({"op": "find", "x": rng.below(n)})
                elif kind < 9:
                    steps.append({"op": "find_compress", "x": rng.below(n)})
                else:
                    steps.append({"op": "connected", "a": rng.below(n), "b": rng.below(n)})
            steps.append({"op": "component_count"})
            add(n, steps)


# ---- KI_MST::FilterKruskal ---------------------------------------------------------------------

def kruskals(c, rng):
    def explicit(vertices, edges, less="weight_id", unite=None):
        inp = {"op": "filter_kruskal", "vertices": vertices, "less": less, "edges": edges}
        if unite is not None:
            inp["unite"] = unite
        c.add("filter_kruskal", inp)

    def generated(vertices, less, spec, unite=None):
        inp = {"op": "filter_kruskal", "vertices": vertices, "less": less, "generate": spec}
        if unite is not None:
            inp["unite"] = unite
        c.add("filter_kruskal", inp)

    for less in ("weight", "weight_id", "weight_uv"):
        explicit(0, [], less)
        explicit(5, [], less)
        explicit(2, [[0, 1, 7]], less)
        explicit(3, [[0, 0, 1], [1, 1, 0], [2, 2, -1]], less)  # self loops
        explicit(4, [[0, 1, 3], [1, 0, 3], [0, 1, 3], [2, 3, 3], [3, 2, 3], [1, 2, 3]], less)  # ties
        explicit(4, [[0, 1, I64_MAX], [1, 2, I64_MIN], [2, 3, 0], [3, 0, -1], [0, 2, I64_MAX - 1]], less)
        explicit(6, [[i, (i * 5 + 1) % 6, (i * 7) % 4] for i in range(6)] + [[5, 0, 0]], less)
    # A forest already joined is never touched: endpoints beyond the set are not read.
    explicit(1, [[5, 9, 1], [0, 0, 2]])
    explicit(2, [[7, 8, 1]], unite=[[0, 1]])
    explicit(3, [[0, 1, 5], [1, 2, 5], [0, 2, 1]], unite=[[2, 0]])
    # Explicit lists just below and just above the threshold, with heavy ties.
    for count in (1023, 1024, 1025, 2049, 4096):
        for less in ("weight", "weight_id"):
            n = 1 + count // 3
            edges = [[rng.below(n), rng.below(n), rng.below(4)] for _ in range(count)]
            explicit(n, edges, less)

    # McIlroy adversary weights (adversary-weights.json, reproduced by the shim's
    # tests): sort-* drive the introsort of a range at or below the threshold into its
    # heap fallback, nth-* the first nth_element of a larger range into heap_select.
    adversary = json.loads(ADVERSARY.read_text(encoding="utf-8"))
    for name, weights in adversary.items():
        path = [[i, i + 1, w] for i, w in enumerate(weights)]
        for less in ("weight_id", "weight"):
            explicit(len(weights) + 1, path, less)
        star = [[0, 1 + (i * 7) % len(weights), w] for i, w in enumerate(weights)]
        explicit(len(weights) + 1, star, "weight_uv", unite=[[1, 2], [3, 4]])

    # qa/tests/common/test_filter_kruskal.cpp, with each trial's std::mt19937 draws.
    qa = Mt19937(555)
    for _ in range(60):  # MatchesPlainKruskal
        nodes = 2 + qa() % 2000
        count = qa() % (5 * nodes)
        spec = {"seed": 555, "discard": qa.drawn, "count": count, "weight_range": 100000}
        qa.skip(3 * count)
        generated(nodes, "weight_uv", spec)
    qa = Mt19937(12)
    for _ in range(8):  # AllWeightsEqual
        nodes = 500 + qa() % 2000
        spec = {"seed": 12, "discard": qa.drawn, "count": 5 * nodes, "weight_range": 100000, "weight_set": 0}
        qa.skip(3 * 5 * nodes)
        generated(nodes, "weight_uv", spec)
        generated(nodes, "weight", spec)
    qa = Mt19937(606)
    for _ in range(8):  # FewDistinctWeights
        nodes = 1000 + qa() % 1500
        spec = {"seed": 606, "discard": qa.drawn, "count": 6 * nodes, "weight_range": 2}
        qa.skip(3 * 6 * nodes)
        generated(nodes, "weight_uv", spec)
        generated(nodes, "weight", spec)
    # DisconnectedGraphYieldsForest, HonoursPreSeededForest, SelectionIsReproducible.
    generated(4000, "weight_uv", {"seed": 808, "discard": 0, "count": 20000, "weight_range": 1000, "islands": 8})
    generated(3000, "weight_uv", {"seed": 909, "discard": 0, "count": 15000, "weight_range": 5000,
                                   "seeds": 2000, "weight_add": 1})
    generated(5000, "weight_uv", {"seed": 246, "discard": 0, "count": 25000, "weight_range": 50})
    generated(4000, "weight_uv", {"seed": 1357, "discard": 0, "count": 20000, "weight_range": 40})
    # Further fixed-seed shapes: comparator variants, dense graphs, pre-united pairs.
    for i in range(24):
        nodes = rng.choice([2, 3, 50, 300, 2000, 8192])
        count = rng.choice([1025, 3000, 8000, 30000, 65536])
        spec = {"seed": rng.below(1 << 32), "discard": rng.below(1000), "count": count,
                "weight_range": rng.choice([1, 2, 3, 1000, 1 << 31])}
        unite = [[rng.below(nodes), rng.below(nodes)] for _ in range(rng.below(3) * 50)]
        generated(nodes, ["weight", "weight_id", "weight_uv"][i % 3], spec, unite or None)


# ---- KISPINLOCK and SYNC_QUEUE ----------------------------------------------------------------

def locks(c, rng):
    for steps in ([], ["test"], ["lock", "test", "unlock", "test"], ["try_lock", "try_lock", "test"],
                  ["unlock", "unlock", "test", "lock"], ["lock", "try_lock", "unlock", "try_lock", "unlock"],
                  ["try_lock", "unlock", "lock", "unlock", "lock", "test"]):
        c.add("spinlock", {"op": "spinlock", "steps": steps})
    for _ in range(8):
        held, steps = False, []
        for _ in range(40):
            op = rng.choice(["try_lock", "unlock", "test", "lock"])
            if op == "lock" and held:
                op = "unlock"
            held = (held or op in ("lock", "try_lock")) and op != "unlock"
            steps.append(op)
        c.add("spinlock", {"op": "spinlock", "steps": steps})


def queues(c, rng):
    def add(receiver, steps):
        c.add("sync_queue", {"op": "sync_queue", "receiver": receiver, "steps": steps})

    add("unchanged", [{"op": "pop"}, {"op": "empty"}, {"op": "size"}, {"op": "clear"}, {"op": "pop"}])
    add("", [{"op": "push", "value": "a"}, {"op": "move_push", "value": "b"}, {"op": "push", "value": ""},
             {"op": "size"}, {"op": "pop"}, {"op": "pop"}, {"op": "pop"}, {"op": "pop"}, {"op": "empty"}])
    add("r", [{"op": "move_push", "value": "µ日\u0000nul"}, {"op": "clear"}, {"op": "pop"},
              {"op": "push", "value": "x" * 1000}, {"op": "pop"}])
    add("", [{"op": "push", "value": str(i)} for i in range(4000)] + [{"op": "size"}, {"op": "pop"},
                                                                      {"op": "clear"}, {"op": "size"}])
    for _ in range(8):
        steps = []
        for _ in range(60):
            op = rng.choice(["push", "move_push", "pop", "pop", "empty", "size", "clear"])
            steps.append({"op": op, "value": f"v{rng.below(100)}"} if op.endswith("push") else {"op": op})
        add("start", steps)


# ---- profile.h ------------------------------------------------------------------------------------

DURATIONS = ["double_nano", "double_micro", "double_milli", "double_seconds",
             "nanoseconds", "microseconds", "milliseconds", "seconds"]


def profiles(c, rng):
    def add(clock, steps):
        c.add("profile", {"op": "profile", "clock": clock, "steps": steps})

    def t(op, timer=0, **extra):
        return {"op": op, "timer": timer, **extra}

    # Show/to_string unit selection and %g formatting over elapsed nanosecond counts.
    elapsed = [0, 1, 7, 999, 1000, 1001, 1234, 12345, 123456, 999999, 1000000, 1000001, 1234565,
               1234575, 9999995, 99999949, 999999999, 1000000000, 1000000001, 123456789012,
               999999500000, 1 << 40, 1 << 61, (1 << 62) - 2, -1, -999, -1000, -123456789, 1500, 2500]
    for value in elapsed:
        start = -1 if value > (1 << 61) else 5
        add([start, start + value, start + value],
            [{"op": "timer_new", "timer": 0, "name": "work"}, t("timer_show"), t("timer_to_string")])
        add([start, start + value],
            [{"op": "timer_default", "timer": 1}, t("timer_stop", 1), t("timer_show", 1), t("timer_to_string", 1),
             t("timer_msecs", 1), t("timer_get_name", 1)])
    # Every SinceStart duration, positive and negative, since start and since last.
    for value in (0, 1, 999, 1000, 999999, 1000000, 1500000000, -1, -999, -1000001, -2500000000,
                  123456789123, -(1 << 50)):
        steps = [{"op": "timer_new", "timer": 2, "name": "d"}]
        steps += [t("timer_since_start", 2, duration=d) for d in DURATIONS]
        steps += [t("timer_since_start", 2, duration=d, since_last=True) for d in DURATIONS]
        add([0] + [value * (i + 1) for i in range(16)], steps)
    # Stop freezes the end; a stopped timer reads no clock; Stop twice; restart.
    add([10, 30, 50, 70, 90, 110, 130],
        [{"op": "timer_new", "timer": 0, "name": "s"}, t("timer_stop"), t("timer_stop"), t("timer_msecs"),
         t("timer_msecs", since_last=True), t("timer_msecs", since_last=True), t("timer_start"),
         t("timer_msecs", since_last=False), t("timer_msecs", since_last=True), t("timer_show"),
         t("timer_stop"), t("timer_to_string")])
    # The named constructor without autostart: Show and msecs read the epoch values.
    add([], [{"op": "timer_new", "timer": 3, "name": "idle", "autostart": False}, t("timer_stop", 3),
             t("timer_show", 3), t("timer_msecs", 3), t("timer_since_start", 3, duration="seconds"),
             t("timer_get_name", 3)])
    add([100, 250, 400],
        [{"op": "timer_new", "timer": 3, "name": "late", "autostart": False}, t("timer_start", 3),
         t("timer_to_string", 3), t("timer_show", 3)])
    add([5, 6], [{"op": "timer_new", "timer": 0, "name": "", "autostart": True}, t("timer_to_string"),
                 t("timer_get_name")])
    add([5, 6], [{"op": "timer_new", "timer": 0, "name": "µ name"}, t("timer_show")])
    # SCOPED_PROF_TIMER: base constructor, then m_counter, then the destructor's read.
    for d in DURATIONS:
        add([1, 2, 1234567890], [{"op": "scoped", "duration": d, "base": []}])
        add([1, 2, 50, 60, 70, -5000000007],
            [{"op": "scoped", "duration": d,
              "base": [{"op": "timer_stop"}, {"op": "timer_msecs"}, {"op": "timer_start"},
                       {"op": "timer_show"}, {"op": "timer_get_name"}]}])
    add([0, 0, 3, 4, 5, 6, 7],
        [{"op": "scoped", "duration": "double_milli",
          "base": [{"op": "timer_to_string"}, {"op": "timer_since_start", "duration": "nanoseconds",
                                               "since_last": True}]}])
    # LATENCY_PROBE: constructor stages, Reset, Checkpoint, AddTimer, to_string, delta_ms.
    for stages in (None, 0, 1, 8, 16, 1 << 16, -1, -8, I32_MIN):
        step = {"op": "probe_new", "probe": 0, "name": "stages"}
        if stages is not None:
            step["stages"] = stages
        # A negative count throws std::length_error and leaves the slot empty.
        tail = [{"op": "probe_to_string", "probe": 0}] if stages is None or stages >= 0 else []
        add([], [step] + tail)
    add([1000, 1500, 999999, 1000000000, 1000000000, 3, 4],
        [{"op": "probe_new", "probe": 1, "name": "zoom"}, {"op": "probe_checkpoint", "probe": 1, "name": "before reset"},
         {"op": "probe_reset", "probe": 1}, {"op": "probe_checkpoint", "probe": 1, "name": "a"},
         {"op": "probe_checkpoint", "probe": 1, "name": "b" * 40},
         {"op": "timer_new", "timer": 0, "name": "timer"}, {"op": "probe_add_timer", "probe": 1, "timer": 0},
         {"op": "probe_to_string", "probe": 1}])
    # Checkpoints before the start (negative deltas print -0.000 or below), ties at .0005 ms.
    add([0, -1, -500, -1500, 2500, 3500, 1000000500],
        [{"op": "probe_new", "probe": 0, "name": "", "stages": 2}, {"op": "probe_checkpoint", "probe": 0, "name": "x"},
         {"op": "probe_checkpoint", "probe": 0, "name": "y"}, {"op": "probe_checkpoint", "probe": 0, "name": "z"},
         {"op": "probe_checkpoint", "probe": 0, "name": "w"}, {"op": "probe_checkpoint", "probe": 0, "name": "v"},
         {"op": "probe_to_string", "probe": 0}])
    add([CLOCK_LIMIT, -CLOCK_LIMIT, 0],
        [{"op": "probe_new", "probe": 0, "name": "far"}, {"op": "probe_reset", "probe": 0},
         {"op": "probe_checkpoint", "probe": 0, "name": "min"}, {"op": "probe_checkpoint", "probe": 0, "name": "zero"},
         {"op": "probe_to_string", "probe": 0}])
    add([7, 8, 9],
        [{"op": "timer_new", "timer": 1, "name": "stopped", "autostart": False},
         {"op": "probe_new", "probe": 0, "name": "p"}, {"op": "probe_add_timer", "probe": 0, "timer": 1},
         {"op": "timer_start", "timer": 1}, {"op": "probe_add_timer", "probe": 0, "timer": 1},
         {"op": "probe_add_timer", "probe": 0, "timer": 1}, {"op": "probe_to_string", "probe": 0}])
    deltas = [(0, 0), (1, 0), (0, 1), (500, 0), (1500, 0), (-500, 0), (I64_MAX, 0), (0, I64_MIN + 1),
              (I64_MIN, 0), (I64_MAX, I64_MAX), (-1, I64_MAX), (I64_MIN, -1), (123456789, 987654321)]
    add([], [{"op": "probe_new", "probe": 0, "name": "d"}]
        + [{"op": "probe_delta_ms", "probe": 0, "a": a, "b": b} for a, b in deltas])
    for _ in range(20):
        clock, steps, now = [], [{"op": "probe_new", "probe": 0, "name": "rand"}, {"op": "probe_reset", "probe": 0}], 0
        for _ in range(30):
            now += rng.below(3_000_000_000) - 500_000_000
            clock.append(now)
        for j in range(1 + rng.below(8)):
            steps.append({"op": "probe_checkpoint", "probe": 0, "name": f"c{j}"})
        steps.append({"op": "probe_to_string", "probe": 0})
        add(clock, steps)
    # PROF_COUNTER: default and named constructors, wrap, Show.
    add([], [{"op": "counter_new", "counter": 0}, {"op": "counter_show", "counter": 0},
             {"op": "counter_inc", "counter": 0}, {"op": "counter_inc", "counter": 0},
             {"op": "counter_count", "counter": 0}, {"op": "counter_show", "counter": 0},
             {"op": "counter_reset", "counter": 0}, {"op": "counter_count", "counter": 0}])
    add([], [{"op": "counter_new", "counter": 1, "name": ""}, {"op": "counter_set", "counter": 1, "value": U64_MAX - 1},
             {"op": "counter_inc", "counter": 1}, {"op": "counter_show", "counter": 1},
             {"op": "counter_inc", "counter": 1}, {"op": "counter_inc", "counter": 1},
             {"op": "counter_show", "counter": 1}])
    add([], [{"op": "counter_new", "counter": 0, "name": "frames"}, {"op": "counter_set", "counter": 0, "value": 1 << 63},
             {"op": "counter_show", "counter": 0}, {"op": "counter_new", "counter": 0, "name": "again"},
             {"op": "counter_show", "counter": 0}])


# ---- OBSERVABLE<T> ------------------------------------------------------------------------------

def step(op, **fields):
    return {"op": op, **fields}


def notify(obs, event, reactions=None, ignore=False, ignored=None):
    value = {"op": "notify_ignore" if ignore else "notify", "obs": obs, "event": event,
             "reactions": [{"observer": o, "steps": s} for o, s in (reactions or {}).items()]}
    if ignore:
        value["ignore"] = ignored
    return value


def observables(c):
    def add(steps):
        c.add("observable", {"op": "observable", "steps": steps})

    base = [step("new", obs=0), step("subscribe", obs=0, observer=0, link=0),
            step("subscribe_unmanaged", obs=0, observer=1), step("subscribe", obs=0, observer=2, link=1)]
    # Subscribe, SubscribeUnmanaged, Unsubscribe and both notifications.
    add(base + [notify(0, 1), notify(0, 2, ignore=True, ignored=1), notify(0, 3, ignore=True, ignored=None),
                step("unsubscribe", obs=0, observer=1), notify(0, 4)])
    add([step("new", obs=0), step("subscribe", obs=0, observer=None, link=0), notify(0, 5),
         step("unsubscribe", obs=0, observer=None), notify(0, 6)])
    add([step("new", obs=0), step("copy", obs=1, **{"from": 0}),
         step("subscribe", obs=1, observer=3, link=0), notify(0, 7), notify(1, 8)])
    # An observer throws: Notify's and NotifyIgnore's catch blocks leave the iteration.
    add(base + [notify(0, 10, {0: [step("throw")]})])
    add(base + [notify(0, 11, {1: [step("throw")]}, ignore=True, ignored=0), notify(0, 12)])
    add(base + [notify(0, 13, {2: [step("unsubscribe", obs=0, observer=0), step("throw")]})])
    add(base + [notify(0, 14, {0: [notify(0, 15, {1: [step("throw")]})]})])
    add(base + [notify(0, 16, {0: [notify(0, 17, {2: [step("throw")]}, ignore=True, ignored=1)]}, ignore=True,
                       ignored=2)])
    add([step("throw"), step("new", obs=0), step("throw")])
    add(base + [step("copy", obs=1, **{"from": 0}), notify(1, 18, {1: [notify(0, 19, {0: [step("throw")]})]})])
    # A throw after an observable dropped inside a NotifyIgnore that has already returned.
    add(base + [notify(0, 20, {1: [step("drop", obs=0)]}, ignore=True, ignored=None), step("throw")])
    # A throw unwinding a NotifyIgnore after an unpaired leave: its catch block wraps
    # the count through UINT_MAX.
    add(base + [notify(0, 21, {0: [step("leave_iteration", obs=0), step("throw")]}, ignore=True, ignored=None),
                notify(0, 23)])
    add(base + [notify(0, 22, {0: [step("link_reset", link=1), step("throw")]})])


# ---- helpers ------------------------------------------------------------------------------------

def helpers(c):
    for kind in ("char[1]", "int[3]", "double[16]", "pair[7]", "char[1000]"):
        c.add("array_dim", {"op": "array_dim", "kind": kind})
    ranges = {"int8": (-128, 127), "uint8": (0, 255), "int16": (-32768, 32767), "uint32": (0, U32_MAX),
              "int64": (I64_MIN, I64_MAX), "uint64": (0, U64_MAX), "int": (I32_MIN, I32_MAX)}
    for kind, (low, high) in ranges.items():
        for value in sorted({low, low + 1, 0, 1, high - 1, high}):
            c.add("to_underlying", {"op": "to_underlying", "type": kind, "value": value})
    for value in ("", "text", "µ"):
        c.add("ignore_unused", {"op": "ignore_unused", "value": value})


# ---- refusal controls ---------------------------------------------------------------------------

def refusals(c):
    r = c.refuse
    # Undefined behaviour: indices outside the parent array.
    r("union-find", {"op": "union_find", "count": 3, "steps": [{"op": "find", "x": 3}]}, "index out of range")
    r("union-find", {"op": "union_find", "count": 3, "steps": [{"op": "unite", "a": 0, "b": 1 << 31}]},
      "index out of range")
    r("union-find", {"op": "union_find", "count": 3, "steps": [{"op": "connected", "a": U64_MAX, "b": 0}]},
      "index out of range")
    r("union-find", {"op": "union_find", "count": 0, "steps": [{"op": "find_compress", "x": 0}]},
      "index out of range")
    r("union-find", {"op": "union_find", "count": 9, "steps": [{"op": "reset", "count": 2},
                                                              {"op": "unite", "a": 1, "b": 5}]},
      "index out of range")
    # Harness limits and malformed requests.
    r("union-find", {"op": "union_find", "count": 8193, "steps": []}, "count too large")
    r("union-find", {"op": "union_find", "count": 3, "steps": [{"op": "reset", "count": U32_MAX}]},
      "count too large")
    r("union-find", {"op": "union_find", "count": 3, "steps": [{"op": "merge"}]}, "unknown operation")
    r("union-find", {"op": "union_find", "count": -1, "steps": []}, "integer out of range: count")
    r("union-find", {"op": "union_find", "count": 3, "steps": [{"op": "find"}]}, "unexpected or missing field")
    r("kruskal", {"op": "filter_kruskal", "vertices": 3, "less": "weight", "edges": [[0, 3, 1]]},
      "endpoint out of range")
    r("kruskal", {"op": "filter_kruskal", "vertices": 3, "less": "weight", "edges": [[0, 1, 1], [9, 2, 0]]},
      "endpoint out of range")
    r("kruskal", {"op": "filter_kruskal", "vertices": 4, "less": "weight_id", "edges": [[0, 1, 1]],
                  "unite": [[0, 4]]}, "index out of range")
    r("kruskal", {"op": "filter_kruskal", "vertices": 8193, "less": "weight", "edges": []}, "count too large")
    r("kruskal", {"op": "filter_kruskal", "vertices": 3, "less": "heavy", "edges": []}, "unknown order")
    r("kruskal", {"op": "filter_kruskal", "vertices": 3, "less": "weight"}, "expected edges or generate")
    r("kruskal", {"op": "filter_kruskal", "vertices": 0, "less": "weight",
                  "generate": {"seed": 1, "discard": 0, "count": 1, "weight_range": 1}}, "generator needs vertices")
    r("kruskal", {"op": "filter_kruskal", "vertices": 6, "less": "weight",
                  "generate": {"seed": 1, "discard": 0, "count": 1, "weight_range": 1, "islands": 4}},
      "generator needs vertices")
    r("kruskal", {"op": "filter_kruskal", "vertices": 6, "less": "weight",
                  "generate": {"seed": 1, "discard": 0, "count": 65537, "weight_range": 1}}, "generator out of range")
    r("kruskal", {"op": "filter_kruskal", "vertices": 6, "less": "weight", "edges": [[0, 1]]},
      "expected edge triple")
    r("kruskal", {"op": "filter_kruskal", "vertices": 6, "less": "weight", "edges": [[0, U32_MAX + 1, 0]]},
      "integer out of range: v")
    # KISPINLOCK::lock on a held lock never returns.
    r("spinlock", {"op": "spinlock", "steps": ["lock", "lock"]}, "lock would spin forever")
    r("spinlock", {"op": "spinlock", "steps": ["try_lock", "test", "lock"]}, "lock would spin forever")
    r("spinlock", {"op": "spinlock", "steps": ["release"]}, "unknown operation")
    r("queue", {"op": "sync_queue", "receiver": "", "steps": [{"op": "front"}]}, "unknown operation")
    r("queue", {"op": "sync_queue", "receiver": 1, "steps": []}, "expected string: receiver")
    # PROF_TIMER::to_string reading the uninitialised m_started.
    r("profile", {"op": "profile", "clock": [1, 2],
                  "steps": [{"op": "timer_new", "timer": 0, "name": "x", "autostart": False},
                            {"op": "timer_to_string", "timer": 0}]}, "indeterminate m_started")
    r("profile", {"op": "profile", "clock": [1, 2, 3],
                  "steps": [{"op": "timer_new", "timer": 0, "name": "x", "autostart": False},
                            {"op": "timer_stop", "timer": 0}, {"op": "timer_to_string", "timer": 0}]},
      "indeterminate m_started")
    # LATENCY_PROBE::delta_ms with an overflowing time point difference.
    r("profile", {"op": "profile", "clock": [], "steps": [{"op": "probe_new", "probe": 0, "name": "p"},
                                                          {"op": "probe_delta_ms", "probe": 0, "a": I64_MAX, "b": -1}]},
      "time point difference overflows")
    r("profile", {"op": "profile", "clock": [], "steps": [{"op": "probe_new", "probe": 0, "name": "p"},
                                                          {"op": "probe_delta_ms", "probe": 0, "a": I64_MIN, "b": 1}]},
      "time point difference overflows")
    # Harness limits: the scripted clock's domain and length, slots, names.
    r("profile", {"op": "profile", "clock": [CLOCK_LIMIT + 1], "steps": []}, "integer out of range: t")
    r("profile", {"op": "profile", "clock": [1], "steps": [{"op": "timer_default", "timer": 0},
                                                           {"op": "timer_msecs", "timer": 0}]}, "clock script exhausted")
    r("profile", {"op": "profile", "clock": [], "steps": [{"op": "scoped", "duration": "seconds", "base": []}]},
      "clock script exhausted")
    r("profile", {"op": "profile", "clock": [], "steps": [{"op": "timer_show", "timer": 2}]}, "timer slot empty")
    r("profile", {"op": "profile", "clock": [], "steps": [{"op": "probe_to_string", "probe": 1}]},
      "probe slot empty")
    r("profile", {"op": "profile", "clock": [], "steps": [{"op": "counter_inc", "counter": 0}]}, "counter slot empty")
    r("profile", {"op": "profile", "clock": [1], "steps": [{"op": "timer_new", "timer": 0, "name": "x"},
                                                           {"op": "timer_since_start", "timer": 0, "duration": "hours"}]},
      "unknown duration")
    r("profile", {"op": "profile", "clock": [], "steps": [{"op": "scoped", "duration": "seconds",
                                                           "base": [{"op": "timer_new", "name": "x"}]}]},
      "unknown operation")
    r("profile", {"op": "profile", "clock": [], "steps": [{"op": "probe_new", "probe": 0, "name": "x",
                                                           "stages": 65537}]}, "integer out of range: stages")
    r("profile", {"op": "profile", "clock": [], "steps": [{"op": "timer_start", "timer": 4}]},
      "integer out of range: timer")
    # An exception unwinding through the catch block of an observable dropped inside
    # its own NotifyIgnore calls leave_iteration() on the destroyed object.
    r("observable", {"op": "observable", "steps": [
        step("new", obs=0), step("subscribe", obs=0, observer=0, link=0), step("subscribe", obs=0, observer=1, link=1),
        notify(0, 1, {0: [step("drop", obs=0)], 1: [step("throw")]}, ignore=True, ignored=None)]},
      "throw through destroyed observable")
    r("observable", {"op": "observable", "steps": [
        step("new", obs=0), step("subscribe", obs=0, observer=0, link=0), step("new", obs=1),
        step("subscribe", obs=1, observer=1, link=1),
        notify(0, 1, {0: [step("drop", obs=0), notify(1, 2, {1: [step("throw")]})]}, ignore=True, ignored=None)]},
      "throw through destroyed observable")
    r("observable", {"op": "observable", "steps": [{"op": "throw", "obs": 0}]}, "unexpected or missing field")
    r("minoptmax", {"op": "minoptmax", "type": "int", "sets": [{"set": "min", "value": I32_MAX + 1}],
                    "probes": [], "defaults": []}, "integer out of range: value")
    r("minoptmax", {"op": "minoptmax", "type": "short", "sets": [], "probes": [], "defaults": []}, "unknown type")
    r("minoptmax", {"op": "minoptmax", "type": "int", "sets": [{"set": "max2", "value": 1}],
                    "probes": [], "defaults": []}, "unknown setter")
    r("helpers", {"op": "to_underlying", "type": "int8", "value": 128}, "integer out of range: value")
    r("helpers", {"op": "array_dim", "kind": "char[2]"}, "unknown kind")
    r("helpers", {"op": "frobnicate"}, "unknown operation")


def generate():
    c = Corpus()
    rng = Stream(0xC0FE_1D3A_0073)
    minoptmaxes(c, rng)
    union_finds(c, rng)
    kruskals(c, rng)
    locks(c, rng)
    queues(c, rng)
    profiles(c, rng)
    observables(c)
    helpers(c)
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
    outputs = [(CASES, data), (CONTROLS, controls)]
    if args.check:
        if any(not path.exists() or path.read_bytes() != text.encode("utf-8") for path, text in outputs):
            raise SystemExit("corpus differs from deterministic generator")
    else:
        for path, text in outputs:
            path.write_bytes(text.encode("utf-8"))
    cases = sum(v for k, v in counts.items() if not k.startswith("refuse-"))
    print(json.dumps({"cases": cases, "controls": len(controls.splitlines()), **counts}, sort_keys=True))
