#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Prove which libm implementation each checked function executed.

Inputs: the primitive probe's traced calls and the locked libm and debug file
copied into the run directory (libm/libm.so.6, libm/libm.debug). The copies
must have the lock's SHA-256; every static fact is re-derived from them
(libm_elf.dispatch_facts()), and the facts recorded in environment.json must
equal the derived ones exactly (canonical JSON, so types count). Recorded
facts are never trusted on their own.

A traced call is a list of instruction addresses from the public entry until
the call returned, the return address read at the entry, the address reached
after the return, and every GOT slot read at a RIP-relative jump. A function
passes only if all of these hold:

* the compiled reference and dlsym() agree, inside the locked libm;
* the entry is the start of the default-version public definition: an IFUNC
  whose resolved target is the entry ("direct"), or a FUNC wrapper
  ("wrapped") that reaches the implementation through a .plt.sec stub of the
  exact form, whose GOT slot has one R_X86_64_IRELATIVE relocation with
  symbol 0 and a resolver addend, and whose recorded slot value is the target;
* the target is the function's locked __<f>_sse2 / __ieee754_<f>_sse2 and the
  SSE2 variant of that resolver;
* every step decodes (x86_decode.py, fail-closed) in libm's executable code and
  every successor is the instruction's fall-through, its direct target, the
  return address pushed by a traced call, or (only for the stub's jump) the
  slot value; the trace ends with the entry's own return, whose target equals
  the return address read at the entry, inside the retained probe's executable
  code and qualified by the recorded compiler output object;
* only the expected multiarch implementation executed (no forbidden or
  other variant).

Scope: one ordinary input per function. A valid trace proves the path that
input took; other branches of a wrapper are not certified. The hashes bind the
evidence to the locked files; they do not authenticate a run.
"""

import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import struct

import libm_elf
import x86_decode

LIBM_COPY = "libm/libm.so.6"
DEBUG_COPY = "libm/libm.debug"
MAX_ELF_BYTES = 16 << 20


def _int(text):
    return int(text, 16)


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def locked_libm(directory, lock):
    """Bytes of the run's libm and debug copies; problems if they are not the locked files."""
    wanted = {LIBM_COPY: next((f["sha256"] for f in lock["toolchain_files"]
                               if f["resolved_path"] == lock["libm"]["path"]), None),
              DEBUG_COPY: lock["libm"]["debug_file_sha256"]}
    data = {}
    problems = []
    for name, digest in wanted.items():
        path = Path(directory) / name
        if path.is_symlink() or not path.is_file():
            problems.append(f"{name} is missing or not a regular file")
            continue
        if path.stat().st_size > MAX_ELF_BYTES:
            problems.append(f"{name} exceeds {MAX_ELF_BYTES} bytes")
            continue
        content = path.read_bytes()
        if hashlib.sha256(content).hexdigest() != digest:
            problems.append(f"{name} is not the locked file")
            continue
        data[name] = content
    return data.get(LIBM_COPY), data.get(DEBUG_COPY), problems


def verified_facts(directory, environment, lock, functions):
    """Re-derive the facts from the locked copies and compare the recorded ones.

    Returns (facts, libm ELF or None, problems). facts are the derived ones
    whenever the copies are the locked files, else None.
    """
    libm_bytes, debug_bytes, problems = locked_libm(directory, lock)
    if libm_bytes is None or debug_bytes is None:
        return None, None, problems
    try:
        facts = libm_elf.dispatch_facts(libm_bytes, debug_bytes, functions)
        table, excluded = libm_elf.multiarch_symbols(libm_elf.Elf64(debug_bytes))
        libm = libm_elf.Elf64(libm_bytes)
    except (ValueError, IndexError, KeyError) as error:
        return None, None, problems + ["locked libm files cannot be read: " + str(error)]
    recorded = environment.get("libm", {}) if type(environment.get("libm")) is dict else {}
    if canonical(recorded.get("dispatch_facts")) != canonical(facts):
        problems.append("recorded dispatch facts differ from the locked ELF files")
    if (canonical(recorded.get("multiarch_symbols_from_debug_file")) != canonical(table)
            or canonical(recorded.get("excluded_suffix_matches")) != canonical(excluded)):
        problems.append("recorded multiarch table differs from the locked debug file")
    return facts, libm, problems


class Facts:
    """Index over dispatch_facts() for address lookups."""

    def __init__(self, facts):
        self.raw = facts
        self.ranges = sorted((_int(start), _int(start) + size, names) for start, size, names in facts["functions"])
        self.starts = {start for start, _end, _names in self.ranges}
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
    if facts.get("schema") != 1 or type(facts.get("schema")) is not int:
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
    rows = [row for row in facts["definitions"].get(function, []) if row["default"] is True]
    return rows[0] if len(rows) == 1 else None


class Caller:
    """The recorded probe object and its file-backed executable load mappings."""

    def __init__(self, name, data):
        self.name = name
        if (len(data) < 64 or data[:7] != b"\x7fELF\x02\x01\x01" or data[7] not in (0, 3) or data[8] != 0
                or struct.unpack_from("<H", data, 0x10)[0] not in (2, 3)
                or struct.unpack_from("<H", data, 0x12)[0] != 62
                or struct.unpack_from("<I", data, 0x14)[0] != 1):
            raise ValueError("probe is not a supported x86-64 ELF executable")
        entry, phoff = struct.unpack_from("<QQ", data, 0x18)
        ehsize, phsize, phnum = struct.unpack_from("<HHH", data, 0x34)
        if (ehsize != 64 or phsize != 56 or not 0 < phnum < 65535 or phoff < ehsize
                or phoff + phsize * phnum > len(data)):
            raise ValueError("invalid probe ELF program-header table")
        self.loads = []
        for i in range(phnum):
            kind, flags, offset, address, _physical, filesz, memsz, align = struct.unpack_from(
                "<IIQQQQQQ", data, phoff + phsize * i)
            if kind != 1:  # PT_LOAD, the loader's authority; section flags are not permissions.
                continue
            if (filesz > memsz or offset + filesz > len(data) or address + memsz >= 1 << 64
                    or (address - offset) % 4096
                    or (align > 1 and (align & (align - 1) or (address - offset) % align))):
                raise ValueError("invalid probe ELF load segment")
            if self.loads and address < self.loads[-1][0]:
                raise ValueError("unordered probe ELF load segments")
            self.loads.append((address, filesz, memsz, flags))
        if not self.loads or not any(flags & 1 and filesz for _a, filesz, _m, flags in self.loads):
            raise ValueError("probe has no file-backed executable load segment")
        # glibc 2.39 dladdr reports l_map_start, the first mapped page, so both
        # ET_EXEC and PIE offsets must be measured relative to that page.
        self.base = self.loads[0][0] & ~4095
        if not self.executable(entry):
            raise ValueError("probe entry is not in a file-backed executable load segment")

    def executable(self, address):
        mapping = None
        for start, filesz, memsz, flags in self.loads:
            page_start, page_end = start & ~4095, (start + memsz + 4095) & ~4095
            if page_start <= address < page_end:
                mapping = (start, filesz, flags)  # A later mapping replaces an overlapping page.
        return (mapping is not None and bool(mapping[2] & 1)
                and mapping[0] <= address < mapping[0] + mapping[1])

    def contains(self, text):
        prefix = "!" + self.name + "+"
        if type(text) is not str or not text.startswith(prefix):
            return False
        offset = text[len(prefix):]
        if not re.fullmatch(r"0x[0-9a-f]{1,16}", offset):
            return False
        return self.executable(self.base + _int(offset))


def recorded_caller(directory, observation, lock, loaded_objects):
    """Bind completion markers to the probe binary, never to a named library."""
    name = observation.get("probe_object")
    if (type(name) is not str or not name.startswith("/") or "\n" in name or "\0" in name
            or PurePosixPath(name).name != "primitive_probe" or name == lock["libm"]["path"]):
        return None, ["missing or invalid recorded probe object"]
    # CollectObject deliberately omits the main executable. A caller cannot
    # simultaneously be one of its recorded shared libraries or aliases.
    if type(loaded_objects) is not dict or any(type(item) is not dict for item in loaded_objects.values()):
        return None, ["invalid recorded shared-object map"]
    shared = set(loaded_objects) | {item.get("resolved") for item in loaded_objects.values()
                                  if type(item.get("resolved")) is str}
    if name in shared:
        return None, ["probe caller is a recorded shared-object alias"]
    path = Path(directory) / "bin/primitive_probe"
    if path.is_symlink() or not path.is_file() or path.stat().st_size > MAX_ELF_BYTES:
        return None, ["missing, invalid or oversized retained probe ELF"]
    try:
        return Caller(name, path.read_bytes()), []
    except (ValueError, IndexError, KeyError, struct.error) as error:
        return None, ["retained probe ELF cannot be read: " + str(error)]


def walk(record, index, libm, table, target, wrapper, row, reasons, caller):
    """Validate every control-flow edge of the trace against the locked code; collect hops."""
    trace = record["trace"]
    reads = record["slot_reads"]
    used_reads = 0
    stack = []
    for position, step in enumerate(trace):
        if step.startswith("!"):
            reasons.append("execution left libm at " + step)
            return
        address = _int(step)
        try:
            instruction = x86_decode.decode(libm.code(address), address)
        except ValueError as error:
            reasons.append(f"unsupported or undecodable instruction at {step}: {error}")
            return
        following = trace[position + 1] if position + 1 < len(trace) else None
        if instruction.kind == x86_decode.RET and not stack:
            if following is not None:
                reasons.append(f"return at {step} leaves the call before the end of the trace")
                return
            if not (caller is not None and caller.contains(record["return_to"])
                    and record["return_to"] == record["return_address"]):
                reasons.append("the entry's return does not reach the recorded return address in executable probe code")
            row["returned_at"] = step
            break
        if following is None:
            reasons.append("trace ended before the entry's return")
            return
        if instruction.kind == x86_decode.SEQ:
            allowed = {instruction.following}
        elif instruction.kind == x86_decode.JCC:
            allowed = {instruction.following, instruction.target}
        elif instruction.kind == x86_decode.JMP:
            allowed = {instruction.target}
        elif instruction.kind == x86_decode.CALL:
            allowed = {instruction.target}
            stack.append(instruction.following)
        elif instruction.kind == x86_decode.RET:
            allowed = {stack.pop()}
        else:  # RIP-relative indirect jump: only a stub's jump, through its slot
            read = reads[used_reads] if used_reads < len(reads) else None
            used_reads += 1
            if not hop(record, index, instruction, position, read, table, target, wrapper, row, reasons):
                return
            allowed = {_int(following)} if not following.startswith("!") else set()
        if following.startswith("!"):
            reasons.append(f"execution left libm at {following} after {step}")
            return
        if _int(following) not in allowed:
            reasons.append(f"impossible successor {following} after {step} ({instruction.kind})")
            return
    else:
        reasons.append("trace does not end with the entry's return")
        return
    if used_reads != len(reads):
        reasons.append("GOT slot reads without a matching stub jump")


def hop(record, index, instruction, position, read, table, target, wrapper, row, reasons):
    """Check one stub jump: stub form and entry, relocation, slot read and target."""
    trace = record["trace"]
    address = instruction.address
    stub = index.stub_at(address)
    following = trace[position + 1]
    entry = {"caller": trace[position - 2] if position >= 2 else None, "stub": None, "slot": None,
             "relocations": [], "resolver": None, "resolver_names": [], "target": following, "target_names": []}
    row["hops"].append(entry)
    if stub is None or not stub["template"] or address != _int(stub["stub"]) + libm_elf.PLT_SEC_JUMP:
        reasons.append("indirect jump outside an exact .plt.sec stub at " + trace[position])
        return False
    entry.update(stub=stub["stub"], slot=stub["slot"], relocations=stub["relocations"])
    if instruction.memory != _int(stub["slot"]):
        reasons.append("stub jump does not use the stub's GOT slot")
        return False
    if trace[position - 1:position] != [stub["stub"]]:
        reasons.append("PLT stub not executed from its start through its jump")
    if wrapper is None or entry["caller"] is None or entry["caller"].startswith("!") or not (
            wrapper[0] <= _int(entry["caller"]) < wrapper[1]):
        reasons.append("PLT stub not entered from the public wrapper")
    relocations = stub["relocations"]
    if (len(relocations) != 1 or relocations[0]["type"] != "R_X86_64_IRELATIVE" or relocations[0]["symbol"] != 0
            or relocations[0]["addend"].startswith("-")):
        reasons.append("PLT slot is not a single symbol-less IRELATIVE relocation: " + str(stub["slot"]))
    else:
        entry["resolver"] = relocations[0]["addend"]
        entry["resolver_names"] = index.ifuncs.get(_int(entry["resolver"]), [])
        if not entry["resolver_names"]:
            reasons.append("IRELATIVE addend is not an IFUNC resolver")
    if read is None or read != {"at": trace[position], "slot": stub["slot"], "value": following}:
        reasons.append("recorded GOT slot read does not match the stub jump and its successor")
    if not following.startswith("!"):
        entry["target_names"] = table.get(following, [])
    if target is None or following != libm_elf.hex_offset(target):
        reasons.append("IRELATIVE slot did not lead to the locked SSE2 implementation")
    elif not is_sse2_of(entry["resolver_names"], entry["target_names"]):
        reasons.append("target is not the SSE2 variant of the slot's IFUNC resolver")
    return True


def prove(record, facts, index, lock, libm, caller):
    """Check one traced function; return its report row (with its violations)."""
    function = record["function"]
    table = lock["libm"]["multiarch_symbols"]
    reasons = []
    row = {"function": function, "object": record["object"], "offset": record["offset"],
           "linked_offset": record["linked_offset"], "trace_status": record["trace_status"],
           "steps": len(record["trace"]), "path": None, "entry_definition": None,
           "entry_symbols": [], "expected": None, "hops": [], "returned_at": None,
           "executed_symbols": [], "executed_variants": []}

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
    if entry not in index.starts:
        reasons.append("entry is not the start of a libm function")
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
    executed, variants = [], []
    for step in trace:
        if step.startswith("!"):
            continue
        address = _int(step)
        names = index.symbols_at(address)
        if not names and index.stub_at(address) is None:
            reasons.append("address not inside a known libm function: " + step)
        for name in names:
            if name not in executed:
                executed.append(name)
            if libm_elf.classify(name, libm_elf.STT_FUNC)[0] and name not in variants:
                variants.append(name)
    row["executed_symbols"] = executed
    row["executed_variants"] = variants
    if libm is None:
        reasons.append("locked libm code is unavailable for control-flow validation")
    elif trace and trace[0] == record["offset"]:
        walk(record, index, libm, table, target, wrapper, row, reasons, caller)
    if row["path"] == "wrapped" and not row["hops"]:
        reasons.append("wrapper returned without reaching an implementation through an IRELATIVE slot")
    if row["path"] == "direct" and row["hops"]:
        reasons.append("direct IFUNC target made a further PLT dispatch")
    if target is not None and sorted(variants) != sorted(target_names):
        reasons.append("executed multiarch implementations differ from the locked SSE2 target: "
                       + ", ".join(variants or ["none"]))
    row["violations"] = reasons
    return row


def dispatch_report(observation, facts, lock, candidate, libm, problems=(), caller=None):
    """Report rows and violations for every checked function in one probe run.

    facts and libm come from verified_facts(); problems are its findings.
    """
    report = []
    violations = []
    identity = list(problems) + (identity_violations(facts, lock) if facts is not None else [])
    if facts is None:
        identity.append("no dispatch facts derived from the locked libm files")
    if identity:
        violations.append({"reason": "dispatch facts identity", "problems": identity})
    expected = candidate["dispatch"]["checked_functions"]
    actual = [item["function"] for item in observation["libm_dispatch"]]
    if actual != expected:
        violations.append({"reason": "missing, extra, duplicate or reordered dispatch records", "functions": actual})
    if facts is None:
        return report, violations
    index = Facts(facts)
    forbidden = tuple(candidate["dispatch"]["forbidden_variant_suffixes"])
    for record in observation["libm_dispatch"]:
        row = prove(record, facts, index, lock, libm, caller)
        forbidden_seen = [name for name in row["executed_variants"] if name.endswith(forbidden)]
        if forbidden_seen:
            row["violations"].append("forbidden variant executed: " + ", ".join(forbidden_seen))
        report.append(row)
        if row["violations"]:
            violations.append({"function": row["function"], "reasons": row["violations"]})
    return report, violations


def run_report(directory, observation, environment, lock, candidate):
    """dispatch_report() for one probe run directory, facts re-derived from its locked copies."""
    facts, libm, problems = verified_facts(directory, environment, lock,
                                           candidate["dispatch"]["checked_functions"])
    resolved = resolve_objects(observation, environment.get("loaded_objects", {}))
    caller, caller_problems = recorded_caller(directory, observation, lock, environment.get("loaded_objects", {}))
    return dispatch_report(resolved, facts, lock, candidate, libm, problems + caller_problems, caller)


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
