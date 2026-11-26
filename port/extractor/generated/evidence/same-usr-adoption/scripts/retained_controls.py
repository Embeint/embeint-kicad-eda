"""Controls for the retained verified-Ninja capture under the final-head refresh code.

1. The retained capture binds the V5 inventory bytes (b9e6986b); applying it to the
   replay's canonical graph must be refused by the source-identity guard.
2. Applying it to the original pre-refresh canonical graph (146e2cca) must still
   reproduce the committed canonical graph, inventory and manifest; only the
   refresh code identity may differ.
"""
import gc, gzip, hashlib, json, pathlib, sys
N = pathlib.Path(sys.argv[1])
K = pathlib.Path.home() / 'kicad-port'
V5 = K / 'corrected-review-v5'
sys.path[:0] = [str(N / 'runner/port'), str(N / 'runner/port/tools')]
from extractor import refresh_generator_dependencies as r
H = N / 'head/port/extractor/generated'
E = H / 'evidence/verified-ninja'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
capture, recipes, audit = E / 'generator-dependency-capture.json', H / 'generated-recipes.json.gz', E / 'base-generation-input-audit.json'
manifest = V5 / 'inputs/generated-sources.json'
retained = json.loads(capture.read_text())
results = {'retained_capture_sha256': sha(capture),
           'retained_capture_inventory_sha256': retained['source_identity']['inventory_sha256'],
           'retained_capture_code_sha256': retained['capture_code_sha256']}
pre = N / 'pre/port/extractor/generated'
try:
    r.apply(pre / 'graph.json.gz', pre / 'inventory.json', manifest, capture, recipes, N / 'control-retained-on-replay',
            generation_inputs_path=audit)
except ValueError as error:
    results['retained_on_replay'] = {'rejected': True, 'error': str(error),
                                     'replay_inventory_sha256': sha(pre / 'inventory.json')}
else:
    raise AssertionError('retained capture was accepted for the replay inventory')
assert 'Capture source identity differs from the original parse' in results['retained_on_replay']['error']
assert not (N / 'control-retained-on-replay/graph.json.gz').exists()
gc.collect()

old = K / 'generator-refresh-ninja-c1e8a467/canonical-old'
assert sha(old / 'graph.json.gz') == '146e2ccac0e8d0ba6d37ceadb26ae5a84d0223609dd55abebb0b5451bfe45735'
assert sha(old / 'inventory.json') == 'b9e6986b36529fdf4041712dfb258af050c2ff61bb3971875394eb16e2a47682'
out = N / 'control-retained-on-v5'
r.apply(old / 'graph.json.gz', old / 'inventory.json', manifest, capture, recipes, out, generation_inputs_path=audit)
gc.collect()
strip = lambda g: (g['provenance']['generator_dependency_refresh'].pop('refresh_code_sha256'), g)[1]
a = json.loads(gzip.decompress((out / 'graph.json.gz').read_bytes()))
new_code = a['provenance']['generator_dependency_refresh']['refresh_code_sha256']
a = hashlib.sha256(json.dumps(strip(a), sort_keys=True).encode()).hexdigest()
gc.collect()
b = json.loads(gzip.decompress((H / 'graph.json.gz').read_bytes()))
old_code = b['provenance']['generator_dependency_refresh']['refresh_code_sha256']
b = hashlib.sha256(json.dumps(strip(b), sort_keys=True).encode()).hexdigest()
gc.collect()
inv_a, inv_b = json.loads((out / 'inventory.json').read_text()), json.loads((H / 'inventory.json').read_text())
for inv in (inv_a, inv_b):
    inv['provenance']['generator_dependency_refresh'].pop('refresh_code_sha256')
results['retained_on_v5'] = {
    'graph_equal_to_committed_except_refresh_code': a == b,
    'inventory_equal_to_committed_except_refresh_code': inv_a == inv_b,
    'manifest_byte_identical_to_committed': sha(out / 'generated-sources.json') == sha(H / 'generated-sources.json'),
    'committed_refresh_code_sha256': old_code, 'final_head_refresh_code_sha256': new_code}
del inv_a, inv_b
assert all(v for k, v in results['retained_on_v5'].items() if not k.endswith('sha256')), results
(N / 'evidence/retained-capture-controls.json').write_text(json.dumps(results, sort_keys=True, indent=2) + '\n')
print(json.dumps(results, sort_keys=True, indent=2))
