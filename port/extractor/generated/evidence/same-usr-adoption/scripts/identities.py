"""Bind the immutable inputs of the adoption and record the final code identities."""
import hashlib, json, pathlib, sys
N = pathlib.Path(sys.argv[1])
K = pathlib.Path.home() / 'kicad-port'
V5 = K / 'corrected-review-v5'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
lf = lambda paths: hashlib.sha256(b''.join(p.read_bytes().replace(b'\r\n', b'\n') for p in paths)).hexdigest()
four = lambda root: lf([root / 'port/tools/extract_graph.py'] + [root / 'port/extractor' / n for n in
                        ('clang_graph.py', 'model.py', 'generated_sources.py')])
expected = {
    'v5_manifest': 'a2466270bdf4aec52b7b8267140ac4b18c8295520c5b94e1ecb3dc8498dacb34',
    'v5_inventory': 'b9e6986b36529fdf4041712dfb258af050c2ff61bb3971875394eb16e2a47682',
    'v5_graph': '40861e9ca678464cedc513561e6e559cbcb5f902bb5b221fe2b6c378fb803e18',
    'v5_parser': 'e1548c1fe920a05d882e3f2de1c14843ca0e7b3b04ca9441d4132e16de77b88d',
    'source_manifest': '9fd04872118352d8ba6e54bb6c799a3087c2a0bf4d681dbb7afce07b7464eca9',
    'compile_commands': 'bc402ade70f0304ed5018793a5d8c37749df978dfd7859ee87e655ee3a616196',
    'libclang': '2c262ccd53ce702daa87c95aa84b2936e13c14f974504dc58f3ed93b12def23b',
    'base_audit': 'bf7a1689587de011b7f2b8879e7d6284827a451ee43155ad72b8963fe30a6de6',
    'head_canonical_graph': '014f05141168b634cd4ef39a9cea434b696ae2d6d6230048e41af3f31738f342',
    'head_canonical_inventory': '41600e43a380594f88e7e9747398e01458c1e71e436d59c31ca23f92dfb603ed',
    'head_recipes_gzip': 'f5a91a1c6f5e2357aeb1db4f6499be3ba7bca7cc3b46fea793c74de42257064e',
    'pr79_replay_graph': 'c3b15c81ae167c540d1bfffe84a5eef3c5616291e65553557a53cac73ff0db54',
}
H = N / 'head/port/extractor/generated'
actual = {
    'v5_manifest': sha(V5 / 'inputs/generated-sources.json'),
    'v5_inventory': sha(V5 / 'output/inventory.json'),
    'v5_graph': sha(V5 / 'output/graph.json.gz'),
    'v5_parser': four(V5 / 'runner'),
    'source_manifest': sha(K / 'source-manifest.json'),
    'compile_commands': sha(K / 'reconciled-output/compile_commands.json'),
    'libclang': sha(pathlib.Path('/usr/lib/llvm-21/lib/libclang-21.so.1')),
    'base_audit': sha(H / 'evidence/verified-ninja/base-generation-input-audit.json'),
    'head_canonical_graph': sha(H / 'graph.json.gz'),
    'head_canonical_inventory': sha(H / 'inventory.json'),
    'head_recipes_gzip': sha(H / 'generated-recipes.json.gz'),
    'pr79_replay_graph': sha(K / 'shape-collision/replay/graph.json.gz'),
}
assert actual == expected, {k: (actual[k], expected[k]) for k in expected if actual[k] != expected[k]}
runner = N / 'runner'
record = {
    'head': '4738a9c23f8c6068784d26ba991a690044343892',
    'inputs_sha256': actual,
    'aggregation_extractor_sha256': four(runner),
    'parse_runtime_extractor_sha256': four(V5 / 'runner'),
    'parser_clang_graph_equal_to_v5': (runner / 'port/extractor/clang_graph.py').read_bytes().replace(b'\r\n', b'\n')
                                      == (V5 / 'runner/port/extractor/clang_graph.py').read_bytes().replace(b'\r\n', b'\n'),
    'capture_refresh_code_sha256': lf([runner / 'port/extractor/refresh_generator_dependencies.py',
                                       runner / 'port/extractor/generated_sources.py']),
    'splitter_sha256': sha(runner / 'port/tools/split_units.py'),
    'v5_cache_files': sum(1 for p in (V5 / 'cache').iterdir() if p.is_file()),
}
assert record['parser_clang_graph_equal_to_v5']
(N / 'evidence/identities.json').write_text(json.dumps(record, sort_keys=True, indent=2) + '\n')
print(json.dumps(record, sort_keys=True, indent=2))
