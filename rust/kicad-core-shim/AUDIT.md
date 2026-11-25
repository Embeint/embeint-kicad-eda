<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# libs/core p001 source audit

Owned unit: `libs.core.application.pure.p001`, 594 lines: `libs/core/base64.cpp:1:164`,
`observable.cpp:1:267`, `profile.cpp:1:96`, `version_compare.cpp:1:67`, 40 extracted
USRs. Reference pin `4cf939778df04d2b747bb2c4714fc5014a7565c7`. Base: `origin/rust`
at `4b18bdb371`, fetched and verified before edits. Open claims were checked with
`gh pr list --state open` and searched for these paths and symbols; the only open port
PR (#68, `libs.kimath.include.math.application.ui.p001`) does not overlap.

Read in full: the four pinned sources and the headers they include
(`core/base64.h`, `observable.h`, `profile.h`, `version_compare.h`), the upstream QA
tests `qa/tests/common/test_base64.cpp` and `test_observable.cpp`, the pinned
`cmake/PerformFeatureChecks.cmake` and `config.h.cmake` (clock selection), and the only
callers (`target3001_parser.cpp`, `SVG_plotter.cpp`, `sch_editor_control.cpp`,
`trace_helpers.cpp`, `kiplatform/os/windows/drivers.cpp`).

## Ownership boundary

The header unit `libs.core.include.core.application.pure.p001` (status `todo`, unclaimed)
owns `PROF_TIMER`, `PROF_COUNTER`, `SCOPED_PROF_TIMER`, every inline `LATENCY_PROBE`
member (constructor, `Reset`, `Checkpoint`, `AddTimer`, `delta_ms`) and the whole
`OBSERVABLE<T>` template (`Subscribe`, `SubscribeUnmanaged`, `Unsubscribe`, `Notify`,
`NotifyIgnore`, copy constructor). This unit owns only `GetRunningMicroSecs` and
`LATENCY_PROBE::to_string` from profile. None of the header bodies are ported here.

Minimum type co-ports, recorded in this unit's notes for the header owner:
`LATENCY_PROBE` -> `kicad_core::profile::LatencyProbe` (the `m_start`, `m_name`,
`m_checkpoints` state `to_string` reads), `CHECKPOINT` -> `profile::Checkpoint`,
`TIME_POINT` -> `profile::TimePoint` (libstdc++ `system_clock` nanosecond count).

## Evidence by owned symbol

| Upstream symbol(s) | Rust | Branches implemented and checked | Remaining gap |
| --- | --- | --- | --- |
| `base64::encode_length`, `decode_length` | `base64::{encode_length, decode_length}` on `u64` | Modular `size_t` arithmetic incl. `n + 2` wrap and `- 2` underflow; `% 4 == 1` zero branch. 0..65, 2^32 +-3, 2^63 +-3, U64_MAX-7..U64_MAX, 256 random | none |
| `base64::encode` | `base64::encode` | Full blocks; remainder 1 (`<< 4`) and 2 (`<< 10 | << 2`); append to a non-empty output; empty input; every single byte, 256 pairs, 256 triples, QA lengths 0..63, random; 64 KiB, 128 KiB, 192 KiB and the 1 MiB maximum | none (capacity from `reserve` is unobservable) |
| `base64::decode` | `base64::decode` | Strip any number of trailing `=`; `4k + 1` and empty decode nothing (also with a prefix); remainders 2 and 3; every byte value in every block and tail position (table `E = 0xFF` garbage bits, `& 0x7F` high-bit folding, `=` inside); non-canonical trailing bits; URL-safe characters; QA vectors; padded/unpadded round trips with 0..5 extra `=`; 1 MiB maximum | none |
| `parseVersionString` | `version_compare::parse_version_string` | `std::from_chars<int>` semantics: optional `-`, no `+`, arbitrary leading zeros, out-of-range leaves 0 but consumes the digits (`2147483648`, 1,000 and 65,536 digit numbers), `INT_MIN`; one `.` skipped per step (`..`, leading/trailing dots, 8,192 dots); `-` as separator (`1-2`) | Non-terminating inputs are a typed refusal (upstream issue) |
| `compareVersionStrings` | `version_compare::compare_version_strings` | First differing component either way; equal prefix with `size() >=` both ways; empty strings | none |
| `GetRunningMicroSecs` | `profile::get_running_micro_secs` + `micro_secs_from_{timespec,file_time,timeval}` | Linux `HAVE_CLOCK_GETTIME` body: `tv_sec * 1000000 + tv_nsec / 1000` over the full defined range incl. both `int64` limits; the oracle reports one `CLOCK_MONOTONIC` (id 1) read per call | `_WIN32` and `HAVE_GETTIMEOFDAY_FUNC` bodies ported and unit-tested only (inactive configurations at the pin) |
| `LATENCY_PROBE::to_string` | `profile::LatencyProbe::to_string` | Header line; `tmr:` lines with `std::left << setw(30)` byte padding (0, 29, 30, 31, 200 byte and multibyte names, NUL) and `fixed << setprecision(3)`: zeros, ties (0.0625), subnormals, negative zero rounding, 1e300, max finite, glibc `inf`/`-inf`/`nan`/`-nan`, 278 random finite doubles | `chp:` lines call header-owned `delta_ms`: **typed deferral**, 25 corpus cases; their continuation is implemented but unreached |
| `OBSERVABLE_BASE` ctor/copy ctor/dtor, `allocate_impl`, `allocate_shared_impl` (both branches), `deallocate_impl`, `get_shared_impl`, `add_observer`, `remove_observer`, `enter_iteration`, `leave_iteration` (incl. release of an unshared single-owner list), `size`, `on_observers_empty` | `observable::ObservableArena` methods on `ObservableBase` | 17 scripted scenarios (the QA case; order; unmanaged release; removal of later/self/earlier entries during notification; link reset/drop/assign in callbacks; nested notification of the same list and of a shared copy; NotifyIgnore's unbalanced iteration; copies from an empty and an owned list; duplicate and null observers; `on_observers_empty` with live links; unsigned count wrap through unpaired `leave_iteration`; depth 8 nesting; all slots) plus 400 fixed-seed random scripts. Every step's full state is compared | Null `impl_` dereference is a refusal |
| `IMPL` ctor/dtor, `is_shared`, `set_shared`, `enter/leave_iteration`, `is_iterating`, `add_observer`, `remove_observer` (null during iteration, erase otherwise, missing = release no-op), `collect`, `equals` | private `Impl`; `use_count` tracked by the arena | As above; `iteration_count_`, `observers_` order and nulls, `is_shared()` and `use_count()` are in every snapshot | Debug-build asserts are not modelled (release semantics) |
| `LINK` ctors, move ctor, move assignment (incl. self), `operator bool`, `reset`, dtor | `observable::Link`, `ObservableArena::{move_assign_link, move_assign_link_in, reset_link, drop_link}` | Moved-from links keep `observer_`; reset on an orphan list after `on_observers_empty`/observable destruction; reset of an observer not in the list; null-token links | none |

## Host-supplied and portability

- `GetRunningMicroSecs` reads no clock in portable code. The host implements
  `profile::RunningClock::read_running_clock`, returning one raw platform reading
  (`Monotonic(Timespec)` on Linux: one `clock_gettime( CLOCK_MONOTONIC )`; `FileTime` on
  Windows; `TimeOfDay` where only `gettimeofday` exists). The portable code does the
  arithmetic, with checked `int64` overflow where the reference is undefined.
- The oracle injects the same reading without a real clock: both oracle executables link
  with `-Wl,--wrap=clock_gettime`, so the static reference objects call
  `__wrap_clock_gettime` in `shims/core_p001.cpp`. Armed by a case, it records the clock
  id and call count and returns the case's `timespec`; unarmed it forwards to the real
  function. Shared libraries are unaffected. The Rust shim's `clock_id` is 1 by
  construction (the `Monotonic` form); the C++ value is measured.
- `LATENCY_PROBE` time points are data (nanosecond counts); the probe is built with the
  upstream constructor and its private members set through explicit-instantiation access.
- Observables are an arena of counted `ImplRef`s instead of `shared_ptr`; no globals,
  pointers or `unsafe`. Allocation is bounded by input sizes (`try_reserve`); arithmetic is
  checked or explicitly modular where the reference is unsigned.
- The crate builds for `wasm32-unknown-unknown`; the live workflow replays the whole
  corpus there against native C++ replies. Native and wasm use identical `u64` length
  arithmetic.

## Defined domain and controls

`port/oracle/fixtures/core-p001-controls.jsonl` (110 requests) holds every refusal:
undefined reference behaviour (null `impl_` dereference, adding into an active
range-for, signed overflow, overflowing time point differences), non-termination
(`parseVersionString`), harness limits and malformed requests. The oracle ctest
`oracle-core-p001-inputs` and `rust/kicad-core-shim/tests/corpus.rs` replay them with a
successful continuation after each. Release (`NDEBUG`) semantics are ported: the
`assert`s in `add_observer`, `remove_observer` and `OBSERVABLE_BASE::remove_observer` are
no-ops in the oracle and in KiCad releases.

Upstream issues recorded: [non-terminating version parse](../../port/upstream-issues/core-version-parse-nonterminating.md),
[unmanaged release and NotifyIgnore](../../port/upstream-issues/core-observable-unmanaged-release.md).
