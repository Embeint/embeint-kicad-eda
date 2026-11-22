#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Recapture generator dependencies and apply them to an existing parse, post-parse.

`capture` queries the pinned private Ninja graph again with the current recipe
capture code. It refuses to run unless every private build file and frozen
generated input is byte-identical to the manifest the parse used, and it never
writes into the build tree. `apply` is a pure transformation: it replaces only
generator dependency metadata and the fingerprints derived from it, records the
original parse identities separately, and verifies that the AST, definitions,
calls, coverage and source fields are unchanged. It needs neither clang nor Ninja.
"""
import argparse
import copy
import gzip
import json
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from extractor.model import digest, read_json, write_graph, write_json
from extractor.generated_sources import build_state, capture_recipes, generator_dependencies, verify

# Provenance fields a dependency refresh may change; every other field must be identical.
REFRESH_FIELDS = ('generated_sources_manifest_sha256', 'generated_input_dependencies',
                  'source_inventory_sha256', 'generator_dependency_refresh')
POLICY = ('Post-parse generator dependency refresh. AST, definitions, calls, overrides, unresolved '
          'records, translation units, coverage and source bytes are those of the original parse '
          '(parse_* identities); only generator dependency metadata and the fingerprints derived '
          'from it were recaptured from the unchanged private build.')


def lf_digest(data):
    return digest(data.replace(b'\r\n', b'\n'))


def code_sha256():
    """Identity of the capture and refresh code, after CRLF-to-LF normalization."""
    here = Path(__file__).resolve()
    return lf_digest(b''.join(path.read_bytes() for path in (here, here.with_name('generated_sources.py'))))


def recipe_bytes(path):
    return gzip.decompress(path.read_bytes()) if path.suffix == '.gz' else path.read_bytes()


def check_tools(manifest, build):
    """Require the tool binaries and CMake/Ninja files recorded at freeze time."""
    checked = {}
    for tool, info in sorted(manifest['provenance'].get('tools', {}).items()):
        binary = Path(info['path'])
        version = subprocess.run([str(binary), '--version'], capture_output=True, text=True, encoding='utf-8')
        if (digest(binary.read_bytes()) != info['sha256']
                or (version.returncode, version.stdout, version.stderr)
                != (info['version_exit'], info['version'], info['stderr'])):
            raise ValueError('Generator tool identity changed: ' + tool)
        checked[tool] = info['sha256']
    for name in ('CMakeCache.txt', 'build.ninja', 'CMakeFiles/rules.ninja'):
        recorded = manifest['provenance'].get(name, {}).get('sha256')
        if recorded is not None:
            if digest((build / name).read_bytes()) != recorded:
                raise ValueError('Build configuration changed: ' + name)
            checked[name] = recorded
    return checked


def source_identity(base, inventory_path, generation_inputs_path, source=None):
    """Bind tracked C/C++ and every pinned generator input to the parse snapshot."""
    inventory, inputs = read_json(inventory_path), read_json(generation_inputs_path)
    provenance = inventory['provenance']
    configured = base['provenance'].get('reconciliation.json', {}).get('record', {})
    root = configured.get('source_root')
    if (not root or root != provenance.get('source_root')
            or (source is not None and Path(source).resolve() != Path(root).resolve())):
        raise ValueError('Refresh source root differs from the original parse source root')
    if (configured.get('source_commit') != provenance['source_commit']
            or inputs.get('source_commit') != provenance['source_commit']
            or inputs.get('upstream_pin') != provenance['upstream_pin']):
        raise ValueError('Refresh source pins differ from the original parse')
    tracked = [{k: v for k, v in f.items() if k != 'coverage'} for f in inventory['files']
               if f.get('origin') != 'build-generated']
    if digest(json.dumps(tracked, sort_keys=True).encode()) != provenance['tracked_source_inventory_sha256']:
        raise ValueError('Tracked inventory differs from the original parse fingerprint')
    files = {entry['path']: entry['sha256'] for entry in tracked}
    for entry in inputs['inputs']:
        path, expected = entry['path'], entry['normalized_sha256']
        if path in files and files[path] != expected:
            raise ValueError('Generator input differs from the tracked parse snapshot: ' + path)
        files[path] = expected
    identity = {'source_root': root, 'source_commit': provenance['source_commit'],
                'upstream_pin': provenance['upstream_pin'],
                'tracked_source_inventory_sha256': provenance['tracked_source_inventory_sha256'],
                'inventory_sha256': lf_digest(inventory_path.read_bytes()),
                'generation_inputs_sha256': lf_digest(generation_inputs_path.read_bytes()),
                'source_files_sha256': digest(json.dumps(files, sort_keys=True).encode()),
                'verified_source_files': len(files),
                'hash_policy': 'SHA-256 after CRLF-to-LF normalization, as in the original parse',
                'files': files}
    if source is not None:
        verify_source(identity)
    return identity


def verify_source(identity):
    root = Path(identity['source_root']).resolve()
    for path, expected in identity['files'].items():
        relative = PurePosixPath(path)
        actual = (root / path).resolve()
        if (not path or relative.is_absolute() or '..' in relative.parts or ':' in path or '\\' in path
                or path.startswith('@build/')
                or not actual.is_relative_to(root) or not actual.is_file()
                or lf_digest(actual.read_bytes()) != expected):
            raise ValueError('Pinned refresh source is missing or changed: ' + path)


def capture(base_manifest_path, source, output, *, inventory_path, generation_inputs_path):
    base = read_json(base_manifest_path)
    build = Path(base['build_root']).resolve()
    if output.resolve().is_relative_to(build):
        raise ValueError('Keep capture output outside the private build tree')
    if read_json(inventory_path)['provenance'].get('generated_sources_manifest_sha256') != lf_digest(base_manifest_path.read_bytes()):
        raise ValueError('Capture inventory differs from the original generated-source manifest')
    source_proof = source_identity(base, inventory_path, generation_inputs_path, source)
    if build_state(build) != base['build_files']:
        raise ValueError('Private build files differ from the parse manifest; a refresh cannot reuse that parse')
    verify(base, build, base_manifest_path.parent)
    tools = check_tools(base, build)
    catalog = subprocess.run(['ninja', '-C', str(build), '-t', 'targets', 'all'],
                             capture_output=True, encoding='utf-8')
    if catalog.returncode:
        raise ValueError('Cannot inventory Ninja targets: ' + catalog.stderr)
    entries = copy.deepcopy(base['files'])
    source_dependencies = {}
    recipes = capture_recipes(build, Path(source), entries, source_dependencies, source_proof['files'])
    if build_state(build) != base['build_files']:
        raise ValueError('Private build files changed during capture')
    verify(base, build, base_manifest_path.parent)
    verify_source(source_proof)
    if check_tools(base, build) != tools:
        raise ValueError('Generator tool or configuration identity changed during capture')
    output.mkdir(parents=True, exist_ok=True)
    write_json(output / 'generated-recipes.json', recipes)
    (output / 'ninja-targets.txt').write_bytes(catalog.stdout.encode())
    record = {'schema_version': 1, 'policy': POLICY,
              'base_manifest_sha256': lf_digest(base_manifest_path.read_bytes()),
              'build_root': str(build), 'source_root': source_proof['source_root'],
              'source_identity': source_proof,
              'build_files_unchanged': len(base['build_files']),
              'frozen_generated_inputs_verified': len(base['files']),
              'tool_and_configuration_sha256': tools,
              'ninja_targets_sha256': digest(catalog.stdout.encode()),
              'ninja_target_records': len(catalog.stdout.splitlines()),
              'recipes_sha256': digest((output / 'generated-recipes.json').read_bytes()),
              'recipe_records': len(recipes), 'capture_code_sha256': code_sha256(),
              'generator_inputs': {e['path']: e['generator_inputs'] for e in entries},
              'source_generator_dependencies': source_dependencies}
    write_json(output / 'generator-dependency-capture.json', record)
    return record


def strip_inventory(inventory):
    files = [{k: v for k, v in f.items() if k != 'generator_inputs'} for f in inventory['files']]
    provenance = {k: v for k, v in inventory['provenance'].items() if k not in REFRESH_FIELDS}
    return {**inventory, 'files': files, 'provenance': provenance}


def strip_graph(graph):
    return {**graph, 'provenance': {k: v for k, v in graph['provenance'].items() if k not in REFRESH_FIELDS}}


def apply(graph_path, inventory_path, base_manifest_path, capture_path, recipes_path, output, *, generation_inputs_path):
    graph, inventory = read_json(graph_path), read_json(inventory_path)
    base, captured = read_json(base_manifest_path), read_json(capture_path)
    base_sha = lf_digest(base_manifest_path.read_bytes())
    provenance = graph['provenance']
    if 'generator_dependency_refresh' in provenance:
        raise ValueError('Apply a refresh to the original parse artifacts, not to a refreshed graph')
    if captured['base_manifest_sha256'] != base_sha:
        raise ValueError('Capture was taken against a different generated-source manifest')
    if inventory['provenance'] != provenance or provenance.get('generated_sources_manifest_sha256') != base_sha:
        raise ValueError('Graph/inventory were not produced from this generated-source manifest')
    identity = captured.get('source_identity', {})
    expected = source_identity(base, inventory_path, generation_inputs_path)
    if not identity or identity != expected or captured.get('source_root') != expected['source_root']:
        raise ValueError('Capture source identity differs from the original parse')
    for entry in inventory['files']:
        if entry.get('origin') != 'build-generated' and identity['files'].get(entry['path']) != entry['sha256']:
            raise ValueError('Capture source bytes differ from the tracked parse snapshot: ' + entry['path'])
    relations = {**captured['source_generator_dependencies'], **captured['generator_inputs']}
    for path, dependencies in relations.items():
        for source_path in [path, *dependencies]:
            if not source_path.startswith('@build/') and source_path not in identity['files']:
                raise ValueError('Capture relation lacks a pinned source identity: ' + source_path)
    if provenance.get('generated_input_dependencies') != generator_dependencies(base):
        raise ValueError('Graph generator dependencies differ from the parse manifest')
    built = [f for f in inventory['files'] if f.get('origin') == 'build-generated']
    if [{k: v for k, v in f.items() if k != 'coverage'} for f in built] != base['files']:
        raise ValueError('Inventory generated entries differ from the parse manifest')
    if set(captured['generator_inputs']) != {f['path'] for f in built}:
        raise ValueError('Capture does not cover exactly the frozen generated inputs')
    if digest(json.dumps([{k: v for k, v in f.items() if k != 'coverage'} for f in inventory['files']],
                         sort_keys=True).encode()) != provenance['source_inventory_sha256']:
        raise ValueError('Inventory fingerprint differs from the parse identity')
    if lf_digest(recipe_bytes(recipes_path)) != captured['recipes_sha256']:
        raise ValueError('Recaptured recipes differ from the capture record')

    output.mkdir(parents=True, exist_ok=True)
    capture_sha = lf_digest(capture_path.read_bytes())
    manifest = copy.deepcopy(base)
    for entry in manifest['files']:
        entry['generator_inputs'] = captured['generator_inputs'][entry['path']]
    manifest['source_generator_dependencies'] = captured['source_generator_dependencies']
    manifest['recipes_sha256'] = captured['recipes_sha256']
    manifest['generator_dependency_refresh'] = {
        'policy': POLICY, 'base_manifest_sha256': base_sha, 'base_recipes_sha256': base.get('recipes_sha256'),
        'capture_sha256': capture_sha, 'capture_code_sha256': captured['capture_code_sha256'],
        'source_identity': {key: value for key, value in identity.items() if key != 'files'}}
    write_json(output / 'generated-sources.json', manifest)
    manifest_sha = lf_digest((output / 'generated-sources.json').read_bytes())

    refreshed = {e['path']: e['generator_inputs'] for e in manifest['files']}
    files = [dict(f, generator_inputs=refreshed[f['path']]) if f.get('origin') == 'build-generated' else f
             for f in inventory['files']]
    snapshot = digest(json.dumps([{k: v for k, v in f.items() if k != 'coverage'} for f in files],
                                 sort_keys=True).encode())
    record = {'policy': POLICY,
              'parse_extractor_sha256': provenance['extractor_sha256'],
              'parse_source_inventory_sha256': provenance['source_inventory_sha256'],
              'parse_generated_sources_manifest_sha256': base_sha,
              'parse_recipes_sha256': base.get('recipes_sha256'),
              'input_graph_sha256': digest(graph_path.read_bytes()),
              'input_inventory_sha256': digest(inventory_path.read_bytes()),
              'capture_sha256': capture_sha, 'capture_code_sha256': captured['capture_code_sha256'],
              'source_identity': {key: value for key, value in identity.items() if key != 'files'},
              'refresh_code_sha256': code_sha256(), 'recipes_sha256': captured['recipes_sha256'],
              'refresh_code_hash_policy': 'refresh_generator_dependencies.py and generated_sources.py concatenated after CRLF-to-LF normalization',
              'changed_fields': ['provenance.' + f for f in REFRESH_FIELDS]
                                + ['inventory build-generated generator_inputs']}
    new_provenance = dict(provenance, generated_sources_manifest_sha256=manifest_sha,
                          generated_input_dependencies=generator_dependencies(manifest),
                          source_inventory_sha256=snapshot, generator_dependency_refresh=record)
    new_graph = dict(graph, provenance=new_provenance)
    new_inventory = dict(inventory, files=files, provenance=new_provenance)
    write_graph(output / 'graph.json', new_graph)
    write_json(output / 'inventory.json', new_inventory)
    shutil.copyfile(capture_path, output / 'generator-dependency-capture.json')
    del graph, inventory, new_graph, new_inventory

    # Independent check from the written bytes: nothing but the declared fields changed.
    before_graph, after_graph = read_json(graph_path), read_json(output / 'graph.json')
    if strip_graph(before_graph) != strip_graph(after_graph):
        raise ValueError('Refresh changed graph content outside generator dependency provenance')
    del before_graph, after_graph
    before, after = read_json(inventory_path), read_json(output / 'inventory.json')
    if strip_inventory(before) != strip_inventory(after):
        raise ValueError('Refresh changed inventory content outside generator dependency metadata')
    if after['provenance']['tracked_source_inventory_sha256'] != before['provenance']['tracked_source_inventory_sha256']:
        raise ValueError('Tracked source inventory changed')
    if after['provenance']['generated_input_dependencies'] != generator_dependencies(read_json(output / 'generated-sources.json')):
        raise ValueError('Published dependencies differ from the refreshed manifest')

    old, new = generator_dependencies(base), after['provenance']['generated_input_dependencies']
    changed = {path: {'added': sorted(set(new.get(path, [])) - set(inputs)),
                      'removed': sorted(set(inputs) - set(new.get(path, [])))}
               for path, inputs in sorted(old.items()) if sorted(inputs) != sorted(new.get(path, []))}
    report = {'schema_version': 1, 'policy': POLICY, 'refresh': record,
              'refreshed_manifest_sha256': manifest_sha, 'refreshed_source_inventory_sha256': snapshot,
              'build_generated_entries': len(built),
              'build_generated_entries_changed': len(changed),
              'build_generated_inputs_added': sum(len(c['added']) for c in changed.values()),
              'build_generated_inputs_removed': sum(len(c['removed']) for c in changed.values()),
              'source_generator_outputs': len(manifest['source_generator_dependencies']),
              'source_generator_outputs_with_inputs': sum(bool(v) for v in manifest['source_generator_dependencies'].values()),
              'recipe_records': captured['recipe_records'],
              'changed_build_generated_entries': changed,
              'invariants': ['graph content outside the declared provenance fields unchanged',
                             'inventory content outside build-generated generator_inputs unchanged',
                             'tracked source inventory fingerprint unchanged',
                             'published dependency map equals the refreshed manifest']}
    write_json(output / 'refresh-report.json', report)
    return report


def main():
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest='command', required=True)
    c = sub.add_parser('capture', help='recapture dependencies from the unchanged private build (WSL, shared lock)')
    c.add_argument('--base-manifest', type=Path, required=True, help='generated-sources.json used by the parse, beside its sources/')
    c.add_argument('--source-root', type=Path, required=True)
    c.add_argument('--inventory', type=Path, required=True, help='inventory.json used by the original parse')
    c.add_argument('--generation-inputs', type=Path, required=True, help='pinned generator/configure input audit')
    c.add_argument('--output', type=Path, required=True)
    a_ = sub.add_parser('apply', help='apply a capture to the original graph/inventory (pure)')
    a_.add_argument('--graph', type=Path, required=True)
    a_.add_argument('--inventory', type=Path, required=True)
    a_.add_argument('--base-manifest', type=Path, required=True)
    a_.add_argument('--capture', type=Path, required=True)
    a_.add_argument('--recipes', type=Path, required=True, help='recaptured generated-recipes.json or its .gz')
    a_.add_argument('--generation-inputs', type=Path, required=True, help='same pinned input audit used by capture')
    a_.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    if a.command == 'capture':
        build = Path(read_json(a.base_manifest)['build_root']).resolve()
        if build == (Path.home() / 'kicad-live/build').resolve():
            p.error('Use only the private pinned build tree')
        if not (Path.home() / 'kicad-port.build-lock').is_dir():
            p.error('Acquire the shared build lock first')
        record = capture(a.base_manifest, a.source_root, a.output,
                         inventory_path=a.inventory, generation_inputs_path=a.generation_inputs)
        print(json.dumps({k: record[k] for k in ('recipe_records', 'ninja_target_records', 'recipes_sha256')}))
    else:
        report = apply(a.graph, a.inventory, a.base_manifest, a.capture, a.recipes, a.output,
                       generation_inputs_path=a.generation_inputs)
        print(json.dumps({k: report[k] for k in ('build_generated_entries_changed', 'build_generated_inputs_added',
                                                 'build_generated_inputs_removed', 'source_generator_outputs')}))
    return 0


if __name__ == '__main__':
    sys.exit(main())
