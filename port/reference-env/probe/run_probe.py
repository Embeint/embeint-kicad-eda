#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Run the bounded reproducibility probe inside the candidate image.

1. Re-check the image against its build-time identity record.
2. Build the C++ primitive probe with the oracle's fixed FP flags, the Rust
   probe with the pinned rustc and, given an audited oracle build, the matrix
   probe with the exact flags of an oracle shim from its compile database.
3. Run the probes under the image's dispatch contract, and the C++ primitive
   probe once more with GLIBC_TUNABLES removed (the host CPU's own dispatch).
4. Write every observation, the exported reference bits, environment
   provenance and summary.json.

Exit status 1 means the environment contract does not hold (identity, libm
dispatch, FP control state, runtime formatting family). libm dispatch is proved
per function from a traced call against copies of the locked libm and debug
file kept in the output (dispatch_proof.py). Exit status 4 means a
C++/Rust comparison differs. Host-default dispatch and separate-versus-paired
libm calls are diagnostic comparisons. Every difference is reported with its
count and examples; none is accepted. Nothing here qualifies the candidate.
"""

import argparse
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tomllib
import artifacts
import dispatch_proof
import shutil

HERE = Path(__file__).resolve().parent
REFENV = HERE.parent
STRICT_FLAGS = ["-O3", "-DNDEBUG", "-std=gnu++20", "-fno-fast-math", "-ffp-contract=off"]
EXPECTED_MXCSR = "00001f80"  # all exceptions masked, round to nearest, no FTZ/DAZ
EXPECTED_X87 = "037f"


def sha256(path):
    import hashlib
    digest = hashlib.sha256()
    with open(path, "rb") as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


# Wall-clock bound of any one command; subprocess kills the command on expiry
# (a traced grandchild dies with its tracer through PTRACE_O_EXITKILL).
COMMAND_TIMEOUT = 1800


def run(command, env=None, stdout=None):
    print("+ " + shlex.join(command), file=sys.stderr)
    return subprocess.run(command, check=True, env=env, stdout=stdout, text=True, timeout=COMMAND_TIMEOUT)


def shim_flags(build):
    """Compile arguments of an oracle shim TU, minus its source, output and depfile."""
    database = json.loads((build / "compile_commands.json").read_text(encoding="utf-8"))
    for entry in sorted(database, key=lambda e: e["file"]):
        tokens = entry.get("arguments") or shlex.split(entry["command"])
        if "/shims/" not in entry["file"] or not any("CMakeFiles/kicad-oracle.dir/" in t for t in tokens):
            continue
        flags = []
        skip = False
        for token in tokens[1:]:
            if skip:
                skip = False
                continue
            if token in ("-o", "-MT", "-MF", "-MQ"):
                skip = True
                continue
            if token in ("-c", "-MD", "-MMD") or token == entry["file"] or token.endswith(entry["file"]):
                continue
            flags.append(token)
        return tokens[0], flags, entry["directory"], entry["file"]
    raise ValueError("No kicad-oracle shim in the oracle compile database")


def build(out, oracle_build):
    built = {}
    primitive = out / "bin" / "primitive_probe"
    primitive.parent.mkdir(parents=True, exist_ok=True)
    command = ["g++", *STRICT_FLAGS, "-o", str(primitive), str(HERE / "primitive_probe.cpp")]
    run(command)
    built["primitive"] = {"command": command, "sha256": sha256(primitive)}
    rust = out / "bin" / "rust_probe"
    command = ["rustc", "--edition", "2021", "-C", "opt-level=3", "-o", str(rust), str(HERE / "rust_probe.rs")]
    run(command)
    built["rust"] = {"command": command, "sha256": sha256(rust)}
    if oracle_build is not None:
        compiler, flags, directory, reference_file = shim_flags(oracle_build)
        objects = out / "bin" / "matrix_probe.o"
        matrix = out / "bin" / "matrix_probe"
        compile_command = [compiler, *flags, "-I", str(HERE), "-c", str(HERE / "matrix_probe.cpp"),
                           "-o", str(objects)]
        subprocess.run(compile_command, check=True, cwd=directory, timeout=COMMAND_TIMEOUT)
        link_command = [compiler, "-O3", "-fno-fast-math", "-ffp-contract=off", str(objects), "-o", str(matrix)]
        run(link_command)
        built["matrix"] = {"flags_from": reference_file, "compile_command": compile_command,
                           "link_command": link_command, "sha256": sha256(matrix)}
    return built


def run_probes(out, candidate, sweep_count, has_matrix):
    contract = dict(os.environ)
    host_default = {key: value for key, value in contract.items() if key != "GLIBC_TUNABLES"}
    functions = ",".join(candidate["dispatch"]["checked_functions"])
    results = {}
    for label, env in (("contract", contract), ("host-default", host_default)):
        sweep = out / f"primitive-{label}.sweep.bin"
        with open(out / f"primitive-{label}.json", "w", encoding="utf-8") as stream:
            run([str(out / "bin/primitive_probe"), "--functions", functions, "--sweep-count", str(sweep_count),
                 "--sweep-out", str(sweep)], env=env, stdout=stream)
        results[label] = artifacts.load(out / f"primitive-{label}.json")
    with open(out / "rust-contract.json", "w", encoding="utf-8") as stream:
        run([str(out / "bin/rust_probe"), "--sweep-count", str(sweep_count),
             "--sweep-out", str(out / "rust-contract.sweep.bin")], env=contract, stdout=stream)
    results["rust"] = artifacts.load(out / "rust-contract.json")
    if has_matrix:
        with open(out / "matrix-contract.json", "w", encoding="utf-8") as stream:
            run([str(out / "bin/matrix_probe")], env=contract, stdout=stream)
        results["matrix"] = artifacts.load(out / "matrix-contract.json")
    return results


def dispatch_report(out, observation, environment, lock, candidate):
    """Per-function dispatch chain proof; see dispatch_proof.py."""
    return dispatch_proof.run_report(out, observation, environment, lock, candidate)


def copy_locked_libm(out, lock):
    """Keep the locked libm and debug file with the run, so its proof can be rechecked."""
    (out / "libm").mkdir()
    for source, name in ((lock["libm"]["path"], dispatch_proof.LIBM_COPY),
                         (lock["libm"]["debug_file"], dispatch_proof.DEBUG_COPY)):
        shutil.copyfile(source, out / name)
    _libm, _debug, problems = dispatch_proof.locked_libm(out, lock)
    if problems:
        raise SystemExit("installed libm files differ from the lock: " + "; ".join(problems))


def sweep_differences(left, right, limit=20):
    a = Path(left).read_bytes()
    b = Path(right).read_bytes()
    if not a or not b or len(a) % 24 or len(b) % 24 or len(a) != len(b):
        return {"status": "different", "reason": "empty, truncated or different sweep lengths"}
    inputs = sin = cos = 0
    examples = []
    for index in range(0, len(a), 24):
        x1, s1, c1 = a[index:index + 8], a[index + 8:index + 16], a[index + 16:index + 24]
        x2, s2, c2 = b[index:index + 8], b[index + 8:index + 16], b[index + 16:index + 24]
        if x1 != x2:
            inputs += 1
        if s1 != s2:
            sin += 1
        if c1 != c2:
            cos += 1
        if (x1 != x2 or s1 != s2 or c1 != c2) and len(examples) < limit:
            examples.append({"index": index // 24, "x": x1[::-1].hex(),
                             "sin": [s1[::-1].hex(), s2[::-1].hex()], "cos": [c1[::-1].hex(), c2[::-1].hex()]})
    status = "identical" if not (inputs or sin or cos) else "different"
    return {"status": status, "angles": len(a) // 24, "input_differences": inputs,
            "sin_differences": sin, "cos_differences": cos, "examples": examples}


def field_differences(left, right, fields, key=("x",)):
    """Compare named fields of records matched by their input key; list every difference."""
    differences = []
    right_by_key = {tuple(item[k] for k in key): item for item in right}
    left_keys = {tuple(item[k] for k in key) for item in left}
    if (len(right_by_key) != len(right) or len(left_keys) != len(left)
            or left_keys != set(right_by_key)
            or [tuple(item[k] for k in key) for item in left]
               != [tuple(item[k] for k in key) for item in right]):
        return {"status": "different", "count": None, "reason": "record sets differ", "differences": []}
    for item in left:
        identity = tuple(item[k] for k in key)
        other = right_by_key.get(identity)
        for left_field, right_field in fields:
            if other is None or item[left_field] != other[right_field]:
                differences.append({"key": list(identity), "field": left_field, "left": item[left_field],
                                    "right": None if other is None else other[right_field]})
    return {"status": "identical" if not differences else "different", "count": len(differences),
            "differences": differences}


def comparison_report(out, results, lock, candidate, environment):
    contract = results["contract"]
    default_report, _ = dispatch_report(out, results["host-default"], environment, lock, candidate)
    return {
        "contract_vs_host_default_dispatch": {
            "note": "Host CPU dispatch without GLIBC_TUNABLES; informational, not the reference.",
            "dispatch": default_report,
            "libm_values": field_differences(results["host-default"]["libm_values"], contract["libm_values"],
                                             [(f, f) for f in ("sin", "cos", "sincos_sin", "sincos_cos",
                                                               "compiled_cos", "compiled_sin")]),
            "sweep": sweep_differences(out / "primitive-host-default.sweep.bin",
                                       out / "primitive-contract.sweep.bin"),
        },
        "cpp_vs_rust_native": {
            "note": "Both call glibc libm; arithmetic NaN results reflect each compiler's operand order.",
            "libm_values": field_differences(contract["libm_values"], results["rust"]["libm_values"],
                                             [("sin", "sin"), ("cos", "cos"), ("sincos_sin", "sin_cos_sin"),
                                              ("sincos_cos", "sin_cos_cos")]),
            "sweep": sweep_differences(out / "primitive-contract.sweep.bin", out / "rust-contract.sweep.bin"),
            "nan_arithmetic": field_differences(contract["nan_arithmetic"], results["rust"]["nan_arithmetic"],
                                                [(op, op) for op in ("add", "sub", "mul", "div")],
                                                key=("a", "b")),
        },
        "compiled_pair_vs_separate_calls": field_differences(
            contract["libm_values"], contract["libm_values"],
            [("compiled_cos", "cos"), ("compiled_sin", "sin"), ("sincos_cos", "cos"), ("sincos_sin", "sin")]),
    }


def analyse(out, results, lock, candidate, built):
    contract = results["contract"]
    checks = []

    def check(name, passed, detail):
        checks.append({"check": name, "passed": bool(passed), "detail": detail})

    environment = json.loads((out / "environment.json").read_text(encoding="utf-8"))
    report, violations = dispatch_report(out, contract, environment, lock, candidate)
    check("every checked libm function executes its locked SSE2 implementation (traced chain)", not violations,
          {"dispatch": report, "violations": violations})
    state = contract["floating_point_state"]
    check("MXCSR is the default (no FTZ/DAZ, round to nearest)", state["mxcsr"] == EXPECTED_MXCSR, state)
    check("x87 control word is the default", state["x87_control"] == EXPECTED_X87, state)
    check("rounding mode is to nearest", state["round_to_nearest"], state)
    check("global C++ locale is classic", state["global_locale"] == "C", state)
    macros = {m["name"]: m["defined"] for m in contract["build"]["macros"]}
    check("probe compiled without fast-math or FMA/AVX code generation",
          macros["__SSE2__"] and not any(macros[m] for m in ("__AVX__", "__FMA__", "__FAST_MATH__",
                                                             "__FP_FAST_FMA")), macros)
    check("glibc runtime matches the contract",
          contract["build"]["glibc_runtime"] == candidate["toolchain"]["glibc_version"], contract["build"])
    infinity = next(v for v in contract["stream_format"] if v["x"] == "7ff0000000000000")
    upper_fixed = infinity["ostream"]["uppercase_fixed"]
    check("libstdc++ runtime formats uppercase|fixed infinity as pre-LWG 4084 'inf'", upper_fixed == "inf",
          {"uppercase_fixed(+inf)": upper_fixed, "contract": candidate["toolchain"]["libstdcxx_runtime_family"]})
    # The vDSO has no file; every other loaded object is hashed.
    loaded = [Path(name).resolve() for name in contract["loaded_objects"] if name.startswith("/")]
    locked = {item["resolved_path"]: item["sha256"] for item in lock["toolchain_files"]}
    runtime_hashes = {}
    for path in loaded:
        digest = sha256(path)
        runtime_hashes[str(path)] = digest
        if str(path) in locked and locked[str(path)] != digest:
            check("loaded runtime object matches the lock: " + str(path), False, digest)
    for required in ("/usr/lib/x86_64-linux-gnu/libm.so.6", "/usr/lib/x86_64-linux-gnu/libc.so.6",
                     "/usr/lib/x86_64-linux-gnu/libstdc++.so.6.0.33"):
        check("probe loaded locked " + required, required in runtime_hashes
              and runtime_hashes[required] == locked.get(required), runtime_hashes.get(required))

    check("installed libm debug symbols name the same implementations as the lock",
          environment["libm"]["matches_lock"],
          {"debug_file": environment["libm"]["debug_file"],
           "excluded_suffix_matches": environment["libm"]["excluded_suffix_matches"]})

    comparisons = comparison_report(out, results, lock, candidate, environment)
    outputs = artifacts.inventory(out)
    summary = {
        "schema": 2,
        "sweep_count": contract["sweep"]["count"],
        "status": lock["status"],
        "qualification": "not asserted: this probe records observations for independent review",
        "lock_sha256": sha256(REFENV / "inputs.lock.json"),
        "contract_passed": all(c["passed"] for c in checks),
        "contract_checks": checks,
        "comparisons": comparisons,
        "cpp_rust_identical": all(value["status"] == "identical" for key, value in
                                  comparisons["cpp_vs_rust_native"].items() if key != "note"),
        "differences_accepted": False,
        "built": built,
        "runtime_object_sha256": runtime_hashes,
        "output_sha256": outputs,
    }
    (out / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--sweep-count", type=int, default=200000)
    parser.add_argument("--oracle-build", type=Path, help="audited oracle build directory (matrix probe)")
    args = parser.parse_args()
    if not 1 <= args.sweep_count <= 2000000:
        parser.error("--sweep-count must be 1..2000000")
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    if any(out.iterdir()):
        parser.error(f"{out} must be empty")
    for name in ("inputs.lock.json", "candidate.toml"):
        shutil.copyfile(REFENV / name, out / name)
    lock = artifacts.load(REFENV / "inputs.lock.json")
    candidate = tomllib.loads((REFENV / "candidate.toml").read_text(encoding="utf-8"))
    run([sys.executable, str(REFENV / "image/verify_image.py"), "--lock", str(REFENV / "inputs.lock.json"),
         "--candidate", str(REFENV / "candidate.toml"), "--expect", str(REFENV / "image-identity.json")])
    copy_locked_libm(out, lock)
    built = build(out, args.oracle_build.resolve() if args.oracle_build else None)
    results = run_probes(out, candidate, args.sweep_count, args.oracle_build is not None)
    with open(out / "environment.json", "w", encoding="utf-8") as stream:
        run([sys.executable, str(HERE / "collect_environment.py"), "--probe", str(out / "primitive-contract.json"),
             "--lock", str(REFENV / "inputs.lock.json"), "--candidate", str(REFENV / "candidate.toml")],
            stdout=stream)
    summary = analyse(out, results, lock, candidate, built)
    for item in summary["contract_checks"]:
        print(("PASS " if item["passed"] else "FAIL ") + item["check"], file=sys.stderr)
    for name, comparison in summary["comparisons"].items():
        parts = {name: comparison} if "status" in comparison else comparison
        for part, value in parts.items():
            if isinstance(value, dict) and "status" in value:
                print(f"compare {name} {part}: {value['status']}", file=sys.stderr)
    print("Qualification is not asserted by this probe.", file=sys.stderr)
    if not summary["contract_passed"]:
        return 1
    return 0 if summary["cpp_rust_identical"] else 4


if __name__ == "__main__":
    sys.exit(main())
