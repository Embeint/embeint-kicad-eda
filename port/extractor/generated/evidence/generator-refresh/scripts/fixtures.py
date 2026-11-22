"""Rerun both independently reported reproducers against the current capture code."""
import json, pathlib, shutil, subprocess, sys
W = pathlib.Path(sys.argv[1])
cur = W / 'current'
sys.path[:0] = [str(cur / 'port'), str(cur / 'port/tools')]
from extractor.generated_sources import capture_recipes, freeze, generator_dependencies
import drift_report

root = W / 'fixtures'
if root.exists():
    shutil.rmtree(root)
result = {}


def repository(source):
    git = lambda *a: subprocess.check_output(['git', '-C', str(source), *a], text=True).strip()
    commit = lambda m: git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', m)
    git('init', '-q'); git('add', '.'); commit('fixture')
    return git, commit


def report(source, pin, deps, includes):
    graph = {'provenance': {'upstream_pin': pin, 'source_commit': pin, 'generated_input_dependencies': deps},
             'functions': {'f': {'name': 'consumer', 'file': 'consumer.cpp', 'start': 1, 'end': 1}},
             'translation_units': [{'file': 'consumer.cpp', 'includes': includes}]}
    return drift_report.report(source, graph, [{'id': 'u', 'status': 'partial', 'functions': ['f'], 'upstream': ['consumer.cpp']}])['affected_units']


# Finding 1: an existing non-C++ intermediate declared only by its absolute name.
base = root / 'absolute-only'
source, build = base / 'source', base / 'build'
source.mkdir(parents=True); (build / 'sub').mkdir(parents=True)
(source / 'schema.proto').write_text('old\n'); (source / 'consumer.cpp').write_text('int consumer(){return 1;}\n')
for name in ['direct.h', 'relative.h', 'absolute.h', 'absoluteonly.h', 'normalized.h', 'cycle.h', 'stage.tokens',
             'only.tokens', 'a.tokens', 'b.tokens', 'downstream.o']:
    (build / name).write_text('// fixture\n')
(build / 'build.ninja').write_text(f'''rule gen
  command = true
build direct.h: gen ../source/schema.proto
build stage.tokens: gen ../source/schema.proto
build relative.h: gen stage.tokens
build absolute.h: gen {build / 'stage.tokens'}
build {build / 'only.tokens'}: gen ../source/schema.proto
build absoluteonly.h: gen {build / 'only.tokens'}
build normalized.h: gen sub/../stage.tokens
build a.tokens: gen b.tokens ../source/schema.proto
build b.tokens: gen a.tokens
build cycle.h: gen a.tokens
build downstream.o: gen relative.h
''')
manifest = freeze(build, base / 'frozen', {})
recipes = capture_recipes(build, source, manifest['files'])
deps = {e['path']: e['generator_inputs'] for e in manifest['files']}
git, commit = repository(source)
pin = git('rev-parse', 'HEAD')
(source / 'schema.proto').write_text('new\n'); git('add', '.'); commit('change')
drift_report.PIN = pin
cases = {}
for name in ['direct', 'relative', 'absolute', 'absoluteonly', 'normalized', 'cycle']:
    cases[name] = {'deps': deps['@build/' + name + '.h'], 'affected': report(source, pin, deps, ['@build/' + name + '.h'])}
    assert cases[name]['affected'] == ['u'], (name, cases[name])
assert cases['absoluteonly']['deps'] == ['@build/only.tokens', 'schema.proto']
assert recipes['@build/only.tokens']['target'] == str(build / 'only.tokens')
assert '@build/downstream.o' not in deps['@build/relative.h']
cases['negative_reverse_output'] = {'downstream.o_in_relative_inputs': False}
cases['negative_unrelated_include'] = {'affected': report(source, pin, deps, [])}
assert cases['negative_unrelated_include']['affected'] == []
cases['only_tokens_query_attempts'] = recipes['@build/only.tokens']['query_attempts']
result['absolute_only_output'] = cases

# Finding 2: a tracked source-tree output with direct and build-generated consumers.
base = root / 'source-output'
source, build = base / 'source', base / 'build'
source.mkdir(parents=True); build.mkdir()
(source / 'schema.md').write_text('old\n'); (source / 'derived.h').write_text('// generated from schema.md\n')
(source / 'consumer.cpp').write_text('int consumer(){return 1;}\n'); (build / 'final.h').write_text('// frozen\n')
(build / 'build.ninja').write_text('rule gen\n  command = true\nbuild ../source/derived.h: gen ../source/schema.md\n'
                                   'build final.h: gen ../source/derived.h\n')
manifest = freeze(build, base / 'frozen', {})
manifest['source_generator_dependencies'] = {}
capture_recipes(build, source, manifest['files'], manifest['source_generator_dependencies'])
deps = generator_dependencies(manifest)
git, commit = repository(source)
pin = git('rev-parse', 'HEAD')
(source / 'schema.md').write_text('new\n'); git('add', '.'); commit('change')
drift_report.PIN = pin
cases = {'deps': deps}
assert deps['derived.h'] == ['schema.md'] and deps['@build/final.h'] == ['derived.h', 'schema.md']
for kind, includes in [('build_consumer', ['@build/final.h']), ('direct_tracked_consumer', ['derived.h'])]:
    cases[kind] = report(source, pin, deps, includes)
    assert cases[kind] == ['u'], (kind, cases[kind])
cases['negative_unrelated_include'] = report(source, pin, deps, [])
assert cases['negative_unrelated_include'] == []
result['source_tree_output'] = cases
(W / 'evidence/reviewed-reproducers.json').write_text(json.dumps(result, sort_keys=True, indent=2) + '\n')
print(json.dumps(result, indent=1)[:4000])
