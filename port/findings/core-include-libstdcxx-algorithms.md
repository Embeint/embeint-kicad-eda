<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# FilterKruskal's results depend on libstdc++'s sort, nth_element and partition

Owning unit: `libs.core.include.core.application.pure.p001`. Source: pinned
`libs/core/include/core/filter_kruskal.h` at `4cf939778df04d2b747bb2c4714fc5014a7565c7`.

`KI_MST::FilterKruskal` documents that it reorders `aEdges`, and with a comparator that
orders only weights (a strict weak ordering with ties) the chosen tree also depends on the
order the algorithms leave equivalent edges in. The C++ standard fixes neither: it states
what `std::sort`, `std::nth_element` and `std::partition` establish, not how. The reference
behaviour is therefore that of the oracle's standard library. Using Rust's `sort_unstable`,
`select_nth_unstable` or `partition` would give a valid minimum spanning forest but a
different span arrangement, and a different tree on ties.

`kicad_core::stdalgo` transcribes the GNU libstdc++ algorithms as compiled at the pin:
introsort (`_S_threshold` 16, median-of-three into the first position, unguarded
partition, depth limit `2 * __lg( n )` with the `__partial_sort` heap fallback, then the
final insertion sort), introselect (the same pivoting, `__heap_select` fallback, insertion
sort below four elements) and the bidirectional `__partition` libstdc++ selects for random
access iterators, read from GCC 15's headers (the local WSL oracle). The hosted oracle
uses GCC 13 (`ubuntu-24.04`); its exact comparison is the check that the bodies agree
there too. libstdc++ is GPL-3.0 with the runtime library
exception; the transcription is licensed GPL-3.0-or-later with the rest of the port.

Evidence: the unit's 166 `filter_kruskal` cases compare the emitted edges, the reordered
span and the union-find parents exactly, including weight-only comparators with heavy ties
and 12 inputs from McIlroy's adversary (`adversary-weights.json`) that reach introsort's
and introselect's heap fallbacks (verified by `stdalgo`'s
`adversary_reaches_the_heap_fallbacks` test). A future oracle on another standard library
(MSVC, libc++) would need its own transcription and comparison.
