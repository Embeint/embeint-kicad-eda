# SPDX-License-Identifier: GPL-3.0-or-later
"""Synthetic evidence-schema fixtures, not executed mathematical/reference proof."""
import copy
import json
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import tomllib
import unittest

ENV = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ENV / "probe"))
sys.path.insert(0, str(Path(__file__).resolve().parent))
import artifacts
import libm_elf
import libm_fixtures
import run_probe


def write(path, value):
    path.write_text(json.dumps(value, sort_keys=True) + "\n")


def fixture(directory):
    directory.mkdir()
    (directory / "bin").mkdir()
    shutil.copy(ENV / "candidate.toml", directory / "candidate.toml")
    import tomllib

    candidate = tomllib.loads((directory / "candidate.toml").read_text())
    # A synthetic libm with real code stands in for the locked one: the lock in
    # this fixture names its hashes, so the proof re-derives facts from it.
    syn = libm_fixtures.synthetic_libm(
        artifacts.load(ENV / "inputs.lock.json"), candidate["dispatch"]["checked_functions"]
    )
    write(directory / "inputs.lock.json", syn.lock)
    (directory / "libm").mkdir()
    (directory / "libm/libm.so.6").write_bytes(syn.libm)
    (directory / "libm/libm.debug").write_bytes(syn.debug)
    lock = artifacts.load(directory / "inputs.lock.json")
    inputs, operands = artifacts.input_tables()
    bit = "3ff0000000000000"
    rust = {
        "schema": 1,
        "probe": "rust",
        "libm_values": [
            dict(x=x, sin=bit, cos=bit, sin_cos_sin=bit, sin_cos_cos=bit) for x in inputs
        ],
        "nan_arithmetic": [
            dict(a=a, b=b, add=bit, sub=bit, mul=bit, div=bit) for a in operands for b in operands
        ],
        "sweep": {
            "seed": "4b69436164526566",
            "count": 2,
            "record": "x,sin,cos as little-endian u64",
        },
    }
    primitive = copy.deepcopy(rust)
    primitive["probe"] = "primitive"
    primitive["libm_values"] = [
        dict(
            x=x,
            sin=bit,
            cos=bit,
            sincos_sin=bit,
            sincos_cos=bit,
            compiled_sin=bit,
            compiled_cos=bit,
        )
        for x in inputs
    ]
    primitive["floating_point_state"] = dict(
        mxcsr="00001f80", x87_control="037f", round_to_nearest=True, global_locale="C"
    )
    macros = [
        "__SSE2__",
        "__AVX__",
        "__FMA__",
        "__FAST_MATH__",
        "__FP_FAST_FMA",
        "__NO_MATH_ERRNO__",
    ]
    primitive["build"] = {
        k: "fixture"
        for k in (
            "compiler",
            "glibcxx_release",
            "glibcxx_header_date",
            "glibc_headers",
            "glibc_runtime",
            "flt_eval_method",
            "gcc_iec_559",
        )
    }
    primitive["build"]["glibc_runtime"] = candidate["toolchain"]["glibc_version"]
    primitive["build"]["macros"] = [
        dict(name=n, defined=n == "__SSE2__", value="1") for n in macros
    ]
    loaded = {
        p["resolved_path"]: {"resolved": p["resolved_path"], "sha256": p["sha256"]}
        for p in lock["toolchain_files"]
        if p["resolved_path"].endswith((".so.6", ".so.6.0.33", ".so.1"))
    }
    primitive["loaded_objects"] = list(loaded)
    primitive["schema"] = 3
    primitive["probe_object"] = "/out/bin/primitive_probe"
    primitive["libm_dispatch"] = syn.records
    table, excluded = libm_elf.multiarch_symbols(libm_elf.Elf64(syn.debug))
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
    primitive["stream_format"] = [
        dict(
            x=x,
            ostream={
                k: "inf"
                for k in (
                    "default fixed scientific hexfloat "
                    "uppercase_default uppercase_fixed uppercase_scientific uppercase_hexfloat showpos_fixed "
                    "precision17_default precision0_fixed"
                ).split()
            },
            printf={k: "inf" for k in "%f %F %e %E %g %G %a %A %.17g".split()},
        )
        for x in stream_inputs
    ]
    for name, data in [
        ("rust-contract", rust),
        ("primitive-contract", primitive),
        ("primitive-host-default", primitive),
    ]:
        write(directory / (name + ".json"), data)
        # Fixed input words independently checked against the compiled Rust
        # probe; only result words are synthetic in this schema fixture.
        (directory / (name + ".sweep.bin")).write_bytes(
            struct.pack("<6Q", 0xc002f0aa94d2eab5, 2, 3, 0xc0b2e9adc292f30e, 5, 6)
        )
    image = dict(
        schema=1,
        status="candidate-unqualified",
        lock_sha256=artifacts.sha256(directory / "inputs.lock.json"),
        candidate_sha256=lock["candidate_sha256"],
        toolchain_files_sha256={p["path"]: p["sha256"] for p in lock["toolchain_files"]},
        rust={"rustc": {"release": lock["rust"]["version"], "host": "x86_64-unknown-linux-gnu"}},
    )
    output = dict(command=["fixture"], returncode=0, stdout="fixture", stderr="")
    env = dict(
        schema=2,
        host=dict(cpu={"model name": "synthetic"}, uname={}),
        loader={
            k: output
            for k in "contract_diagnostics host_default_diagnostics contract_help host_default_help".split()
        },
        environment=dict(recorded={}, names=[], umask="022"),
        tools={
            k: output
            for k in "gcc g++ ld as cmake ninja python3 rustc cargo git wx-config dpkg-query".split()
        },
        image_identity=image,
        os_release="fixture",
        libm=dict(
            debug_file=lock["libm"]["debug_file"],
            multiarch_symbols_from_debug_file=table,
            excluded_suffix_matches=excluded,
            matches_lock=True,
            dispatch_facts=syn.facts,
        ),
        loaded_objects=loaded,
    )
    write(directory / "environment.json", env)
    built = {}
    for name in ("primitive", "rust"):
        binary = directory / "bin" / (name + "_probe")
        binary.write_bytes(libm_fixtures.probe_elf() if name == "primitive" else b"synthetic binary")
        command = ["fixture", "-o", "/out/bin/primitive_probe"] if name == "primitive" else ["fixture"]
        built[name] = dict(sha256=artifacts.sha256(binary), command=command)
    comparisons = run_probe.comparison_report(
        directory,
        {"contract": primitive, "host-default": primitive, "rust": rust},
        lock,
        candidate,
        env,
    )
    image.update(
        dpkg_query_sha256="0" * 64,
        glibc_tunables=candidate["dispatch"]["glibc_tunables"],
        compiler={
            **{
                n: dict(path="/usr/bin/gcc", version=candidate["toolchain"]["gcc_version"])
                for n in ("gcc", "g++", "cc", "c++")
            },
            "predefined_macros_sha256": "0" * 64,
            "target_options": [],
            "glibc": "glibc " + candidate["toolchain"]["glibc_version"],
            "libstdcxx_runtime": candidate["toolchain"]["libstdcxx_runtime_soname"],
        },
    )
    wanted = sorted(
        f"{c['name']}-{c['target']}" if c["name"] == "rust-std" else c["name"]
        for c in lock["rust"]["components"]
    )
    closure = [
        "bin/rustc",
        "bin/cargo",
        "lib/rustlib/components",
        "lib/rustlib/rust-installer-version",
        "lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld",
        "lib/rustlib/x86_64-unknown-linux-gnu/lib/libstd-native.rlib",
        "lib/rustlib/wasm32-unknown-unknown/lib/libstd-wasm.rlib",
    ] + ["lib/rustlib/manifest-" + n for n in wanted]
    image["rust"].update(
        cargo="cargo 1.99.1",
        components=wanted,
        inputs={"/usr/local/" + p: dict(kind="file", sha256="0" * 64, mode=420) for p in closure},
    )
    write(directory / "environment.json", env)
    summary = dict(
        schema=2,
        status="candidate-unqualified",
        qualification="not asserted",
        sweep_count=2,
        lock_sha256=image["lock_sha256"],
        contract_passed=True,
        cpp_rust_identical=True,
        differences_accepted=False,
        contract_checks=[dict(check="synthetic", passed=True, detail={}) for _ in range(12)],
        comparisons=comparisons,
        built=built,
        runtime_object_sha256={v["resolved"]: v["sha256"] for v in loaded.values()},
        output_sha256=artifacts.inventory(directory),
    )
    write(directory / "summary.json", summary)


class ArtifactTest(unittest.TestCase):
    def pair(self):
        root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, root)
        for side in ("left", "right"):
            fixture(root / side)
        return root

    def compare(self, root):
        return subprocess.run(
            [
                sys.executable,
                str(ENV / "probe/compare_observations.py"),
                str(root / "left"),
                str(root / "right"),
            ],
            capture_output=True,
            text=True,
        )

    def mutate(self, directory, change):
        """Change one run, then recompute its comparisons and inventory as a forger could."""
        lock = artifacts.load(directory / "inputs.lock.json")
        candidate = tomllib.loads((directory / "candidate.toml").read_text())
        env = artifacts.load(directory / "environment.json")
        results = {
            "contract": artifacts.load(directory / "primitive-contract.json"),
            "host-default": artifacts.load(directory / "primitive-host-default.json"),
            "rust": artifacts.load(directory / "rust-contract.json"),
        }
        change(directory, env, results["contract"]["libm_dispatch"])
        write(directory / "environment.json", env)
        write(directory / "primitive-contract.json", results["contract"])
        summary = artifacts.load(directory / "summary.json")
        summary["comparisons"] = run_probe.comparison_report(directory, results, lock, candidate, env)
        summary["output_sha256"] = artifacts.inventory(directory)
        write(directory / "summary.json", summary)

    def assertComparatorRejects(self, change, fragment):
        root = self.pair()
        self.mutate(root / "left", change)
        result = self.compare(root)
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn(fragment, result.stderr)

    def test_contradictory_dispatch_facts_fail_the_comparator(self):
        def stubs(_directory, env, _rows):
            for stub in env["libm"]["dispatch_facts"]["plt_stubs"]:
                stub.update(bytes="cc" * 16, slot="0xdeadbeef")
                for relocation in stub["relocations"]:
                    relocation["symbol"] = 17

        def versions(_directory, env, _rows):
            facts = env["libm"]["dispatch_facts"]
            facts["schema"] = True
            for rows in facts["definitions"].values():
                for row in rows:
                    row.update(version=None, default=1 if row["default"] else 0)

        def library(directory, _env, _rows):
            path = directory / "libm/libm.so.6"
            data = bytearray(path.read_bytes())
            data[-1] ^= 1
            path.write_bytes(bytes(data))

        def missing(directory, _env, _rows):
            (directory / "libm/libm.debug").unlink()

        self.assertComparatorRejects(stubs, "recorded dispatch facts differ from the locked ELF files")
        self.assertComparatorRejects(versions, "recorded dispatch facts differ from the locked ELF files")
        self.assertComparatorRejects(library, "libm/libm.so.6 is not the locked file")
        self.assertComparatorRejects(missing, "unexpected/missing artifact set")

    def test_incomplete_or_impossible_traces_fail_the_comparator(self):
        wrapped = libm_fixtures.WRAPPED

        def entry_only(_directory, _env, rows):
            for row in rows:
                if row["function"] not in wrapped:
                    row["trace"] = row["trace"][:1]

        def truncated(_directory, _env, rows):
            for row in rows:
                if row["function"] in wrapped:
                    row["trace"] = row["trace"][: row["trace"].index(row["slot_reads"][0]["at"]) + 2]

        def mid_instruction(_directory, _env, rows):
            rows[0]["trace"].insert(1, f"0x{int(rows[0]['trace'][0], 16) + 1:x}")

        def forged_return(_directory, _env, rows):
            rows[0]["return_to"] = "!/elsewhere+0x10"

        def forged_slot(_directory, _env, rows):
            row = next(r for r in rows if r["function"] == "exp")
            row["slot_reads"][0]["value"] = "0x10"

        def forged_status(_directory, _env, rows):
            rows[0]["trace_status"] = "returned; child not reaped"

        self.assertComparatorRejects(entry_only, "trace ended before the entry's return")
        self.assertComparatorRejects(truncated, "trace ended before the entry's return")
        self.assertComparatorRejects(mid_instruction, "impossible successor")
        self.assertComparatorRejects(forged_return, "does not reach the recorded return address")
        self.assertComparatorRejects(forged_slot, "recorded GOT slot read does not match")
        self.assertComparatorRejects(forged_status, "trace incomplete")

    def test_invalid_completion_objects_fail_the_full_comparator(self):
        cases = [
            ("!" + artifacts.load(ENV / "inputs.lock.json")["libm"]["path"] + "+0x20000",
             "executable probe code"),
            ("!+0x0", "invalid dispatch trace"),
            ("!/out/bin/primitive_probe+0x0", "executable probe code"),
            ("!/unrecorded/primitive_probe+0x1234", "executable probe code"),
        ]
        for marker, reason in cases:
            def change(_directory, _env, rows):
                for row in rows:
                    row["return_address"] = row["return_to"] = marker
            with self.subTest(marker=marker):
                self.assertComparatorRejects(change, reason)

    def test_loaded_library_cannot_be_the_caller_alias(self):
        canonical = "/review-alias/primitive_probe"
        variants = (
            (canonical, canonical, None),
            (canonical, "/review-alias/library.so", canonical),
            ("/review-alias/./primitive_probe", canonical, None),
            ("/review-alias//primitive_probe", canonical, None),
            ("//review-alias///primitive_probe", canonical, None),
            (canonical, "/review-alias/./primitive_probe", None),
            (canonical, "/review-alias//primitive_probe", None),
            (canonical, "/review-alias/library.so", "/review-alias/./primitive_probe"),
            (canonical, "/review-alias/library.so", "/review-alias//primitive_probe"),
        )
        for name, alias, resolved in variants:
            root = self.pair()
            directory = root / "left"
            env = artifacts.load(directory / "environment.json")
            lock = artifacts.load(directory / "inputs.lock.json")
            candidate = tomllib.loads((directory / "candidate.toml").read_text())
            library = copy.deepcopy(env["loaded_objects"][lock["libm"]["path"]])
            if resolved is not None:
                library["resolved"] = resolved
            env["loaded_objects"][alias] = library
            results = {}
            for label in ("contract", "host-default"):
                result = artifacts.load(directory / ("primitive-" + label + ".json"))
                result["probe_object"] = name
                result["loaded_objects"].append(alias)
                for row in result["libm_dispatch"]:
                    for field in ("return_address", "return_to"):
                        row[field] = "!" + name + "+" + row[field].rsplit("+", 1)[1]
                results[label] = result
                write(directory / ("primitive-" + label + ".json"), result)
            results["rust"] = artifacts.load(directory / "rust-contract.json")
            summary = artifacts.load(directory / "summary.json")
            summary["built"]["primitive"]["command"][-1] = name
            summary["runtime_object_sha256"] = {v["resolved"]: v["sha256"]
                                                 for v in env["loaded_objects"].values()}
            summary["comparisons"] = run_probe.comparison_report(directory, results, lock, candidate, env)
            write(directory / "environment.json", env)
            summary["output_sha256"] = artifacts.inventory(directory)
            write(directory / "summary.json", summary)
            with self.subTest(name=name, alias=alias, resolved=resolved):
                result = self.compare(root)
                self.assertEqual(result.returncode, 1, result.stdout)
                self.assertIn("recorded shared-object alias", result.stderr)

    def test_program_header_table_loader_bound_in_full_comparator(self):
        for count in (1170, 1171, 2000):
            root = self.pair()
            directory = root / "left"
            path = directory / "bin/primitive_probe"
            data = bytearray(path.read_bytes())
            old = struct.unpack_from("<Q", data, 0x20)[0]
            headers = data[old:old + 2 * 56]
            offset = len(data)
            data.extend(headers + bytes((count - 2) * 56))
            struct.pack_into("<Q", data, 0x20, offset)
            struct.pack_into("<H", data, 0x38, count)
            path.write_bytes(data)
            summary = artifacts.load(directory / "summary.json")
            summary["built"]["primitive"]["sha256"] = artifacts.sha256(path)
            write(directory / "summary.json", summary)
            self.rehash(directory)
            with self.subTest(count=count):
                result = self.compare(root)
                self.assertEqual(result.returncode, int(count > 1170), result.stderr)
                if count > 1170:
                    self.assertIn("invalid probe ELF program-header table", result.stderr)

    def test_caller_machine_and_load_flags_fail_the_full_comparator(self):
        for machine in (False, True):
            def change(directory, _env, _rows):
                path = directory / "bin/primitive_probe"
                data = bytearray(path.read_bytes())
                if machine:
                    struct.pack_into("<H", data, 0x12, 183)
                else:
                    phoff = struct.unpack_from("<Q", data, 0x20)[0]
                    struct.pack_into("<I", data, phoff + 56 + 4, 4)
                path.write_bytes(data)
                summary = artifacts.load(directory / "summary.json")
                summary["built"]["primitive"]["sha256"] = artifacts.sha256(path)
                write(directory / "summary.json", summary)
            with self.subTest(machine=machine):
                self.assertComparatorRejects(change, "retained probe ELF cannot be read")

    def rehash(self, directory):
        summary = artifacts.load(directory / "summary.json")
        summary["output_sha256"] = artifacts.inventory(directory)
        write(directory / "summary.json", summary)

    def test_valid_run(self):
        result = self.compare(self.pair())
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_fixed_seed_input_bits_cover_all_four_angle_families(self):
        # Native Rust 1.96.1 probe output, not generated by the Python validator.
        expected = [
            "c002f0aa94d2eab5", "c0b2e9adc292f30e", "3fc951649f011f31", "bf1bf1a3267f8448",
            "bff501cfbf952a79", "c0b0a37a44cb9d37", "40157e207167deb7", "bf27d829d93ab3c0",
        ]
        self.assertEqual([f"{x:016x}" for x in artifacts.sweep_inputs(8)], expected)

    def test_matching_sweeps_cannot_replace_or_reorder_fixed_seed_inputs(self):
        for sides in (("left",), ("right",), ("left", "right")):
            for damage in ("nan", "finite", "duplicate-record", "reordered"):
                with self.subTest(sides=sides, damage=damage):
                    root = self.pair()
                    for side in sides:
                        d = root / side
                        for path in d.glob("*.sweep.bin"):
                            data = bytearray(path.read_bytes())
                            if damage == "duplicate-record":
                                data[24:] = data[:24]
                            elif damage == "reordered":
                                data[:] = data[24:] + data[:24]
                            else:
                                word = 0x7ff8000000000001 if damage == "nan" else 0x3ff0000000000000
                                struct.pack_into("<Q", data, 0, word)
                            path.write_bytes(data)
                        self.rehash(d)
                    result = self.compare(root)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("sweep input identity/order differs", result.stderr)

    def test_every_nested_comparison_count_rejects_boolean_values(self):
        root = self.pair()
        original = artifacts.load(root / "left/summary.json")["comparisons"]

        def integer_paths(value, path=()):
            if type(value) is int:
                yield path
            elif type(value) is dict:
                for key, child in value.items():
                    yield from integer_paths(child, (*path, key))
            elif type(value) is list:
                for index, child in enumerate(value):
                    yield from integer_paths(child, (*path, index))

        paths = list(integer_paths(original))
        self.assertTrue(paths)
        for path in paths:
            for sides in (("left",), ("right",), ("left", "right")):
                with self.subTest(path=path, sides=sides):
                    root = self.pair()
                    for side in sides:
                        target = root / side / "summary.json"
                        summary = artifacts.load(target)
                        record = summary["comparisons"]
                        for part in path[:-1]:
                            record = record[part]
                        record[path[-1]] = False
                        write(target, summary)
                    result = self.compare(root)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("invalid comparison field type", result.stderr)

    def test_each_side_and_both_are_independently_validated(self):
        for sides in (("left",), ("right",), ("left", "right")):
            for damage in (
                "empty",
                "truncated",
                "missing",
                "extra",
                "hash",
                "duplicate",
                "nonfinite",
                "float-count",
                "bool-count",
                "wrong-count",
                "wrong-schema",
                "wrong-identity",
                "missing-field",
                "reordered",
                "bits-number",
                "stale-summary",
                "binary",
                "duplicate-operand",
            ):
                with self.subTest(sides=sides, damage=damage):
                    root = self.pair()
                    for side in sides:
                        d = root / side
                        p = d / "primitive-contract.json"
                        value = artifacts.load(p)
                        if damage == "empty":
                            (d / "primitive-contract.sweep.bin").write_bytes(b"")
                        elif damage == "truncated":
                            (d / "primitive-contract.sweep.bin").write_bytes(bytes(23))
                        elif damage == "missing":
                            p.unlink()
                        elif damage == "extra":
                            (d / "extra.json").write_text("{}")
                        elif damage == "hash":
                            p.write_text(p.read_text() + " ")
                        elif damage == "duplicate":
                            p.write_text(
                                p.read_text().replace(
                                    '"sin":', '"sin":"0000000000000000","sin":', 1
                                )
                            )
                        elif damage == "nonfinite":
                            p.write_text(p.read_text().replace('"count": 2', '"count": NaN'))
                        elif damage == "float-count":
                            value["sweep"]["count"] = 2.0
                            write(p, value)
                        elif damage == "bool-count":
                            value["sweep"]["count"] = True
                            write(p, value)
                        elif damage == "wrong-count":
                            value["sweep"]["count"] = 3
                            write(p, value)
                        elif damage == "wrong-schema":
                            value["schema"] = True
                            write(p, value)
                        elif damage == "wrong-identity":
                            value["libm_values"][0]["x"] = "1111111111111111"
                            write(p, value)
                        elif damage == "missing-field":
                            del value["libm_values"][0]["sin"]
                            write(p, value)
                        elif damage == "reordered":
                            value["libm_values"].reverse()
                            write(p, value)
                        elif damage == "bits-number":
                            value["libm_values"][0]["sin"] = 1
                            write(p, value)
                        elif damage == "duplicate-operand":
                            value["nan_arithmetic"][1] = value["nan_arithmetic"][0]
                            write(p, value)
                        elif damage == "binary":
                            (d / "bin/rust_probe").write_bytes(b"mutated binary")
                        else:
                            value["libm_values"][0]["sin"] = "0000000000000000"
                            write(p, value)
                        # Schema tests also get fresh hashes: integrity alone must not certify malformed data.
                        if damage not in ("hash", "empty", "truncated", "missing", "extra"):
                            self.rehash(d)
                    self.assertNotEqual(self.compare(root).returncode, 0)

    def test_matrix_artifact_shape_and_identities(self):
        root = self.pair()
        for side in ("left", "right"):
            d = root / side
            row = dict(case=0, scalar="3ff0000000000000", determinant="3ff0000000000000")
            for name in ("a", "b", "inverse", "mul_matrix", "mul_scalar_right", "mul_scalar_left"):
                row[name] = ["3ff0000000000000"] * 9
            for name in ("v2", "mul_vector2"):
                row[name] = ["3ff0000000000000"] * 2
            for name in ("v3", "mul_vector3"):
                row[name] = ["3ff0000000000000"] * 3
            matrix = dict(
                schema=1,
                probe="matrix",
                compiler="synthetic",
                cases=[dict(copy.deepcopy(row), case=i) for i in range(96)],
                stream=dict(
                    default="fixture", uppercase_fixed="fixture", uppercase_scientific="fixture"
                ),
            )
            write(d / "matrix-contract.json", matrix)
            (d / "bin/matrix_probe").write_bytes(b"fixture")
            (d / "bin/matrix_probe.o").write_bytes(b"fixture object")
            summary = artifacts.load(d / "summary.json")
            summary["built"]["matrix"] = dict(
                sha256=artifacts.sha256(d / "bin/matrix_probe"),
                compile_command=["fixture"],
                link_command=["fixture"],
            )
            write(d / "summary.json", summary)
            self.rehash(d)
        result = self.compare(root)
        self.assertEqual(result.returncode, 0, result.stderr)
        value = artifacts.load(root / "right/matrix-contract.json")
        value["cases"][0]["case"] = False
        write(root / "right/matrix-contract.json", value)
        self.rehash(root / "right")
        self.assertNotEqual(self.compare(root).returncode, 0)

    def test_original_adversarial_minimal_fixture_is_rejected(self):
        root = self.pair()
        for side in ("left", "right"):
            (root / side / "summary.json").write_text(
                '{"contract_passed":true,"cpp_rust_identical":true}'
            )
        self.assertNotEqual(self.compare(root).returncode, 0)


if __name__ == "__main__":
    unittest.main()
