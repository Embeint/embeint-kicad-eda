"""Bind the immutable inputs of the guarded recapture and record code identities."""
import hashlib, json, pathlib, shutil, sys
N = pathlib.Path(sys.argv[1])
home = pathlib.Path.home() / 'kicad-port'
V5, OLD = home / 'corrected-review-v5', home / 'generator-refresh-57e'
ROOT = pathlib.Path('/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-04614140-10140941291c27ae')
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
lf = lambda paths: hashlib.sha256(b''.join(p.read_bytes().replace(b'\r\n', b'\n') for p in paths)).hexdigest()
expected = {
    'base_audit': 'bf7a1689587de011b7f2b8879e7d6284827a451ee43155ad72b8963fe30a6de6',
    'v5_manifest': 'a2466270bdf4aec52b7b8267140ac4b18c8295520c5b94e1ecb3dc8498dacb34',
    'v5_inventory': 'b9e6986b36529fdf4041712dfb258af050c2ff61bb3971875394eb16e2a47682',
    'v5_graph': '40861e9ca678464cedc513561e6e559cbcb5f902bb5b221fe2b6c378fb803e18',
    'canonical_graph': '146e2ccac0e8d0ba6d37ceadb26ae5a84d0223609dd55abebb0b5451bfe45735',
    'units_report': '0aac539054de1437248d5f97c510efee16de779cf546919c136a23938b023753',
    'v5_parser': 'e1548c1fe920a05d882e3f2de1c14843ca0e7b3b04ca9441d4132e16de77b88d',
    'libclang': '2c262ccd53ce702daa87c95aa84b2936e13c14f974504dc58f3ed93b12def23b'}
# The D2 pinned audit is used byte-exact for both capture and apply.
shutil.copyfile(ROOT / 'port/extractor/generated/evidence/source-guard/base-generation-input-audit.json', N / 'base-generation-input-audit.json')
assert sha(N / 'base-generation-input-audit.json') == expected['base_audit']
assert sha(ROOT / 'port/extractor/generated/evidence/generator-refresh/generation-input-audit.json') == expected['base_audit']
assert sha(V5 / 'inputs/generated-sources.json') == expected['v5_manifest']
assert sha(V5 / 'output/inventory.json') == expected['v5_inventory']
assert sha(V5 / 'output/graph.json.gz') == expected['v5_graph']
# Pre-refresh canonical graph/inventory/report: reproduced from V5 in the D2 workflow
# (canonical-from-v5.json) and independently twice; reuse those exact bytes read-only.
C = N / 'canonical-old'
if C.exists():
    shutil.rmtree(C)
C.mkdir(parents=True)
old = OLD / 'canonical-old/port/extractor/generated'
for name in ('graph.json.gz', 'inventory.json', 'units.json'):
    shutil.copyfile(old / name, C / name)
assert sha(C / 'graph.json.gz') == expected['canonical_graph']
assert sha(C / 'inventory.json') == expected['v5_inventory']
assert sha(C / 'units.json') == expected['units_report']
d2_canonical = json.loads((ROOT / 'port/extractor/generated/evidence/generator-refresh/canonical-from-v5.json').read_text())
assert all(r['graph_sha256'] == expected['canonical_graph'] and r['unit_files_equal_committed'] for r in d2_canonical['runs'].values())
d2_replay = json.loads((ROOT / 'port/extractor/generated/evidence/generator-refresh/v5-replay.json').read_text())
assert d2_replay['new_clang_parses'] == 0 and d2_replay['cache_reused_commands'] == 2511 and d2_replay['graph_semantics_equal']
parser = lambda root: lf([root / 'port/tools/extract_graph.py'] + [root / 'port/extractor' / n for n in ('clang_graph.py', 'model.py', 'generated_sources.py')])
cur = N / 'current'
record = {'inputs_sha256': expected,
          'v5_runtime_parser_sha256': parser(N / 'v5-runtime'),
          'current_four_file_hash_policy_sha256': parser(cur),
          'current_capture_refresh_code_sha256': lf([cur / 'port/extractor/refresh_generator_dependencies.py', cur / 'port/extractor/generated_sources.py']),
          'ast_files_equal_v5': {n: (N / 'v5-runtime/port/extractor' / n).read_bytes().replace(b'\r\n', b'\n') == (cur / 'port/extractor' / n).read_bytes().replace(b'\r\n', b'\n') for n in ('clang_graph.py', 'model.py')},
          'libclang_sha256': sha(pathlib.Path('/usr/lib/llvm-21/lib/libclang-21.so.1')),
          'reused_d2_proofs': {'v5-replay.json': d2_replay, 'canonical-from-v5.json': 'all runs reproduce 146e2cca and every unit file'},
          'policy': 'No new AST parse or V5 replay: the immutable D2 replay/reproduction proofs are reused after checking the hashes of every input they bind'}
assert record['v5_runtime_parser_sha256'] == expected['v5_parser'] and record['libclang_sha256'] == expected['libclang']
assert all(record['ast_files_equal_v5'].values())
(N / 'evidence/code-and-input-identities.json').write_text(json.dumps(record, sort_keys=True, indent=2) + '\n')
print(json.dumps({k: v for k, v in record.items() if k != 'reused_d2_proofs'}, indent=2))
