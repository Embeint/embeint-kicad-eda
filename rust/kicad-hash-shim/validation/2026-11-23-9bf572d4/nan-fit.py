#!/usr/bin/env python3
"""Fit the oracle's two-NaN payload choice per operation over every BEZIER<double> case.

Values are modelled as IEEE bit patterns: a NaN operand is quieted; with two NaN operands the
rule picks one; an invalid operation without NaN operands gives x86's indefinite NaN. Non-NaN
results are Python (IEEE double) arithmetic. Prints the mismatch count of each rule combination.
"""
import itertools
import json
import struct
import sys

QUIET = 0x0008000000000000
INDEFINITE = 0xFFF8000000000000


def isnan(bits):
    return (bits & 0x7FF0000000000000) == 0x7FF0000000000000 and (bits & 0x000FFFFFFFFFFFFF) != 0


def f(bits):
    return struct.unpack("<d", struct.pack("<Q", bits))[0]


def b(value):
    return struct.unpack("<Q", struct.pack("<d", value))[0]


def ieee(kind, x, y):
    if kind == "add":
        return x + y
    if kind == "sub":
        return x - y
    if kind == "mul":
        return x * y
    if y != 0.0:
        return x / y
    if x != x or x == 0.0:
        return float("nan")
    negative_divisor = struct.pack("<d", y)[7] & 0x80
    return float("inf") * (1 if (x > 0) != bool(negative_divisor) else -1)


class Model:
    def __init__(self, rules):
        self.rules = rules

    def op(self, kind, x, y, site=None):
        nx, ny = isnan(x), isnan(y)
        if nx and ny:
            rule = self.rules.get(site, self.rules[kind])
            return (x if rule == "first" else y) | QUIET
        if nx:
            return x | QUIET
        if ny:
            return y | QUIET
        result = b(ieee(kind, f(x), f(y)))
        return INDEFINITE if isnan(result) else result

    def scaled(self, s, p, site=None):
        return (self.op("mul", s, p[0], site), self.op("mul", s, p[1], site))

    def plus(self, p, q, site=None):
        return (self.op("add", p[0], q[0], site), self.op("add", p[1], q[1], site))

    def minus(self, p, q, site=None):
        return (self.op("sub", p[0], q[0], site), self.op("sub", p[1], q[1], site))

    def point_at(self, c, t):
        one, three = b(1.0), b(3.0)
        t2 = self.op("mul", t, t)
        t3 = self.op("mul", t2, t)
        tm1 = self.op("sub", one, t)
        tm12 = self.op("mul", tm1, tm1)
        tm13 = self.op("mul", tm12, tm1)
        s = self.scaled(tm13, c[0], "p_scale0")
        c1 = self.scaled(self.op("mul", self.op("mul", three, t), tm12), c[1], "p_scale1")
        c2 = self.scaled(self.op("mul", self.op("mul", three, t2), tm1), c[2], "p_scale2")
        e = self.scaled(t3, c[3], "p_scale3")
        return self.plus(self.plus(self.plus(s, c1, "p_add1"), c2, "p_add2"), e, "p_add3")

    def lerp(self, a, q, t):
        return self.plus(a, self.scaled(t, self.minus(q, a, "l_sub"), "l_scale"), "l_add")

    def split(self, c, t):
        sc1 = self.lerp(c[0], c[1], t)
        c1c2 = self.lerp(c[1], c[2], t)
        c2e = self.lerp(c[2], c[3], t)
        lc2 = self.lerp(sc1, c1c2, t)
        rc1 = self.lerp(c1c2, c2e, t)
        shared = self.lerp(lc2, rc1, t)
        return (c[0], sc1, lc2, shared), (shared, rc1, c2e, c[3])

    def sub_curve(self, c, t0, t1):
        def clamp(t):
            v = f(t)
            return b(0.0) if v < 0.0 else b(1.0) if v > 1.0 else t
        t0, t1 = clamp(t0), clamp(t1)
        v0, v1 = f(t0), f(t1)
        if v1 <= v0:
            p = self.point_at(c, t0)
            return (p, p, p, p)
        if v0 == 0.0 and v1 == 1.0:
            return c
        if v0 == 0.0:
            return self.split(c, t1)[0]
        if v1 == 1.0:
            return self.split(c, t0)[1]
        left = self.split(c, t1)[0]
        return self.split(left, self.op("div", t0, t1))[1]


def hexs(p):
    return [f"{p[0]:016x}", f"{p[1]:016x}"]


def main():
    cases = [json.loads(line) for line in open(sys.argv[1])]
    replies = {json.loads(line)["case"]: json.loads(line) for line in open(sys.argv[2])}
    selected = [c for c in cases if c["input"].get("op") == "bezier" and c["input"]["type"] == "f64"]
    sites = ["p_scale0", "p_scale1", "p_scale2", "p_scale3", "p_add1", "p_add2", "p_add3",
             "l_sub", "l_scale", "l_add", "div"]
    ranking = []
    for combo in itertools.product(["first", "second"], repeat=len(sites)):
        rules = {"add": "first", "sub": "first", "mul": "first"}
        rules.update(dict(zip(sites, combo)))
        model = Model(rules)
        bad = []
        for case in selected:
            data = case["input"]
            curve = tuple((int(p[0], 16), int(p[1], 16)) for p in data["points"])
            call = data["call"]
            if call == "point_at":
                got = {"point": hexs(model.point_at(curve, int(data["t"], 16)))}
            elif call == "split":
                left, right = model.split(curve, int(data["t"], 16))
                got = {"left": [hexs(p) for p in left], "right": [hexs(p) for p in right]}
            elif call == "sub_curve":
                got = {"sub_curve": [hexs(p) for p in model.sub_curve(curve, int(data["t0"], 16), int(data["t1"], 16))]}
            else:
                continue
            result = replies[case["case"]]["result"]
            if any(result[key] != value for key, value in got.items()):
                bad.append(case["case"])
        ranking.append((len(bad), combo, bad))
    ranking.sort()
    for count, combo, bad in ranking[:6]:
        print(count, {s: c for s, c in zip(sites, combo) if c != "first"}, bad[:12])


if __name__ == "__main__":
    main()
