"""Compare the final-head replay with the V5 original AST graph and the PR79 replay.

Graphs are loaded one at a time; only digests, key sets and edge sets are retained.
"""
import gc, gzip, hashlib, json, pathlib, sys
from collections import Counter
N = pathlib.Path(sys.argv[1])
K = pathlib.Path.home() / 'kicad-port'
SHAPE = 'c:@S@SHAPE@F@~SHAPE#'
VRML_BODY = SHAPE + '@definition:861533d9c907d7fb'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
canon = lambda v: hashlib.sha256(json.dumps(v, sort_keys=True).encode()).hexdigest()
load = lambda p: json.loads(gzip.decompress(p.read_bytes()))


def summarize(path):
    g = load(path)
    out = {'sha256': sha(path), 'provenance': g.pop('provenance'),
           'edges': set(map(tuple, g.pop('edges'))), 'overrides': set(map(tuple, g.pop('overrides')))}
    f = g.pop('functions')
    out['functions'] = {k: canon(v) for k, v in f.items()}
    out['function_files'] = {k: v.get('file') for k, v in f.items()}
    out['wx'] = {k: v.get('wx_class') for k, v in f.items()}
    out['copies'] = g.get('definition_copies', {})
    out['tu_without_cache_trace'] = canon([{k: v for k, v in t.items() if k != 'cache_reused_from'}
                                           for t in g.pop('translation_units', [])])
    out['rest'] = {k: canon(v) for k, v in g.items()}
    del f, g
    gc.collect()
    return out


new = summarize(N / 'replay/graph.json.gz')
pr79 = summarize(K / 'shape-collision/replay/graph.json.gz')
volatile = {'aggregation_extractor_sha256', 'parse_cache_replay'}
prov_diff = sorted(k for k in set(new['provenance']) | set(pr79['provenance'])
                   if new['provenance'].get(k) != pr79['provenance'].get(k))
pr79_cmp = {'pr79_replay_graph_sha256': pr79['sha256'],
            'provenance_differences': prov_diff,
            'parse_cache_replay_differences': sorted(k for k in new['provenance']['parse_cache_replay']
                                                     if new['provenance']['parse_cache_replay'][k]
                                                     != pr79['provenance']['parse_cache_replay'].get(k)),
            'edges_equal': new['edges'] == pr79['edges'], 'overrides_equal': new['overrides'] == pr79['overrides'],
            'functions_equal': new['functions'] == pr79['functions'],
            'translation_units_equal_excluding_cache_trace': new['tu_without_cache_trace'] == pr79['tu_without_cache_trace'],
            'other_top_level_equal': new['rest'] == pr79['rest']}
pr79_cmp['semantics_equal_except_aggregation_identity'] = (
    set(prov_diff) <= volatile and pr79_cmp['edges_equal'] and pr79_cmp['overrides_equal']
    and pr79_cmp['functions_equal'] and pr79_cmp['other_top_level_equal']
    and pr79_cmp['translation_units_equal_excluding_cache_trace'])
del pr79
gc.collect()

old = summarize(K / 'corrected-review-v5/output/graph.json.gz')
removed, added = sorted(old['edges'] - new['edges']), sorted(new['edges'] - old['edges'])
copies = old['copies']
bodies = set(copies) | set(copies.values())
usr = lambda k: copies.get(k, k)
v5_cmp = {
    'v5_graph_sha256': old['sha256'], 'replay_graph_sha256': new['sha256'],
    'functions_equal': old['functions'] == new['functions'],
    'wx_class_changes': sum(old['wx'][k] != new['wx'][k] for k in old['wx']),
    'other_top_level_changed': sorted(k for k in set(old['rest']) | set(new['rest']) if old['rest'].get(k) != new['rest'].get(k)),
    'translation_units_equal_excluding_cache_trace': old['tu_without_cache_trace'] == new['tu_without_cache_trace'],
    'old_edges': len(old['edges']), 'new_edges': len(new['edges']),
    'removed_edges': len(removed), 'added_edges': len(added),
    'removed_edges_by_kind': dict(sorted(Counter(k for _, _, k in removed).items())),
    'old_overrides': len(old['overrides']), 'new_overrides': len(new['overrides']),
    'removed_overrides': len(old['overrides'] - new['overrides']), 'added_overrides': len(new['overrides'] - old['overrides']),
    'removed_edges_without_copy_endpoint': sum(a not in bodies and b not in bodies for a, b, _ in removed),
    'copies': len(copies),
    'shape_destructor_removed_edges': [[a, new['function_files'].get(a), b, k] for a, b, k in removed
                                       if usr(a) == SHAPE or usr(b) == SHAPE],
    'shape_destructor_remaining_vrml_body_callers': [[a, new['function_files'].get(a), b, k] for a, b, k in sorted(new['edges'])
                                                     if b == VRML_BODY],
    'provenance_added': sorted(set(new['provenance']) - set(old['provenance'])),
    'provenance_changed': sorted(k for k in old['provenance'] if k in new['provenance']
                                 and old['provenance'][k] != new['provenance'][k]),
    'replay_provenance': {k: new['provenance'].get(k) for k in
                          ('extractor_sha256', 'aggregation_extractor_sha256', 'parse_cache_replay', 'cache_reused_commands',
                           'commands_attempted', 'parse_complete', 'gate_complete', 'definition_endpoint_policy')},
    'parse_failures': [[f['file'], f['status']] for f in new['provenance']['parse_failures']],
}
v5_cmp['shape_destructor_removed_edge_count'] = len(v5_cmp['shape_destructor_removed_edges'])
result = {'versus_v5_original': v5_cmp, 'versus_pr79_replay': pr79_cmp,
          'replay_inventory_sha256': sha(N / 'replay/inventory.json')}
(N / 'evidence/replay-comparison.json').write_text(json.dumps(result, sort_keys=True, indent=2) + '\n')
print(json.dumps({'pr79': pr79_cmp, 'v5': {k: v for k, v in v5_cmp.items() if not isinstance(v, list) or k == 'parse_failures'}},
                 sort_keys=True, indent=1))
assert pr79_cmp['semantics_equal_except_aggregation_identity'], pr79_cmp
assert v5_cmp['functions_equal'] and v5_cmp['added_edges'] == 0 and v5_cmp['added_overrides'] == 0
assert v5_cmp['removed_edges_without_copy_endpoint'] == 0
