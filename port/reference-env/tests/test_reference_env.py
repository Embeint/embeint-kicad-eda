#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline tests for the reference environment tooling (no network, no Docker)."""

import base64
import hashlib
import io
import json
import lzma
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest import mock

ENV = Path(__file__).resolve().parents[1]
for directory in ("tools", "image", "probe"):
    sys.path.insert(0, str(ENV / directory))

import check_candidate  # noqa: E402
import compare_observations  # noqa: E402
import debian  # noqa: E402
import fetch_inputs  # noqa: E402
import refenv  # noqa: E402
import input_plan  # noqa: E402
import run_probe  # noqa: E402
import verify_image  # noqa: E402


def stanza(name, version, depends="", provides="", arch="amd64", **extra):
    fields = {"Package": name, "Version": version, "Architecture": arch, "_suite": "noble",
              "Filename": f"pool/{name}_{version}.deb", "SHA256": "0" * 64, "Size": "1"}
    if depends:
        fields["Depends"] = depends
    if provides:
        fields["Provides"] = provides
    fields.update(extra)
    return fields


class VersionTest(unittest.TestCase):
    def test_dpkg_ordering(self):
        ordered = ["1.0~rc1", "1.0", "1.0-1", "1.0-1ubuntu1", "1.0a", "1.1", "13.2.0-23ubuntu4",
                   "13.3.0-6ubuntu2~24.04", "13.3.0-6ubuntu2~24.04.1", "13.3.0-6ubuntu2", "1:0.1"]
        for left, right in zip(ordered, ordered[1:]):
            self.assertLess(debian.compare_versions(left, right), 0, (left, right))
            self.assertGreater(debian.compare_versions(right, left), 0, (left, right))
        self.assertLess(debian.compare_versions("2.39-0ubuntu8.9", "2.39-0ubuntu8.12"), 0)
        self.assertEqual(debian.compare_versions("4:13.2.0-7ubuntu1", "4:13.2.0-7ubuntu1"), 0)
        self.assertEqual(debian.compare_versions("1.00", "1.0"), 0)

    def test_relations(self):
        groups = debian.parse_relations("libc6 (>= 2.34), python3:any | python3-minimal (<< 3.13),\n foo")
        self.assertEqual(groups[0], [("libc6", None, ">=", "2.34")])
        self.assertEqual(groups[1], [("python3", "any", None, None), ("python3-minimal", None, "<<", "3.13")])
        self.assertEqual(groups[2], [("foo", None, None, None)])
        with self.assertRaises(ValueError):
            debian.parse_relations("foo [amd64]")
        with self.assertRaises(ValueError):
            debian.parse_relations("foo (~ 1)")

    def test_paragraphs(self):
        text = "Package: a\nDescription: x\n more\n\nPackage: b\n"
        self.assertEqual([p["Package"] for p in debian.parse_paragraphs(text)], ["a", "b"])
        with self.assertRaises(ValueError):
            list(debian.parse_paragraphs(" orphan\n"))


class ResolverTest(unittest.TestCase):
    def test_highest_candidate_and_closure(self):
        archive = debian.Archive([stanza("app", "1.0", "lib (>= 2)"), stanza("app", "2.0", "lib (>= 2)"),
                                  stanza("lib", "2.0"), stanza("lib", "3.0"), stanza("other", "1", arch="arm64")])
        selected, upgraded = debian.resolve(archive, {}, ["app"])
        self.assertEqual({n: s["Version"] for n, s in selected.items()}, {"app": "2.0", "lib": "3.0"})
        self.assertEqual(upgraded, set())
        self.assertNotIn("other", archive.candidates)

    def test_installed_satisfier_is_kept(self):
        archive = debian.Archive([stanza("app", "1", "lib (>= 1)"), stanza("lib", "5")])
        selected, _ = debian.resolve(archive, {"lib": stanza("lib", "1")}, ["app"])
        self.assertEqual(sorted(selected), ["app"])

    def test_exact_dependency_upgrades_base_and_cascades(self):
        archive = debian.Archive([stanza("dev", "8.12", "libc (= 8.12)"), stanza("libc", "8.12"),
                                  stanza("bin", "8.12", "libc (= 8.12)")])
        base = {"libc": stanza("libc", "8.9"), "bin": stanza("bin", "8.9", "libc (= 8.9)")}
        selected, upgraded = debian.resolve(archive, base, ["dev"])
        self.assertEqual(upgraded, {"libc", "bin"})
        self.assertEqual(selected["bin"]["Version"], "8.12")

    def test_virtual_providers(self):
        archive = debian.Archive([stanza("app", "1", "awk"), stanza("mawk", "1", provides="awk"),
                                  stanza("gawk", "1", provides="awk")])
        with self.assertRaisesRegex(ValueError, "Ambiguous virtual package awk"):
            debian.resolve(archive, {}, ["app"])
        selected, _ = debian.resolve(archive, {}, ["app"], {"awk": "gawk"})
        self.assertIn("gawk", selected)
        with self.assertRaisesRegex(ValueError, "not available"):
            debian.resolve(archive, {}, ["app"], {"awk": "busybox"})
        single = debian.Archive([stanza("app", "1", "awk"), stanza("mawk", "1", provides="awk")])
        self.assertIn("mawk", debian.resolve(single, {}, ["app"])[0])
        installed = debian.resolve(archive, {"mawk": stanza("mawk", "0", provides="awk")}, ["app"])[0]
        self.assertEqual(sorted(installed), ["app"])

    def test_versioned_virtual_needs_versioned_provides(self):
        archive = debian.Archive([stanza("app", "1", "api (>= 2)"), stanza("impl", "1", provides="api (= 3)"),
                                  stanza("old", "1", provides="api")])
        self.assertIn("impl", debian.resolve(archive, {}, ["app"])[0])

    def test_failures(self):
        with self.assertRaisesRegex(ValueError, "Unsatisfiable"):
            debian.resolve(debian.Archive([stanza("app", "1", "missing")]), {}, ["app"])
        with self.assertRaisesRegex(ValueError, "not in snapshot"):
            debian.resolve(debian.Archive([]), {}, ["app"])
        conflict = debian.Archive([stanza("app", "1", "lib", Conflicts="base (<< 2)"), stanza("lib", "1")])
        with self.assertRaisesRegex(ValueError, "Conflicts"):
            debian.resolve(conflict, {"base": stanza("base", "1")}, ["app"])

    def test_unsatisfiable_upgrade_does_not_loop_or_make_inconsistent_lock(self):
        archive = debian.Archive([stanza("app", "1", "lib (= 1)"), stanza("lib", "2")])
        base = {"app": stanza("app", "1", "lib (= 1)"), "lib": stanza("lib", "1")}
        with self.assertRaisesRegex(ValueError, "Unsatisfiable"):
            debian.resolve(archive, base, ["lib"])
        archive = debian.Archive([stanza("old", "1", "lib (= 1)"), stanza("new", "1", "lib (= 2)"),
                                  stanza("lib", "1"), stanza("lib", "2")])
        with self.assertRaisesRegex(ValueError, "Unsatisfiable"):
            debian.resolve(archive, {"lib": stanza("lib", "1")}, ["old", "new"])


def make_deb(files, links=None):
    data = io.BytesIO()
    with tarfile.open(fileobj=data, mode="w") as archive:
        for name, content in files.items():
            info = tarfile.TarInfo("." + name)
            info.size = len(content)
            archive.addfile(info, io.BytesIO(content))
        for name, target in (links or {}).items():
            info = tarfile.TarInfo("." + name)
            info.type = tarfile.SYMTYPE
            info.linkname = target
            archive.addfile(info)
    members = [("debian-binary", b"2.0\n"), ("control.tar.xz", lzma.compress(b"")),
               ("data.tar.xz", lzma.compress(data.getvalue()))]
    out = b"!<arch>\n"
    for name, payload in members:
        header = f"{name + '/':<16}{0:<12}{0:<6}{0:<6}{100644:<8}{len(payload):<10}`\n".encode()
        out += header + payload + (b"\n" if len(payload) % 2 else b"")
    return out


def make_elf(build_id, symbols):
    """ELF64 with .note.gnu.build-id, .symtab, .strtab and .shstrtab sections."""
    names = b"\0.note.gnu.build-id\0.symtab\0.strtab\0.shstrtab\0"
    strtab = b"\0" + b"".join(name.encode() + b"\0" for name, _value in symbols)
    note = struct.pack("<III", 4, len(build_id), 3) + b"GNU\0" + build_id
    entries = [struct.pack("<IBBHQQ", 0, 0, 0, 0, 0, 0)]
    offset = 1
    for name, value in symbols:
        entries.append(struct.pack("<IBBHQQ", offset, 2, 0, 1, value, 16))
        offset += len(name) + 1
    symtab = b"".join(entries)
    body = note + symtab + strtab + names
    base = 64
    sections = [struct.pack("<IIQQQQIIQQ", 0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
                struct.pack("<IIQQQQIIQQ", 1, 7, 0, 0, base, len(note), 0, 0, 4, 0),
                struct.pack("<IIQQQQIIQQ", 20, 2, 0, 0, base + len(note), len(symtab), 3, 1, 8, 24),
                struct.pack("<IIQQQQIIQQ", 28, 3, 0, 0, base + len(note) + len(symtab), len(strtab), 0, 0, 1, 0),
                struct.pack("<IIQQQQIIQQ", 36, 3, 0, 0, base + len(note) + len(symtab) + len(strtab), len(names),
                            0, 0, 1, 0)]
    shoff = base + len(body)
    header = b"\x7fELF\x02\x01\x01" + b"\0" * 9 + struct.pack("<HHIQQQIHHHHHH", 3, 62, 1, 0, 0, shoff, 0, 64,
                                                                 0, 0, 64, len(sections), 4)
    return header + body + b"".join(sections)


class FormatTest(unittest.TestCase):
    def test_inrelease(self):
        text = ("Origin: Ubuntu\nDate: Thu, 19 Nov 2026\nMD5Sum:\n aa 1 main/x\nSHA256:\n"
                f" {'a' * 64}      12 main/binary-amd64/Packages.gz\n {'b' * 64} 3 universe/y\nAcquire: yes\n")
        sums = refenv.inrelease_sha256(text)
        self.assertEqual(sums["main/binary-amd64/Packages.gz"], ("a" * 64, 12))
        self.assertEqual(len(sums), 2)
        self.assertEqual(refenv.inrelease_field(text, "Date"), "Thu, 19 Nov 2026")
        with self.assertRaises(ValueError):
            refenv.inrelease_sha256("Origin: x\n")

    def test_deb_extraction_and_links(self):
        deb = make_deb({"/usr/lib/libstdc++.so.6.0.33": b"runtime"},
                       {"/usr/lib/libstdc++.so.6": "libstdc++.so.6.0.33", "/usr/lib/up": "../lib/libstdc++.so.6"})
        archive = refenv.deb_data_tar(deb)
        self.assertEqual(refenv.tar_file_bytes(archive, "/usr/lib/libstdc++.so.6"),
                         ("/usr/lib/libstdc++.so.6.0.33", b"runtime"))
        self.assertEqual(refenv.tar_file_bytes(archive, "/usr/lib/up")[1], b"runtime")

    def test_elf_symbols(self):
        elf = refenv.Elf64(make_elf(bytes.fromhex("f772fc0c"), [("__sin_sse2", 0x2f8e0), ("__sin_fma", 0x7b2d0)]))
        self.assertEqual(elf.build_id(), "f772fc0c")
        self.assertEqual([(n, v) for n, v, _b in elf.function_symbols()],
                         [("__sin_sse2", 0x2f8e0), ("__sin_fma", 0x7b2d0)])
        with self.assertRaises(ValueError):
            refenv.Elf64(b"\x7fELF\x01\x01" + b"\0" * 60)

    def test_openpgp(self):
        body = bytes([4]) + b"\x00\x00\x00\x00\x01" + b"key material"
        packet = bytes([0x99]) + len(body).to_bytes(2, "big") + body  # old format, tag 6, 2-byte length
        armoured = ("-----BEGIN PGP PUBLIC KEY BLOCK-----\nVersion: GnuPG v1\n\n"
                    + base64.b64encode(packet).decode() + "\n=abcd\n-----END PGP PUBLIC KEY BLOCK-----\n")
        data = refenv.dearmor(armoured)
        self.assertEqual(data, packet)
        expected = hashlib.sha1(b"\x99" + len(body).to_bytes(2, "big") + body).hexdigest().upper()
        self.assertEqual(refenv.openpgp_primary_fingerprint(data), expected)
        new_format = bytes([0xC6, len(body)]) + body
        self.assertEqual(refenv.openpgp_primary_fingerprint(new_format), expected)
        with self.assertRaises(ValueError):
            refenv.dearmor("no armour")


class CandidateTest(unittest.TestCase):
    def copy(self):
        directory = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, directory)
        for name in ("candidate.toml", "inputs.lock.json", "Dockerfile"):
            shutil.copy(ENV / name, directory / name)
        shutil.copytree(ENV / "probe", directory / "probe")
        return directory

    def test_repository_files_are_consistent(self):
        self.assertEqual(check_candidate.check(), [])

    def test_mutations_are_rejected(self):
        cases = {
            "Dockerfile": ("@sha256:9a04", "@sha256:9a05"),
            "probe/rust_probe.rs": ("0x7ff8000000000123", "0x7ff8000000000124"),
            "candidate.toml": ('id = "20261120T000000Z"', 'id = "20261121T000000Z"'),
            "inputs.lock.json": ('"status": "candidate-unqualified"', '"status": "qualified"'),
        }
        for name, (old, new) in cases.items():
            directory = self.copy()
            path = directory / name
            text = path.read_text(encoding="utf-8")
            self.assertIn(old, text, name)
            path.write_text(text.replace(old, new, 1), encoding="utf-8")
            self.assertNotEqual(check_candidate.check(directory), [], name)
        directory = self.copy()
        path = directory / "Dockerfile"
        path.write_text(path.read_text(encoding="utf-8").replace("-AVX512F", ""), encoding="utf-8")
        self.assertTrue(any("GLIBC_TUNABLES" in f for f in check_candidate.check(directory)))

    def test_lock_contract(self):
        lock = refenv.load_lock()
        final = {p["name"]: p["version"] for p in lock["packages"]["final"]}
        self.assertTrue(final["gcc-13"].startswith("13.3.0-"))
        self.assertTrue(final["libc6"].startswith("2.39-"))
        self.assertTrue(final["libstdc++6"].startswith("14."))
        runtime = [f for f in lock["toolchain_files"] if f["path"].endswith("libstdc++.so.6")]
        self.assertEqual(runtime[0]["resolved_path"], "/usr/lib/x86_64-linux-gnu/libstdc++.so.6.0.33")


class FetchTest(unittest.TestCase):
    def test_context_from_file_urls(self):
        directory = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, directory)
        blobs = {}

        def blob(name, content):
            path = directory / "src" / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content)
            blobs[name] = hashlib.sha256(content).hexdigest()
            return path.as_uri(), blobs[name]

        key = bytes([0xC6, 6, 4, 0, 0, 0, 0, 1])
        armour = ("-----BEGIN PGP PUBLIC KEY BLOCK-----\n\n" + base64.b64encode(key).decode()
                  + "\n-----END PGP PUBLIC KEY BLOCK-----\n").encode()
        inrelease_url, inrelease_sha = blob("InRelease", b"signed")
        index_url, index_sha = blob("Packages.gz", b"index")
        deb_url, deb_sha = blob("pool/a_1_amd64.deb", b"deb")
        tar_url, tar_sha = blob("rustc-1.99.1-x86_64-unknown-linux-gnu.tar.xz", b"rust")
        manifest_url, manifest_sha = blob("channel.toml", b"manifest")
        signature_url, signature_sha = blob("channel.toml.asc", b"sig")
        key_url, key_sha = blob("rust-key.gpg.ascii", armour)
        snapshot_root = (directory / "src").as_uri()
        lock = {
            "snapshot": {"url": snapshot_root.rsplit("/", 1)[0], "id": "src", "architecture": "amd64",
                         "keyring": "/k.gpg", "keyring_sha256": "f" * 64,
                         "suites": {"noble": {"inrelease_sha256": inrelease_sha,
                                              "indices": {"main": {"path": "../../Packages.gz",
                                                                   "sha256": index_sha}}}}},
            "packages": {"install": [{"filename": "pool/a_1_amd64.deb", "url": deb_url, "sha256": deb_sha,
                                      "name": "a", "version": "1", "architecture": "amd64"}]},
            "rust": {"channel_manifest_url": manifest_url, "channel_manifest_sha256": manifest_sha,
                     "channel_signature_url": signature_url, "channel_signature_sha256": signature_sha,
                     "signing_key_url": key_url, "signing_key_sha256": key_sha,
                     "signing_key_fingerprint": refenv.openpgp_primary_fingerprint(key),
                     "signing_keyring_sha256": hashlib.sha256(key).hexdigest(),
                     "components": [{"name": "rustc", "url": tar_url, "sha256": tar_sha}]},
        }
        # InRelease and index URLs are derived from the snapshot root and suite.
        (directory / "src/dists/noble").mkdir(parents=True)
        shutil.copy(directory / "src/InRelease", directory / "src/dists/noble/InRelease")
        dest = directory / "context"
        lock["verification_plan_sha256"] = input_plan.digest(input_plan.plan(lock)[1])
        self.assertEqual(fetch_inputs.fetch_inputs(lock, directory / "cache", dest), (1, 1))
        self.assertEqual((dest / "debs.list").read_text(), f"pool/a_1_amd64.deb {deb_sha} a 1 amd64\n")
        self.assertEqual((dest / "debs/a_1_amd64.deb").read_bytes(), b"deb")
        self.assertEqual((dest / "rust/rust-key.gpg").read_bytes(), key)
        self.assertIn(f"{tar_sha}  rustc-1.99.1-x86_64-unknown-linux-gnu.tar.xz", (dest / "rust.sha256").read_text())
        lock["packages"]["install"][0]["sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
            fetch_inputs.fetch_inputs(lock, directory / "cache", dest)


class ProbeAnalysisTest(unittest.TestCase):
    def test_excludes(self):
        rules = [("path-exclude", "/usr/share/doc/*"), ("path-include", "/usr/share/doc/*/copyright")]
        self.assertTrue(verify_image.excluded("/usr/share/doc/gcc/README", rules))
        self.assertFalse(verify_image.excluded("/usr/share/doc/gcc/copyright", rules))
        self.assertFalse(verify_image.excluded("/usr/bin/gcc", rules))

    def test_dispatch_violations(self):
        lock = {"libm": {"path": "/libm", "multiarch_symbols": {
            "0x10": ["__sin_sse2"], "0x20": ["__sin_fma"], "0x30": ["__ieee754_atan2_sse2"]}}}
        candidate = {"dispatch": {"forbidden_variant_suffixes": ["_fma", "_avx"],
                                    "checked_functions": ["sin", "atan2"]}}
        observation = {"libm_dispatch": [{"function": "sin", "object": "/libm", "offset": "0x10"},
                                         {"function": "atan2", "object": "/libm", "offset": "0x30"}]}
        report, violations = run_probe.dispatch_report(observation, lock, candidate)
        self.assertEqual(violations, [])
        for offset in ("0x20", "0x99", "0x30"):
            observation["libm_dispatch"][0]["offset"] = offset
            self.assertTrue(run_probe.dispatch_report(observation, lock, candidate)[1])
        observation["libm_dispatch"].pop()
        self.assertTrue(run_probe.dispatch_report(observation, lock, candidate)[1])

    def test_runtime_package_check_needs_no_root_only_reads(self):
        lock = {"packages": {"final": [{"name": "a", "architecture": "amd64", "version": "1"}]}}
        with mock.patch.object(verify_image, "run", return_value="a\tamd64\t1\tinstall ok installed\n"), \
                mock.patch.object(verify_image, "verify_package_files") as verify:
            failures = []
            verify_image.check_packages(lock, failures)
            verify.assert_not_called()
            verify_image.check_packages(lock, failures, verify_files=True)
            verify.assert_called_once_with(failures)
            self.assertEqual(failures, [])

    def test_sweep_and_field_differences(self):
        directory = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, directory)
        a = struct.pack("<6Q", 1, 2, 3, 4, 5, 6)
        b = struct.pack("<6Q", 1, 2, 3, 4, 5, 7)
        (directory / "a").write_bytes(a)
        (directory / "b").write_bytes(b)
        result = run_probe.sweep_differences(directory / "a", directory / "b")
        self.assertEqual((result["status"], result["sin_differences"], result["cos_differences"]),
                         ("different", 0, 1))
        self.assertEqual(run_probe.sweep_differences(directory / "a", directory / "a")["status"], "identical")
        (directory / "short").write_bytes(a[:-1])
        self.assertEqual(run_probe.sweep_differences(directory / "short", directory / "short")["status"],
                         "different")
        left = [{"x": "1", "sin": "a"}, {"x": "2", "sin": "b"}]
        self.assertEqual(run_probe.field_differences(left, left, [("sin", "sin")])["status"], "identical")
        changed = run_probe.field_differences(left, [{"x": "1", "sin": "a"}, {"x": "2", "sin": "c"}],
                                              [("sin", "sin")])
        self.assertEqual((changed["count"], changed["differences"][0]["key"]), (1, ["2"]))
        self.assertEqual(run_probe.field_differences(left, left[:1], [("sin", "sin")])["status"], "different")
        self.assertEqual(run_probe.field_differences(left, list(reversed(left)), [("sin", "sin")])["status"], "different")
        self.assertEqual(run_probe.field_differences([left[0], left[0]], left, [("sin", "sin")])["status"],
                         "different")

    def test_shim_flags(self):
        directory = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, directory)
        database = [
            {"directory": "/b", "file": "/o/reference/x.cpp", "command": "g++ -O3 -c /o/reference/x.cpp"},
            {"directory": "/b", "file": "/o/shims/md5.cpp",
             "command": "/usr/bin/g++ -I/b -O3 -DNDEBUG -fno-fast-math -ffp-contract=off -MD -MT "
                        "CMakeFiles/kicad-oracle.dir/shims/md5.cpp.o -MF d -o CMakeFiles/kicad-oracle.dir/"
                        "shims/md5.cpp.o -c /o/shims/md5.cpp"},
        ]
        (directory / "compile_commands.json").write_text(json.dumps(database))
        compiler, flags, cwd, source = run_probe.shim_flags(directory)
        self.assertEqual((compiler, cwd, source), ("/usr/bin/g++", "/b", "/o/shims/md5.cpp"))
        self.assertEqual(flags, ["-I/b", "-O3", "-DNDEBUG", "-fno-fast-math", "-ffp-contract=off"])

    def test_json_differences_are_exact(self):
        self.assertEqual(compare_observations.json_differences({"a": [1, 2]}, {"a": [1, 3]}),
                         [("$.a[1]", 2, 3)])


class ShellTest(unittest.TestCase):
    def test_syntax(self):
        for shell, script in (("bash", "qualify.sh"), ("sh", "image/verify_chain.sh")):
            if shutil.which(shell) is None:
                self.skipTest(shell + " unavailable")
            subprocess.run([shell, "-n", str(ENV / script)], check=True)
            self.assertNotIn(b"\r\n", (ENV / script).read_bytes())


if __name__ == "__main__":
    unittest.main()
