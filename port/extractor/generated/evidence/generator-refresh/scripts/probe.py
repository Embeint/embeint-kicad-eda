import json,sys,collections
from pathlib import Path
W=Path.home()/'kicad-port/generator-refresh-57e'
rep=json.loads((W/'refreshed/refresh-report.json').read_text())
ch=rep['changed_build_generated_entries']
rem=collections.Counter(p for c in ch.values() for p in c['removed'])
add=collections.Counter(p for c in ch.values() for p in c['added'])
print('removed',rem.most_common(10)); print('added',add.most_common(10))
cap=json.loads((W/'capture/generator-dependency-capture.json').read_text())
outs=('.o','.obj','.a','.so','.dll','.exe')
deps={**cap['source_generator_dependencies'],**cap['generator_inputs']}
leak=collections.Counter(p for v in deps.values() for p in v if p.endswith(outs))
print('leak',leak.most_common(10), [k for k,v in deps.items() if any(p.endswith(outs) for p in v)][:10])
sd=cap['source_generator_dependencies']
print('srcdeps', {k:v for k,v in sd.items() if v})
