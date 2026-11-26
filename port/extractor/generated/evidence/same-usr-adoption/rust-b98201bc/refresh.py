from pathlib import Path
import collections,hashlib,json,shutil,subprocess,sys,tomllib
root=Path.cwd();base='b98201bc6b4c9e90cf725a0ab62b29d7b7b37a42';prior='f7dd7eac749c8d5c467b379da382f880a246a985';meta=Path('C:/Users/KarlVonRichter/AppData/Local/Temp/rust-final-integration/graph94-regenerated');g=root/'port/extractor/generated';e=g/'evidence/same-usr-adoption/rust-b98201bc';e.mkdir(exist_ok=True)
for p in meta.glob('*.toml'):shutil.copyfile(p,root/'port/units'/p.name)
def blobs_at(rev):
 entries=subprocess.check_output(['git','ls-tree','-r','-z',rev,'--','port/units']).split(b'\0');proc=subprocess.Popen(['git','cat-file','--batch'],stdin=subprocess.PIPE,stdout=subprocess.PIPE);out={}
 for item in entries:
  if not item:continue
  fields,name=item.split(b'\t',1);oid=fields.split()[2];proc.stdin.write(oid+b'\n');proc.stdin.flush();header=proc.stdout.readline().split();size=int(header[2]);data=proc.stdout.read(size);assert proc.stdout.read(1)==b'\n';out[Path(name.decode()).stem]=tomllib.loads(data.decode())
 proc.stdin.close();assert proc.wait()==0;return out
accepted=blobs_at(base);reviewed=blobs_at(prior);units={p.stem:tomllib.loads(p.read_text()) for p in sorted((root/'port/units').glob('*.toml'))};keys={'depends_on','soft_depends_on','layer'}
assert units.keys()==accepted.keys()==reviewed.keys()
for uid,u in units.items():
 assert {k:v for k,v in u.items() if k not in keys}=={k:v for k,v in accepted[uid].items() if k not in keys},uid
 assert {k:u[k] for k in keys}=={k:reviewed[uid][k] for k in keys},uid
print('METADATA_AND_GRAPH_FIELDS_VERIFIED',len(units),flush=True)
sys.path.insert(0,str(root/'port/tools'));from extraction_report import measure_units
from extractor.model import read_json,write_json
measurement=read_json(g/'measurement.json');old_units=measurement['units'];measurement['units']=measure_units(list(units.values()),read_json(g/'units.json'));write_json(g/'measurement.json',measurement)
assert measurement['units']['statuses']==dict(sorted(collections.Counter(u['status'] for u in units.values()).items()))
print('MEASUREMENT_REFRESHED',measurement['units']['statuses'],flush=True)
from drift_report import report
graph=read_json(g/'graph.json');assert hashlib.sha256((g/'graph.json.gz').read_bytes()).hexdigest()==hashlib.sha256(subprocess.check_output(['git','show',prior+':port/extractor/generated/graph.json.gz'])).hexdigest()
checks={}
for include,name in [(False,'drift-ported.json'),(True,'drift-all.json')]:
 d=report(root,graph,list(units.values()),base,include);write_json(g/name,d);checks[name]={'target':d['target'],'ported_units':d['ported_unit_count'],'affected_units':len(d['affected_units']),'sha256':hashlib.sha256((g/name).read_bytes()).hexdigest()};print('DRIFT_REFRESHED',name,checks[name],flush=True)
result={'accepted_rust':base,'prior_reviewed_head':prior,'merged_head':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'unit_count':len(units),'all_metadata_equals_accepted':True,'all_graph_fields_equal_reviewed':True,'canonical_split':{'iterations':2,'semantic_changes':[],'only_byte_changes':sorted(p.name for p in meta.glob('*.toml')),'report_changed':False,'repeat_identical':True},'graph_sha256_unchanged':hashlib.sha256((g/'graph.json.gz').read_bytes()).hexdigest(),'measurement_statuses':measurement['units']['statuses'],'old_measurement_statuses':old_units['statuses'],'drift':checks,'no_new_parse':True}
write_json(e/'refresh.json',result);print('REFRESH_COMPLETE',flush=True)
