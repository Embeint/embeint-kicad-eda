"""The d2 review's real-Ninja source reproducer, rerun against the guarded API (fixture only)."""
import copy, json, pathlib, shutil, subprocess, sys
N = pathlib.Path(sys.argv[1])
cur = N / 'current'
sys.path[:0] = [str(cur / 'port'), str(cur / 'port/tools'), str(cur / 'port/extractor')]
from extractor import refresh_generator_dependencies as r
from extractor.generated_sources import freeze, capture_recipes, generator_dependencies, build_state
from extractor.model import PIN, digest, write_json, read_json
from test_extractor import fixture, save_fixture
import drift_report

p = N / 'repro-controls'
if p.exists():
    shutil.rmtree(p)
source, build = p / 'source', p / 'build'
source.mkdir(parents=True); build.mkdir(); (p / 'foreign').mkdir()
g, inv = fixture(source)
(source / 'schema.md').write_text('old\n'); (source / 'derived.h').write_text('// generated\n')
(build / 'final.h').write_text('// generated\n')
(build / 'build.ninja').write_text('rule gen\n  command = true\nbuild ../source/derived.h: gen ../source/schema.md\nbuild final.h: gen ../source/derived.h\n')
git = lambda *a: subprocess.check_output(['git', '-C', str(source), *a], text=True).strip()
commit = lambda m: git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', m)
git('init', '-q'); git('add', '.'); commit('public fixture')
pin = git('rev-parse', 'HEAD')
tool = p / 'tool-sentinel'
tool.write_text('#!/bin/sh\nprintf "sentinel version 1\\n"\n'); tool.chmod(0o755)
ninja = pathlib.Path(shutil.which('ninja'))
tools = {}
for name, binary in (('ninja', ninja), ('sentinel', tool)):
    v = subprocess.run([str(binary), '--version'], capture_output=True, text=True)
    tools[name] = {'path': str(binary), 'sha256': digest(binary.read_bytes()), 'version_exit': v.returncode, 'version': v.stdout, 'stderr': v.stderr}
base = p / 'base'
m = freeze(build, base, {'tools': tools, 'reconciliation.json': {'record': {'source_root': str(source.resolve()), 'source_commit': pin}}})
write_json(base / 'generated-recipes.json', capture_recipes(build, source, copy.deepcopy(m['files'])))
m['recipes_sha256'] = digest((base / 'generated-recipes.json').read_bytes())
write_json(base / 'generated-sources.json', m)
inv['files'].extend(dict(e, coverage='parsed') for e in m['files'])
gp = g['provenance']
gp.update(source_commit=pin, source_root=str(source.resolve()), original_source_root=str(source), extractor_sha256='e' * 64,
          tracked_source_inventory_sha256=digest(json.dumps([{k: v for k, v in f.items() if k != 'coverage'} for f in inv['files'] if f.get('origin') != 'build-generated'], sort_keys=True).encode()),
          source_inventory_sha256=digest(json.dumps([{k: v for k, v in f.items() if k != 'coverage'} for f in inv['files']], sort_keys=True).encode()),
          generated_sources_manifest_sha256=digest((base / 'generated-sources.json').read_bytes()), generated_input_dependencies=generator_dependencies(m))
save_fixture(source, g, inv)
for name in ('graph.json', 'inventory.json'):
    shutil.copyfile(source / 'port/extractor/generated' / name, base / name)
audit = p / 'generation-inputs.json'
write_json(audit, {'source_commit': pin, 'upstream_pin': PIN,
                   'inputs': [{'path': n, 'normalized_sha256': digest((source / n).read_bytes())} for n in ('derived.h', 'schema.md')]})
options = {'inventory_path': base / 'inventory.json', 'generation_inputs_path': audit}
before = build_state(build)
originals = {n: (source / n).read_bytes() for n in ('schema.md', 'derived.h', 'a.cpp')}
results = {}


def attempt(label, root=source, mutate=None, after=None, audit_path=audit):
    if mutate:
        mutate()
    capture, out = p / ('capture-' + label), p / ('out-' + label)
    original = r.capture_recipes
    if after:
        def changed(*a, **kw):
            value = original(*a, **kw); after(); return value
        r.capture_recipes = changed
    try:
        record = r.capture(base / 'generated-sources.json', root, capture, inventory_path=base / 'inventory.json',
                           generation_inputs_path=audit_path)
        r.apply(base / 'graph.json', base / 'inventory.json', base / 'generated-sources.json', capture / 'generator-dependency-capture.json',
                capture / 'generated-recipes.json', out, generation_inputs_path=audit_path)
        results[label] = {'accepted': True, 'dependencies': read_json(out / 'graph.json')['provenance']['generated_input_dependencies']}
    except ValueError as error:
        results[label] = {'accepted': False, 'error': str(error)}
    finally:
        r.capture_recipes = original
        for n, data in originals.items():
            (source / n).write_bytes(data)
        tool.write_text('#!/bin/sh\nprintf "sentinel version 1\\n"\n')
        (base / 'sources/final.h').write_text('// generated\n')


attempt('correct')
attempt('foreign_root', root=p / 'foreign')
attempt('deleted_leaf', mutate=lambda: (source / 'schema.md').unlink())
attempt('deleted_output', mutate=lambda: (source / 'derived.h').unlink())
attempt('stale_markdown', mutate=lambda: (source / 'schema.md').write_text('stale\n'))
attempt('stale_cpp', mutate=lambda: (source / 'a.cpp').write_text('int a() { return 999; }\n'))
reduced = read_json(audit); reduced['inputs'] = [e for e in reduced['inputs'] if e['path'] != 'schema.md']
write_json(p / 'reduced-audit.json', reduced)
attempt('unrecorded_source_node', audit_path=p / 'reduced-audit.json')
attempt('source_changed_after_query', after=lambda: (source / 'schema.md').write_text('changed during capture\n'))
attempt('tool_changed_after_query', after=lambda: tool.write_text('#!/bin/sh\nprintf "sentinel version 2\\n"\n'))
attempt('frozen_copy_changed_after_query', after=lambda: (base / 'sources/final.h').write_text('// tampered\n'))
(build / 'late.h').write_text('// late\n')
attempt('added_build_file')
(build / 'late.h').unlink()
assert build_state(build) == before
assert results['correct']['accepted'] and results['correct']['dependencies']['@build/final.h'] == ['derived.h', 'schema.md']
assert results['correct']['dependencies']['derived.h'] == ['schema.md']
rejected = [k for k in results if k != 'correct']
assert all(not results[k]['accepted'] for k in rejected), {k: results[k] for k in rejected if results[k]['accepted']}
(source / 'schema.md').write_text('changed\n'); git('add', 'schema.md'); commit('change public markdown')
drift_report.PIN = pin
drift = {}
for kind, includes in (('direct', ['derived.h']), ('through_build', ['@build/final.h'])):
    graph = {'provenance': {'upstream_pin': pin, 'source_commit': pin, 'generated_input_dependencies': results['correct']['dependencies']},
             'functions': {'f': {'name': 'a', 'file': 'a.cpp', 'start': 1, 'end': 1}}, 'translation_units': [{'file': 'a.cpp', 'includes': includes}]}
    drift[kind] = drift_report.report(source, graph, [{'id': 'u', 'status': 'partial', 'functions': ['f'], 'upstream': ['a.cpp']}])['affected_units']
assert drift == {'direct': ['u'], 'through_build': ['u']}
summary = {'controls': results, 'accepted': ['correct'], 'rejected': rejected, 'correct_markdown_drift': drift,
           'd2_behaviour': 'foreign/deleted/stale states were accepted and lost relations at d2bdddb904 (review39-d2 source-controls.json)'}
(N / 'evidence/repro-controls.json').write_text(json.dumps(summary, sort_keys=True, indent=2) + '\n')
print(json.dumps({'accepted': summary['accepted'], 'rejected': {k: results[k]['error'][:100] for k in rejected}, 'drift': drift}, indent=1))
