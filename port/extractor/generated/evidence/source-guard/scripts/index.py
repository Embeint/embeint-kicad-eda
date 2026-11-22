"""Index audit after staging: every byte-preserved artifact equals its staged Git blob."""
import hashlib, json, subprocess
from pathlib import Path
root = Path(r'C:\Users\KarlVonRichter\.t3\worktrees\embeint-kicad-eda\t3-7461f604-ff4777aca1333cd1')
G = root / 'port/extractor/generated'
E = G / 'evidence/source-guard'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
expected = {}
for base in (G / 'sources', G / 'history', G / 'evidence', G / 'raw', G / 'reconciled-extraction', G / 'corrected-extraction'):
    for path in base.rglob('*'):
        if path.is_file():
            expected[path.relative_to(root).as_posix()] = sha(path)
expected.pop('port/extractor/generated/evidence/source-guard/index-artifact-audit.json', None)
for path in G.glob('*'):
    if path.is_file() and path.suffix in ('.json', '.gz'):
        expected[path.relative_to(root).as_posix()] = sha(path)
# Unit records and top-level log copies are normalized text; compare LF-normalized bytes.
units = {}
for path in sorted(G.glob('*.log')):
    units[path.relative_to(root).as_posix()] = hashlib.sha256(path.read_bytes().replace(b'\r\n', b'\n')).hexdigest()
for path in sorted((root / 'port/units').glob('*.toml')):
    units[path.relative_to(root).as_posix()] = hashlib.sha256(path.read_bytes().replace(b'\r\n', b'\n')).hexdigest()
p = subprocess.Popen(['git', 'cat-file', '--batch'], cwd=root, stdin=subprocess.PIPE, stdout=subprocess.PIPE)
for relative, wanted in sorted({**expected, **units}.items()):
    p.stdin.write((':' + relative + '\n').encode()); p.stdin.flush()
    header = p.stdout.readline().decode().strip().split()
    assert len(header) == 3 and header[1] == 'blob', (relative, header)
    data = p.stdout.read(int(header[2])); assert p.stdout.read(1) == b'\n'
    assert hashlib.sha256(data).hexdigest() == wanted, relative
p.stdin.close(); assert p.wait() == 0
measurement = json.loads((G / 'measurement.json').read_text())
for record in measurement['canonical']['artifacts']:
    assert expected['port/extractor/generated/' + record['filename']] == record['sha256']
result = {'indexed_artifacts_verified': len(expected) + len(units), 'byte_preserved_artifacts': len(expected),
          'lf_normalized_text_files': len(units), 'unit_files': sum(k.startswith('port/units/') for k in units),
          'frozen_sources': sum(k.startswith('port/extractor/generated/sources/') for k in expected),
          'all_index_bytes_match_verified_files': True,
          'policy': 'Generated inputs, transcripts, historical evidence and published JSON/gzip artifacts are compared byte for byte with their staged blobs; unit TOMLs and top-level log copies after LF normalization',
          'artifact_sha256': dict(sorted(expected.items())), 'unit_sha256_lf': units}
(E / 'index-artifact-audit.json').write_text(json.dumps(result, sort_keys=True, indent=2) + '\n', encoding='utf-8', newline='\n')
print({k: v for k, v in result.items() if not isinstance(v, dict)})
