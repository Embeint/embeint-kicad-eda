# SPDX-License-Identifier: GPL-3.0-or-later
"""Strict, lossless validation of a complete probe run before comparison.

Hashes establish internal consistency, not authenticity or qualification. JSON
floating numbers are forbidden: all floating observations use exact hex bits.
"""
import hashlib
import json
from contextlib import ExitStack
from pathlib import Path
import re
import struct
import tomllib

HEX = re.compile(r"[0-9a-f]{16}")
SHA = re.compile(r"[0-9a-f]{64}")


def require(value, message):
    if not value:
        raise ValueError(message)


def object_pairs(pairs):
    out = {}
    for key, value in pairs:
        require(key not in out, "duplicate JSON key: " + key)
        out[key] = value
    return out


def reject_number(value):
    raise ValueError("non-integer/nonfinite JSON number: " + value)


def load(path):
    return json.loads(
        Path(path).read_text(encoding="utf-8"),
        object_pairs_hook=object_pairs,
        parse_float=reject_number,
        parse_constant=reject_number,
    )


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def inventory(directory):
    files = {}
    for path in sorted(directory.rglob("*")):
        require(not path.is_symlink(), "symlink artifact: " + str(path))
        if not path.is_dir():
            require(path.is_file(), "non-file artifact")
            name = path.relative_to(directory).as_posix()
            if name != "summary.json":
                files[name] = sha256(path)
    return files


def keys(value, wanted):
    require(
        type(value) is dict and set(value) == set(wanted.split()),
        "unexpected/missing fields: " + wanted,
    )


def strings(values):
    require(type(values) is list and all(type(x) is str for x in values), "expected string list")


def integer(value, minimum, maximum):
    require(type(value) is int and minimum <= value <= maximum, "invalid integer/count")


def matching_types(value, expected):
    """Require the complete recomputed schema, including bool versus int."""
    require(type(value) is type(expected), "invalid comparison field type")
    if type(expected) is dict:
        require(set(value) == set(expected), "invalid comparison fields")
        for key in expected:
            matching_types(value[key], expected[key])
    elif type(expected) is list:
        require(len(value) == len(expected), "invalid comparison list length")
        for actual, wanted in zip(value, expected):
            matching_types(actual, wanted)


def sweep_inputs(count):
    """Reconstruct input bits independently of all recorded probe outputs.

    Match the fixed SplitMix64 seed and separately rounded binary64 operations
    in probe_common.h and rust_probe.rs; no transcendental result is computed.
    """
    mask = (1 << 64) - 1
    state = 0x4b69436164526566
    for index in range(count):
        state = (state + 0x9e3779b97f4a7c15) & mask
        x = ((state ^ (state >> 30)) * 0xbf58476d1ce4e5b9) & mask
        x = ((x ^ (x >> 27)) * 0x94d049bb133111eb) & mask
        x ^= x >> 31
        family = index % 4
        if family < 2:
            u = float(x >> 11) * (1.0 / 9007199254740992.0)
            angle = (u * 2.0 - 1.0) * (6.283185307179586 if family == 0 else 10000.0)
        elif family == 2:
            angle = float(x % 3600000) / 10000.0 * (3.141592653589793 / 180.0)
        else:
            exponent = 1023 - 30 + ((x >> 52) % 61)
            yield ((x >> 63) << 63) | (exponent << 52) | (x & 0x000fffffffffffff)
            continue
        yield struct.unpack("<Q", struct.pack("<d", angle))[0]


def validate_sweeps(paths, count):
    """Check every input identity/order, even if all three outputs agree."""
    with ExitStack() as stack:
        streams = []
        for path in paths:
            require(path.stat().st_size == 24 * count, "empty/truncated/wrong-count sweep: " + path.name)
            streams.append((path.name, stack.enter_context(path.open("rb"))))
        for index, wanted in enumerate(sweep_inputs(count)):
            for name, stream in streams:
                record = stream.read(24)
                require(
                    len(record) == 24 and struct.unpack_from("<Q", record)[0] == wanted,
                    f"sweep input identity/order differs: {name} record {index}",
                )
        for name, stream in streams:
            require(not stream.read(1), "extra sweep record: " + name)


def bits(value):
    require(type(value) is str and HEX.fullmatch(value), "invalid floating bit string")


def bitlist(value, count):
    require(type(value) is list and len(value) == count, "wrong bit array length")
    for item in value:
        bits(item)


def input_tables():
    common = (Path(__file__).parent / "probe_common.h").read_text()

    def table(name):
        body = re.search(name + r"\[\]\s*=\s*\{(.*?)\};", common, re.S).group(1)
        return re.findall(r"0x([0-9a-f]{16})ULL", body)

    return table("LIBM_INPUTS"), table("NAN_OPERANDS")


def primitive(value, rust, count, candidate):
    common = "schema probe libm_values nan_arithmetic sweep"
    keys(
        value,
        (
            common
            if rust
            else common + " build loaded_objects floating_point_state libm_dispatch stream_format"
        ),
    )
    integer(value["schema"], 1, 1)
    require(value["probe"] == ("rust" if rust else "primitive"), "wrong probe identity")
    inputs, operands = input_tables()
    require(
        type(value["libm_values"]) is list and [r["x"] for r in value["libm_values"]] == inputs,
        "missing/duplicate/reordered libm input identities",
    )
    outputs = (
        "sin cos sin_cos_sin sin_cos_cos"
        if rust
        else "sin cos sincos_sin sincos_cos compiled_cos compiled_sin"
    )
    for row in value["libm_values"]:
        keys(row, "x " + outputs)
        for item in row.values():
            bits(item)
    require(
        type(value["nan_arithmetic"]) is list
        and [(r["a"], r["b"]) for r in value["nan_arithmetic"]]
        == [(a, b) for a in operands for b in operands],
        "wrong arithmetic operand identities",
    )
    for row in value["nan_arithmetic"]:
        keys(row, "a b add sub mul div")
        for item in row.values():
            bits(item)
    keys(value["sweep"], "seed count record")
    integer(value["sweep"]["count"], 1, 2000000)
    require(
        value["sweep"]
        == {"seed": "4b69436164526566", "count": count, "record": "x,sin,cos as little-endian u64"},
        "sweep metadata differs",
    )
    if rust:
        return
    strings(value["loaded_objects"])
    keys(value["floating_point_state"], "mxcsr x87_control round_to_nearest global_locale")
    state = value["floating_point_state"]
    require(type(state["round_to_nearest"]) is bool, "invalid rounding flag")
    require(
        re.fullmatch(r"[0-9a-f]{8}", state["mxcsr"])
        and re.fullmatch(r"[0-9a-f]{4}", state["x87_control"])
        and type(state["global_locale"]) is str,
        "malformed FP state",
    )
    keys(
        value["build"],
        "compiler glibcxx_release glibcxx_header_date glibc_headers glibc_runtime "
        "flt_eval_method gcc_iec_559 macros",
    )
    require(
        all(type(v) is str for k, v in value["build"].items() if k != "macros"),
        "malformed build facts",
    )
    macros = value["build"]["macros"]
    require(
        [m["name"] for m in macros]
        == [
            "__SSE2__",
            "__AVX__",
            "__FMA__",
            "__FAST_MATH__",
            "__FP_FAST_FMA",
            "__NO_MATH_ERRNO__",
        ],
        "macro identities differ",
    )
    for item in macros:
        keys(item, "name defined value")
        require(type(item["defined"]) is bool and type(item["value"]) is str, "malformed macro")
    require(
        [r["function"] for r in value["libm_dispatch"]]
        == candidate["dispatch"]["checked_functions"],
        "dispatch identities differ",
    )
    for row in value["libm_dispatch"]:
        keys(row, "function object offset")
        require(
            type(row["object"]) is str and re.fullmatch(r"0x[0-9a-f]+", row["offset"]),
            "invalid dispatch",
        )
    stream_inputs = [
        "7ff0000000000000",
        "fff0000000000000",
        "7ff8000000000000",
        "fff8000000000000",
        "7ff8000000000123",
        "0000000000000000",
        "8000000000000000",
        "3fb999999999999a",
        "7e37e43c8800759c",
        "0000000000000001",
        "40fe240c9fbe76c9",
        "3fd5555555555555",
    ]
    require([r["x"] for r in value["stream_format"]] == stream_inputs, "stream identities differ")
    for row in value["stream_format"]:
        keys(row, "x ostream printf")
        keys(
            row["ostream"],
            "default fixed scientific hexfloat uppercase_default uppercase_fixed "
            "uppercase_scientific uppercase_hexfloat showpos_fixed precision17_default precision0_fixed",
        )
        keys(row["printf"], "%f %F %e %E %g %G %a %A %.17g")
        require(
            all(type(v) is str for part in ("ostream", "printf") for v in row[part].values()),
            "bad stream text",
        )


def matrix(value):
    keys(value, "schema probe compiler cases stream")
    integer(value["schema"], 1, 1)
    require(
        value["probe"] == "matrix" and type(value["compiler"]) is str, "invalid matrix identity"
    )
    require(type(value["cases"]) is list and len(value["cases"]) == 96, "wrong matrix count")
    for index, row in enumerate(value["cases"]):
        keys(
            row,
            "case a b v2 v3 scalar determinant inverse mul_matrix mul_vector2 mul_vector3 "
            "mul_scalar_right mul_scalar_left",
        )
        integer(row["case"], index, index)
        for name in ("a", "b", "inverse", "mul_matrix", "mul_scalar_right", "mul_scalar_left"):
            bitlist(row[name], 9)
        for name in ("v2", "mul_vector2"):
            bitlist(row[name], 2)
        for name in ("v3", "mul_vector3"):
            bitlist(row[name], 3)
        bits(row["scalar"])
        bits(row["determinant"])
    keys(value["stream"], "default uppercase_fixed uppercase_scientific")
    require(all(type(x) is str for x in value["stream"].values()), "invalid matrix stream")


def validate(directory):
    # Import lazily: run_probe also uses the strict decoder/inventory writer.
    import run_probe

    summary = load(directory / "summary.json")
    keys(
        summary,
        "schema status qualification lock_sha256 contract_passed contract_checks comparisons "
        "cpp_rust_identical differences_accepted built runtime_object_sha256 output_sha256 sweep_count",
    )
    integer(summary["schema"], 2, 2)
    integer(summary["sweep_count"], 1, 2000000)
    require(
        summary["status"] == "candidate-unqualified" and summary["differences_accepted"] is False,
        "invalid candidate/acceptance status",
    )
    require(
        type(summary["qualification"]) is str and bool(summary["qualification"]),
        "missing qualification label",
    )
    actual = inventory(directory)
    require(
        type(summary["output_sha256"]) is dict and summary["output_sha256"] == actual,
        "artifact hash/inventory mismatch",
    )
    expected = {
        "inputs.lock.json",
        "candidate.toml",
        "environment.json",
        "primitive-contract.json",
        "primitive-host-default.json",
        "rust-contract.json",
        "primitive-contract.sweep.bin",
        "primitive-host-default.sweep.bin",
        "rust-contract.sweep.bin",
        "bin/primitive_probe",
        "bin/rust_probe",
    }
    has_matrix = "matrix-contract.json" in actual
    if has_matrix:
        expected.update({"matrix-contract.json", "bin/matrix_probe", "bin/matrix_probe.o"})
    require(set(actual) == expected, "unexpected/missing artifact set")
    for name in actual:
        if name.endswith(".json"):
            load(directory / name)
    lock = load(directory / "inputs.lock.json")
    candidate = tomllib.loads((directory / "candidate.toml").read_text())
    require(
        lock["schema"] == 1 and type(lock["schema"]) is int and lock["status"] == summary["status"],
        "invalid lock schema",
    )
    require(
        summary["lock_sha256"] == actual["inputs.lock.json"]
        and lock["candidate_sha256"] == actual["candidate.toml"],
        "lock/candidate identity mismatch",
    )
    results = {}
    sweep_paths = []
    for label in ("contract", "host-default", "rust"):
        name = "rust-contract" if label == "rust" else "primitive-" + label
        results[label] = load(directory / (name + ".json"))
        primitive(results[label], label == "rust", summary["sweep_count"], candidate)
        sweep_paths.append(directory / (name + ".sweep.bin"))
    validate_sweeps(sweep_paths, summary["sweep_count"])
    if has_matrix:
        matrix(load(directory / "matrix-contract.json"))
    built = summary["built"]
    require(
        set(built) == ({"primitive", "rust", "matrix"} if has_matrix else {"primitive", "rust"}),
        "build identities differ",
    )
    for name, record in built.items():
        binary = "bin/" + ("primitive_probe" if name == "primitive" else name + "_probe")
        require(record["sha256"] == actual[binary], "built binary hash mismatch")
        for key in (("compile_command", "link_command") if name == "matrix" else ("command",)):
            strings(record[key])
            require(bool(record[key]), "empty build command")
    env = load(directory / "environment.json")
    for field in (
        "host",
        "loader",
        "environment",
        "tools",
        "image_identity",
        "os_release",
        "libm",
        "loaded_objects",
    ):
        require(field in env, "missing environment field: " + field)
    integer(env["schema"], 1, 1)
    require(
        type(env["host"]["cpu"]) is dict and type(env["host"]["uname"]) is dict,
        "missing host identity",
    )
    keys(
        env["loader"],
        "contract_diagnostics host_default_diagnostics contract_help host_default_help",
    )
    keys(env["tools"], "gcc g++ ld as cmake ninja python3 rustc cargo git wx-config dpkg-query")
    keys(env["environment"], "recorded names umask")
    strings(env["environment"]["names"])
    require(
        type(env["environment"]["recorded"]) is dict and type(env["os_release"]) is str,
        "malformed environment",
    )
    for commands in (env["loader"], env["tools"]):
        require(type(commands) is dict and bool(commands), "missing tool/loader facts")
        for command in commands.values():
            keys(command, "command returncode stdout stderr")
            strings(command["command"])
            integer(command["returncode"], 0, 0)
            require(
                type(command["stdout"]) is str and type(command["stderr"]) is str,
                "malformed tool output",
            )
    image = env["image_identity"]
    keys(
        image,
        "schema status lock_sha256 candidate_sha256 dpkg_query_sha256 toolchain_files_sha256 compiler rust glibc_tunables",
    )
    integer(image["schema"], 1, 1)
    require(
        type(image["dpkg_query_sha256"]) is str and SHA.fullmatch(image["dpkg_query_sha256"]),
        "invalid package identity",
    )
    require(
        image["glibc_tunables"] == candidate["dispatch"]["glibc_tunables"],
        "image dispatch environment differs",
    )
    keys(
        image["compiler"],
        "gcc g++ cc c++ predefined_macros_sha256 target_options glibc libstdcxx_runtime",
    )
    for name in ("gcc", "g++", "cc", "c++"):
        keys(image["compiler"][name], "path version")
        require(
            image["compiler"][name]["version"] == candidate["toolchain"]["gcc_version"],
            "compiler version differs",
        )
    require(
        image["compiler"]["glibc"] == "glibc " + candidate["toolchain"]["glibc_version"]
        and image["compiler"]["libstdcxx_runtime"]
        == candidate["toolchain"]["libstdcxx_runtime_soname"],
        "compiler runtime differs",
    )
    require(
        image["lock_sha256"] == summary["lock_sha256"]
        and image["candidate_sha256"] == lock["candidate_sha256"]
        and image["status"] == summary["status"],
        "image identity differs",
    )
    require(
        image["toolchain_files_sha256"]
        == {p["path"]: p["sha256"] for p in lock["toolchain_files"]},
        "toolchain bytes differ",
    )
    require(
        image["rust"]["rustc"]["release"] == lock["rust"]["version"]
        and image["rust"]["rustc"]["host"] == "x86_64-unknown-linux-gnu",
        "Rust identity differs",
    )
    rust_identity = image["rust"]
    keys(rust_identity, "rustc cargo components inputs")
    strings(rust_identity["components"])
    wanted = sorted(
        f"{c['name']}-{c['target']}" if c["name"] == "rust-std" else c["name"]
        for c in lock["rust"]["components"]
    )
    require(
        rust_identity["components"] == wanted and type(rust_identity["cargo"]) is str,
        "Rust components differ",
    )
    closure = rust_identity["inputs"]
    require(type(closure) is dict and bool(closure), "missing Rust input closure")
    for path, entry in closure.items():
        require(
            path.startswith("/usr/local/") and ".." not in Path(path).parts,
            "unsafe Rust input path",
        )
        if entry["kind"] == "file":
            keys(entry, "kind sha256 mode")
            integer(entry["mode"], 0, 4095)
        else:
            keys(entry, "kind sha256 target resolved")
            require(
                entry["kind"] == "symlink"
                and type(entry["target"]) is str
                and entry["resolved"] in closure
                and closure[entry["resolved"]]["kind"] == "file",
                "invalid Rust input link",
            )
        require(
            type(entry["sha256"]) is str and SHA.fullmatch(entry["sha256"]),
            "invalid Rust input digest",
        )
    for required in (
        "bin/rustc",
        "bin/cargo",
        "lib/rustlib/components",
        "lib/rustlib/rust-installer-version",
        "lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld",
        *("lib/rustlib/manifest-" + name for name in wanted),
    ):
        require("/usr/local/" + required in closure, "missing Rust compiler/linker/manifest input")
    for target in ("x86_64-unknown-linux-gnu", "wasm32-unknown-unknown"):
        require(
            any(
                path.startswith("/usr/local/lib/rustlib/" + target + "/lib/libstd-")
                and path.endswith(".rlib")
                for path in closure
            ),
            "missing Rust target stdlib",
        )
    contract = results["contract"]
    state = contract["floating_point_state"]
    require(
        state
        == {
            "mxcsr": "00001f80",
            "x87_control": "037f",
            "round_to_nearest": True,
            "global_locale": "C",
        },
        "FP contract failed",
    )
    dispatch = []
    for row in contract["libm_dispatch"]:
        require(
            row["object"] in env["loaded_objects"],
            "dispatch object is not a recorded loaded object",
        )
        dispatch.append({**row, "object": env["loaded_objects"][row["object"]]["resolved"]})
    require(
        not run_probe.dispatch_report({"libm_dispatch": dispatch}, lock, candidate)[1],
        "dispatch contract failed",
    )
    macros = {m["name"]: m["defined"] for m in contract["build"]["macros"]}
    require(
        macros["__SSE2__"]
        and not any(macros[n] for n in ("__AVX__", "__FMA__", "__FAST_MATH__", "__FP_FAST_FMA")),
        "macro contract failed",
    )
    require(
        contract["build"]["glibc_runtime"] == candidate["toolchain"]["glibc_version"]
        and contract["stream_format"][0]["ostream"]["uppercase_fixed"] == "inf",
        "runtime contract failed",
    )
    require(
        env["libm"]["multiarch_symbols_from_nm"] == lock["libm"]["multiarch_symbols"]
        and env["libm"]["matches_lock"] is True,
        "libm symbol identity differs",
    )
    hashes = {entry["resolved"]: entry["sha256"] for entry in env["loaded_objects"].values()}
    require(
        set(env["loaded_objects"]) == {n for n in contract["loaded_objects"] if n.startswith("/")},
        "loaded object set differs",
    )
    require(
        all(type(p) is str and p.startswith("/") and SHA.fullmatch(h) for p, h in hashes.items()),
        "invalid runtime hash",
    )
    require(hashes == summary["runtime_object_sha256"], "runtime observation hashes differ")
    for name in ("libm.so.6", "libc.so.6", candidate["toolchain"]["libstdcxx_runtime_soname"]):
        path = "/usr/lib/x86_64-linux-gnu/" + name
        wanted = next(p["sha256"] for p in lock["toolchain_files"] if p["resolved_path"] == path)
        require(hashes.get(path) == wanted, "loaded runtime identity differs")
    require(
        summary["contract_passed"] is True
        and type(summary["contract_checks"]) is list
        and len(summary["contract_checks"]) >= 12,
        "missing/failed contract checks",
    )
    for check in summary["contract_checks"]:
        keys(check, "check passed detail")
        require(check["passed"] is True and type(check["check"]) is str, "failed contract check")
    comparisons = run_probe.comparison_report(directory, results, lock, candidate)
    matching_types(summary["comparisons"], comparisons)
    require(
        summary["comparisons"] == comparisons,
        "recorded comparisons differ from recomputed observations",
    )
    require(
        all(
            value["status"] == "identical"
            for key, value in comparisons["cpp_vs_rust_native"].items()
            if key != "note"
        )
        and summary["cpp_rust_identical"] is True,
        "recorded/recomputed C++/Rust comparison failed",
    )
    return env
