"""Compare the final-head capture with the retained verified-Ninja capture.

The verified relations must be preserved exactly: identical recipes, Ninja catalog,
build-generated generator inputs and source-tree generator dependencies, tool and
configuration identities and pinned source files. Only the inventory the capture is
bound to (the replay inventory) and the capture code identity may differ.
"""
import gzip, hashlib, json, pathlib, sys
N = pathlib.Path(sys.argv[1])
K = pathlib.Path.home() / 'kicad-port'
H = N / 'head/port/extractor/generated'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
new = json.loads((N / 'capture/generator-dependency-capture.json').read_text())
old = json.loads((H / 'evidence/verified-ninja/generator-dependency-capture.json').read_text())
identity = json.loads((N / 'evidence/identities.json').read_text())
differing = sorted(k for k in set(new) | set(old) if new.get(k) != old.get(k))
identity_diff = sorted(k for k in set(new['source_identity']) | set(old['source_identity'])
                       if new['source_identity'].get(k) != old['source_identity'].get(k))
raw = (N / 'capture/generated-recipes.json').read_bytes()
retained_catalog = K / 'generator-refresh-ninja-c1e8a467/capture/ninja-targets.txt'
result = {
    'new_capture_sha256': sha(N / 'capture/generator-dependency-capture.json'),
    'retained_capture_sha256': sha(H / 'evidence/verified-ninja/generator-dependency-capture.json'),
    'differing_fields': differing, 'differing_source_identity_fields': identity_diff,
    'source_identity_inventory_sha256': [old['source_identity']['inventory_sha256'], new['source_identity']['inventory_sha256']],
    'capture_code_sha256': [old['capture_code_sha256'], new['capture_code_sha256']],
    'capture_code_equals_final_head_refresh_code': new['capture_code_sha256'] == identity['capture_refresh_code_sha256'],
    'replay_inventory_sha256': sha(N / 'replay/inventory.json'),
    'recipes_raw_equal_to_committed': raw == gzip.decompress((H / 'generated-recipes.json.gz').read_bytes()),
    'ninja_catalog_equal_to_retained': (N / 'capture/ninja-targets.txt').read_bytes() == retained_catalog.read_bytes(),
    'generator_inputs_equal': new['generator_inputs'] == old['generator_inputs'],
    'source_generator_dependencies_equal': new['source_generator_dependencies'] == old['source_generator_dependencies'],
    'tool_and_configuration_equal': new['tool_and_configuration_sha256'] == old['tool_and_configuration_sha256'],
    'source_files_equal': new['source_identity']['files'] == old['source_identity']['files'],
    'recipe_records': new['recipe_records'], 'ninja_target_records': new['ninja_target_records'],
    'verified_source_files': new['source_identity']['verified_source_files'],
    'build_files_unchanged': new['build_files_unchanged'],
    'frozen_generated_inputs_verified': new['frozen_generated_inputs_verified'],
}
del raw
(N / 'evidence/capture-comparison.json').write_text(json.dumps(result, sort_keys=True, indent=2) + '\n')
print(json.dumps(result, sort_keys=True, indent=2))
assert set(differing) == {'source_identity', 'capture_code_sha256'}, differing
assert identity_diff == ['inventory_sha256'], identity_diff
assert new['source_identity']['inventory_sha256'] == result['replay_inventory_sha256']
assert all(result[k] for k in ('recipes_raw_equal_to_committed', 'ninja_catalog_equal_to_retained', 'generator_inputs_equal',
                               'source_generator_dependencies_equal', 'tool_and_configuration_equal', 'source_files_equal',
                               'capture_code_equals_final_head_refresh_code'))
