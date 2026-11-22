"""Replay the exact V5 cache with the frozen V5 runtime; refuse parses and cache writes."""
import gc, gzip, hashlib, json, pathlib, sys
W = pathlib.Path(sys.argv[1])
runtime = W / 'v5-runtime'
sys.path[:0] = [str(runtime / 'port'), str(runtime / 'port/tools')]
import extract_graph
base = pathlib.Path.home() / 'kicad-port'
run = base / 'corrected-review-v5'
out = W / 'replay'


class NoParsePool:
    def __init__(self, *a, **kw): pass
    def __enter__(self): return self
    def __exit__(self, *a): return False
    def submit(self, *a, **kw): raise AssertionError('A cache miss would require a fresh clang parse')
    def shutdown(self, *a, **kw): pass


extract_graph.ProcessPoolExecutor = NoParsePool
overlay = run / 'cache/source-overlay.json'
overlay_before = overlay.read_bytes()
original_write = extract_graph.write_json


def guarded_write(path, value):
    if path == overlay:
        assert json.loads(overlay_before) == value and path.read_bytes() == overlay_before
        return
    assert path.resolve().is_relative_to(out.resolve()), path
    original_write(path, value)


def reject_write(*a, **kw): raise AssertionError('Attempted write to the V5 cache')


extract_graph.write_json = guarded_write
extract_graph.write_cache = reject_write
sha = lambda b: hashlib.sha256(b).hexdigest()
cache_hashes = lambda: {p.name: sha(p.read_bytes()) for p in (run / 'cache').iterdir() if p.is_file()}
before = cache_hashes()
manifest_before = sha((run / 'inputs/generated-sources.json').read_bytes())
sys.argv = ['extract_graph.py', '--compile-commands', str(base / 'reconciled-output/compile_commands.json'),
            '--source-root', str(base / 'extract-source'), '--original-source-root', str(base / 'extract-source'),
            '--manifest', str(base / 'source-manifest.json'),
            '--generated-manifest', str(run / 'inputs/generated-sources.json'),
            '--cache', str(run / 'cache'), '--output', str(out), '--jobs', '8']
assert extract_graph.main() == 1
assert before == cache_hashes() and overlay.read_bytes() == overlay_before
assert sha((run / 'inputs/generated-sources.json').read_bytes()) == manifest_before == 'a2466270bdf4aec52b7b8267140ac4b18c8295520c5b94e1ecb3dc8498dacb34'


def load(p):
    with (gzip.open(p, 'rt') if p.suffix == '.gz' else p.open()) as f:
        return json.load(f)


key = lambda x: json.dumps(x, sort_keys=True)
original, replay = load(run / 'output/graph.json.gz'), load(out / 'graph.json.gz')
assert original['provenance']['cache_reused_commands'] == 0
assert replay['provenance']['cache_reused_commands'] == 2511
assert sorted(original['provenance']['parse_failures'], key=key) == sorted(replay['provenance']['parse_failures'], key=key)
order_equal = original['provenance']['parse_failures'] == replay['provenance']['parse_failures']
for g in (original, replay):
    g['provenance'].pop('cache_reused_commands')
    g['provenance']['parse_failures'].sort(key=key)
    for t in g['translation_units']:
        t.pop('cache_reused_from', None)
assert original == replay, 'Semantic graph mismatch beyond declared cache-trace fields and failure order'
functions, edges, failures = len(original['functions']), len(original['edges']), len(original['provenance']['parse_failures'])
extractor = original['provenance']['extractor_sha256']
del original, replay
gc.collect()
a, b = load(run / 'output/inventory.json'), load(out / 'inventory.json')
for inv in (a, b):
    inv['provenance'].pop('cache_reused_commands')
    inv['provenance']['parse_failures'].sort(key=key)
assert a == b
report = {'runtime': 'frozen V5 runner (parser e1548c1f...), exact V5 manifest, compile database and cache',
          'cache_artifacts_unchanged': len(before), 'cache_reused_commands': 2511, 'new_clang_parses': 0,
          'graph_semantics_equal': True, 'inventory_semantics_equal': True,
          'failure_reporting_order_equal': order_equal, 'functions': functions, 'edges': edges,
          'parse_failures': failures, 'parse_extractor_sha256': extractor,
          'excluded_fields': ['provenance.cache_reused_commands', 'translation_units[].cache_reused_from'],
          'failure_comparison': 'complete records sorted by complete JSON content; no payload omitted'}
(W / 'evidence/v5-replay.json').write_text(json.dumps(report, sort_keys=True, indent=2) + '\n')
print(json.dumps(report))
