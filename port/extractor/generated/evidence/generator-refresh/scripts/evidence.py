"""Real-data proofs for the refreshed generator relations, with negative controls."""
import gc, json, pathlib, shutil, subprocess, sys, tomllib
from pathlib import Path, PurePosixPath
W = Path(sys.argv[1])
home = Path.home() / 'kicad-port'
V5 = home / 'corrected-review-v5'
cur = W / 'current'
sys.path[:0] = [str(cur / 'port'), str(cur / 'port/tools')]
from extractor.model import read_json
from extractor.generated_sources import SUFFIXES, generator_dependencies
import drift_report

G = W / 'final/port/extractor/generated'
graph = read_json(G / 'graph.json')
units = [tomllib.loads(p.read_text(encoding='utf-8')) for p in sorted((W / 'final/port/units').glob('*.toml'))]
capture = read_json(W / 'capture/generator-dependency-capture.json')
recipes = read_json(W / 'capture/generated-recipes.json')
base_manifest = read_json(V5 / 'inputs/generated-sources.json')
old = generator_dependencies(base_manifest)
new = graph['provenance']['generated_input_dependencies']
build, source = Path(capture['build_root']), Path(capture['source_root'])
evidence = {}

# 1. The concrete reviewed relation: tracked generated header -> Markdown + generator script.
header, markdown, script = 'pcbnew/pcb_text_help_md.h', 'pcbnew/pcb_text_help.md', 'cmake/BuildSteps/markdown2C.cmake'
record = recipes[header]
listing = sorted(p for p, v in new.items() if header in v and p != header)
direct = sorted(t['file'] for t in graph['translation_units'] if header in t.get('includes', []))
assert markdown in new[header] and script in new[header], new.get(header)
assert header not in old and all(markdown not in v for v in old.values())
assert all(markdown in new[p] for p in listing)


def closure(path):
    pending, seen = [path], set()
    while pending:
        item = pending.pop()
        if item not in seen:
            seen.add(item)
            pending.extend(new.get(item, []))
    return seen


includes = {t['file']: t.get('includes', []) for t in graph['translation_units']}
# Consumers whose only route to the Markdown is the tracked header itself.
direct_only = sorted(f for f in direct if not any(i.startswith('@build/') and markdown in closure(i) for i in includes[f]))
assert direct_only, 'expected a direct tracked consumer without a build-generated route'
evidence['pcb_text_help'] = {
    'direct_consumers_without_build_route': direct_only,
    'source_generator_dependencies': new[header], 'recipe_target': record['target'], 'recipe_stage': record['stage'],
    'query_attempts': [{k: a[k] for k in ('target', 'exit', 'stdout', 'stderr')} for a in record['query_attempts']],
    'previous_map_had_source_key': False, 'previous_entries_reaching_markdown': [],
    'entries_listing_header': listing, 'entries_listing_header_now_reach_markdown': True,
    'direct_tracked_translation_units': direct}

# 2. Real-data drift: a disposable repository changes only the Markdown input.
repo = W / 'drift-proof'
if repo.exists():
    shutil.rmtree(repo)
(repo / 'pcbnew').mkdir(parents=True)
shutil.copyfile(source / markdown, repo / markdown)
git = lambda *a: subprocess.check_output(['git', '-C', str(repo), *a], text=True).strip()
commit = lambda m: git('-c', 'user.name=Refresh proof', '-c', 'user.email=proof@example.invalid', 'commit', '-qm', m)
git('init', '-q'); git('add', '.'); commit('pinned markdown input')
pin = git('rev-parse', 'HEAD')
with (repo / markdown).open('a') as f:
    f.write('\nRefresh proof change.\n')
git('add', '.'); commit('change generator input')
changed = git('rev-parse', 'HEAD')
git('checkout', '-q', pin)
(repo / 'pcbnew/unrelated_refresh_control.md').write_text('Not a recorded generator input.\n')
git('add', '.'); commit('unrelated control')
unrelated = git('rev-parse', 'HEAD')
drift_report.PIN = pin


def drift(dependencies, target):
    g = {'provenance': dict(graph['provenance'], upstream_pin=pin, source_commit=pin,
                            generated_input_dependencies=dependencies),
         'functions': graph['functions'], 'translation_units': graph['translation_units']}
    r = drift_report.report(repo, g, units, target, include_todo=True)
    return {'affected': r['affected'], 'affected_units': r['affected_units'],
            'changed_inputs': sorted({i for d in r['dependency_reviews'] for i in d['changed_inputs']})}


owners = {}
for unit in units:
    for path in unit['upstream']:
        owners.setdefault(path, set()).add(unit['id'])
direct_owner_units = sorted({u for path in direct for u in owners.get(path, ())})
refreshed_change = drift(new, changed)
previous_change = drift(old, changed)
refreshed_unrelated = drift(new, unrelated)
assert refreshed_change['affected'] == [] and refreshed_change['changed_inputs'] == [markdown]
assert set(direct_owner_units) <= set(refreshed_change['affected_units']) and direct_owner_units
assert previous_change['affected_units'] == [], 'negative control: previous map missed the relation'
assert refreshed_unrelated['affected_units'] == []
evidence['pcb_text_help_drift'] = {
    'disposable_repository_commits': {'pin': pin, 'markdown_change': changed, 'unrelated_change': unrelated},
    'refreshed_map_markdown_change': {'affected_unit_count': len(refreshed_change['affected_units']),
                                      'affected_units': refreshed_change['affected_units'],
                                      'changed_inputs': refreshed_change['changed_inputs']},
    'direct_tracked_consumer_owner_units': direct_owner_units,
    'negative_control_previous_map_markdown_change': previous_change,
    'negative_control_refreshed_map_unrelated_change': refreshed_unrelated}

# 3. Catalog identities: aliases, absolute-only outputs, non-C++ stages, reverse-output exclusion.
spellings = {}
catalog = (W / 'capture/ninja-targets.txt').read_text().splitlines()
for line in catalog:
    target = line.rpartition(': ')[0]
    path = Path(target)
    path = (path if path.is_absolute() else build / path).resolve()
    logical = (path.relative_to(source).as_posix() if path.is_relative_to(source) else
               '@build/' + path.relative_to(build).as_posix() if path.is_relative_to(build) else None)
    if logical is not None:
        spellings.setdefault(logical, []).append(target)
reached = {p for v in new.values() for p in v} | set(new)
aliases = {k: v for k, v in spellings.items() if len(v) > 1}
absolute_only = sorted(k for k, v in spellings.items() if all(s.startswith('/') for s in v))
non_cpp = sorted(k for k, r in recipes.items() if k.startswith('@build/') and r['query_exit'] == 0
                 and PurePosixPath(k).suffix.lower() not in SUFFIXES and k in reached)
unresolved = sorted(k for k, r in recipes.items() if r['query_exit'] != 0)
objects = ('.o', '.obj', '.a', '.so', '.dll', '.exe')


def logical_of(item):
    path = Path(item.strip().removeprefix('|| ').removeprefix('| '))
    path = (path if path.is_absolute() else build / path).resolve()
    return (path.relative_to(source).as_posix() if path.is_relative_to(source) else
            '@build/' + path.relative_to(build).as_posix() if path.is_relative_to(build) else None)


def sections(record):
    """Direct inputs (with order-only marks) and consumers from every successful query spelling."""
    inputs, order_only, consumers = set(), set(), set()
    for attempt in record.get('query_attempts', []):
        if attempt['exit']:
            continue
        section = None
        for line in attempt['stdout'].splitlines()[1:]:
            if line.startswith('  ') and not line.startswith('    '):
                section = line.strip().split(':', 1)[0]
            elif line.startswith('    ') and section in ('input', 'outputs'):
                logical = logical_of(line)
                if logical is None:
                    continue
                if section == 'outputs':
                    consumers.add(logical)
                else:
                    inputs.add(logical)
                    if line.strip().startswith('||'):
                        order_only.add(logical)
    return inputs, order_only, consumers


# Negative control: a node's consumers (Ninja 'outputs:') never become its inputs, unless
# Ninja also lists them as inputs of a node in the same closure (a genuine cycle).
reverse, order_only_nodes = {}, set()
direct = {k: sections(r) for k, r in recipes.items()}
for key, deps in new.items():
    if key not in direct:
        continue
    consumers = direct[key][2] - direct[key][0]
    leaked = sorted(c for c in consumers & set(deps)
                    if not any(c in direct.get(d, (set(),))[0] for d in deps if d in direct))
    if leaked:
        reverse[key] = leaked
    order_only_nodes.update(direct[key][1])
assert not reverse, reverse
removed = {k: sorted(set(v) - set(new.get(k, []))) for k, v in old.items() if set(v) - set(new.get(k, []))}
# The only removal is the build directory node itself: neither a file nor a Ninja target.
assert all(r == ['@build/.'] for r in removed.values()), removed
evidence['catalog'] = {
    'ninja_target_records': len(catalog), 'logical_targets': len(spellings),
    'alias_groups': len(aliases), 'alias_groups_reached': sorted(k for k in aliases if k in reached),
    'absolute_only_targets': len(absolute_only), 'absolute_only_reached': sorted(k for k in absolute_only if k in reached),
    'non_cpp_stages_reached': non_cpp, 'recipe_records': len(recipes),
    'unresolved_queries': unresolved,
    'unresolved_policy': 'All recorded spellings failed; these are CMake-configure outputs or compiler probes with no Ninja rule (stage label retained), not guessed edges',
    'reverse_output_nodes_in_inputs': 0,
    'reverse_output_policy': "Consumers in each node's Ninja 'outputs:' section are never followed",
    'previous_inputs_removed': {'entries': len(removed), 'paths': ['@build/.'],
        'reason': 'V5 recorded the build directory node; it is neither a file nor a Ninja target and is never a Git path, so drift is unaffected'},
    'order_only_over_approximation': {
        'policy': 'Order-only (||) and implicit (|) Ninja inputs are followed conservatively, as in V5, so library/object nodes and their sources can appear in a closure',
        'order_only_nodes_reached': len(order_only_nodes),
        'v5_build_entries_with_object_nodes': sum(any(p.endswith(objects) for p in v) for v in old.values()),
        'v5_object_node_references': sum(p.endswith(objects) for v in old.values() for p in v),
        'refreshed_build_entries_with_object_nodes': sum(any(p.endswith(objects) for p in v) for k, v in new.items() if k.startswith('@build/')),
        'refreshed_source_outputs_with_object_nodes': sorted(k for k, v in new.items() if not k.startswith('@build/') and any(p.endswith(objects) for p in v))},
    'source_generator_outputs': len(capture['source_generator_dependencies']),
    'source_generator_outputs_with_inputs': sorted(k for k, v in capture['source_generator_dependencies'].items() if v)}
del graph
gc.collect()

# 4. Current raw Ninja queries agree with every V5 record, or identify the exact target V5 missed.
v5_recipes = read_json(V5 / 'inputs/generated-recipes.json')
agree, resolved_by_spelling, not_requeried = 0, [], []
for key, before in v5_recipes.items():
    if key not in recipes:
        # Only the build directory node, which is no longer treated as an input.
        assert key == '@build/.', key
        not_requeried.append({'path': key, 'v5_exit': before['query_exit'], 'v5_query': before['query'],
                              'v5_query_stderr': before['query_stderr']})
        continue
    after = recipes[key]
    relative = key.removeprefix('@build/')
    attempt = next((a for a in after['query_attempts'] if a['target'] == relative), None)
    if attempt is not None:
        assert (attempt['exit'], attempt['stdout'], attempt['stderr']) == (before['query_exit'], before['query'], before['query_stderr']), key
        agree += 1
    else:
        assert before['query_exit'] != 0, key
        resolved_by_spelling.append({'path': key, 'v5_exit': before['query_exit'], 'target': after['target'],
                                     'exit': after['query_exit']})
evidence['v5_recipe_consistency'] = {'v5_records': len(v5_recipes), 'same_spelling_identical': agree,
                                     'v5_failures_now_queried_by_exact_target': resolved_by_spelling,
                                     'v5_records_not_requeried': not_requeried,
                                     'new_records': sorted(set(recipes) - set(v5_recipes))}
(W / 'evidence/relations.json').write_text(json.dumps(evidence, sort_keys=True, indent=2) + '\n')
print(json.dumps({k: (v if k != 'catalog' else {x: (y if not isinstance(y, list) else len(y)) for x, y in v.items()})
                  for k, v in evidence.items() if k != 'pcb_text_help_drift'}, indent=1)[:6000])
print(json.dumps({k: v for k, v in evidence['pcb_text_help_drift'].items() if k != 'refreshed_map_markdown_change'}, indent=1))
print('affected', len(evidence['pcb_text_help_drift']['refreshed_map_markdown_change']['affected_units']))
