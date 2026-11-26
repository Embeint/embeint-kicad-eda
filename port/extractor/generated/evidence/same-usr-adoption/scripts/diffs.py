"""Compare the adopted canonical artifacts and unit records with the accepted head."""
import gc, gzip, hashlib, json, pathlib, sys, tomllib
from collections import Counter
N = pathlib.Path(sys.argv[1])
H = N / 'head/port/extractor/generated'
F = N / 'final'
G = F / 'port/extractor/generated'
P003 = 'libs.kimath.include.geometry.application.ui.p003'
VRML = 'plugins.3d.vrml.application.ui.p001'
SHAPE = 'c:@S@SHAPE@F@~SHAPE#'
VRML_BODY = SHAPE + '@definition:861533d9c907d7fb'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
canon = lambda v: hashlib.sha256(json.dumps(v, sort_keys=True).encode()).hexdigest()

# Unit records.
load = lambda d: {p.stem: tomllib.loads(p.read_text(encoding='utf-8')) for p in sorted(d.glob('*.toml'))}
b, n = load(N / 'head/port/units'), load(F / 'port/units')
assert set(b) == set(n), set(b) ^ set(n)
preserved = ('id', 'upstream', 'rust_module', 'est_lines', 'tests', 'status', 'claimed_by', 'claimed_at', 'notes',
             'schema_version', 'wx_class', 'source_category', 'functions', 'source_ranges', 'coverage_gaps')
keys, changed, transitions = Counter(), {}, Counter()
for uid in b:
    diff = sorted(k for k in set(b[uid]) | set(n[uid]) if b[uid].get(k) != n[uid].get(k))
    keys.update(diff)
    if not diff:
        continue
    entry = {}
    for k in diff:
        if isinstance(b[uid].get(k), list):
            entry[k] = {'removed': sorted(set(b[uid][k]) - set(n[uid][k])), 'added': sorted(set(n[uid][k]) - set(b[uid][k]))}
        else:
            entry[k] = [b[uid].get(k), n[uid].get(k)]
    changed[uid] = entry
    bh, bs, nh, ns = (set(x) for x in (b[uid]['depends_on'], b[uid]['soft_depends_on'], n[uid]['depends_on'], n[uid]['soft_depends_on']))
    for dep in nh - bh:
        transitions['hard_added_from_soft' if dep in bs else 'hard_added_new'] += 1
    for dep in bh - nh:
        transitions['hard_removed_now_soft' if dep in ns else 'hard_removed_entirely'] += 1
    for dep in ns - bs:
        transitions['soft_added_from_hard' if dep in bh else 'soft_added_new'] += 1
    for dep in bs - ns:
        transitions['soft_removed_now_hard' if dep in nh else 'soft_removed_entirely'] += 1
bad = {u: d for u, d in changed.items() if set(d) & set(preserved)}
non_todo = {u: b[u]['status'] for u in b if b[u]['status'] != 'todo'}
vrml_dependents = lambda units: sorted(u for u, r in units.items() if VRML in r['depends_on'])
units = {'units': len(b), 'changed_units': len(changed), 'changed_keys': dict(sorted(keys.items())),
         'dependency_transitions': dict(sorted(transitions.items())),
         'preserved_field_changes': bad,
         'statuses_before': dict(sorted(Counter(r['status'] for r in b.values()).items())),
         'statuses_after': dict(sorted(Counter(r['status'] for r in n.values()).items())),
         'non_todo_units': non_todo,
         'non_todo_units_changed': {u: changed[u] for u in non_todo if u in changed},
         'p003': {'before': {k: b[P003][k] for k in ('depends_on', 'soft_depends_on', 'layer', 'status')},
                  'after': {k: n[P003][k] for k in ('depends_on', 'soft_depends_on', 'layer', 'status')}},
         'vrml_p001_hard_dependents_before': vrml_dependents(b), 'vrml_p001_hard_dependents_after': vrml_dependents(n),
         'max_layer': [max(r['layer'] for r in b.values()), max(r['layer'] for r in n.values())],
         'est_lines_total': [sum(r['est_lines'] for r in b.values()), sum(r['est_lines'] for r in n.values())],
         'changes': changed}
assert not bad, bad
assert units['statuses_before'] == units['statuses_after']
(N / 'evidence/unit-record-changes.json').write_text(json.dumps(units, sort_keys=True, indent=2) + '\n')
print(json.dumps({k: v for k, v in units.items() if k not in ('changes', 'non_todo_units')}, sort_keys=True, indent=1))
del b, n
gc.collect()


# Canonical graph: committed head versus adopted.
def summarize(path):
    g = json.loads(gzip.decompress(path.read_bytes()))
    out = {'sha256': sha(path), 'provenance': g.pop('provenance'),
           'edges': set(map(tuple, g.pop('edges'))), 'overrides': set(map(tuple, g.pop('overrides'))),
           'functions': canon(g.pop('functions')),
           'tu': canon([{k: v for k, v in t.items() if k != 'cache_reused_from'} for t in g.pop('translation_units', [])]),
           'range_postprocessing': g.pop('range_postprocessing', None),
           'rest': {k: canon(v) for k, v in g.items()}}
    del g
    gc.collect()
    return out


old, new = summarize(H / 'graph.json.gz'), summarize(G / 'graph.json.gz')
removed = sorted(old['edges'] - new['edges'])
po, pn = old['provenance'], new['provenance']
graph = {'committed_graph_sha256': old['sha256'], 'adopted_graph_sha256': new['sha256'],
         'functions_equal': old['functions'] == new['functions'],
         'translation_units_equal_excluding_cache_trace': old['tu'] == new['tu'],
         'other_top_level_changed': sorted(k for k in set(old['rest']) | set(new['rest']) if old['rest'].get(k) != new['rest'].get(k)),
         'range_postprocessing': [old['range_postprocessing'], new['range_postprocessing']],
         'edges': [len(old['edges']), len(new['edges'])], 'removed_edges': len(removed),
         'added_edges': len(new['edges'] - old['edges']),
         'removed_edges_by_kind': dict(sorted(Counter(k for _, _, k in removed).items())),
         'overrides': [len(old['overrides']), len(new['overrides'])],
         'provenance_changed': sorted(k for k in set(po) | set(pn) if po.get(k) != pn.get(k)),
         'generated_input_dependencies_equal': po['generated_input_dependencies'] == pn['generated_input_dependencies'],
         'refresh_fields_changed': sorted(k for k in set(po['generator_dependency_refresh']) | set(pn['generator_dependency_refresh'])
                                          if po['generator_dependency_refresh'].get(k) != pn['generator_dependency_refresh'].get(k)),
         'adopted_refresh': pn['generator_dependency_refresh'],
         'vrml_body_callers': sorted(a for a, b_, _ in new['edges'] if b_ == VRML_BODY),
         'vrml_body_callers_committed': sorted(a for a, b_, _ in old['edges'] if b_ == VRML_BODY)}
assert graph['functions_equal'] and graph['added_edges'] == 0 and graph['generated_input_dependencies_equal']
del old, new
gc.collect()

# Inventory: only provenance should differ.
io_, in_ = json.loads((H / 'inventory.json').read_text()), json.loads((G / 'inventory.json').read_text())
graph['inventory_files_equal'] = io_['files'] == in_['files']
graph['inventory_provenance_equals_graph'] = in_['provenance'] == pn
assert graph['inventory_files_equal'] and graph['inventory_provenance_equals_graph']
del io_, in_
mh, mn = json.loads((H / 'generated-sources.json').read_text()), json.loads((G / 'generated-sources.json').read_text())
graph['manifest_changed_fields'] = sorted(k for k in set(mh) | set(mn) if mh.get(k) != mn.get(k))
graph['manifest_refresh_changed'] = sorted(k for k in set(mh['generator_dependency_refresh']) | set(mn['generator_dependency_refresh'])
                                           if mh['generator_dependency_refresh'].get(k) != mn['generator_dependency_refresh'].get(k))
(N / 'evidence/canonical-graph-changes.json').write_text(json.dumps(graph, sort_keys=True, indent=2) + '\n')
print(json.dumps({k: v for k, v in graph.items() if k != 'adopted_refresh'}, sort_keys=True, indent=1))

# Measurement and units report.
mo, mn_ = json.loads((H / 'measurement.json').read_text()), json.loads((G / 'measurement.json').read_text())


def delta(a, b_, path=''):
    if isinstance(a, dict) and isinstance(b_, dict):
        out = {}
        for k in sorted(set(a) | set(b_)):
            if a.get(k) != b_.get(k):
                out.update(delta(a.get(k), b_.get(k), f'{path}.{k}' if path else k))
        return out
    if isinstance(a, list) and isinstance(b_, list) and len(a) > 20:
        return {path: f'list changed: {len(a)} -> {len(b_)} entries'}
    return {path: [a, b_]}


measurement = delta(mo, mn_)
(N / 'evidence/measurement-changes.json').write_text(json.dumps(measurement, sort_keys=True, indent=2) + '\n')
print(json.dumps(measurement, sort_keys=True, indent=1)[:20000])
