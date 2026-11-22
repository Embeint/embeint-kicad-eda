"""Guard controls on the actual pinned snapshot and build (expected rejections; no source edits)."""
import copy, hashlib, json, pathlib, shutil, subprocess, sys
N = pathlib.Path(sys.argv[1])
cur = N / 'current'
sys.path[:0] = [str(cur / 'port'), str(cur / 'port/tools')]
from extractor import refresh_generator_dependencies as r
from extractor.generated_sources import build_state, capture_recipes
from extractor.model import read_json, write_json
home = pathlib.Path.home() / 'kicad-port'
V5, OLD = home / 'corrected-review-v5', home / 'generator-refresh-57e'
ROOT = pathlib.Path('/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-7461f604-ff4777aca1333cd1')
source = home / 'extract-source'
manifest, inventory, audit = V5 / 'inputs/generated-sources.json', V5 / 'output/inventory.json', N / 'base-generation-input-audit.json'
work = N / 'controls'
if work.exists():
    shutil.rmtree(work)
work.mkdir()
base = read_json(manifest)
build = pathlib.Path(base['build_root'])
before_build = build_state(build)
results = {}


def expect_reject(label, call, needle):
    try:
        call()
    except ValueError as error:
        assert needle in str(error), (label, str(error))
        results[label] = {'rejected': True, 'error': str(error)}
        return
    raise AssertionError(label + ' was accepted')


def capture(**kw):
    args = dict(inventory_path=inventory, generation_inputs_path=audit)
    args.update(kw.pop('flags', {}))
    return r.capture(kw.pop('manifest', manifest), kw.pop('source', source), kw.pop('output'), **args)


# Capture-side controls: rejected before any Ninja query unless stated.
expect_reject('capture_refreshed_inventory', lambda: capture(output=work / 'c1', flags={'inventory_path': ROOT / 'port/extractor/generated/inventory.json'}),
              'Capture inventory differs from the original generated-source manifest')
(work / 'foreign-root').mkdir()
expect_reject('capture_foreign_root', lambda: capture(output=work / 'c2', source=work / 'foreign-root'),
              'Refresh source root differs')
forged = read_json(audit)
markdown = next(e for e in forged['inputs'] if e['path'] == 'pcbnew/pcb_text_help.md')
markdown['normalized_sha256'] = '0' * 64
write_json(work / 'stale-audit.json', forged)
expect_reject('capture_stale_markdown_identity', lambda: capture(output=work / 'c3', flags={'generation_inputs_path': work / 'stale-audit.json'}),
              'Pinned refresh source is missing or changed: pcbnew/pcb_text_help.md')
forged = read_json(audit)
forged['inputs'].append(dict(markdown, path='pcbnew/source_guard_control_missing.md'))
write_json(work / 'missing-audit.json', forged)
expect_reject('capture_missing_source_identity', lambda: capture(output=work / 'c4', flags={'generation_inputs_path': work / 'missing-audit.json'}),
              'Pinned refresh source is missing or changed: pcbnew/source_guard_control_missing.md')
forged = read_json(audit)
tracked = next(e for e in forged['inputs'] if e['path'].endswith('.h') and e['path'] in {f['path'] for f in read_json(inventory)['files']})
tracked['normalized_sha256'] = '1' * 64
write_json(work / 'conflicting-audit.json', forged)
expect_reject('capture_audit_conflicts_with_tracked_snapshot', lambda: capture(output=work / 'c5', flags={'generation_inputs_path': work / 'conflicting-audit.json'}),
              'Generator input differs from the tracked parse snapshot')
forged = read_json(audit); forged['source_commit'] = '0' * 40
write_json(work / 'foreign-pin-audit.json', forged)
expect_reject('capture_foreign_pin_audit', lambda: capture(output=work / 'c6', flags={'generation_inputs_path': work / 'foreign-pin-audit.json'}),
              'Refresh source pins differ')

# Unrecorded Ninja source nodes fail during the real query routine on the real build.
identity = r.source_identity(base, inventory, audit, source)
for label, missing, needle in (('ninja_unrecorded_source_input', 'pcbnew/pcb_text_help.md', 'pcbnew/pcb_text_help.md'),
                               ('ninja_unrecorded_source_target', 'pcbnew/pcb_text_help_md.h', 'pcbnew/pcb_text_help_md.h')):
    files = {k: v for k, v in identity['files'].items() if k != missing}
    entries = copy.deepcopy([e for e in base['files'] if e['path'] == '@build/common/pcb_lexer.h'])
    expect_reject(label, lambda: capture_recipes(build, source, entries, {}, files), 'Ninja source lacks a pinned snapshot: ' + needle)

# Apply-side controls (pure).
graph, inv = N / 'canonical-old/graph.json.gz', N / 'canonical-old/inventory.json'
cap, recipes = N / 'capture/generator-dependency-capture.json', N / 'capture/generated-recipes.json'


def apply(**kw):
    return r.apply(kw.get('graph', graph), kw.get('inventory', inv), manifest, kw.get('capture', cap), recipes,
                   work / kw['output'], generation_inputs_path=kw.get('audit', audit))


forged = read_json(cap)
forged['source_identity']['files']['pcbnew/pcb_text_help.md'] = '0' * 64
write_json(work / 'forged-files-capture.json', forged)
expect_reject('apply_forged_capture_files', lambda: apply(capture=work / 'forged-files-capture.json', output='a1'), 'Capture source identity differs')
forged = read_json(cap)
forged['source_identity']['source_files_sha256'] = '0' * 64
write_json(work / 'forged-summary-capture.json', forged)
expect_reject('apply_forged_capture_summary', lambda: apply(capture=work / 'forged-summary-capture.json', output='a2'), 'Capture source identity differs')
forged = read_json(cap)
forged['source_root'] = str(work / 'foreign-root'); forged['source_identity']['source_root'] = str(work / 'foreign-root')
write_json(work / 'forged-root-capture.json', forged)
expect_reject('apply_forged_capture_root', lambda: apply(capture=work / 'forged-root-capture.json', output='a3'), 'Capture source identity differs')
expect_reject('apply_d2_capture_without_identity', lambda: apply(capture=OLD / 'capture/generator-dependency-capture.json', output='a4'),
              'Capture source identity differs')
expect_reject('apply_different_audit', lambda: apply(audit=work / 'stale-audit.json', output='a5'), 'Capture source identity differs')
expect_reject('apply_refreshed_graph', lambda: apply(graph=ROOT / 'port/extractor/generated/graph.json.gz',
              inventory=ROOT / 'port/extractor/generated/inventory.json', output='a6'), 'original parse artifacts')
assert build_state(build) == before_build
r.verify_source(identity)
summary = {'controls': results, 'all_rejected': all(v['rejected'] for v in results.values()), 'control_count': len(results),
           'build_unchanged': True, 'source_snapshot_verified_after': identity['verified_source_files'],
           'source_files_sha256': identity['source_files_sha256'],
           'scope': ('Real pinned source snapshot, real build and real Ninja; stale/missing/foreign states are introduced through '
                     'forged identity inputs, never by editing the shared source or build trees. Physical file and after-query '
                     'mutations are exercised in the disposable real-Ninja fixture (repro-controls.json).')}
assert summary['all_rejected']
(N / 'evidence/actual-controls.json').write_text(json.dumps(summary, sort_keys=True, indent=2) + '\n')
print(json.dumps({k: (v if k != 'controls' else {x: y['error'][:120] for x, y in v.items()}) for k, v in summary.items()}, indent=1))
