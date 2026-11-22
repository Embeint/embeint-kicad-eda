"""Maintain current provenance documentation from the verified-Ninja proof records."""
import hashlib
import json
from pathlib import Path

root = Path(__file__).resolve().parents[1] / 'worktrees/embeint-kicad-eda/t3-04614140-10140941291c27ae'
g = root / 'port/extractor/generated'
e = g / 'evidence/verified-ninja'
checks = json.loads((e / 'checks.json').read_bytes())
identity = checks['identities']
refresh = identity['refresh']
assert checks['units']['count'] == checks['units']['solidcore_roundtrip_units'] == 1801
assert checks['units']['changed_from_base'] == 0
assert checks['head'] == 'c1e8a467da149eac67341d31de0228e498916ba3'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
code, graph, inventory, manifest = (refresh['refresh_code_sha256'],
    identity['canonical_graph_sha256'], identity['canonical_inventory_sha256'],
    identity['refreshed_manifest_sha256'])
capture = json.loads((e / 'generator-dependency-capture.json').read_bytes())
ninja_info = json.loads((g / 'corrected-extraction/generated-sources.json').read_bytes())['provenance']['tools']['ninja']
assert ninja_info['path'] == '/usr/bin/ninja'
assert capture['tool_and_configuration_sha256']['ninja'] == json.loads(
    (g / 'corrected-extraction/generated-sources.json').read_bytes())['provenance']['tools']['ninja']['sha256']

section = f'''## Verified Ninja recapture (current)

The independent review of `2888cd057f` verified the guarded artifacts but found
that capture checked the recorded Ninja binary while invoking `ninja` through
PATH. Code `c1e8a467da` binds every catalog, query and command listing to the
manifest's verified absolute executable. Missing or relative recorded paths are
rejected. The real-Ninja regression uses a PATH wrapper that selects a second
graph without the Markdown dependency; the fixed capture never invokes it.
Changing the executable actually invoked after querying is also rejected. The
same focused test fails against the published 2888 implementation, as retained
in `evidence/verified-ninja/logs/old-implementation-control.log.gz`.

A fresh recapture from the original V5 manifest `a2466270`, inventory `b9e6986b`
and canonical graph `146e2cca` used the unchanged `bf7a1689` audit. It invoked
the recorded `/usr/bin/ninja` under the shared lock. All 9,655 source files,
1,612 build files, 178 frozen inputs and recorded tool identities were checked
before and after. Recipes and the 5,709-record catalog are byte-identical to D2;
both dependency maps are unchanged. The only graph changes remain the refresh
record and manifest fingerprint. Every unit file and the original AST parse
identities are unchanged. No new AST parse or full cache replay was performed.

Current code `{code[:8]}`, manifest `{manifest[:8]}`, graph `{graph[:8]}` and
inventory `{inventory[:8]}` have their full identities in
`evidence/verified-ninja/checks.json`. That directory retains the capture,
repeated application, canonical regeneration, guard controls, validation and
commands/logs. The D2 and 2888 evidence directories remain byte-identical.
The prior full replay and ownership proofs remain separately identified and are
reused only after input-hash checks. A new independent review of this head is
required; no numerical parity, green unit or phase-zero acceptance is claimed.

'''

readme = g / 'README.md'
text = readme.read_text(encoding='utf-8')
old = '''`ce9681f06f` adds that guard. The current root artifacts come from a guarded
recapture with the same relations. A new independent review of this head is
required.

## Guarded source recapture (current)
'''
assert old in text
text = text.replace(old, '''`ce9681f06f` adds that guard. The guarded recapture at `2888cd057f`
republished the same relations; its evidence remains below as history.

''' + section + '## Guarded source recapture (historical 2888)\n', 1)
text = text.replace('canonical graph.json.gz (guarded, current)', 'canonical graph.json.gz (source-guarded 2888, historical)')
text = text.replace('canonical inventory.json (guarded, current)', 'canonical inventory.json (source-guarded 2888, historical)')
text = text.replace('generated-sources.json (guarded, current, LF)', 'generated-sources.json (source-guarded 2888, historical, LF)')
anchor = '| Refresh artifact | SHA256 |\n| --- | --- |\n'
assert anchor in text
text = text.replace(anchor, anchor + f'''| canonical graph.json.gz (verified Ninja, current) | `{graph}` |
| canonical inventory.json (verified Ninja, current) | `{inventory}` |
| generated-sources.json (verified Ninja, current) | `{manifest}` |
''', 1)
readme.write_text(text, encoding='utf-8', newline='\n')

validation = g / 'VALIDATION.md'
text = validation.read_text(encoding='utf-8')
anchor = '## Guarded source recapture (current)'
assert anchor in text
summary = '''## Verified Ninja recapture (current)

Validated clean code `c1e8a467da149eac67341d31de0228e498916ba3`. Evidence:
`evidence/verified-ninja/`. The AST parser and original V5 identities are unchanged.

| Check | Result |
| --- | --- |
| Executable binding | Every catalog/query/command invokes the manifest's verified absolute Ninja; missing/relative records refused |
| Regression controls | PATH wrapper selects a second real graph and is never invoked by guarded capture; invoked-tool mutation after querying refused; published 2888 implementation fails the new focused regression |
| Fresh real capture | Original V5 inputs, unchanged pinned audit; 9,655 source files, 1,612 build files, 178 frozen inputs and tools checked before/after under the shared lock |
| Dependency equivalence | Raw recipes and catalog byte-identical to D2; both relation maps unchanged; graph and inventory differ only in refresh metadata and derived identities |
| Application/regeneration | Two WSL applications and two canonical splits byte-identical; Windows pure apply and two canonical splits also preserve every byte |
| Extractor suites | WSL 36 passed, no skips; Windows 25 passed and 11 expected libclang/Ninja skips |
| Source validation | Default passes on WSL and Windows; strict fails only on the four known fmt module parses |
| Records/measurement | All 1,801 units unchanged and round-trip through pinned solidcore; only graph/inventory measurement hashes change; all pilot scopes/GetReader owner preserved |
| Prior guards | All 14 actual-snapshot and 10 synthetic invalid-source controls rejected; valid Markdown drift still reaches consumers |
| Maintained checks | Progress/bootstrap/policy 28/9/7, progress build and committed golden-change policy pass |

No full AST parse/replay or pilot numerical validation was performed. V5/D2
replay and ownership evidence is reused after checking its bound inputs. Historical
V5, D2 and source-guard evidence remains unchanged. Fresh independent review,
parent acceptance and the three-pilot gate remain outstanding.

'''
text = text.replace(anchor, summary + '## Guarded source recapture (historical 2888)', 1)
validation.write_text(text, encoding='utf-8', newline='\n')

pilots = root / 'port/extractor/PILOTS.md'
text = pilots.read_text(encoding='utf-8')
text += '''
The verified-Ninja recapture (`generated/evidence/verified-ninja/`) invokes only
the manifest's absolute verified executable. Its fresh relations are identical
to D2 and 2888; all 1,801 unit files, these pilot scopes and the GetReader owner
remain unchanged. Independent acceptance and numerical pilot reviews remain
separate requirements.
'''
pilots.write_text(text, encoding='utf-8', newline='\n')

path = g / 'finalization.json'
final = json.loads(path.read_bytes())
final['source_guarded_refresh']['status'] = 'historical 2888 source guard; superseded at the root by verified_ninja_refresh'
final['head_at_validation'] = checks['head']
final['verified_ninja_refresh'] = {
    'evidence': 'evidence/verified-ninja/',
    'status': 'fresh capture published; new independent review required',
    'ninja_path': ninja_info['path'],
    'ninja_sha256': capture['tool_and_configuration_sha256']['ninja'],
    'parse_extractor_sha256': refresh['parse_extractor_sha256'],
    'refresh_code_sha256': code,
    'canonical_graph_sha256': graph,
    'canonical_inventory_sha256': inventory,
    'refreshed_manifest_sha256': manifest,
    'recipes_sha256': refresh['recipes_sha256'],
    'source_files_sha256': refresh['source_identity']['source_files_sha256'],
    'generation_inputs_sha256': refresh['source_identity']['generation_inputs_sha256'],
    'verified_source_files': 9655,
    'relations_equal_d2': True,
    'unit_records_unchanged': 1801,
    'pilot_records_unchanged': True}
path.write_text(json.dumps(final, sort_keys=True, indent=2) + '\n', encoding='utf-8', newline='\n')
print(json.dumps(final['verified_ninja_refresh'], indent=2))
