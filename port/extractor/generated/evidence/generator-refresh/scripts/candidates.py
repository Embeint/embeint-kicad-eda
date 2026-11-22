"""Collect tracked generator/configure input fingerprints for a pinned-Git byte audit on Windows."""
import hashlib, json, pathlib, re, subprocess, sys
W = pathlib.Path(sys.argv[1])
V5 = pathlib.Path.home() / 'kicad-port/corrected-review-v5'
G = W / 'final/port/extractor/generated'
manifest = json.loads((G / 'generated-sources.json').read_text())
recipes = json.loads((W / 'capture/generated-recipes.json').read_text())
configuration = manifest['provenance']['reconciliation.json']['record']
source = pathlib.Path(configuration['source_root'])
query = subprocess.check_output(['ninja', '-C', manifest['build_root'], '-t', 'query', 'build.ninja'], text=True)
assert query == (V5 / 'configure-inputs.txt').read_text(), 'configure inputs changed'
dependencies = {**manifest['source_generator_dependencies'], **{e['path']: e['generator_inputs'] for e in manifest['files']}}
paths = {p for k, v in dependencies.items() for p in [k, *v] if not p.startswith('@build/')}
relations = set(paths)
texts = [query] + [r[key] for r in recipes.values() for key in ('query', 'commands')]
for record in recipes.values():
    texts += [a['stdout'] for a in record.get('query_attempts', []) + record.get('command_attempts', [])]
for text in texts:
    paths.update(m.removeprefix(str(source) + '/') for m in re.findall(re.escape(str(source)) + r'/[^\s\x22\x27;]+', text))
blob = lambda b: hashlib.sha1(b'blob ' + str(len(b)).encode() + b'\0' + b).hexdigest()
previous = {r['path'] for r in json.loads((V5 / 'generation-input-audit.json').read_text())['inputs']}
records = []
for filename in sorted({str((source / p).resolve()) for p in paths if (source / p).is_file()}):
    path = pathlib.Path(filename)
    if not path.is_relative_to(source):
        continue
    relative = path.relative_to(source).as_posix()
    data = path.read_bytes()
    normal = data.replace(b'\r\n', b'\n')
    records.append({'path': relative, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest(),
                    'normalized_sha256': hashlib.sha256(normal).hexdigest(), 'raw_blob': blob(data),
                    'normalized_blob': blob(normal), 'in_dependency_map': relative in relations,
                    'in_v5_audit': relative in previous})
missing = sorted(p for p in relations if not (source / p).is_file())
assert not missing, missing
(W / 'evidence/generation-input-candidates.json').write_text(json.dumps({
    'source_commit': configuration['source_commit'], 'upstream_pin': configuration['upstream_pin'], 'inputs': records,
    'refreshed_manifest_sha256': hashlib.sha256((G / 'generated-sources.json').read_bytes()).hexdigest(),
    'configure_query_sha256': hashlib.sha256(query.encode()).hexdigest(),
    'v5_audited_paths': len(previous), 'v5_audited_paths_still_present': len(previous & {r['path'] for r in records}),
    'new_paths': sorted({r['path'] for r in records} - previous)}, sort_keys=True, indent=2) + '\n')
print(len(records), 'candidates;', len({r['path'] for r in records} - previous), 'new beyond the V5 audit')
