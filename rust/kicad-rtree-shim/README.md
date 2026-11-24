<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Dynamic tree JSONL differential adapter

Unit `libs.kimath.include.geometry.rtree.application.pure.p001`. Input is exactly
`{config,steps}`; configs are i32.d2.m4/m16/m28, i32.d3.m3/m5, i32.d1.m8,
i64.d2.m16 and i64.d3.m8. 1..4096 operations per request, <=65536 items and IDs
0..65535. IDs denote real stable ITEM pointers on C++, Item handles on Rust.

Operations: insert/remove `{min,max,data}`, bulk `{entries:[{min,max,data}]}`,
clear/state `{}`, iter `{limit}` (negative unlimited), query `{min,max,stop}`
(nonpositive unlimited visitor), nearest `{point,k}`, move `{kind}`
(construct/assign/self). Each operation reports its observable value and current
size/empty/reference-memory accounting. State includes full traversal order and
the header's unusual iterator equality. Query returns visitor count/hits plus the
full lazy range and emptiness. NN starts with a nonempty output vector, so empty
and k<=0 paths prove clearing. Mutation requests are independent; a per-line
refusal is followed by a valid request in the separate guard suite.

Generate/check corpus with `generate_cases.py [--check]`. `test_inputs.py <binary>`
checks refusals separately; it is never included in valid-result comparison. See
[the owning crate audit](../kicad-rtree/README.md) for valid-domain and native UB
preflight limits. The live workflow runs the unchanged pinned own-library oracle,
compares debug/release native Rust and a WASI execution of the same shim byte/bit
exactly through kicad-difftest, and builds the portable crate for unknown-unknown.
No golden generation, tolerance, lookup table or reference fallback exists.
