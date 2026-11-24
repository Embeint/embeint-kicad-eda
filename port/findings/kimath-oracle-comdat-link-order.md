<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Preserve matrix template selection when adding the hash adapter

At `f04de29400ffc23c8f7e2d7a591f4095ab4cffdb`, native object-reference run
45366439386 built and verified both images, the oracle and its actual capture.
The downloaded artifact SHA-256 is
`13a5ff44315580c9daded97b3a3b979b8b501409d2dcddf340fab9fbc0eb16b8`;
the complete ZIP, member CRCs and all 981 captured-file hashes were verified.

The gate rejected the integrated reference, as intended. The sorted shim glob
places `kimath_header_hash.cpp.o` before `kimath_math.cpp.o`. GNU ld consequently
selects some weak/COMDAT JSON and string helpers from the hash object instead of
the measured matrix object. Nine measured home sections become absent from the
selected-home inventory, twelve extra provider units appear, and all seven
context-callee fingerprints change. The seven arithmetic graphs still match;
that does not authorize accepting changed context.

Some replacement bodies are genuinely different compiler outputs: for example,
the selected const JSON `at` helper is 717 bytes versus 771 bytes in the matrix
object, and `push_back` is 886 versus 1109 bytes. A byte-equivalence exception
would therefore be incorrect. Reordering proof candidates also cannot establish
equivalence of their transitive local callees and is not used.

The oracle build now places the hash shim immediately after the matrix shim,
preserving every other shim's relative order. The protocol-dispatch test uses
the same list. This preserves the measured template selection without changing
either compiled adapter, an arithmetic graph, a reference lock or a golden.
Actual linked-byte, context and graph checks still fail on any unreviewed body.

Fresh native object eligibility and both matrix/hash live validation must rerun
on this order. The named context review and the math golden remain pending;
the prior ordinary rust-port run failed on the absent math golden.
