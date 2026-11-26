N=~/kicad-port/canonical-adopt-merge-98a12c11
~/kicad-port-extractor-venv/bin/python /mnt/c/Users/KarlVonRichter/.t3/canon-adopt-merge/scripts/mdiff.py $N/committed-measurement.json $N/final/port/extractor/generated/measurement.json
cat $N/logs/validate-default.json; cat $N/logs/validate-strict.json | head -c 600; cat $N/logs/runner-check.log; cat $N/logs/measurement.log | tail -5
