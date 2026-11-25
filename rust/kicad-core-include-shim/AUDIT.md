<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# libs/core/include/core p001 source audit

Owned unit: `libs.core.include.core.application.pure.p001`, 1,404 lines in 13 headers
(`arraydim.h`, `base64.h`, `filter_kruskal.h`, `ignore.h`, `minoptmax.h`, `observable.h`,
`profile.h`, `spinlock.h`, `sync_queue.h`, `type_helpers.h`, `union_find.h`,
`version_compare.h`, `wx_stl_compat.h`), 67 extracted USRs. Reference pin
`4cf939778df04d2b747bb2c4714fc5014a7565c7`; the worktree's copies of these headers equal the
pin. Base: `origin/rust` at `c45b38a296` (including #71, `libs.core.application.pure.p001`).
No open pull request existed when the unit was claimed (#73).

Read in full: the 13 pinned headers, `libs/core/observable.cpp` and `profile.cpp` (the
dependency unit's bodies the templates call), the upstream QA tests
`qa/tests/common/test_union_find.cpp`, `test_filter_kruskal.cpp` and `test_observable.cpp`,
the callers of every template (`pcbnew/ratsnest/ratsnest_data.cpp`, the
connectivity/zone-filler spinlock users, `footprint_info*` and `pcm_task_manager.h` queue
users, the DRC `MINOPTMAX<int>`/`<long long>` users, `trace_helpers.cpp`,
`gal_display_options.cpp`, the QA profile tools) and libstdc++'s `bits/stl_algo.h`,
`bits/stl_heap.h` and `chrono` (`duration_cast`, `system_clock`).

`base64.h`, `version_compare.h` and `wx_stl_compat.h` contain only declarations; their
bodies belong to `libs.core.application.pure.p001` or to `common` (the wx stream and
`std::less<wxPoint>` helpers, not extracted into this unit), and `std::hash<wxPoint>`'s
operator is deleted. `always_false` is a compile-time trait with no runtime body.

## Evidence by owned symbol

Counts are corpus cases compared exactly against the pinned oracle in local WSL (GCC 15);
all 669 succeeded on both sides, none deferred. Controls are refusals both sides must give.

| Upstream symbol(s) | Rust | Branches implemented and checked | Remaining gap |
| --- | --- | --- | --- |
| `PROF_TIMER` ctors (named with/without `aAutostart`, default), `Start`, `Stop` (running and not), `SinceStart<D>` (running/stopped, since start/last, `m_lasttime` update), `msecs`, `Show`, `to_string`, `GetName` | `profile::ProfTimer`, `ChronoDuration` for eight `std::chrono` durations | 132 `profile` scripts: unit selection at 999/1000/999999/1e6/1e9 boundaries, negative and huge counts, `%g` precision-6 rounding and ties, `µs` bytes, empty and multibyte names, default argument and explicit `false` for `since_last`, every duration's `duration_cast` (float division, integer truncation toward zero), epoch values of a never-started timer, restart and double stop | `to_string`'s `"none"` branch is reachable only through the uninitialised `m_started` (control `indeterminate m_started`; [upstream issue](../../port/upstream-issues/core-prof-timer-uninitialised-started.md)); streams other than a default-state `std::ostream` |
| `SCOPED_PROF_TIMER<D>` ctor/dtor | `profile::ScopedProfTimer::{new, finish}` | Construction order (base then `m_counter`, two clock reads), destructor read, base operations that do not affect the result, all eight durations | none |
| `PROF_COUNTER` ctors, `Count`, `Reset`, `operator++( int )`, `Show` | `profile::ProfCounter` | `"Anonymous"`, empty and other names, wrap from 2^64 - 1 (count set through the private member) | Atomicity (no threads in portable code) |
| `LATENCY_PROBE` ctor (default and explicit `aStages`), `Reset`, `Checkpoint`, `AddTimer`, `delta_ms` | `profile::LatencyProbe::{new, reset, checkpoint, add_timer, delta_ms}` | Negative stages throw `length_error("vector::reserve")`; `AddTimer` of stopped/never-started/running timers (`msecs()` side effects); `delta_ms` over the int64 range incl. both limits; `to_string` with negative, tie and far-apart checkpoints; wired into `libs.core.application.pure.p001`'s `to_string` (its 25 former deferrals now compare) | Overflowing differences are controls |
| `OBSERVABLE<T>` ctor, copy ctor, `SubscribeUnmanaged`, `Subscribe`, `Unsubscribe`, `Notify`, `NotifyIgnore` | `observable_template::{Observable, notify, notify_ignore, NotifyHost}` | 13 own scripts (null observers, copies, ignore of null and of a subscriber, throws from `Notify`/`NotifyIgnore` incl. nested and after removals: both `catch` blocks' `leave_iteration`, the wrap through `UINT_MAX` on the exception path) plus all 428 `observable` cases of `libs.core.application.pure.p001`, whose Rust driver now drives this port | Iterator invalidation and use of a destroyed `this` are typed errors (`ListInvalidated`, `Destroyed`, `UnwoundDestroyed`) and controls |
| `KI_UNION_FIND` ctor, `Reset`, `Size`, `ComponentCount`, `Unite`, `Find`, `FindCompress`, `Connected` | `union_find::KiUnionFind` | 61 scripts: QA shapes (degenerate sizes, reset, chains in both directions, stars), 32-bit truncation of indices, counts above 2^32 - 1 throwing `length_error` from the constructor and from `Reset` with the set unchanged, random scripts up to 8,192 elements; the private parent array is compared after every script | The compare-exchange failure branches of `Unite` need a concurrent writer and are unreachable single-threaded (ported as written) |
| `KI_MST::FilterKruskal` (`kruskal`, `filter`, `recurse` lambdas) | `filter_kruskal::filter_kruskal` over `stdalgo::{sort, nth_element, partition}` | 166 cases: empty, self loops, ties, int64 extremes, both early exits (`ComponentCount() == 1` before and after the lighter half), never-read out-of-range endpoints of an already joined forest, explicit lists around `KRUSKAL_THRESHOLD`, the QA trials (`MatchesPlainKruskal` 60, `AllWeightsEqual`, `FewDistinctWeights`, islands, pre-seeded, reproducibility) with their `std::mt19937` draws, three comparators (weight-only ties exercise libstdc++'s arrangement), and 12 McIlroy adversary inputs that reach introsort's and introselect's heap fallbacks; emitted order, return value, the reordered span and the forest are compared | Comparators that are not strict weak orderings are undefined (`InvalidOrdering`) |
| `MINOPTMAX<T>` (13 members) | `minoptmax::MinOptMax<T>` | 226 cases for `int` and `long long`: every setter at the type limits, all six setter orders over inverted/equal/extreme triples, overwrites, `PinnedOpt` with and without an optimum, `Matches` at the limits | Other `T` are not instantiated by KiCad |
| `KISPINLOCK` ctor, `lock`, `try_lock`, `unlock`, `test` | `spinlock::KiSpinLock` | 15 sequences incl. unlock of an unheld lock | `lock` on a held lock never returns with one thread: `WouldSpinForever` (control) |
| `SYNC_QUEUE<T>` ctor, `push`, `move_push`, `pop`, `empty`, `size`, `clear` | `sync_queue::SyncQueue<T>` | 12 scripts with `std::string`: FIFO order, `pop` leaving the receiver untouched when empty, `clear`, 4,000 entries, NUL and multibyte values | The mutex (no threads in portable code) |
| `arrayDim`, `ignore_unused`, `to_underlying` | `helpers::{array_dim, ignore_unused, to_underlying, Underlying}` | 5 array kinds, 36 enum values at each fixed underlying type's limits, 3 strings | none |

## libstdc++ algorithms

`FilterKruskal` reorders its caller's span and selects edges in the order `std::sort`,
`std::nth_element` and `std::partition` leave, which the standard does not fix. The pinned
oracle's libstdc++ is therefore part of the reference behaviour. `kicad_core::stdalgo`
transcribes GCC's `__introsort_loop`, `__final_insertion_sort`, `__unguarded_partition_pivot`,
`__move_median_to_first`, the heap helpers, `__introselect` and the bidirectional
`__partition` step by step (hole movements as equivalent swaps), as read from GCC 15's
headers (local WSL). The hosted oracle uses GCC 13 (`ubuntu-24.04`); its exact comparison is
the check that those bodies agree.
See [the finding](../../port/findings/core-include-libstdcxx-algorithms.md).

## Host-supplied and portability

- No clock in portable code: `profile::ChronoClock::now` is one `high_resolution_clock::now()`
  (libstdc++'s `system_clock`). The oracle scripts it by linking with
  `-Wl,--wrap=_ZNSt6chrono3_V212system_clock3nowEv` (static objects only; unarmed calls
  forward). The Rust shim counts its scripted reads; both report `clock_calls`.
- No threads, atomics or locks in portable code: `KI_UNION_FIND`, `PROF_COUNTER`,
  `KISPINLOCK` and `SYNC_QUEUE` are owned exclusively (`&mut`). A host sharing them across
  threads supplies its own synchronisation.
- Checked arithmetic where the reference is undefined (time point differences), modular
  where it is unsigned (`ComponentCount`, `PROF_COUNTER`, iteration counts); fallible
  allocation for every caller-sized buffer; bounded recursion (introsort depth limit,
  FilterKruskal halving).
- `kicad-core` builds for `wasm32-unknown-unknown`; the live workflow replays this corpus
  there against native C++ replies.

## Defined domain and controls

`port/oracle/fixtures/core-include-p001-controls.jsonl` (49 requests): undefined behaviour
(indices and endpoints outside the parent array, the uninitialised `m_started`, overflowing
`delta_ms`, a throw through a destroyed observable), non-termination (`lock` on a held
spinlock), harness limits (counts, clock domain and length, list sizes) and malformed
requests. The ctest `oracle-core-include-p001-inputs` and `tests/corpus.rs` replay them with
a successful continuation after each.

## Affected callers

`LATENCY_PROBE::to_string` (`libs.core.application.pure.p001`) was the only `UNPORTED` caller
of this unit; its corpus was rerun (6,227 of 6,227 exact, none deferred) and its Rust
observable driver now drives `OBSERVABLE<T>`'s port with identical replies to before for all
6,202 previously compared cases.
