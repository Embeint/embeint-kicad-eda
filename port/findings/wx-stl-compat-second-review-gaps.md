<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Corrected wx stream review: remaining source behavior

Independent Sol 6.1 review of head058165f437 requests changes. The previous
ordinary exception, setup overlap, termination boundary, runtime-facet binding
and attribution corrections are substantially sound, but passing corpus results
and documented partial status do not resolve these remaining behaviors:

- P1: missing ambient outer-unwinding context can falsely terminate after the
  first literal; the genuine insertion during outer unwinding writes all output
  with zero syncs. Context must reach main and tied sentries.
- P2: the facet callback lacks mutable ios state. A real facet can set width,
  precision and flags before stock formatting, reaching integer padding paths.
- P2: forced unwind needs a distinguished host signal and unconditional
  propagation after badbit; it cannot be swallowed as an ordinary exception.
- P2: a null rdbuf has distinct clear/sentry/flush state transitions and must
  be represented explicitly, rather than by a failing real buffer.

The fifth finding, allocating derived Clone, is corrected by removing Clone
from StreamBuf and OStream and from the shim Sink that contains them. The
existing refusal-before-change test compares an independently initialized stream
instead of heap-cloning it. No insertion, conversion, public request, refusal,
comparison or golden changes. This is a bounded parent correction, not resolution
of the other four source-contract findings.

The reviewer independently reproduced20focused tests, both4719case local-profile
comparisons,140controls and both complete import-free wasm replays. It verified
that no private formatter/cache interposer exists. Four Windows difftest failures
come from unchanged accepted sources: three missing c++ executable cases and a
CRLF stderr-count mismatch.

Local source provenance caveat: the saved918-file reference manifest matches its
saved files, but880files differ from raw pinned Git blobs by CRLF only. These
runs are diagnostic and must not be described as byte-identical raw-blob exports
or target qualification. Actual new-head hosted evidence still needs collecting.
The adapter termination reporter allocates JSON and may fail under exhausted
allocation; successful ordinary termination observations are genuine but do not
qualify that resource-failure path. The prior immutable finding remains intact.

Keep draft/partial with no golden. Substantive corrections stay with the
requested implementation route; no new dispatch or reviewer retry is started
while that route is quota-limited. A fresh independent review is required after
those corrections.
