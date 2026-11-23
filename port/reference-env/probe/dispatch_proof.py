#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Prove which libm implementation each checked function executed.

Input: the primitive probe's traced calls (every instruction address from the
public entry point until the call returned) and libm_elf.dispatch_facts() read
from the installed libm and its detached debug file. A function passes only if
all of these hold:

* the facts were read from the locked libm and debug file (SHA-256, build ID)
  and name the same multiarch implementations as the lock;
* the address compiled code binds to equals dlsym()'s, in the locked libm;
* the entry is the default-version public definition of the function: either
  an IFUNC whose resolved target is the entry ("direct"), or a FUNC wrapper
  ("wrapped") that reaches the implementation through a .plt.sec stub of the
  exact IBT form whose GOT slot has one R_X86_64_IRELATIVE relocation;
* the target is the function's locked __<f>_sse2 / __ieee754_<f>_sse2, and it
  is the SSE2 variant of the IFUNC resolver whose slot was used;
* the trace returned, stayed inside libm, every address belongs to a known
  function or stub, and the only multiarch implementation executed is the
  expected one (no forbidden or other variant).

Anything else is a violation with its reason; nothing is inferred from names
alone or from the GLIBC_TUNABLES setting.
"""

import libm_elf


def _int(text):
    return int(text, 16)


class Facts:
    """Index over dispatch_facts() for address lookups."""

    def __init__(self, facts):
        self.raw = facts
        self.ranges = sorted((_int(start), _int(start) + size, names) for start, size, names in facts["functions"])
        self.stubs = {_int(stub["stub"]): stub for stub in facts["plt_stubs"]}
        plt = facts.get("plt_sec")
        self.plt = (_int(plt["start"]), _int(plt["end"])) if plt else (0, 0)
        self.ifuncs = {_int(offset): names for offset, names in facts["ifuncs"].items()}
        self._symbols = {}

    def symbols_at(self, address):
        """Names of every function symbol whose range contains the address."""
        if address not in self._symbols:
            self._symbols[address] = sorted({name for start, end, names in self.ranges
                                             if start <= address < end for name in names})
        return self._symbols[address]

    def stub_at(self, address):
        if not self.plt[0] <= address < self.plt[1]:
            return None
        start = address - (address - self.plt[0]) % libm_elf.PLT_SEC_ENTRY
        return self.stubs.get(start, {"stub": libm_elf.hex_offset(start), "template": False, "slot": None,
                                      "relocations": []})


def identity_violations(facts, lock):
    """Facts must come from the locked libm and debug file bytes."""
    libm_sha = next((f["sha256"] for f in lock["toolchain_files"] if f["resolved_path"] == lock["libm"]["path"]),
                    None)
    problems = []
    if facts.get("schema") != 1:
        problems.append("dispatch facts schema differs")
    if facts.get("libm_sha256") != libm_sha:
        problems.append("dispatch facts were not read from the locked libm bytes")
    if facts.get("debug_sha256") != lock["libm"]["debug_file_sha256"]:
        problems.append("dispatch facts were not read from the locked debug file")
    build_id = lock["libm"]["build_id"]
    if facts.get("libm_build_id") != build_id or facts.get("debug_build_id") != build_id:
        problems.append("libm or debug file build ID differs from the lock")
    if facts.get("multiarch_symbols") != lock["libm"]["multiarch_symbols"]:
        problems.append("debug file multiarch implementations differ from the lock")
    return problems


def expected_target(function, table):
    """The locked SSE2 implementation offset of a public function, or None."""
    wanted = {f"__{function}_sse2", f"__ieee754_{function}_sse2"}
    offsets = [offset for offset, names in table.items() if wanted.intersection(names)]
    return (_int(offsets[0]), table[offsets[0]]) if len(offsets) == 1 else (None, [])


def is_sse2_of(resolver_names, target_names):
    """The target is named as the SSE2 variant of one of the resolver's names."""
    return any(name + "_sse2" in target_names for name in resolver_names)


def default_definition(facts, function):
    rows = [row for row in facts["definitions"].get(function, []) if row["default"]]
    return rows[0] if len(rows) == 1 else None


def prove(record, facts, index, lock):
    """Check one traced function; return its report row (with its violations)."""
    function = record["function"]
    table = lock["libm"]["multiarch_symbols"]
    reasons = []
    row = {"function": function, "object": record["object"], "offset": record["offset"],
           "linked_offset": record["linked_offset"], "trace_status": record["trace_status"],
           "steps": len(record["trace"]), "path": None, "entry_definition": None,
           "entry_symbols": [], "expected": None, "hops": [], "executed_symbols": [], "executed_variants": []}

    if record["object"] != lock["libm"]["path"]:
        reasons.append("entry is not in the locked libm")
    if record["linked_object"] != record["object"] or record["linked_offset"] != record["offset"]:
        reasons.append("compiled reference and dlsym() resolve to different addresses")
    target, target_names = expected_target(function, table)
    if target is None:
        reasons.append("no unique locked SSE2 implementation for this function")
    else:
        row["expected"] = {"offset": libm_elf.hex_offset(target), "names": target_names}
    entry = _int(record["offset"])
    row["entry_symbols"] = index.symbols_at(entry)
    definition = default_definition(facts, function)
    row["entry_definition"] = definition
    wrapper = None
    if definition is None:
        reasons.append("no unique default-version public definition")
    elif definition["type"] == "IFUNC":
        row["path"] = "direct"
        resolver_names = index.ifuncs.get(_int(definition["value"]), [])
        if not resolver_names:
            reasons.append("public IFUNC has no resolver symbol in the debug file")
        if target is not None and (entry != target or not is_sse2_of(resolver_names, target_names)):
            reasons.append("IFUNC resolved to an address other than the locked SSE2 implementation")
    elif definition["type"] == "FUNC":
        row["path"] = "wrapped"
        wrapper = (_int(definition["value"]), _int(definition["value"]) + definition["size"])
        if entry != wrapper[0]:
            reasons.append("entry is not the public wrapper definition")
    else:
        reasons.append("public definition is neither FUNC nor IFUNC")

    trace = record["trace"]
    if record["trace_status"] != "returned":
        reasons.append("trace incomplete: " + record["trace_status"])
    if not trace or trace[0] != record["offset"]:
        reasons.append("trace does not start at the entry")
    executed = []
    variants = []
    position = 0
    while position < len(trace):
        step = trace[position]
        if step.startswith("!"):
            reasons.append("execution left libm at " + step)
            position += 1
            continue
        address = _int(step)
        stub = index.stub_at(address)
        if stub is None:
            names = index.symbols_at(address)
            if not names:
                reasons.append("address not inside a known libm function: " + step)
            for name in names:
                if name not in executed:
                    executed.append(name)
                variant, _reason = libm_elf.classify(name, libm_elf.STT_FUNC)
                if variant and name not in variants:
                    variants.append(name)
            position += 1
            continue
        hop = {"caller": trace[position - 1] if position else None, "stub": stub["stub"], "slot": stub["slot"],
               "relocations": stub["relocations"], "resolver": None, "resolver_names": [], "target": None,
               "target_names": []}
        row["hops"].append(hop)
        start = _int(stub["stub"])
        if not stub["template"]:
            reasons.append("PLT stub is not the exact .plt.sec form: " + stub["stub"])
        if address != start or trace[position + 1:position + 2] != [libm_elf.hex_offset(start + libm_elf.PLT_SEC_JUMP)]:
            reasons.append("PLT stub not executed from its start through its jump: " + step)
        relocations = stub["relocations"]
        if len(relocations) != 1 or relocations[0]["type"] != "R_X86_64_IRELATIVE":
            reasons.append("PLT slot is not a single IRELATIVE relocation: " + str(stub["slot"]))
        else:
            hop["resolver"] = relocations[0]["addend"]
            hop["resolver_names"] = index.ifuncs.get(_int(relocations[0]["addend"]), [])
        if wrapper is None or hop["caller"] is None or hop["caller"].startswith("!") or not (
                wrapper[0] <= _int(hop["caller"]) < wrapper[1]):
            reasons.append("PLT stub not entered from the public wrapper")
        if position + 2 < len(trace):
            hop["target"] = trace[position + 2]
            if not hop["target"].startswith("!"):
                hop["target_names"] = table.get(hop["target"], [])
        if target is None or hop["target"] != libm_elf.hex_offset(target):
            reasons.append("IRELATIVE slot did not lead to the locked SSE2 implementation")
        elif not is_sse2_of(hop["resolver_names"], hop["target_names"]):
            reasons.append("target is not the SSE2 variant of the slot's IFUNC resolver")
        position += 2
    row["executed_symbols"] = executed
    row["executed_variants"] = variants
    if row["path"] == "wrapped" and not row["hops"]:
        reasons.append("wrapper returned without reaching an implementation through an IRELATIVE slot")
    if row["path"] == "direct" and row["hops"]:
        reasons.append("direct IFUNC target made a further PLT dispatch")
    if target is not None and sorted(variants) != sorted(target_names):
        reasons.append("executed multiarch implementations differ from the locked SSE2 target: "
                       + ", ".join(variants or ["none"]))
    row["violations"] = reasons
    return row


def dispatch_report(observation, facts, lock, candidate):
    """Report rows and violations for every checked function in one probe run."""
    index = Facts(facts)
    report = []
    violations = []
    identity = identity_violations(facts, lock)
    if identity:
        violations.append({"reason": "dispatch facts identity", "problems": identity})
    expected = candidate["dispatch"]["checked_functions"]
    actual = [item["function"] for item in observation["libm_dispatch"]]
    if actual != expected:
        violations.append({"reason": "missing, extra, duplicate or reordered dispatch records", "functions": actual})
    forbidden = tuple(candidate["dispatch"]["forbidden_variant_suffixes"])
    for record in observation["libm_dispatch"]:
        row = prove(record, facts, index, lock)
        forbidden_seen = [name for name in row["executed_variants"] if name.endswith(forbidden)]
        if forbidden_seen:
            row["violations"].append("forbidden variant executed: " + ", ".join(forbidden_seen))
        report.append(row)
        if row["violations"]:
            violations.append({"function": row["function"], "reasons": row["violations"]})
    return report, violations


def resolve_objects(observation, loaded_objects):
    """Replace probe object names by their recorded resolved paths (unrecorded names stay marked)."""
    rows = []
    for row in observation["libm_dispatch"]:
        resolved = dict(row)
        for key in ("object", "linked_object"):
            entry = loaded_objects.get(row[key])
            resolved[key] = entry["resolved"] if entry else "unrecorded:" + row[key]
        rows.append(resolved)
    return {"libm_dispatch": rows}
