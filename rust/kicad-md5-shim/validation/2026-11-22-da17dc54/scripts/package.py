import hashlib
import json
from pathlib import Path
import shutil
import subprocess

root = Path.cwd()
work = Path('//wsl.localhost/Ubuntu/home/karlvonrichter/kicad-port/md5-literal-escaped-da17dc54')
old = Path('//wsl.localhost/Ubuntu/home/karlvonrichter/kicad-port/md5-envelope-regressions')
source = 'da17dc54dc2b2be2b15cb4a2c6c171c62950a5cc'
base = 'c5ec29e70d07506ac3fb8b8b5c1b6f5102c7ef3e'
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip() == source
assert (work / 'completed').is_file()
out = root / 'rust/kicad-md5-shim/validation/2026-11-22-da17dc54'
out.mkdir(exist_ok=False)
sha = lambda data: hashlib.sha256(data).hexdigest()
blob = lambda rev, path: subprocess.check_output(['git', 'cat-file', 'blob', f'{rev}:{path}'])
proof = json.loads((work / 'proof.json').read_bytes())
registered = json.loads((work / 'registered.json').read_bytes())
assert registered['cases'] == 5973 and registered['deferred'] == 0 and registered['status'] == 'green'
assert registered['oracle_output_sha256'] == '9b8b9a761911d19c39e45136b28d17eb47ef50dc582102b4b4e538ba909034cb'
assert registered['rust_output_sha256'] == '709084cd7bce4bd431415418ae7456b198d0d30d80613d2c9be373e8f67c6cf4'
assert registered['rust_sha256'] == proof['adapters']['rust']['sha256']
for name, prior in [('oracle', 'kicad-oracle-regressions.stdout.jsonl'),
                    ('rust', 'kicad-md5-shim-regressions.stdout.jsonl')]:
    assert (work / f'{name}.stdout.jsonl').read_bytes().startswith((old / prior).read_bytes())
source_hashes = {}
paths = subprocess.check_output(['git', 'ls-tree', '-r', '--name-only', source, '--', 'rust'], text=True).splitlines()
paths = [p for p in paths if '/validation/' not in p]
paths += ['port/oracle/test_md5_inputs.py',
          'port/cases/libs.kimath.src.md5_hash.application.pure.p001.jsonl',
          'port/goldens/libs.kimath.src.md5_hash.application.pure.p001.jsonl']
for path in paths:
    actual = (work / 'repo' / path).read_bytes()
    expected = blob(source, path)
    assert actual == expected, path
    source_hashes[path] = sha(actual)
production = lambda data: data.split(b'#[cfg(test)]', 1)[0]
assert production(blob(source, 'rust/kicad-md5-shim/src/main.rs')) == production(blob(base, 'rust/kicad-md5-shim/src/main.rs'))
protected = ['rust/kicad-md5', 'rust/kicad-difftest', 'rust/Cargo.lock', 'port/units',
             'port/cases', 'port/goldens', 'port/golden-evidence', 'rust/kicad-md5-shim/validation']
assert not subprocess.check_output(['git', 'diff', '--name-only', base, source, '--', *protected])
for name in ['fmt.log', 'clippy.log', 'tests.log', 'build.log', 'wasm.log', 'envelopes.log',
             'proof.log', 'proof.json', 'requests.jsonl', 'expected.jsonl', 'oracle.stdout.jsonl',
             'rust.stdout.jsonl', 'old-helper.stdout.jsonl', 'oracle-provenance.json',
             'registered.json', 'registered.log']:
    shutil.copyfile(work / name, out / name)
(out / 'scripts').mkdir()
for path in [Path(__file__), Path(__file__).with_name('proof.py'),
             Path('C:/Users/KarlVonRichter/.t3/md5-literal-escaped-check.sh')]:
    (out / 'scripts' / path.name).write_bytes(path.read_bytes())
audit = {'source_commit': source, 'baseline': base, 'exact_export_source_files': source_hashes,
         'export_archive_sha256': sha(Path(__file__).with_name('code.tar').read_bytes()),
         'production_shim_prefix_unchanged': True, 'protected_paths_unchanged': protected,
         'prior_199_requests_expected_and_actual_stdout_are_unchanged_prefixes': True,
         'oracle_reused_from': '65d5b0a9732551db35a9348d69d7b2ccfa86a1fb',
         'oracle_provenance_unchanged': True, 'fresh_cpp_build': False,
         'default_timeout_seconds': 60, 'carry_case_included': True}
(out / 'source-audit.json').write_bytes((json.dumps(audit, sort_keys=True, indent=2) + '\n').encode())
(out / 'README.md').write_text('''# Literal escaped MD5 marker keys

Tested source: `da17dc54dc2b2be2b15cb4a2c6c171c62950a5cc`.

Independent review of c5ec29e closed the runtime reserved-object P2 but found a
P3 coverage gap: `reserved-escaped-id`, `reserved-escaped-byte` and
`reserved-escaped-unit` use plain marker keys. Escaped quotes inside their VALUES
do not cover escaped KEYS. Only the old duplicate-key fixture escaped a key;
that refusal happens before object conversion. The nonduplicate escaped-key
claim in the 65d5b0a9 evidence is withdrawn here; that historical evidence and all
199 prior checks remain unchanged.

The default Python guard appends three nonduplicate literal key tokens, using
escaped dollar signs and `Raw\\u0056alue`, plus a successful recovery. It checks
their raw spelling and decoded keys. The Rust shim adds three complete-envelope
assertions with literal raw key spellings. Default oracle CTest registration is
unchanged and calls the updated Python script; the Rust test is not ignored.

Fresh checks, private Linux tree, actual shared mkdir lock, at most eight jobs:

- fmt and warnings-denied Clippy pass; all 11 shim tests pass, none ignored.
- Native selected-package shim and portable MD5 wasm build pass.
- Both real adapters match 203 complete replies: 197 refusals and six successes.
  The old 199 requests, expected replies AND actual adapter stdout remain exact
  byte prefixes. The pre-fix helper fails all three new refusals; recovery matches.
- The unchanged exact runner checks all 5,973 cases at the default 60-second
  per-process budget, including the streamed 512 MiB carry case, with zero
  failures/deferrals. Output hashes equal the existing corpus/golden proof.
- The exact LF source export and all Rust/Cargo inputs match the tested Git
  source; production shim code, portable MD5, shared parser/comparator, dependency
  lock, cases, goldens, unit records and historical evidence are unchanged.

The C++ oracle and exact runner are REUSED, with identity and live embedded
provenance checked before execution. Oracle SHA256 is
`ef081cadadaadc81a14b966e492d312872ef3429e20d02ee44ef51af98ead6d1`, from the
qualified 65d5b0a9 build; its original reference-export representation and
provenance are preserved. No new C++ build, four-CTest run, raw-LF reference
export, C++ source/link/FP audit or expanded compiler/architecture qualification is claimed.
Fresh Rust binary identities are in proof.json and registered.json. Earlier full
reference-source/link/FP audit remains prior qualified evidence.

This additive correction does not rebless results or change green/graph/gate
acceptance. Independent review and final hosted CI remain required.
''', encoding='utf-8', newline='\n')
lines = [f'{sha(path.read_bytes())}  ./{path.relative_to(out).as_posix()}\n'
         for path in sorted(out.rglob('*')) if path.is_file()]
(out / 'files.sha256').write_bytes(''.join(lines).encode())
print(f'Packaged {len(paths)} exact source inputs, 203 replies and full 5973-case proof.')
