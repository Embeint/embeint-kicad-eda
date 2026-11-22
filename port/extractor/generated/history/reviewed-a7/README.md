# Extraction evidence

The completed raw and reconciled runs are independent measurements. Canonical
`port/units/` records come from the reconciled graph with the recorded physical
EOF normalization. No unit is ported or oracle-green.

| Measurement | Raw | Reconciled |
| --- | ---: | ---: |
| Recorded commands | 2,471 / 2,471 | 2,511 / 2,511 |
| Successful parses | 2,462 | 2,507 |
| Proven-deleted/excluded commands | 4 | 0 |
| Other failed commands | 5 | 4 |
| Definition entries | 68,939 | 69,416 |
| Call edges | 573,759 | 559,555 |
| Observed override relations | 10,530 | 10,580 |
| Unresolved TU/configuration call records | 5,216,898 | 5,247,105 |

`measurement.json` reports independent database, graph, source-manifest and inventory
hashes, coverage flags, diagnostics, source counts, layers and classifications.
`raw/` preserves historical graph/inventory and deletion-aware coverage.
`reconciled-extraction/` preserves the original reconciled AST graph/inventory.
The root graph/inventory and `units.json` are canonical. `evidence/artifacts.json`
hashes the uncompressed database/log bytes in adjacent gzip files.

| Artifact | SHA256 |
| --- | --- |
| Raw database | `10519703c213d66a3abf4e00fa091bdaa616d9865c4ec47ecad0acca91c2fa8b` |
| Reconciled database | `bc402ade70f0304ed5018793a5d8c37749df978dfd7859ee87e655ee3a616196` |
| Raw graph gzip | `a4768bc824e8947f3373dbec4afc939c91ac94ee6ac9ff22355eeedf70a2e516` |
| Original reconciled AST graph gzip | `133c45e58c4f42fade25280971cf2f9c67433bbe83efb0051cbeddef24e3967f` |
| Canonical graph gzip | `df959d0e3b8db15fe299c6f6dad84797f73ef22cf4948e75132ebc1ffbce4122` |
| Canonical units report | `03389a47455a92d3d60fc63c1ad02ace5cbd6e77d9fb53f1ebab009a0d925d0e` |

Source snapshot `3cd2c9afc6d2749b7d3a79fd1deaa86ac51cb3a7` and upstream pin
`4cf939778df04d2b747bb2c4714fc5014a7565c7` cover 9,401 tracked C/C++ files:
1,946,361 application/generated lines, 340,033 QA lines and 1,059,199 third-party
lines, measured separately. Units own 1,956,434 physical lines, including 10,073
Clipper2 lines. There are 1,506 todo and 64 blocked records; 385 layers (0-384,
393 L0 units); 45,695 hard edges; 8,631 back-edges with dropped weight 73,460.
Pure / thin-replacement / UI classifications are 333 / 130 / 1,107 units and
292,894 / 63,635 / 1,599,905 lines. 170 units exceed the 2,000-line target
(maximum 5,485): function boundaries and conservative enclosing ranges remain
intact. wx coupling does not prove portability or measure implementation effort.

`evidence/deleted-source-proof.json` proves four sources existed before deletion
commit `3ac0ec003d72760106d673493225b7eda9cf54e8` and were absent at both pins.
One historical cached failure remains a failure; three were excluded without
invoking clang. Other raw failures are a stale generated protobuf enum header
and four fmt module commands. Private generated headers resolve protobuf in the
reconciled run. All four fmt commands retain their unsupported GCC module flags
and failure diagnostics. No flags were stripped or invented.

Raw parser/aggregator identity is
`aabb3c0be3176689df4f9d383aa145d6613ab48c354f7d3380f781c4fb6ad89a`.
`evidence/cache-reuse-audit.json` proves 1,136 initial reusable results; the raw
graph did not store cache-hit flags, so its per-result cache counter does not
reconstruct that resume. The runtime wrapper added proven-deletion guards and
later gzip compatibility without rewriting historical AST logic.
`resume-raw-current.py` is runtime evidence, not a standalone repository entry
point. Original caches and lossless compaction proofs remain.

The first reconciled aggregation rejected primary-template USRs differing only
in expression whitespace. `template-owner-variants.json` preserves the observation;
the fix retains all observed owners and each TU's resolved calls.
`aggregation-cache-migration.json` proves unchanged parser/model bytes, inventory
and compiler arguments for 288 migrated results. Corrected extractor identity is
`95743d0fe1c1967ae7609b2afecb59ea7e26c8b2ac41361d058ac8f32aff0806`.
Original logs/cache remain in the private reconciliation tree.

The full graph owns 244 additional same-USR bodies and ten conservative enclosing
ranges. Three recovered extents in `gal_modern.cpp` and `step_pcb_model.cpp` end
at clang's exclusive EOF line after a trailing LF. `split_units.py` verifies exact
source/body hashes, byte EOF and enclosing-range tag before clamping the inclusive
physical line. It retains `clang_end_line`, original graph hash, splitter hash
and three normalization records. Failed initial validation/measurement remain in
`evidence/failed-initial-generation/`; those counts are historical. No reparsing
or source edit was used to correct accounting.

`evidence/repeat-generation.json` confirms identical graph/report bytes and all
1,570 unit files on repeat generation, plus unchanged hashes of 178 private
generated inputs. Those inputs were frozen: external/generated headers are not
covered by the tracked-source cache fingerprint. `aggregation-probe-*` and
`pilot-audit.log` are earlier diagnostic subsets; [PILOTS.md](../PILOTS.md) and
the full unit records supersede their scopes.

`finalization.json` records default validation success and expected strict failure.
Inventory and compile-database processing are complete; parsing and extractor gate
remain incomplete because of fmt. All-configuration completeness remains false.
63 uncovered application/generated implementation sources remain accounted for
by blocked records. Indirect/dependent calls and inactive platform branches remain
explicit limitations. The plan's three-live-green-pilot gate is not satisfied.

`drift-ported.json` has zero ported units, so proves no parity. `drift-all.json`
compares the upstream pin to its recorded committed target: 56 changed files,
188 direct function-range findings, 1,197 affected units and 1,197 conservative
include/declaration dependency reviews. These include fork changes already present
at extraction; they are review requests, not newly measured regressions.
Working-tree edits and uncompiled include branches are outside that comparison.
See [VALIDATION.md](VALIDATION.md) for final checks.
