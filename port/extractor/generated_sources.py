#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Freeze generated build inputs under explicit logical, non-Git source paths."""
import argparse
import json
from pathlib import Path, PurePosixPath
import subprocess
import shutil
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from extractor.model import PIN, digest, read_json, write_json

SUFFIXES = {'.c', '.cc', '.cpp', '.cxx', '.h', '.hh', '.hpp', '.hxx', '.m', '.mm', '.inc', '.ipp', '.tcc'}


def build_state(build):
    """Bind all private build files, including macro headers with unusual suffixes."""
    return [{'path': p.relative_to(build).as_posix(), 'sha256': digest(p.read_bytes())}
            for p in sorted(build.rglob('*')) if p.is_file()]


def source_path(entry, source_root, generated_root):
    if entry.get('origin') != 'build-generated':
        if entry['path'].startswith('@build/'):
            raise ValueError('Generated logical path lacks its storage origin')
        return source_root / entry['path']
    relative = entry['path'].removeprefix('@build/')
    if (not entry['path'].startswith('@build/') or PurePosixPath(relative).is_absolute()
            or '..' in PurePosixPath(relative).parts or '\\' in relative or ':' in relative
            or entry.get('storage_path') != 'sources/' + relative):
        raise ValueError('Unsafe or inconsistent generated source mapping')
    return generated_root / entry['storage_path']


def verify(manifest, build_root, generated_root):
    if manifest.get('build_files') != build_state(build_root):
        raise ValueError('Frozen build file inventory/bytes changed; refreeze generated inputs before using the cache')
    entries = manifest['files']
    if len({e['path'] for e in entries}) != len(entries):
        raise ValueError('Duplicate generated source path')
    for entry in entries:
        stored = source_path(entry, Path('.'), generated_root).read_bytes()
        original = (build_root / entry['path'].removeprefix('@build/')).read_bytes()
        if (stored != original or digest(stored) != entry['bytes_sha256']
                or digest(stored.replace(b'\r\n', b'\n')) != entry['sha256']
                or len(stored.splitlines()) != entry['lines']):
            raise ValueError('Frozen generated input changed: ' + entry['path'])
    return entries


def freeze(build, output, provenance, extra_inputs=()):
    build, output = build.resolve(), output.resolve()
    if output.resolve().is_relative_to(build.resolve()):
        raise ValueError('Keep frozen output outside the private build tree')
    entries = []
    paths = {p for p in build.rglob('*') if p.is_file() and p.suffix.lower() in SUFFIXES}
    for relative in extra_inputs:
        path = (build / relative).resolve()
        if not path.is_relative_to(build.resolve()) or not path.is_file():
            raise ValueError('Extra generated input must be an existing private build file')
        paths.add(path)
    for path in sorted(paths):
        relative = path.relative_to(build).as_posix()
        content = path.read_bytes()
        storage = output / 'sources' / relative
        storage.parent.mkdir(parents=True, exist_ok=True)
        storage.write_bytes(content)
        # CMake's compiler identification probes are generator tooling, not
        # KiCad application lines. Keep their bytes in the frozen fingerprint.
        tooling = relative.startswith(('CMakeFiles/', 'thirdparty/'))
        entries.append({'path': '@build/' + relative, 'origin': 'build-generated',
                        'storage_path': 'sources/' + relative, 'lines': len(content.splitlines()),
                        'sha256': digest(content.replace(b'\r\n', b'\n')),
                        'bytes_sha256': digest(content),
                        'category': 'thirdparty' if tooling else 'generated'})
    manifest = {'schema_version': 1, 'upstream_pin': PIN,
                'build_root': str(build), 'provenance': provenance, 'files': entries,
                'build_files': build_state(build)}
    write_json(output / 'generated-sources.json', manifest)
    return manifest


def capture_recipes(build, source, entries):
    """Retain generated links and flatten tracked inputs through all Ninja stages."""
    build, source = build.resolve(), source.resolve()
    recipes, dependencies = {}, {}

    def query(logical):
        if logical in dependencies:
            return dependencies[logical]
        relative = logical.removeprefix('@build/')
        result = subprocess.run(['ninja', '-C', str(build), '-t', 'query', relative],
                                capture_output=True, text=True, encoding='utf-8')
        commands = subprocess.run(['ninja', '-C', str(build), '-t', 'commands', relative],
                                  capture_output=True, text=True, encoding='utf-8')
        recipes[logical] = {'query_exit': result.returncode, 'query': result.stdout,
            'query_stderr': result.stderr, 'commands_exit': commands.returncode,
            'commands': commands.stdout, 'commands_stderr': commands.stderr,
            'stage': 'ninja-target' if result.returncode == 0 else 'cmake-configure output or compiler probe'}
        inputs, in_inputs = set(), False
        for line in result.stdout.splitlines():
            if line.startswith('  input:'):
                in_inputs = True
            elif line.startswith('  ') and not line.startswith('    '):
                in_inputs = False
            elif in_inputs and line.startswith('    '):
                candidate = Path(line.strip().lstrip('| '))
                candidate = (candidate if candidate.is_absolute() else build / candidate).resolve()
                if candidate.is_relative_to(source) and candidate.is_file():
                    inputs.add(candidate.relative_to(source).as_posix())
                elif candidate.is_relative_to(build):
                    inputs.add('@build/' + candidate.relative_to(build).as_posix())
        dependencies[logical] = inputs
        return inputs

    for entry in entries:
        pending, seen, inputs = [entry['path']], set(), set()
        while pending:
            logical = pending.pop()
            if logical in seen:
                continue
            seen.add(logical)
            for dependency in query(logical):
                inputs.add(dependency)
                if dependency.startswith('@build/'):
                    pending.append(dependency)
        # Intermediates need not be C/C++ files in the frozen source inventory.
        # Keep their source leaves here so the published drift graph still closes.
        entry['generator_inputs'] = sorted(inputs - {entry['path']})
    return recipes


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--build-root', type=Path, required=True)
    p.add_argument('--reconciliation', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--extra-input', action='append', default=[], help='existing build-relative include with a nonstandard suffix')
    a = p.parse_args()
    build = a.build_root.resolve()
    if build == (Path.home() / 'kicad-live/build').resolve():
        p.error('Use only the private pinned build tree')
    if not (Path.home() / 'kicad-port.build-lock').is_dir():
        p.error('Acquire the shared build lock first')
    provenance = {}
    provenance['tools'] = {}
    for tool in ('cmake', 'ninja', 'protoc', 'c++'):
        binary = shutil.which(tool)
        if binary:
            version = subprocess.run([binary, '--version'], capture_output=True, text=True, encoding='utf-8')
            provenance['tools'][tool] = {'path': binary, 'sha256': digest(Path(binary).read_bytes()),
                'version_exit': version.returncode, 'version': version.stdout, 'stderr': version.stderr}
    for name in ('reconciliation.json', 'generated-inputs.json', 'environment.json'):
        path = a.reconciliation / name
        if path.exists():
            provenance[name] = {'sha256': digest(path.read_bytes()), 'record': read_json(path)}
    for name in ('CMakeCache.txt', 'build.ninja', 'CMakeFiles/rules.ninja'):
        path = build / name
        provenance[name] = {'sha256': digest(path.read_bytes())}
    manifest = freeze(build, a.output, provenance, a.extra_input)
    source = Path(read_json(a.reconciliation / 'reconciliation.json')['source_root'])
    recipes = capture_recipes(build, source, manifest['files'])
    write_json(a.output / 'generated-recipes.json', recipes)
    manifest['recipes_sha256'] = digest((a.output / 'generated-recipes.json').read_bytes())
    write_json(a.output / 'generated-sources.json', manifest)
    print(json.dumps({'frozen_files': len(manifest['files']), 'recipe_records': len(recipes)}))
    return 0


if __name__ == '__main__':
    sys.exit(main())
