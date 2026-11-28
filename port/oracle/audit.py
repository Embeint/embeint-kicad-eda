#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Verify source bytes and all generated compile/link flags before any reference build."""

import argparse
import hashlib
import json
from pathlib import Path
import shlex
import subprocess
import tempfile

from pin import KICAD_PIN, SOURCE_PATHS


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check_flags(tokens):
    forbidden = {
        "-Ofast", "-ffast-math", "-funsafe-math-optimizations", "-fassociative-math",
        "-freciprocal-math", "-ffinite-math-only", "-fno-signed-zeros",
        "-fno-trapping-math", "-fno-honor-nans", "-fno-honor-infinities",
        "-fapprox-func", "-fno-math-errno", "-fno-rounding-math", "-fcx-limited-range",
        "-fexcess-precision=fast", "-fno-protect-parens", "-fallow-store-data-races",
        "-ffp-model=fast", "-ffp-model=aggressive", "-fdenormal-fp-math=positive-zero",
        "-fdenormal-fp-math=preserve-sign", "-fno-precise-fpmad",
    }
    for token in tokens:
        if (token in forbidden or token.startswith(("@", "-flto", "-mllvm", "-Xclang"))
                or token.startswith("-ffp-contract=") and token != "-ffp-contract=off"
                or token.startswith("-ffp-model=") and token != "-ffp-model=strict"
                or token.startswith("-fdenormal-fp-math=")
                or token.startswith("-fdenormal-fp-math-f32=")):
            raise ValueError("Conflicting or unauditable compiler/link flag: " + token)


def verify_source(source):
    manifest_path = source / "source-manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest["kicad_pin"] != KICAD_PIN:
        raise ValueError("Wrong KiCad reference pin")
    actual = {}
    for relative in SOURCE_PATHS:
        path = source / relative
        paths = sorted(path.rglob("*")) if path.is_dir() else [path]
        for entry in paths:
            if entry.is_symlink():
                raise ValueError("Symlink in pinned reference: " + str(entry))
            if entry.is_file():
                actual[entry.relative_to(source).as_posix()] = sha(entry)
    if actual != manifest["files"]:
        raise ValueError("Pinned source changed or stale files present; re-export and use a clean tree")
    return manifest, sha(manifest_path)


def write_changed(path, text):
    if not path.exists() or path.read_text(encoding="utf-8") != text:
        path.write_text(text, encoding="utf-8")


def check_resistor_reference(commands, link_commands, source, overlay):
    """Require real library compilation (including the accessor TU) and linkage."""
    required = {source / "pcb_calculator/eseries.cpp", overlay / "reference/resistors_access.cpp"}
    compiled = set()
    for entry in commands:
        path = Path(entry["file"])
        if any(
                "CMakeFiles/resistor-reference.dir/" in token for token in entry["arguments"]):
            compiled.add(path)
    if compiled != required:
        raise ValueError("Missing actual resistor-reference compilation: " + repr(required - compiled))
    # The accessor must compile the unmodified original source, not an overlay
    # replacement. Include search order and both files are retained in provenance.
    wrapper = overlay / "reference/resistors_access.cpp"
    if "#include <resistor_substitution_utils.cpp>" not in wrapper.read_text(encoding="utf-8"):
        raise ValueError("Accessor does not compile the actual pinned resistor source")
    if not any("-o kicad-oracle" in command and "libresistor-reference.a" in command
               and "libfmt.a" in command for command in link_commands):
        raise ValueError("Oracle does not link the actual resistor-reference and fmt archives")


def verify_reader_generated(build, source):
    """Regenerate with the pinned upstream generator, comparing every output byte."""
    arguments = ["cmake", "-Denum=PCBCALC_DATA_T",
                 "-DinputFile=" + str(source / "pcb_calculator/pcb_calculator_datafile.keywords"),
                 "-DoutHeaderFile=pcb_calculator_datafile_lexer.h",
                 "-DoutCppFile=pcb_calculator_datafile_keywords.cpp",
                 "-P", str(source / "cmake/BuildSteps/TokenList2DsnLexer.cmake")]
    files = {}
    with tempfile.TemporaryDirectory() as directory:
        subprocess.run(arguments, cwd=directory, check=True, capture_output=True)
        for name in ("pcb_calculator_datafile_lexer.h", "pcb_calculator_datafile_keywords.cpp"):
            actual = build / "reader-generated" / name
            if actual.read_bytes() != (Path(directory) / name).read_bytes():
                raise ValueError("Generated reader source differs from pinned generator: " + name)
            files[str(actual)] = sha(actual)
    return {"arguments": arguments, "files_sha256": files}


def check_reader_reference(commands, link_commands, source, build):
    required = {source / path for path in (
        "pcb_calculator/datafile_read_write.cpp", "common/dsnlexer.cpp",
        "common/richio.cpp", "common/exceptions.cpp")}
    required.add(build / "reader-generated/pcb_calculator_datafile_keywords.cpp")
    compiled = {Path(entry["file"]) for entry in commands if any(
        "CMakeFiles/calculator-reader-reference.dir/" in token for token in entry["arguments"])}
    if compiled != required:
        raise ValueError("Missing actual calculator-reader-reference compilation: " + repr(required - compiled))
    if not any("-o kicad-oracle" in command and "libcalculator-reader-reference.a" in command
               for command in link_commands):
        raise ValueError("Oracle does not link the actual calculator-reader-reference archive")


def check_kiplatform_unix_reference(commands, link_commands, source):
    """Require the unchanged pinned Unix drivers.cpp and sysinfo.cpp, compiled and linked."""
    required = {source / "libs/kiplatform/os/unix/drivers.cpp",
                source / "libs/kiplatform/os/unix/sysinfo.cpp"}
    compiled = {Path(entry["file"]) for entry in commands if any(
        "CMakeFiles/kiplatform-unix-reference.dir/" in token for token in entry["arguments"])}
    if compiled != required:
        raise ValueError("Missing actual kiplatform-unix-reference compilation: " + repr(required ^ compiled))
    if not any("-o kicad-oracle" in command and "libkiplatform-unix-reference.a" in command
               for command in link_commands):
        raise ValueError("Oracle does not link the actual kiplatform-unix-reference archive")


def check_sexpr_reference(commands, link_commands, source):
    """Require the unchanged pinned libs/sexpr/sexpr.cpp, compiled and linked."""
    required = {source / "libs/sexpr/sexpr.cpp"}
    compiled = {Path(entry["file"]) for entry in commands if any(
        "CMakeFiles/sexpr-reference.dir/" in token for token in entry["arguments"])}
    if compiled != required:
        raise ValueError("Missing actual sexpr-reference compilation: " + repr(required ^ compiled))
    if not any("-o kicad-oracle" in command and "libsexpr-reference.a" in command
               for command in link_commands):
        raise ValueError("Oracle does not link the actual sexpr-reference archive")


FONT_REFERENCE_SOURCES = (
    "common/font/font.cpp", "common/font/outline_font.cpp", "common/font/glyph.cpp",
    "common/font/outline_decomposer.cpp", "common/font/text_attributes.cpp",
    "common/markup_parser.cpp", "common/string_utils.cpp", "common/gal/color4d.cpp")


def check_font_reference(commands, link_commands, source):
    """Require the unchanged pinned font sources of the include/font adapter, compiled and linked."""
    required = {source / path for path in FONT_REFERENCE_SOURCES}
    compiled = {Path(entry["file"]) for entry in commands if any(
        "CMakeFiles/font-reference.dir/" in token for token in entry["arguments"])}
    if compiled != required:
        raise ValueError("Missing actual font-reference compilation: " + repr(required ^ compiled))
    if not any("-o kicad-oracle" in command and "libfont-reference.a" in command
               for command in link_commands):
        raise ValueError("Oracle does not link the actual font-reference archive")


KIPLATFORM_UNIX_UI_SOURCES = (
    "libs/kiplatform/os/unix/app.cpp", "libs/kiplatform/os/unix/environment.cpp",
    "libs/kiplatform/os/unix/io.cpp", "libs/kiplatform/os/unix/printing.cpp",
    "libs/kiplatform/os/unix/secrets.cpp", "libs/kiplatform/os/common/io.cpp")


def check_kiplatform_unix_ui_reference(commands, link_commands, source):
    """Require the unchanged pinned Linux kiplatform bodies of the unix ui adapter, compiled and
    linked with their external calls wrapped."""
    required = {source / path for path in KIPLATFORM_UNIX_UI_SOURCES}
    compiled = {Path(entry["file"]) for entry in commands if any(
        "CMakeFiles/kiplatform-unix-ui-reference.dir/" in token for token in entry["arguments"])}
    if compiled != required:
        raise ValueError("Missing actual kiplatform-unix-ui-reference compilation: "
                         + repr(required ^ compiled))
    if not any("-o kicad-oracle" in command and "libkiplatform-unix-ui-reference.a" in command
               and "--wrap=g_file_trash" in command for command in link_commands):
        raise ValueError("Oracle does not link the wrapped kiplatform-unix-ui-reference archive")


def audit(build, source, overlay):
    manifest, manifest_sha = verify_source(source)
    database = json.loads((build / "compile_commands.json").read_text(encoding="utf-8"))
    if not database:
        raise ValueError("Empty compile database")
    generated = None
    commands = []
    compilers = set()
    targets = set()
    for entry in database:
        tokens = entry.get("arguments") or shlex.split(entry["command"])
        check_flags(tokens)
        for required in ("-fno-fast-math", "-ffp-contract=off"):
            if required not in tokens:
                raise ValueError("Missing " + required + " for " + entry["file"])
        path = Path(entry["file"]).resolve()
        if path.is_relative_to(source):
            if path.relative_to(source).as_posix() not in manifest["files"]:
                raise ValueError("Unpinned reference compilation: " + str(path))
        elif not path.is_relative_to(overlay):
            if path != build / "reader-generated/pcb_calculator_datafile_keywords.cpp":
                raise ValueError("Unexpected compilation outside pinned source/overlay: " + str(path))
            generated = verify_reader_generated(build, source)
        for target in ("core", "clipper2", "kimath", "fmt", "iec60664-reference", "resistor-reference",
                       "calculator-reader-reference", "kiplatform-unix-reference", "sexpr-reference",
                       "font-reference", "kiplatform-unix-ui-reference", "kicad-oracle"):
            if any("CMakeFiles/" + target + ".dir/" in token for token in tokens):
                if (target == "iec60664-reference" and path != source /
                        "pcb_calculator/calculator_panels/iec60664.cpp"):
                    raise ValueError("IEC60664 reference target must compile the actual pinned source")
                targets.add(target)
        compilers.add(tokens[0])
        commands.append({"file": str(path), "arguments": tokens})
    if targets != {"core", "clipper2", "kimath", "fmt", "iec60664-reference", "resistor-reference",
                   "calculator-reader-reference", "kiplatform-unix-reference", "sexpr-reference",
                   "font-reference", "kiplatform-unix-ui-reference", "kicad-oracle"}:
        raise ValueError("Unexpected reference target closure: " + repr(targets))
    link_commands = subprocess.check_output(
        ["ninja", "-C", str(build), "-t", "commands", "kicad-oracle"], text=True).splitlines()
    for command in link_commands:
        check_flags(shlex.split(command))
    if not any("-o kicad-oracle" in command and "libkimath.a" in command
               for command in link_commands):
        raise ValueError("Oracle does not link the actual kimath archive")
    if not any("-o kicad-oracle" in command and "libiec60664-reference.a" in command
               for command in link_commands):
        raise ValueError("Oracle does not link the actual IEC60664 archive")
    check_resistor_reference(commands, link_commands, source, overlay)
    check_reader_reference(commands, link_commands, source, build)
    check_kiplatform_unix_reference(commands, link_commands, source)
    check_sexpr_reference(commands, link_commands, source)
    check_font_reference(commands, link_commands, source)
    check_kiplatform_unix_ui_reference(commands, link_commands, source)
    overlay_files = {p.relative_to(overlay).as_posix(): sha(p) for p in sorted(overlay.rglob("*"))
                     if p.is_file() and "__pycache__" not in p.parts and p.suffix != ".pyc"}
    result = {
        "schema": 1, "kicad_pin": KICAD_PIN, "source_manifest_sha256": manifest_sha,
        "reference_source": str(source), "reference_file_count": len(manifest["files"]),
        "overlay_revision": manifest["overlay_revision"],
        "overlay_dirty_at_export": manifest["overlay_dirty_at_export"],
        "overlay_files_sha256": overlay_files,
        "compilers": {c: subprocess.check_output([c, "--version"], text=True).strip()
                      for c in sorted(compilers)},
        "compiler_targets": {c: subprocess.check_output([c, "-dumpmachine"], text=True).strip()
                             for c in sorted(compilers)},
        "wx_version": subprocess.check_output(["wx-config", "--version"], text=True).strip(),
        "system_libc_version": subprocess.check_output(["getconf", "GNU_LIBC_VERSION"], text=True).strip(),
        "cmake_version": subprocess.check_output(["cmake", "--version"], text=True).strip(),
        "compile_commands": commands, "build_commands": link_commands,
        "reader_generated": generated,
        "config_h_sha256": sha(build / "config.h"),
        "floating_point": {"contract": "off", "fast_math": False},
        "system_libraries": "wxWidgets, C/C++ runtimes and libm are distribution binaries; not rebuilt",
    }
    encoded = json.dumps(result, sort_keys=True, separators=(",", ":"))
    write_changed(build / "oracle-provenance.json", encoded + "\n")
    write_changed(build / "oracle_provenance.h",
                  '// Generated by the oracle build audit.\n#pragma once\n'
                  'inline constexpr const char* ORACLE_PROVENANCE = R"ORACLE('
                  + encoded + ')ORACLE";\n')
    print(f"Oracle audit: {len(database)} compilations, pin {KICAD_PIN}, strict FP flags verified")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build", type=Path, required=True)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--overlay", type=Path, required=True)
    args = parser.parse_args()
    audit(args.build.resolve(), args.source.resolve(), args.overlay.resolve())


if __name__ == "__main__":
    main()
