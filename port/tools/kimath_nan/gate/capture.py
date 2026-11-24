# SPDX-License-Identifier: GPL-3.0-or-later
"""Capture actual Ninja-linked oracle inputs for review; this never grants eligibility."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import tempfile


HOME = 'kimath_math.cpp.o'
SOURCE_FILES = {
    'compiler-input-matrix3x3.h': 'oracle-src/reference/libs/kimath/include/math/matrix3x3.h',
    'compiler-input-vector2d.h': 'oracle-src/reference/libs/kimath/include/math/vector2d.h',
    'compiler-input-vector3.h': 'oracle-src/reference/libs/kimath/include/math/vector3.h',
    'compiler-input-kimath_math.cpp': 'oracle-src/overlay/shims/kimath_math.cpp',
    'compile_commands.json': 'oracle-build/compile_commands.json',
}


def source_file(root, path):
    path = Path(path)
    if not path.is_absolute():
        path = root / path
    relative = path.relative_to(root)
    if '..' in relative.parts or any(p.is_symlink() for p in (path, *path.parents) if p != root.parent):
        raise ValueError(f'linked/escaping capture input: {path}')
    if not path.is_file():
        raise ValueError(f'missing capture input: {path}')
    return path


def link_objects(root, command_text):
    build = root / 'oracle-build'
    candidates = []
    for line in command_text.splitlines():
        args = shlex.split(line)
        if '-o' in args:
            i = args.index('-o')
            if i + 1 < len(args) and (build / args[i + 1]).resolve() == build / 'kicad-oracle':
                candidates.append(args)
    if len(candidates) != 1:
        raise ValueError('expected one actual Ninja link command for kicad-oracle')
    args = candidates[0]
    if any(a.startswith('@') for a in args):
        raise ValueError('response-file link commands need separate reviewed capture support')
    objects = [source_file(root, build / a) for a in args if a.endswith('.o')]
    names = [p.name for p in objects]
    if names.count(HOME) != 1 or len(set(names)) != len(names):
        raise ValueError('home object missing/duplicated, or ambiguous provider basenames')
    return objects


def capture(root, output, command_text, identity):
    root, output = Path(root).resolve(strict=True), Path(output)
    if output.exists():
        raise ValueError('capture output already exists; preserve the earlier run')
    objects = link_objects(root, command_text)
    # Validate mandatory inputs before creating the temporary capture.
    files = {name: source_file(root, rel) for name, rel in SOURCE_FILES.items()}
    files['kicad-oracle'] = source_file(root, 'oracle-build/kicad-oracle')
    identity = Path(identity)
    if identity.is_symlink() or not identity.is_file():
        raise ValueError('expected the run identity as a regular file')
    with tempfile.TemporaryDirectory(dir=output.parent, prefix='.object-capture-') as directory:
        temp = Path(directory) / 'inputs'
        (temp / 'providers').mkdir(parents=True)
        (temp / 'compiler-inputs').mkdir()
        for name, path in files.items():
            dest = temp / 'kicad-oracle' if name == 'kicad-oracle' else temp / 'compiler-inputs' / name
            shutil.copyfile(path, dest)
        for path in objects:
            shutil.copyfile(path, temp / 'providers' / path.name)
        (temp / 'providers/link-order.txt').write_text(''.join(p.name + '\n' for p in objects))
        (temp / 'ninja-commands.txt').write_text(command_text)
        shutil.copyfile(identity, temp / 'image-identity.json')
        tree = temp / 'tree'
        tree.mkdir()
        for name in ('oracle-src', 'oracle-build/reader-generated'):
            base = root / name
            if not base.is_dir() or base.is_symlink():
                raise ValueError(f'missing/linked include tree: {name}')
            (tree / name).mkdir(parents=True, exist_ok=True)
            for path in sorted(base.rglob('*')):
                if path.is_symlink():
                    raise ValueError(f'linked include-tree entry: {path}')
                dest = tree / path.relative_to(root)
                if path.is_dir():
                    dest.mkdir(parents=True, exist_ok=True)
                elif path.is_file():
                    source_file(root, path)
                    dest.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(path, dest)
                else:
                    raise ValueError(f'unsupported include-tree entry: {path}')
        for name in ('config.h', 'oracle_provenance.h'):
            path = source_file(root, 'oracle-build/' + name)
            shutil.copyfile(path, tree / 'oracle-build' / name)
        manifest = {str(p.relative_to(temp)): hashlib.sha256(p.read_bytes()).hexdigest()
                    for p in sorted(temp.rglob('*')) if p.is_file()}
        (temp / 'capture.json').write_text(json.dumps({
            'purpose': 'actual build-input capture, NOT eligibility or review',
            'provider_order': [p.name for p in objects], 'sha256': manifest,
        }, indent=2) + '\n')
        os.replace(temp, output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', required=True)
    parser.add_argument('--out', required=True)
    parser.add_argument('--ninja-commands', required=True)
    parser.add_argument('--identity', required=True)
    args = parser.parse_args()
    capture(args.root, args.out, Path(args.ninja_commands).read_text(), args.identity)


if __name__ == '__main__':
    main()
