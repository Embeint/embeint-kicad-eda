"""Windows checks after adoption: suites, identities, pure-apply and split replays, records and metrics."""
import gzip, hashlib, json, os, re, runpy, shutil, subprocess, sys, tarfile, tempfile, tomllib
from pathlib import Path
root = Path(r'C:\Users\KarlVonRichter\.t3\worktrees\embeint-kicad-eda\t3-290c6870-81dcfa65b1526978')
os.chdir(root)
N = Path(r'\\wsl.localhost\Ubuntu\home\karlvonrichter\kicad-port\canonical-adopt-4738a9c2')
G = root / 'port/extractor/generated'
E = G / 'evidence/same-usr-adoption'
R = G / 'replay-extraction'
BASE = '4738a9c23f8c6068784d26ba991a690044343892'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
lf = lambda b: hashlib.sha256(b.replace(b'\r\n', b'\n')).hexdigest()
blob = lambda rev, path: subprocess.check_output(['git', 'cat-file', 'blob', f'{rev}:{path}'])
results = {'base': BASE}

# 1. Suites on Windows; WSL suite from the workflow log.
for name, command in [
        ('extractor-windows', [sys.executable, '-m', 'unittest', 'discover', '-s', 'port/extractor', '-p', 'test_*.py', '-v']),
        ('progress-tests', [sys.executable, '-m', 'unittest', 'discover', '-s', 'port/site', '-p', 'test_*.py', '-v']),
        ('bootstrap-tests', [sys.executable, '-m', 'unittest', 'discover', '-s', 'port/tools', '-p', 'test_*.py', '-v']),
        ('policy-tests', [sys.executable, '-m', 'unittest', 'discover', '-s', 'rust/kicad-difftest/tests', '-p', 'test_policy.py', '-v']),
        ('progress-build', [sys.executable, 'port/tools/progress.py', '--out', r'C:\Users\KarlVonRichter\.t3\canon-adopt\site']),
        ('golden-changes', [sys.executable, 'port/tools/check_goldens_policy.py', '--base', 'origin/rust'])]:
    with (E / f'{name}.log').open('w', encoding='utf-8', newline='\n') as log:
        code = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT).returncode
    text = (E / f'{name}.log').read_text(encoding='utf-8')
    results[name] = {'command': command[1:], 'exit_code': code}
    count, skipped = re.search(r'Ran (\d+) tests?', text), re.search(r'OK \(skipped=(\d+)\)', text)
    if count:
        results[name].update(tests=int(count[1]), skipped=int(skipped[1]) if skipped else 0)
    print(name, results[name], flush=True)
    assert code == 0, (name, code)
wsl = gzip.decompress((E / 'logs/extractor-tests-wsl.log.gz').read_bytes()).decode()
count = re.search(r'Ran (\d+) tests', wsl)
assert count and wsl.rstrip().endswith('OK') and 'skipped' not in wsl
results['extractor-wsl'] = {'tests': int(count[1]), 'skipped': 0, 'log': 'logs/extractor-tests-wsl.log.gz'}

# 2. Identities.
identity = json.loads((E / 'identities.json').read_text())
graph = json.loads(gzip.decompress((G / 'graph.json.gz').read_bytes()))
p = graph['provenance']
r = p['generator_dependency_refresh']
inventory = json.loads((G / 'inventory.json').read_text())
assert inventory['provenance'] == p
replay_inventory = json.loads((R / 'inventory.json').read_text())
four = lambda: lf(b''.join((root / x).read_bytes() for x in ('port/tools/extract_graph.py', 'port/extractor/clang_graph.py',
                                                             'port/extractor/model.py', 'port/extractor/generated_sources.py')))
code = lf(b''.join((root / 'port/extractor' / n).read_bytes() for n in ('refresh_generator_dependencies.py', 'generated_sources.py')))
replay = p['parse_cache_replay']
checks = {
    'parse_extractor_is_v5': p['extractor_sha256'] == r['parse_extractor_sha256'] == identity['inputs_sha256']['v5_parser'],
    'aggregation_identity_is_head': p['aggregation_extractor_sha256'] == replay['aggregation_extractor_sha256']
                                    == identity['aggregation_extractor_sha256'] == four(),
    'zero_parses_all_reused': replay['parses'] == 0 and p['cache_reused_commands'] == p['commands_attempted'] == 2511,
    'refresh_and_capture_code_is_head': r['refresh_code_sha256'] == r['capture_code_sha256'] == code,
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
}
results['identities'] = {'checks': checks, 'aggregation_extractor_sha256': p['aggregation_extractor_sha256'],
                         'parse_extractor_sha256': p['extractor_sha256'], 'refresh': r,
                         'canonical_graph_sha256': sha(G / 'graph.json.gz'), 'canonical_inventory_sha256': sha(G / 'inventory.json'),
                         'refreshed_manifest_sha256': p['generated_sources_manifest_sha256'],
                         'replay_graph_sha256': sha(R / 'graph.json.gz'), 'replay_inventory_sha256': sha(R / 'inventory.json')}
assert all(checks.values()), checks
del graph, inventory, replay_inventory

# 3. Pure apply and canonical split on Windows.
with tempfile.TemporaryDirectory() as tmp:
    tmp = Path(tmp)
    for name in ('graph.json.gz', 'inventory.json'):
        shutil.copyfile(N / 'pre/port/extractor/generated' / name, tmp / name)
    out = subprocess.run([sys.executable, 'port/extractor/refresh_generator_dependencies.py', 'apply', '--graph', str(tmp / 'graph.json.gz'),
                          '--inventory', str(tmp / 'inventory.json'), '--base-manifest', str(G / 'corrected-extraction/generated-sources.json'),
                          '--capture', str(E / 'generator-dependency-capture.json'), '--recipes', str(G / 'generated-recipes.json.gz'),
                          '--generation-inputs', str(G / 'evidence/verified-ninja/base-generation-input-audit.json'),
                          '--output', str(tmp / 'out')], capture_output=True, text=True)
    assert out.returncode == 0, out.stderr
    replay_apply = {name: sha(tmp / 'out' / name) == sha(G / name) for name in ('graph.json.gz', 'inventory.json', 'generated-sources.json')}
    replay_apply['refresh-report.json'] = sha(tmp / 'out/refresh-report.json') == sha(E / 'refresh-report.json')
    assert all(replay_apply.values()), replay_apply
    results['windows_refresh_cli_replay'] = dict(replay_apply, pre_refresh_graph_sha256=sha(tmp / 'graph.json.gz'))
    regen = tmp / 'regen'
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
    results['windows_canonical_regeneration'] = {'iterations': 2, 'byte_identical_artifacts': len(before)}

# 4. Unit records: only depends_on/soft_depends_on/layer may differ from the base; solidcore loader.
units = {path.stem: tomllib.loads(path.read_text(encoding='utf-8')) for path in (root / 'port/units').glob('*.toml')}
base_units = {}
for name in subprocess.check_output(['git', 'ls-tree', '--name-only', BASE, 'port/units/'], text=True).split():
    if name.endswith('.toml'):
        base_units[Path(name).stem] = tomllib.loads(blob(BASE, name).decode())
assert set(units) == set(base_units) and len(units) == 1801
changed_keys = {}
for uid in units:
    for k in set(units[uid]) | set(base_units[uid]):
        if units[uid].get(k) != base_units[uid].get(k):
            changed_keys[k] = changed_keys.get(k, 0) + 1
assert set(changed_keys) <= {'depends_on', 'soft_depends_on', 'layer'}, changed_keys
non_todo = {u: x['status'] for u, x in units.items() if x['status'] not in ('todo', 'blocked')}
non_todo_changed = {u: sorted(k for k in ('depends_on', 'soft_depends_on', 'layer') if units[u][k] != base_units[u][k]) for u in non_todo}
reference = Path(r'C:\Users\KarlVonRichter\.t3\phase0-solidcore-reference')
assert subprocess.check_output(['git', '-C', str(reference), 'rev-parse', 'HEAD'], text=True).strip() == '7a588fa284765a83823293ae20b84da8356ab479'
loader = runpy.run_path(str(reference / 'harness/plan/units.py'))
for uid, unit in units.items():
    assert set(loader['FIELDS']) <= set(unit) and loader['parse_unit'](loader['dump_unit'](unit)) == unit
results['units'] = {'count': len(units), 'changed_key_counts': changed_keys,
                    'changed_units': sum(units[u] != base_units[u] for u in units),
                    'statuses': {s: sum(u['status'] == s for u in units.values()) for s in sorted({u['status'] for u in units.values()})},
                    'statuses_equal_base': all(units[u]['status'] == base_units[u]['status'] for u in units),
                    'non_todo_units': non_todo, 'non_todo_dependency_or_layer_changes': {u: v for u, v in non_todo_changed.items() if v},
                    'solidcore_roundtrip_units': len(units)}

# 5. Measurement and blob limits.
sys.path.insert(0, str(root / 'port/tools'))
from extraction_report import measure_units
measurement = json.loads((G / 'measurement.json').read_text())
assert measure_units(list(units.values()), json.loads((G / 'units.json').read_text())) == measurement['units']
for record in measurement['canonical']['artifacts']:
    assert sha(G / record['filename']) == record['sha256'] and (G / record['filename']).stat().st_size == record['bytes']
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
results['head'] = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
(E / 'checks.json').write_text(json.dumps(results, sort_keys=True, indent=2) + '\n', encoding='utf-8', newline='\n')
print(json.dumps({k: v for k, v in results.items() if k not in ('identities',)}, indent=1)[:6000])
