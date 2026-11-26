<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Same-USR adoption after the Unix ports

Addendum to [the adoption record](extractor-same-usr-adoption.md). The original
record and evidence remain unchanged. Only accepted rust
`b98201bc6b4c9e90cf725a0ab62b29d7b7b37a42` was merged into reviewed graph head
`f7dd7eac749c8d5c467b379da382f880a246a985`, producing merge `0fb0aeaf23` without
conflicts. No open topic was imported.

Canonical splitter regeneration ran twice over the merged records. All 1,801
unit values are preserved; only the new policy/sysinfo difftest tables change
key order. The report and repeated outputs are byte-identical. Every metadata
field outside dependencies/layer equals accepted rust; every graph-derived
field equals the previously reviewed graph head. Header co-port notes from
both accepted Unix units remain intact.

The measurement's units section is regenerated with the canonical
`extraction_report.measure_units` function; its extraction/provenance sections
are unchanged. Statuses are 3 green, 26 partial, 64 blocked and 1,708 todo.
Both drift reports are regenerated through `drift_report.report` against
accepted rust `b98201bc6b`; they cover 29 ported units. Their target is this
explicit committed source revision, not an uncommitted worktree. No graph,
parser, aggregation, function, edge, pin, corpus, golden or implementation
changes in this refresh. It is not a new parse or runtime qualification.

Evidence is in
`port/extractor/generated/evidence/same-usr-adoption/rust-b98201bc/`.
Windows extractor/tool/site/policy suites and default validation pass. Strict
validation retains exactly the known incomplete-extraction diagnostic. The
prior full WSL/libclang suite remains historical evidence; it is not relabelled
as a fresh run. The four fmt failures, conservative cross-TU endpoints and
incomplete all-configuration coverage remain unchanged. Fresh independent
integration review is required on the final committed refresh.
