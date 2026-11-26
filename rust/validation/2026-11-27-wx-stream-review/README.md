<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Corrected wx stream independent review

Fresh Sol6.1 review of058165f437 requests changes. The [full report](review.md)
binds the exact source/tree, reproductions and verification. Earlier ordinary
exception, setup-overlap, termination-boundary and attribution findings are
substantially corrected; runtime facet delegation has no private formatter
interposer. The new four source-contract findings are ambient unwind context,
mutable facet ios state/padding, forced-unwind propagation and null-buffer state.
These are readiness blockers even with an honestly partial record.

Parent fix4aaae34988 removes unused allocating Clone from StreamBuf/OStream and
the shim Sink. All20focused tests, fmt, clippy, portable wasm checking and the
4719case/140control generator check pass. Insertion/conversion behavior, public
inputs, refusals and goldens are unchanged. This closes one static allocation
finding without claiming the four behavior gaps resolved. Draft/partial and no
golden remain; substantive corrections await the requested implementation route.

The review also establishes880CRLF-only differences across918saved local pinned
files. Their manifest agreement is not raw Git-byte identity or target-stack
qualification. The complete report distinguishes the genuine local/profile
comparisons and wasm runs from pending hosted target evidence, and records
termination-handler allocation and unchanged Windows difftest limits.

[review.json](review.json) records the parent correction and checks. Fresh
independent review is required after the remaining source corrections; none is
dispatched merely to recheck this bounded removal while those findings remain.
