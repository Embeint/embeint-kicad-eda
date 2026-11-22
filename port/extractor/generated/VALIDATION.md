# Final extraction validation

Validated implementation commit `21839dda17a2fe3a421469b6f4c35dba2951a2ac` and the
generated records published with this report, on 2026-11-22. The following evidence
does not certify a ported unit or the plan's three-pilot gate.

| Check | Result |
| --- | --- |
| WSL extractor suite: `python -m unittest discover -s port/extractor -p 'test_*.py' -v` | 23 passed, including three real-libclang integration tests; `evidence/extractor-tests-final-wsl.log` |
| Windows extractor suite, same command | 20 passed, three libclang tests skipped on this host |
| Progress suite: `python -m unittest discover -s port/site -p 'test_*.py' -v` | 22 passed |
| Bootstrap suite: `python -m unittest discover -s port/tools -p 'test_*.py' -v` | 9 passed |
| Evidence-policy suite: `python -m unittest discover -s rust/kicad-difftest/tests -p test_policy.py -v` | 7 passed |
| `python port/tools/validate_units.py --json` | Passed with source hashes on Windows checkout; `validate.log` |
| Same validator against the private WSL source snapshot | Passed; `evidence/validate-snapshot.json` |
| Validator with `--require-complete` | Expected exit 1; retained fmt failures; `validate-strict.log` |
| Repeat graph/report/unit generation | Identical graph/report bytes and all 1,570 TOML files; frozen 178 generated inputs unchanged; `evidence/repeat-generation.json` |
| Pinned solidcore loader/schema | All 1,570 records parsed and round-tripped with core fields and extensions intact; `evidence/final-artifact-audit.json` |
| Artifact identity audit | All raw, original reconciled and canonical artifact sizes/hashes match measurement; decompressed database/log hashes match; every blob below 100 MB |
| Pilot scope audit | Final L0/pure/todo records have no dependencies/gaps, correct line/definition counts, and source bytes identical to the upstream oracle pin |
| Both drift CLIs | Exit 0; committed target recorded in each JSON |
| `python port/tools/progress.py --out <private-output>` | Generated the site with 1,570 records, phase 0 explicitly unverified |
| `python port/tools/check_goldens_policy.py --base origin/rust` | Passed; no changed goldens |
| Evidence-policy CI command with final records | Passed structural/infrastructure checks; 0/3 live green L0 units, phase 0 INCOMPLETE; `evidence/phase0-policy/phase0.json` |

The policy command's runner/oracle paths are unused because this extraction PR
has no cases, goldens or green records. It does not claim a live comparison.
No new C++ application build was needed for extractor validation. The separate
own-library oracle was merged through the reviewed base; its parent-reported
build results are not represented as new extraction evidence here.

Source, manifest, database, graph and report SHA256 values and all command
diagnostics remain in [measurement.json](measurement.json). Both original AST
graphs and the canonical normalized graph are committed. The original EOF
validation failure is retained under `evidence/failed-initial-generation/`;
the byte-proven correction is covered by a regression test.

Drift over ported records has zero entries because there are no ported records.
The all-record report has 56 changed files, 188 direct function-range findings,
1,197 affected units and 1,197 conservative dependency reviews against the pin.
Fork changes already in the extraction snapshot are included in that comparison.
This is an audit queue, not evidence of 1,197 new behavioural regressions.

Remaining limits are four fmt GCC-module parse failures, 63 uncovered
application/generated implementation sources (64 blocked units), inactive
platform/configuration branches, and unresolved indirect/dependent calls.
Inventory/database processing are complete; parsing, extractor gate and
all-configuration completeness are false. The phase 0 live-pilot gate remains
incomplete. [PILOTS.md](../PILOTS.md) supplies exact unit scopes, source spans and
required oracle cases for MD5, E-series/resistor substitution and IEC60664.
