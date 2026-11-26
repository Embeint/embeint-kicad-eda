"""Re-run the canonical split over the merged unit records in a scratch copy; report byte/field deltas."""
import hashlib, json, shutil, subprocess, sys, tempfile, tomllib
from pathlib import Path
root = Path(sys.argv[1]); out = Path(sys.argv[2])
G = root / 'port/extractor/generated'
sha = lambda b: hashlib.sha256(b).hexdigest()
tmp = Path(tempfile.mkdtemp())
(tmp / 'port/units').mkdir(parents=True)
for p in (root / 'port/units').glob('*.toml'):
    (tmp / 'port/units' / p.name).write_bytes(p.read_bytes().replace(b'\r\n', b'\n'))
gd = tmp / 'port/extractor/generated'; gd.mkdir(parents=True)
for n in ('graph.json.gz', 'inventory.json', 'units.json'):
    shutil.copyfile(G / n, gd / n)
shutil.copytree(G / 'sources', gd / 'sources')
before = {p.name: p.read_bytes() for p in (tmp / 'port/units').glob('*.toml')}
before_rep = (gd / 'units.json').read_bytes()
runs = []
for i in range(2):
    r = subprocess.run([sys.executable, str(root / 'port/tools/split_units.py'), '--graph', str(gd / 'graph.json'), '--inventory', str(gd / 'inventory.json'),
                        '--units-dir', str(tmp / 'port/units'), '--report', str(gd / 'units.json'), '--source-root', str(root)],
                       capture_output=True, text=True, cwd=root)
    assert r.returncode == 0, r.stderr
    runs.append({p.name: sha(p.read_bytes()) for p in (tmp / 'port/units').glob('*.toml')} | {'units.json': sha((gd / 'units.json').read_bytes())})
after = {p.name: p.read_bytes() for p in (tmp / 'port/units').glob('*.toml')}
changed = sorted(n for n in after if after[n] != before.get(n))
semantic = sorted(n for n in changed if tomllib.loads(after[n].decode()) != tomllib.loads(before[n].decode()))
res = {'units': len(after), 'byte_changed': changed, 'semantic_changed': semantic,
       'report_changed': (gd / 'units.json').read_bytes() != before_rep, 'repeat_identical': runs[0] == runs[1]}
out.mkdir(parents=True, exist_ok=True)
for n in changed:
    (out / n).write_bytes(after[n])
print(json.dumps(res, indent=1))
shutil.rmtree(tmp)
