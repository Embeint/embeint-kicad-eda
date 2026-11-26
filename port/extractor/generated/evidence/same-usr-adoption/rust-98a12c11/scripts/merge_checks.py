"""Final Windows checks for the same-USR graph adoption after merging accepted rust 98a12c1123.

Run from the worktree at the final commit. Writes evidence/same-usr-adoption/rust-98a12c11/checks.json.
Checks: suites, provenance identities, unit metadata survival against accepted rust, graph-derived
fields against the adoption at 4738a9c23f, canonical split stability, measurement, drift on the
final tree and with the accepted graph, source accounting and change scope.
"""
import gc, gzip, hashlib, json, os, re, runpy, shutil, subprocess, sys, tempfile, tomllib
from pathlib import Path

root = Path(r'C:\Users\KarlVonRichter\.t3\worktrees\embeint-kicad-eda\t3-290c6870-81dcfa65b1526978')
os.chdir(root)
S = Path(r'C:\Users\KarlVonRichter\.t3\canon-adopt-merge')
G = root / 'port/extractor/generated'
E = G / 'evidence/same-usr-adoption'
M = E / 'rust-98a12c11'
R = G / 'replay-extraction'
RUST = '98a12c1123efc4904e0f0c5a59299452a7a15e5d'
ADOPTED = 'f66f4e965c'  # adoption commit at 4738a9c23f, before the merge
GRAPH_KEYS = ('depends_on', 'soft_depends_on', 'layer')
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
lf = lambda b: hashlib.sha256(b.replace(b'\r\n', b'\n')).hexdigest()
blob = lambda rev, path: subprocess.check_output(['git', 'cat-file', 'blob', f'{rev}:{path}'])
git = lambda *a: subprocess.check_output(['git', *a], text=True)
results = {'accepted_rust': RUST, 'head': git('rev-parse', 'HEAD').strip(), 'head_tree': git('rev-parse', 'HEAD^{tree}').strip(),
           'worktree_clean_except_evidence': [x for x in git('status', '--porcelain').splitlines() if 'same-usr-adoption/rust-98a12c11/' not in x], 'adoption_commit': git('rev-parse', ADOPTED).strip()}
M.mkdir(exist_ok=True)

# 1. Suites on Windows.
for name, command in [
        ('extractor-windows', [sys.executable, '-m', 'unittest', 'discover', '-s', 'port/extractor', '-p', 'test_*.py', '-v']),
        ('progress-tests', [sys.executable, '-m', 'unittest', 'discover', '-s', 'port/site', '-p', 'test_*.py', '-v']),
        ('bootstrap-tests', [sys.executable, '-m', 'unittest', 'discover', '-s', 'port/tools', '-p', 'test_*.py', '-v']),
        ('policy-tests', [sys.executable, '-m', 'unittest', 'discover', '-s', 'rust/kicad-difftest/tests', '-p', 'test_policy.py', '-v']),
        ('progress-build', [sys.executable, 'port/tools/progress.py', '--out', str(S / 'site')]),
        ('golden-changes', [sys.executable, 'port/tools/check_goldens_policy.py', '--base', RUST]),
        ('validate', [sys.executable, 'port/tools/validate_units.py', '--json']),
        ('validate-strict', [sys.executable, 'port/tools/validate_units.py', '--json', '--require-complete'])]:
    with (M / f'{name}.log').open('w', encoding='utf-8', newline='\n') as log:
        code = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT).returncode
    text = (M / f'{name}.log').read_text(encoding='utf-8')
    results[name] = {'command': command[1:], 'exit_code': code}
    count, skipped = re.search(r'Ran (\d+) tests?', text), re.search(r'OK \(skipped=(\d+)\)', text)
    if count:
        results[name].update(tests=int(count[1]), skipped=int(skipped[1]) if skipped else 0)
    print(name, results[name], flush=True)
    assert code == (1 if name == 'validate-strict' else 0), (name, code)
assert json.loads((M / 'validate-strict.log').read_text())['errors'] == \
    ['WSL extraction diagnostic incomplete; inspect provenance coverage and parse failures']
for name in ('validate', 'validate-strict'):
    assert (M / f'{name}.log').read_bytes() == (G / f'{name}.log').read_bytes(), name

# 2. Provenance identities of the committed canonical artifacts.
identity = json.loads((E / 'identities.json').read_text())
graph = json.loads(gzip.decompress((G / 'graph.json.gz').read_bytes()))
p = graph['provenance']
r = p['generator_dependency_refresh']
inventory = json.loads((G / 'inventory.json').read_text())
replay_inventory = json.loads((R / 'inventory.json').read_text())
aggregation = lf(b''.join((root / x).read_bytes() for x in ('port/tools/extract_graph.py', 'port/extractor/clang_graph.py',
                                                             'port/extractor/model.py', 'port/extractor/generated_sources.py')))
refresh_code = lf(b''.join((root / 'port/extractor' / n).read_bytes() for n in ('refresh_generator_dependencies.py', 'generated_sources.py')))
replay = p['parse_cache_replay']
checks = {
    'inventory_provenance_equals_graph': inventory['provenance'] == p,
    'parse_extractor_is_v5': p['extractor_sha256'] == r['parse_extractor_sha256'] == identity['inputs_sha256']['v5_parser'],
    'aggregation_identity_is_final_tree': p['aggregation_extractor_sha256'] == replay['aggregation_extractor_sha256']
                                          == identity['aggregation_extractor_sha256'] == aggregation,
    'zero_parses_all_reused': replay['parses'] == 0 and p['cache_reused_commands'] == p['commands_attempted'] == 2511,
    'four_fmt_parse_failures': sorted(f['file'] if isinstance(f, dict) else f[0] for f in p['parse_failures'])
                               == ['thirdparty/fmt/src/fmt-c.cc', 'thirdparty/fmt/src/fmt.cc', 'thirdparty/fmt/src/format.cc', 'thirdparty/fmt/src/os.cc'],
    'parse_and_gate_incomplete': p['parse_complete'] is False and p['gate_complete'] is False and p['all_configurations_complete'] is False,
    'refresh_and_capture_code_is_final_tree': r['refresh_code_sha256'] == r['capture_code_sha256'] == refresh_code,
    'refresh_input_is_pre_refresh_canonical': r['input_graph_sha256'] == json.loads((E / 'assembly.json').read_text())['pre_refresh_canonical_graph_sha256'],
    'refresh_input_inventory_is_replay': r['input_inventory_sha256'] == sha(R / 'inventory.json') == r['source_identity']['inventory_sha256'],
    'parse_identities_unchanged': (r['parse_generated_sources_manifest_sha256'], r['parse_recipes_sha256'], r['parse_source_inventory_sha256'])
                                  == ('a2466270bdf4aec52b7b8267140ac4b18c8295520c5b94e1ecb3dc8498dacb34',
                                      'dff5b79afdbe58e2e4c7a4f0e4d8b093fd34a94a0302d38cddd6483660496951',
                                      '3d52afc68446cc7e00344409c2fc1ac6908ed5aa554f55d2501126885e5ee175'),
    'manifest_bound': lf((G / 'generated-sources.json').read_bytes()) == p['generated_sources_manifest_sha256'],
    'capture_bound': lf((E / 'generator-dependency-capture.json').read_bytes()) == r['capture_sha256'],
    'recipes_bound': lf(gzip.decompress((G / 'generated-recipes.json.gz').read_bytes())) == r['recipes_sha256']
                     == json.loads((G / 'generated-sources.json').read_text())['recipes_sha256'],
    'range_postprocessing_input_is_replay': graph['range_postprocessing']['input_graph_sha256'] == sha(R / 'graph.json.gz'),
    'replay_inventory_provenance_matches_parse_identity': replay_inventory['provenance']['source_inventory_sha256']
                                                          == r['parse_source_inventory_sha256'],
    'corrected_extraction_v5_unchanged': sha(G / 'corrected-extraction/graph.json.gz') == identity['inputs_sha256']['v5_graph'],
    'canonical_artifacts_unchanged_by_merge': all(blob(ADOPTED, f'port/extractor/generated/{n}') == (G / n).read_bytes()
                                                  for n in ('graph.json.gz', 'inventory.json', 'units.json', 'generated-sources.json')),
}
print(checks, flush=True)
assert all(checks.values()), checks
results['identities'] = {'checks': checks, 'aggregation_extractor_sha256': p['aggregation_extractor_sha256'],
                         'parse_extractor_sha256': p['extractor_sha256'],
                         'canonical_graph_sha256': sha(G / 'graph.json.gz'), 'canonical_inventory_sha256': sha(G / 'inventory.json'),
                         'units_report_sha256': sha(G / 'units.json'), 'measurement_sha256': sha(G / 'measurement.json'),
                         'generated_sources_sha256': sha(G / 'generated-sources.json'),
                         'replay_graph_sha256': sha(R / 'graph.json.gz'), 'replay_inventory_sha256': sha(R / 'inventory.json'),
                         'accepted_graph_sha256': hashlib.sha256(blob(RUST, 'port/extractor/generated/graph.json.gz')).hexdigest()}

# 3. Source accounting: inventory files and line counts against accepted rust.
rust_inventory = json.loads(blob(RUST, 'port/extractor/generated/inventory.json'))
differing = sorted(k for k in set(rust_inventory) | set(inventory) if rust_inventory.get(k) != inventory.get(k))
def strip_generator_inputs(value):
    if isinstance(value, dict):
        return {k: strip_generator_inputs(v) for k, v in value.items() if k != 'generator_inputs'}
    if isinstance(value, list):
        return [strip_generator_inputs(v) for v in value]
    return value
results['source_accounting'] = {
    'differing_top_level_keys': differing,
    'equal_except_provenance_and_generator_inputs': strip_generator_inputs({k: v for k, v in rust_inventory.items() if k != 'provenance'})
                                                    == strip_generator_inputs({k: v for k, v in inventory.items() if k != 'provenance'}),
}
assert results['source_accounting']['equal_except_provenance_and_generator_inputs'], differing
del graph, inventory, replay_inventory, rust_inventory
gc.collect()

# 4. Unit records: accepted metadata survives; graph-derived fields equal the adoption at 4738a9c23f.
def tree_units(rev):
    out = {}
    for line in git('ls-tree', rev, 'port/units/').splitlines():
        meta, path = line.split('\t', 1)
        if path.endswith('.toml'):
            out[Path(path).stem] = tomllib.loads(subprocess.check_output(['git', 'cat-file', 'blob', meta.split()[2]]).decode())
    return out
units = {path.stem: tomllib.loads(path.read_text(encoding='utf-8')) for path in (root / 'port/units').glob('*.toml')}
rust_units, adopted_units, base_units = tree_units(RUST), tree_units(ADOPTED), tree_units('4738a9c23f8c6068784d26ba991a690044343892')
assert set(units) == set(rust_units) == set(adopted_units) and len(units) == 1801
metadata_survives = {u: all(units[u].get(k) == rust_units[u].get(k) for k in set(units[u]) | set(rust_units[u]) if k not in GRAPH_KEYS)
                     for u in units}
graph_fields_adopted = {u: all(units[u][k] == adopted_units[u][k] for k in GRAPH_KEYS) for u in units}
assert all(metadata_survives.values()), [u for u, ok in metadata_survives.items() if not ok]
assert all(graph_fields_adopted.values()), [u for u, ok in graph_fields_adopted.items() if not ok]
changed_keys = {}
for u in units:
    for k in set(units[u]) | set(rust_units[u]):
        if units[u].get(k) != rust_units[u].get(k):
            changed_keys[k] = changed_keys.get(k, 0) + 1
assert set(changed_keys) <= set(GRAPH_KEYS), changed_keys
accepted_since_base = sorted(u for u in units if rust_units[u] != base_units[u])
non_todo = {u: x['status'] for u, x in units.items() if x['status'] not in ('todo', 'blocked')}
non_todo_graph_changes = {u: {k: {'accepted': rust_units[u][k], 'adopted': units[u][k]} for k in GRAPH_KEYS if units[u][k] != rust_units[u][k]}
                          for u in sorted(units) if units[u]['status'] != 'todo'}
non_todo_graph_changes = {u: v for u, v in non_todo_graph_changes.items() if v}
hard_added = sorted((u, d) for u in units for d in set(units[u]['depends_on']) - set(rust_units[u]['depends_on']))
hard_removed = sorted((u, d) for u in units for d in set(rust_units[u]['depends_on']) - set(units[u]['depends_on']))
soft_added = sorted((u, d) for u in units for d in set(units[u]['soft_depends_on']) - set(rust_units[u]['soft_depends_on']))
soft_removed = sorted((u, d) for u in units for d in set(rust_units[u]['soft_depends_on']) - set(units[u]['soft_depends_on']))
rust_all = lambda u: set(rust_units[u]['depends_on']) | set(rust_units[u]['soft_depends_on'])
new_all = lambda u: set(units[u]['depends_on']) | set(units[u]['soft_depends_on'])
reference = Path(r'C:\Users\KarlVonRichter\.t3\phase0-solidcore-reference')
assert git('-C', str(reference), 'rev-parse', 'HEAD').strip() == '7a588fa284765a83823293ae20b84da8356ab479'
loader = runpy.run_path(str(reference / 'harness/plan/units.py'))
roundtrip, unsupported = 0, {}
for uid, unit in units.items():
    assert set(loader['FIELDS']) <= set(unit)
    try:
        assert loader['parse_unit'](loader['dump_unit'](unit)) == unit
        roundtrip += 1
    except Exception as error:  # solidcore's loader rejects the port's extension tables
        unsupported[uid] = str(error)
rust_unsupported = {}
for uid, unit in rust_units.items():
    try:
        loader['parse_unit'](loader['dump_unit'](unit))
    except Exception as error:
        rust_unsupported[uid] = str(error)
assert unsupported == rust_unsupported, (unsupported, rust_unsupported)
results['units'] = {
    'count': len(units), 'metadata_equal_to_accepted_rust': True, 'graph_fields_equal_to_adoption': True,
    'changed_key_counts_vs_accepted': changed_keys, 'changed_units_vs_accepted': sum(units[u] != rust_units[u] for u in units),
    'accepted_records_changed_since_base': accepted_since_base,
    'statuses': {s: sum(u['status'] == s for u in units.values()) for s in sorted({u['status'] for u in units.values()})},
    'non_todo_units': non_todo, 'non_todo_or_blocked_graph_field_changes': non_todo_graph_changes,
    'hard_edges_added': len(hard_added), 'hard_edges_removed': len(hard_removed),
    'soft_edges_added': len(soft_added), 'soft_edges_removed': len(soft_removed),
    'hard_added_were_soft': sum(d in rust_units[u]['soft_depends_on'] for u, d in hard_added),
    'soft_added_were_hard': sum(d in rust_units[u]['depends_on'] for u, d in soft_added),
    'new_unit_dependencies': sorted([u, d] for u in units for d in new_all(u) - rust_all(u)),
    'dropped_unit_dependencies': len([1 for u in units for d in rust_all(u) - new_all(u)]),
    'max_layer': [max(u['layer'] for u in rust_units.values()), max(u['layer'] for u in units.values())],
    'solidcore_roundtrip_units': roundtrip, 'solidcore_unsupported_equal_accepted': True,
}
print(json.dumps({k: v for k, v in results['units'].items() if k != 'non_todo_or_blocked_graph_field_changes'}, indent=1)[:3000], flush=True)

# 5. Canonical split over the final records is byte-stable (two iterations).
with tempfile.TemporaryDirectory() as tmp:
    regen = Path(tmp)
    (regen / 'port/units').mkdir(parents=True)
    for path in (root / 'port/units').glob('*.toml'):
        (regen / 'port/units' / path.name).write_bytes(path.read_bytes().replace(b'\r\n', b'\n'))
    gd = regen / 'port/extractor/generated'
    gd.mkdir(parents=True)
    for name in ('graph.json.gz', 'inventory.json', 'units.json'):
        shutil.copyfile(G / name, gd / name)
    shutil.copytree(G / 'sources', gd / 'sources')
    files = lambda: {str(x.relative_to(regen)): sha(x) for x in sorted(regen.joinpath('port/units').glob('*.toml'))} | \
        {n: sha(gd / n) for n in ('graph.json.gz', 'inventory.json', 'units.json')}
    before = files()
    for _ in range(2):
        subprocess.run([sys.executable, 'port/tools/split_units.py', '--graph', str(gd / 'graph.json'), '--inventory', str(gd / 'inventory.json'),
                        '--units-dir', str(regen / 'port/units'), '--report', str(gd / 'units.json'), '--source-root', str(root)],
                       check=True, capture_output=True)
        assert files() == before
    results['canonical_regeneration'] = {'iterations': 2, 'byte_identical_artifacts': len(before)}

# 6. Measurement over the final records.
sys.path.insert(0, str(root / 'port/tools'))
from extraction_report import measure_units
measurement = json.loads((G / 'measurement.json').read_text())
assert measure_units(list(units.values()), json.loads((G / 'units.json').read_text())) == measurement['units']
for record in measurement['canonical']['artifacts']:
    assert sha(G / record['filename']) == record['sha256'] and (G / record['filename']).stat().st_size == record['bytes'], record
for record in measurement['runs']['corrected']['artifacts']:
    assert sha(R / record['filename']) == record['sha256'], record
m = measurement['units']
results['metrics'] = {'unit_count': m['unit_count'], 'accounted_lines': m['accounted_lines'], 'layers': len(m['layers']),
                      'hard_edge_count': m['hard_edge_count'], 'back_edge_count': m['back_edge_count'],
                      'back_edge_weight': m['back_edge_weight'],
                      'wx': {k: (v['units'], v['lines']) for k, v in m['wx_classes'].items()}, 'statuses': m['statuses'],
                      'corrected_edges': measurement['runs']['corrected']['edges'],
                      'corrected_functions': measurement['runs']['corrected']['functions'],
                      'corrected_parsed_ok': measurement['runs']['corrected']['parsed_ok'],
                      'oversized_units': len(m['oversized_units']) if isinstance(m['oversized_units'], list) else m['oversized_units'],
                      'largest_generated_blob_bytes': max(x.stat().st_size for x in G.rglob('*') if x.is_file())}
assert results['metrics']['largest_generated_blob_bytes'] < 100_000_000
rust_measurement = json.loads(blob(RUST, 'port/extractor/generated/measurement.json'))
def walk(a, b, prefix=''):
    if isinstance(a, dict) and isinstance(b, dict):
        return [x for k in sorted(set(a) | set(b)) for x in walk(a.get(k), b.get(k), f'{prefix}.{k}')]
    return [prefix] if a != b else []
results['measurement_changed_paths_vs_accepted'] = walk(rust_measurement, measurement)
del measurement, rust_measurement

# 7. Drift on the final tree, and with the accepted graph over the same records.
from drift_report import report
drift = {}
for include_todo, name in ((False, 'drift-ported.json'), (True, 'drift-all.json')):
    committed = json.loads((G / name).read_text())
    out = subprocess.run([sys.executable, 'port/tools/drift_report.py', *(['--include-todo'] if include_todo else []),
                          '--output', str(M / name)], capture_output=True, text=True)
    assert out.returncode == 0, out.stderr
    current = json.loads((M / name).read_text())
    targets = (committed.pop('target'), current.pop('target'))
    drift[name] = {'final_tree_equal_to_committed_except_target': committed == current, 'committed_target': targets[0], 'final_target': targets[1]}
    assert committed == current, name
unit_list = [units[u] for u in sorted(units)]
for label, data in (('adopted', (G / 'graph.json.gz').read_bytes()), ('accepted', blob(RUST, 'port/extractor/generated/graph.json.gz'))):
    graph = json.loads(gzip.decompress(data))
    del data
    for include_todo in (False, True):
        drift.setdefault('graph_independence', {})[f'{label}:{include_todo}'] = report(root, graph, unit_list, 'HEAD', include_todo)
    del graph
    gc.collect()
gi = drift.pop('graph_independence')
drift['accepted_graph_gives_same_drift'] = {str(t): gi[f'adopted:{t}'] == gi[f'accepted:{t}'] for t in (False, True)}
assert all(drift['accepted_graph_gives_same_drift'].values())
results['drift'] = drift

# 8. Scope against accepted rust: only owned graph/report/unit/docs/evidence paths differ.
changed = [x for x in git('diff', '--name-only', RUST, 'HEAD').splitlines()]
allowed = ('port/units/', 'port/extractor/generated/', 'port/findings/extractor-same-usr-adoption', 'rust/PORT_PLAN.md')
results['scope'] = {'changed_paths_vs_accepted': len(changed), 'outside_owned': [x for x in changed if not x.startswith(allowed)]}
assert not results['scope']['outside_owned'], results['scope']
results['graph_comparison'] = json.loads((M / 'graph-comparison.json').read_text())
(M / 'checks.json').write_text(json.dumps(results, sort_keys=True, indent=2) + '\n', encoding='utf-8', newline='\n')
print(json.dumps({k: v for k, v in results.items() if k not in ('identities', 'graph_comparison', 'units')}, indent=1)[:5000])
