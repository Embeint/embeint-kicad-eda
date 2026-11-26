"""Compare the accepted rust canonical graph with the adopted same-USR canonical graph.

Loads one graph at a time (each is about 2.4 GB of JSON) and keeps compact summaries.
Usage: graphcmp.py <old graph.json.gz> <new graph.json.gz> <old units dir> <new units dir> <output json>
"""
import collections, gzip, hashlib, json, sys, tomllib
from pathlib import Path

VRML_BODY = 'c:@S@SHAPE@F@~SHAPE#@definition:861533d9c907d7fb'
SHAPE_PRIMARY = 'c:@S@SHAPE@F@~SHAPE#'
P003 = 'libs.kimath.include.geometry.application.ui.p003'
VRML_P001 = 'plugins.3d.vrml.application.ui.p001'


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def strings(value):
    found = set()
    stack = [value]
    while stack:
        x = stack.pop()
        if isinstance(x, str):
            if x.startswith('c:'):
                found.add(x.split('@definition:')[0])
        elif isinstance(x, dict):
            stack.extend(x.keys()); stack.extend(x.values())
        elif isinstance(x, list):
            stack.extend(x)
    return found


def summarize(path):
    graph = json.loads(gzip.decompress(Path(path).read_bytes()))
    functions = graph['functions']
    summary = {
        'functions': {usr: (f['file'], f['start'], f['end'], f['lines'], f['sha256'], f['range_kind']) for usr, f in functions.items()},
        'wx': {usr: (f['wx_class'], f['wx_direct'], tuple(sorted(f['wx']))) for usr, f in functions.items()},
        'edges': {tuple(e) for e in graph['edges']},
        'overrides': {json.dumps(o, sort_keys=True) for o in graph['overrides']},
        'copies': digest(graph['definition_copies']),
        'copy_group_usrs': strings(graph['definition_copies']),
        'copies_shape': json.dumps(graph['definition_copies'] if isinstance(graph['definition_copies'], dict) and False else
                                   (list(graph['definition_copies'].items())[:1] if isinstance(graph['definition_copies'], dict)
                                    else graph['definition_copies'][:1]))[:600],
        'variants': digest(graph['definition_variants']),
        'aliases': digest(graph['template_aliases']),
        'unresolved': digest(graph['unresolved_calls']),
        'unresolved_count': len(graph['unresolved_calls']),
        'tus': sorted((t.get('file'), t.get('status'), t.get('configuration', '')) for t in graph['translation_units']),
        'range_normalizations': digest(graph['range_normalizations']),
        'provenance': graph['provenance'],
        'files': collections.Counter(f['file'] for f in functions.values()),
        'lines_by_file': {},
    }
    del graph
    return summary


old = summarize(sys.argv[1])
new = summarize(sys.argv[2])
removed = sorted(old['edges'] - new['edges'])
added = sorted(new['edges'] - old['edges'])
kinds = collections.Counter(e[2] for e in removed)
copy_endpoint = lambda e: e[0].split('@definition:')[0] in new['copy_group_usrs'] or e[1].split('@definition:')[0] in new['copy_group_usrs']
func_file = lambda usr: new['functions'].get(usr.split('@definition:')[0] if usr not in new['functions'] else usr, ('?',))[0]


def units(directory):
    return {p.stem: tomllib.loads(p.read_text(encoding='utf-8')) for p in sorted(Path(directory).glob('*.toml'))}


old_units, new_units = units(sys.argv[3]), units(sys.argv[4])
owner = {usr: uid for uid, u in new_units.items() for usr in u['functions']}

shape_removed = [e for e in removed if SHAPE_PRIMARY in (e[0].split('@definition:')[0], e[1].split('@definition:')[0])]
into_vrml_removed = [e for e in removed if e[1] == VRML_BODY]
vrml_callers_new = sorted(e for e in new['edges'] if e[1] == VRML_BODY)
primary_from_vrml_callers = sorted(e for e in new['edges'] if e[1] == SHAPE_PRIMARY and e[0] in {c[0] for c in vrml_callers_new})
p003_functions = set(new_units[P003]['functions'])
p003_into_vrml_old = sorted(e for e in old['edges'] if e[0] in p003_functions and owner.get(e[1]) == VRML_P001)
p003_into_vrml_new = sorted(e for e in new['edges'] if e[0] in p003_functions and owner.get(e[1]) == VRML_P001)
dependents_old = sorted(u for u, x in old_units.items() if VRML_P001 in x['depends_on'])
dependents_new = sorted(u for u, x in new_units.items() if VRML_P001 in x['depends_on'])

result = {
    'old_graph_provenance_extractor': old['provenance']['extractor_sha256'],
    'new_graph_aggregation_extractor': new['provenance'].get('aggregation_extractor_sha256'),
    'functions_equal': old['functions'] == new['functions'],
    'function_count': [len(old['functions']), len(new['functions'])],
    'wx_classification_equal': old['wx'] == new['wx'],
    'wx_class_counts': dict(collections.Counter(v[0] for v in new['wx'].values())),
    'function_files_equal': old['files'] == new['files'],
    'definition_copies_equal': old['copies'] == new['copies'],
    'definition_variants_equal': old['variants'] == new['variants'],
    'template_aliases_equal': old['aliases'] == new['aliases'],
    'unresolved_calls_equal': old['unresolved'] == new['unresolved'],
    'unresolved_call_count': new['unresolved_count'],
    'range_normalizations_equal': old['range_normalizations'] == new['range_normalizations'],
    'translation_units_equal': old['tus'] == new['tus'],
    'translation_unit_count': len(new['tus']),
    'edges': [len(old['edges']), len(new['edges'])],
    'added_edges': len(added),
    'removed_edges': len(removed),
    'removed_edges_by_kind': dict(sorted(kinds.items())),
    'removed_edges_without_copy_endpoint': sum(not copy_endpoint(e) for e in removed),
    'removed_edges_with_suffixed_copy_endpoint': sum('@definition:' in e[0] or '@definition:' in e[1] for e in removed),
    'copy_group_usrs': len(new['copy_group_usrs']),
    'copy_group_usrs_equal': old['copy_group_usrs'] == new['copy_group_usrs'],
    'copies_shape_sample': new['copies_shape'],
    'overrides': [len(old['overrides']), len(new['overrides'])],
    'added_overrides': len(new['overrides'] - old['overrides']),
    'removed_overrides': len(old['overrides'] - new['overrides']),
    'provenance_keys_added': sorted(set(new['provenance']) - set(old['provenance'])),
    'provenance_keys_changed': sorted(k for k in set(old['provenance']) & set(new['provenance'])
                                      if old['provenance'][k] != new['provenance'][k]),
    'shape_destructor_edges_removed': len(shape_removed),
    'edges_into_vrml_body_removed': len(into_vrml_removed),
    'edges_into_vrml_body_removed_by_caller_unit': dict(sorted(collections.Counter(owner.get(e[0], '?') for e in into_vrml_removed).items())),
    'vrml_body_callers_retained': [list(e) + [owner.get(e[0], '?')] for e in vrml_callers_new],
    'vrml_body_callers_also_reach_primary': [list(e) for e in primary_from_vrml_callers],
    'p003': {
        'old_edges_into_vrml_p001': [list(e) for e in p003_into_vrml_old],
        'new_edges_into_vrml_p001': [list(e) for e in p003_into_vrml_new],
        'old_depends_on': old_units[P003]['depends_on'], 'new_depends_on': new_units[P003]['depends_on'],
        'old_soft_depends_on': old_units[P003]['soft_depends_on'], 'new_soft_depends_on': new_units[P003]['soft_depends_on'],
        'old_layer': old_units[P003]['layer'], 'new_layer': new_units[P003]['layer'],
    },
    'hard_dependents_of_vrml_p001': {'old': dependents_old, 'new': dependents_new},
}
Path(sys.argv[5]).write_text(json.dumps(result, sort_keys=True, indent=2) + '\n')
print(json.dumps({k: v for k, v in result.items() if k not in ('vrml_body_callers_retained', 'p003')}, indent=1))
print(json.dumps(result['p003'], indent=1)[:4000])
