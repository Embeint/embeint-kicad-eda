"""Package bounded freezer checks while preserving the qualified capture epoch."""
import ast
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

root = Path.cwd()
baseline = '6f02c95d98c1728e99238e92d54383d04c398216'
source = 'b9f5a76b5820945e8e06101718f380ff648b1422'
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip() == source
work = Path('//wsl.localhost/Ubuntu/home/karlvonrichter/kicad-port/freeze-symlink-preflight')
out = root / 'port/extractor/generated/evidence/freeze-symlink'
out.mkdir(exist_ok=False)
scripts = out / 'scripts'; scripts.mkdir()
sha = lambda data: hashlib.sha256(data).hexdigest()
blob = lambda rev, path: subprocess.check_output(['git', 'cat-file', 'blob', f'{rev}:{path}'])
def write(name, value):
    (out / name).write_bytes((json.dumps(value, indent=2, sort_keys=True) + '\n').encode())

files = {}
for relative in ['port/extractor/generated_sources.py', 'port/extractor/test_extractor.py',
                 'port/extractor/refresh_generator_dependencies.py']:
    expected = blob(source, relative)
    actual = (work / 'current' / relative).read_bytes().replace(b'\r\n', b'\n')
    assert actual == expected, relative
    files[relative] = sha(expected)
current = ast.parse(blob(source, 'port/extractor/generated_sources.py'))
old = ast.parse(blob(baseline, 'port/extractor/generated_sources.py'))
functions = lambda tree: {node.name: ast.dump(node) for node in tree.body if isinstance(node, ast.FunctionDef)}
current_functions, old_functions = functions(current), functions(old)
assert current_functions.keys() == old_functions.keys()
assert [name for name in current_functions if current_functions[name] != old_functions[name]] == ['main']
assert blob(source, 'port/extractor/refresh_generator_dependencies.py') == blob(baseline, 'port/extractor/refresh_generator_dependencies.py')
shutil.copyfile(work / 'tests-wsl.log', out / 'tests-wsl.log')
shutil.copyfile(work / 'old-control.log', out / 'old-control.log')
assert b'Ran 37 tests' in (out / 'tests-wsl.log').read_bytes()
assert (out / 'tests-wsl.log').read_bytes().rstrip().endswith(b'OK')
assert b'tool-manager: invoke through a tool symlink' in (out / 'old-control.log').read_bytes()
assert b'FAILED (failures=2)' in (out / 'old-control.log').read_bytes()
shutil.copyfile(root.parent.parent.parent / 'pr39-freeze-symlink-check.sh', scripts / 'validate.sh')
old_source = Path(__file__).with_name('old-generated_sources.py').read_bytes()
assert old_source == blob(baseline, 'port/extractor/generated_sources.py')
(scripts / 'old-generated_sources.py').write_bytes(old_source)
(scripts / 'package.py').write_bytes(Path(__file__).read_bytes())
write('source-identity.json', {
    'source_commit': source, 'reviewed_baseline': baseline,
    'tested_code_sha256_lf': files, 'changed_freezer_functions': ['main'],
    'guarded_refresh_script_unchanged': True, 'recipe_traversal_ast_unchanged': True,
    'wsl': {'tests': 37, 'skipped': 0, 'log': 'tests-wsl.log'},
    'old_control': {'fails_absolute_and_relative_path': True, 'log': 'old-control.log'},
    'windows_observed': {'tests': 37, 'passed': 25, 'expected_skips': 12,
                         'evidence': 'interactive session stdout; no separate retained Windows log'},
    'policy': 'No new capture, apply, split, AST replay, oracle build or numerical qualification. Retained capture runtime remains c1e8a467da.'})

index = json.loads(blob(baseline, 'port/extractor/generated/evidence/verified-ninja/index-artifact-audit.json'))
expected = {**index['artifact_sha256'], **index['unit_sha256_lf']}
process = subprocess.Popen(['git', 'cat-file', '--batch'], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
for relative, wanted in expected.items():
    process.stdin.write(('HEAD:' + relative + '\n').encode()); process.stdin.flush()
    header = process.stdout.readline().decode().split()
    assert len(header) == 3 and header[1] == 'blob', (relative, header)
    data = process.stdout.read(int(header[2])); assert process.stdout.read(1) == b'\n'
    assert sha(data) == wanted, relative
process.stdin.close(); assert process.wait() == 0
protected_paths = ['port/units', 'port/cases', 'port/goldens', 'rust', 'port/tools',
                   'port/extractor/refresh_generator_dependencies.py',
                   'port/extractor/generated/graph.json.gz', 'port/extractor/generated/inventory.json',
                   'port/extractor/generated/generated-sources.json', 'port/extractor/generated/measurement.json',
                   'port/extractor/generated/finalization.json', 'port/extractor/generated/evidence/verified-ninja',
                   'port/extractor/generated/evidence/source-guard', 'port/extractor/generated/evidence/generator-refresh',
                   'port/extractor/generated/corrected-extraction', 'port/extractor/generated/history']
assert not subprocess.check_output(['git', 'diff', '--name-only', baseline, '--', *protected_paths]), 'Protected paths changed'
write('preservation.json', {'baseline': baseline, 'indexed_records_unchanged': len(expected),
    'historical_index_sha256': sha(blob(baseline, 'port/extractor/generated/evidence/verified-ninja/index-artifact-audit.json')),
    'unit_files_unchanged': 1801, 'protected_paths_unchanged': protected_paths,
    'retained_capture_source': 'c1e8a467da149eac67341d31de0228e498916ba3',
    'retained_capture_refresh_code_sha256': '2b4eca5d431fd57fe824e54eaa384bea5215abe29cefa0cff9d9238269f4e929',
    'policy': 'Checks apply to the retained publication; its provenance has not been rebound to the newer CLI source.'})
lines = [f'{sha(p.read_bytes())}  ./{p.relative_to(out).as_posix()}\n'
         for p in sorted(out.rglob('*')) if p.is_file()]
(out / 'files.sha256').write_bytes(''.join(lines).encode('ascii'))
print(f'Packaged 37-test symlink proof; all {len(expected)} retained indexed records unchanged.')
