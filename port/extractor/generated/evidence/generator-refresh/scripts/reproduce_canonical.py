"""Reproduce the committed canonical graph/report/units from the immutable V5 parse."""
import hashlib, json, pathlib, shutil, subprocess, sys, tarfile
W = pathlib.Path(sys.argv[1])
home = pathlib.Path.home() / 'kicad-port'
V5 = home / 'corrected-review-v5'
PY = str(pathlib.Path.home() / 'kicad-port-extractor-venv/bin/python')
C = W / 'canonical-old'
G = C / 'port/extractor/generated'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
if C.exists():
    shutil.rmtree(C)
G.mkdir(parents=True)
with tarfile.open(W / 'units.tar') as tar:
    tar.extractall(C, filter='data')
committed = {p.name: p.read_bytes() for p in (C / 'port/units').glob('*.toml')}
assert len(committed) == 1801 and all(b'\r\n' not in b for b in committed.values())
shutil.copyfile(V5 / 'output/graph.json.gz', G / 'graph.json.gz')
shutil.copyfile(V5 / 'output/inventory.json', G / 'inventory.json')
shutil.copytree(V5 / 'inputs/sources', G / 'sources')
assert sha(G / 'graph.json.gz') == '40861e9ca678464cedc513561e6e559cbcb5f902bb5b221fe2b6c378fb803e18'
results = {}
for label, runtime in (('v5-runtime', W / 'v5-runtime'), ('current', W / 'current'), ('current-repeat', W / 'current')):
    out = subprocess.run([PY, str(runtime / 'port/tools/split_units.py'), '--graph', str(G / 'graph.json'),
                          '--inventory', str(G / 'inventory.json'), '--units-dir', str(C / 'port/units'),
                          '--report', str(G / 'units.json'), '--source-root', str(home / 'extract-source')],
                         check=True, capture_output=True, text=True)
    units = {p.name: p.read_bytes() for p in (C / 'port/units').glob('*.toml')}
    results[label] = {'stdout': out.stdout, 'graph_sha256': sha(G / 'graph.json.gz'),
                      'units_report_sha256': sha(G / 'units.json'), 'unit_files_equal_committed': units == committed,
                      'inventory_sha256': sha(G / 'inventory.json')}
    assert results[label]['graph_sha256'] == '146e2ccac0e8d0ba6d37ceadb26ae5a84d0223609dd55abebb0b5451bfe45735', results
    assert results[label]['units_report_sha256'] == '0aac539054de1437248d5f97c510efee16de779cf546919c136a23938b023753', results
    assert results[label]['inventory_sha256'] == 'b9e6986b36529fdf4041712dfb258af050c2ff61bb3971875394eb16e2a47682', results
    assert units == committed, label
(W / 'evidence/canonical-from-v5.json').write_text(json.dumps({
    'input_v5_graph_sha256': '40861e9ca678464cedc513561e6e559cbcb5f902bb5b221fe2b6c378fb803e18',
    'committed_unit_files': len(committed), 'runs': results,
    'interpretation': 'The V5 runtime splitter reproduces the committed pre-refresh canonical graph, report and every unit byte from the V5 parse; current split code then leaves them byte-identical.'},
    sort_keys=True, indent=2) + '\n')
print(json.dumps(results, indent=2))
