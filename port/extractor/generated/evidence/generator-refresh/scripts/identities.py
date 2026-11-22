import hashlib, json, pathlib, sys
W = pathlib.Path(sys.argv[1])
lf = lambda paths: hashlib.sha256(b''.join(p.read_bytes().replace(b'\r\n', b'\n') for p in paths)).hexdigest()
def parser(root):
    return lf([root / 'port/tools/extract_graph.py'] + [root / 'port/extractor' / n for n in ('clang_graph.py', 'model.py', 'generated_sources.py')])
v5, cur = W / 'v5-runtime', W / 'current'
record = {
    'v5_runtime_parser_sha256': parser(v5),
    'current_parser_hash_policy_sha256': parser(cur),
    'current_refresh_code_sha256': lf([cur / 'port/extractor/refresh_generator_dependencies.py', cur / 'port/extractor/generated_sources.py']),
    'v5_ast_files_equal_current': {n: (v5 / 'port/extractor' / n).read_bytes().replace(b'\r\n', b'\n') == (cur / 'port/extractor' / n).read_bytes().replace(b'\r\n', b'\n')
                                   for n in ('clang_graph.py', 'model.py')},
    'libclang_sha256': hashlib.sha256(pathlib.Path('/usr/lib/llvm-21/lib/libclang-21.so.1').read_bytes()).hexdigest(),
}
assert record['v5_runtime_parser_sha256'] == 'e1548c1fe920a05d882e3f2de1c14843ca0e7b3b04ca9441d4132e16de77b88d', record
assert record['libclang_sha256'] == '2c262ccd53ce702daa87c95aa84b2936e13c14f974504dc58f3ed93b12def23b', record
(W / 'evidence/code-identities.json').write_text(json.dumps(record, sort_keys=True, indent=2) + '\n')
print(json.dumps(record, indent=2))
