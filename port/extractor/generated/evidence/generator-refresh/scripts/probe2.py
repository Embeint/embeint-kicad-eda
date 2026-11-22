import json,collections
from pathlib import Path
W=Path.home()/'kicad-port/generator-refresh-57e'; V5=Path.home()/'kicad-port/corrected-review-v5'
outs=('.o','.obj','.a','.so','.dll','.exe')
m=json.loads((V5/'inputs/generated-sources.json').read_text())
old={e['path']:e['generator_inputs'] for e in m['files']}
print('old entries with outputs', sum(any(p.endswith(outs) for p in v) for v in old.values()), 'old .o total', sum(p.endswith(outs) for v in old.values() for p in v))
print('old entries with @build/.', sum('@build/.' in v for v in old.values()))
cap=json.loads((W/'capture/generator-dependency-capture.json').read_text())
new=cap['generator_inputs']
print('new build entries with outputs', sum(any(p.endswith(outs) for p in v) for v in new.values()))
sd=cap['source_generator_dependencies']
print('src outputs with outputs', sum(any(p.endswith(outs) for p in v) for v in sd.values()), 'of', len(sd), 'nonempty', sum(bool(v) for v in sd.values()))
print('sizes', sorted((len(v),k) for k,v in sd.items())[-5:])
import subprocess
b=cap['build_root']; s=cap['source_root']
for t in ['pcb_calculator/attenuators/pi_formula.h']:
  r=subprocess.run(['ninja','-C',b,'-t','query',s+'/'+t],capture_output=True,text=True); print(r.stdout[:1500])
