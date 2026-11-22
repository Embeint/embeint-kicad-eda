import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True
work = Path(sys.argv[1])
old = Path.home() / 'kicad-port/md5-envelope-regressions'
spec = importlib.util.spec_from_file_location('md5_inputs', work / 'repo/port/oracle/test_md5_inputs.py')
module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
checks = module.regression_cases()
assert len(checks) == 203 and sum('error' in expected for _, expected in checks) == 197
raw = ''.join(text + '\n' for text, _ in checks).encode()
expected = ''.join(json.dumps(value, separators=(',', ':')) + '\n' for _, value in checks).encode()
prior_raw = (old / 'raw-regression-requests.jsonl').read_bytes()
prior_expected = (old / 'regression-expected.jsonl').read_bytes()
assert raw.startswith(prior_raw) and expected.startswith(prior_expected)
assert len(prior_raw.splitlines()) == len(prior_expected.splitlines()) == 199
(work / 'requests.jsonl').write_bytes(raw)
(work / 'expected.jsonl').write_bytes(expected)
report = {'source_commit': 'da17dc54dc2b2be2b15cb4a2c6c171c62950a5cc',
          'baseline': 'c5ec29e70d07506ac3fb8b8b5c1b6f5102c7ef3e',
          'cases': 203, 'refusals': 197, 'successes': 6, 'original_199_byte_prefix_preserved': True,
          'adapters': {}}
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
for name, executable in [('oracle', old / 'runners/kicad-oracle'),
                         ('rust', work / 'runners/kicad-md5-shim')]:
    process = subprocess.run([str(executable)], input=raw, capture_output=True, check=True, timeout=30)
    assert not process.stderr
    replies = [json.loads(line) for line in process.stdout.split(b'\n') if line]
    assert replies == [value for _, value in checks], name
    (work / f'{name}.stdout.jsonl').write_bytes(process.stdout)
    report['adapters'][name] = {'sha256': sha(executable), 'complete_replies': len(replies),
                              'stdout_sha256': hashlib.sha256(process.stdout).hexdigest()}
new_raw = ''.join(text + '\n' for text, _ in checks[199:]).encode()
old_executable = Path.home() / 'kicad-port/md5-corrected-graph/runners/kicad-md5-shim'
assert sha(old_executable) == 'c05b0be1d811d52d6fb42b0df1794eb30f993df0d8eacba9d40fccc6bf5b1ae7'
negative = subprocess.run([str(old_executable)], input=new_raw, capture_output=True, check=True, timeout=10)
assert not negative.stderr
(work / 'old-helper.stdout.jsonl').write_bytes(negative.stdout)
replies = [json.loads(line) for line in negative.stdout.split(b'\n') if line]
differences = [i for i, (reply, (_, value)) in enumerate(zip(replies, checks[199:])) if reply != value]
assert len(replies) == 4 and differences == [0, 1, 2]
report['old_helper_control'] = {'sha256': sha(old_executable), 'new_case_mismatches': differences,
                              'recovery_success_matches': True}
report['oracle_provenance_sha256'] = sha(work / 'oracle-provenance.json')
assert report['oracle_provenance_sha256'] == 'a1814b2726e80179130ce4d17cf6f0fc85fe0ab6ae1e7972f6e3bf614af72c8d'
(work / 'proof.json').write_text(json.dumps(report, indent=2, sort_keys=True) + '\n')
print(json.dumps(report, indent=2))
