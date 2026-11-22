import shutil,subprocess,sys,tempfile,pathlib
src=pathlib.Path('.').resolve()
muts={
 "drop-iec-target-from-closure":("    if targets != {", "    if targets - {\"iec60664-reference\"} != {\"core\", \"clipper2\", \"kimath\", \"fmt\", \"resistor-reference\", \"calculator-reader-reference\", \"kicad-oracle\"} and targets != {"),
 'drop-iec-source-check':('raise ValueError("IEC60664 reference target must compile the actual pinned source")','pass'),
 'drop-iec-archive-check':('raise ValueError("Oracle does not link the actual IEC60664 archive")','pass'),
 'drop-resistor-from-closure':('"fmt", "iec60664-reference", "resistor-reference",\n                   "calculator-reader-reference", "kicad-oracle"}','"fmt", "iec60664-reference",\n                   "calculator-reader-reference", "kicad-oracle"} | (targets & {"resistor-reference"})'),
}
for name,(a,b) in muts.items():
    with tempfile.TemporaryDirectory() as d:
        d=pathlib.Path(d)/'o'; shutil.copytree(src,d,ignore=shutil.ignore_patterns('__pycache__'))
        t=(d/'audit.py').read_text(); assert t.count(a)==1,name; (d/'audit.py').write_text(t.replace(a,b))
        r=subprocess.run([sys.executable,'-m','unittest','test_iec60664_guards'],cwd=d,capture_output=True,text=True)
        print(name,'->','FAILS (control effective)' if r.returncode else 'PASSES (control ineffective)', r.stderr.strip().splitlines()[-1])
