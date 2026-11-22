"""Prove the guarded recapture changes only source attestation and derived identities vs D2."""
import filecmp, gc, gzip, hashlib, json, pathlib, sys
N = pathlib.Path(sys.argv[1])
OLD = pathlib.Path.home() / 'kicad-port/generator-refresh-57e'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()


def load(p):
    with (gzip.open(p, 'rt') if p.suffix == '.gz' else p.open()) as f:
        return json.load(f)


result = {}
# Raw capture outputs.
assert sha(N / 'capture/generated-recipes.json') == 'f44049e2ea0a2ad8b1a16133e5c6bc25df084366d07995b7369d0d44fd9d1900'
assert filecmp.cmp(N / 'capture/generated-recipes.json', OLD / 'capture/generated-recipes.json', shallow=False)
assert filecmp.cmp(N / 'capture/ninja-targets.txt', OLD / 'capture/ninja-targets.txt', shallow=False)
new, old = load(N / 'capture/generator-dependency-capture.json'), load(OLD / 'capture/generator-dependency-capture.json')
assert sha(OLD / 'capture/generator-dependency-capture.json') == 'f4126f134144f18f35b849e22dc44984d23a52e66aec4a372b262d8eb57220d1'
assert new['generator_inputs'] == old['generator_inputs']
assert new['source_generator_dependencies'] == old['source_generator_dependencies']
changed = sorted(k for k in set(new) | set(old) if new.get(k) != old.get(k))
assert changed == ['capture_code_sha256', 'source_identity'], changed
identity = new['source_identity']
assert identity['source_root'] == old['source_root'] and identity['verified_source_files'] == len(identity['files'])
result['capture'] = {'recipes_sha256': new['recipes_sha256'], 'recipes_byte_identical_to_d2': True,
                     'ninja_catalog_byte_identical_to_d2': True, 'relation_maps_equal_d2': True,
                     'changed_capture_fields': changed, 'd2_capture_code_sha256': old['capture_code_sha256'],
                     'capture_code_sha256': new['capture_code_sha256'],
                     'source_identity': {k: v for k, v in identity.items() if k != 'files'}}
del new, old
gc.collect()

# Applied outputs against the published D2 refresh.
NR, OR = N / 'refreshed', OLD / 'refreshed'
assert sha(OR / 'graph.json.gz') == '8c39a7f481663b011fb37404004874b781aa77a0e35397b8f563d08417f498c8'
g_new, g_old = load(NR / 'graph.json.gz'), load(OR / 'graph.json.gz')
r_new, r_old = g_new['provenance'].pop('generator_dependency_refresh'), g_old['provenance'].pop('generator_dependency_refresh')
m_new, m_old = g_new['provenance'].pop('generated_sources_manifest_sha256'), g_old['provenance'].pop('generated_sources_manifest_sha256')
assert g_new == g_old, 'graph differs from D2 beyond refresh record and manifest fingerprint'
record_changes = sorted(k for k in set(r_new) | set(r_old) if r_new.get(k) != r_old.get(k))
assert record_changes == ['capture_code_sha256', 'capture_sha256', 'refresh_code_sha256', 'source_identity'], record_changes
del g_new, g_old
gc.collect()
i_new, i_old = load(NR / 'inventory.json'), load(OR / 'inventory.json')
assert i_new['files'] == i_old['files']
for inv in (i_new, i_old):
    inv['provenance'].pop('generator_dependency_refresh'); inv['provenance'].pop('generated_sources_manifest_sha256')
assert i_new == i_old
del i_new, i_old
mf_new, mf_old = load(NR / 'generated-sources.json'), load(OR / 'generated-sources.json')
mr_new, mr_old = mf_new.pop('generator_dependency_refresh'), mf_old.pop('generator_dependency_refresh')
assert mf_new == mf_old
rep_new, rep_old = load(NR / 'refresh-report.json'), load(OR / 'refresh-report.json')
for rep in (rep_new, rep_old):
    rep.pop('refresh'); rep.pop('refreshed_manifest_sha256')
assert rep_new == rep_old
result['applied'] = {
    'graph_equal_d2_except': ['provenance.generator_dependency_refresh', 'provenance.generated_sources_manifest_sha256'],
    'refresh_record_changed_fields': record_changes,
    'inventory_files_equal_d2': True, 'manifest_equal_d2_except': ['generator_dependency_refresh'],
    'refresh_report_equal_d2_except': ['refresh', 'refreshed_manifest_sha256'],
    'refreshed_manifest_sha256': m_new, 'd2_refreshed_manifest_sha256': m_old,
    'source_inventory_sha256': r_new['parse_source_inventory_sha256'] and load(NR / 'inventory.json')['provenance']['source_inventory_sha256'],
    'graph_sha256': sha(NR / 'graph.json.gz'), 'inventory_sha256': sha(NR / 'inventory.json'),
    'd2_graph_sha256': sha(OR / 'graph.json.gz'), 'd2_inventory_sha256': sha(OR / 'inventory.json'),
    'parse_identities_unchanged': {k: r_new[k] for k in ('parse_extractor_sha256', 'parse_source_inventory_sha256',
                                                       'parse_generated_sources_manifest_sha256', 'parse_recipes_sha256',
                                                       'input_graph_sha256', 'input_inventory_sha256')},
    'manifest_refresh_record': mr_new}
assert all(r_new[k] == r_old[k] for k in result['applied']['parse_identities_unchanged'])
(N / 'evidence/d2-equivalence.json').write_text(json.dumps(result, sort_keys=True, indent=2) + '\n')
print(json.dumps(result, indent=1)[:4000])
