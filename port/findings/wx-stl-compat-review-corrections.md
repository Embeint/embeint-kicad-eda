<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# wx stream review corrections: call context, facet state, forced unwind, null buffer

The independent review of `058165f437` (recorded in
`wx-stl-compat-second-review-gaps.md`) found four stream-contract gaps that the documented
partial status did not resolve. Each is now implemented from the libstdc++/libsupc++
sources and compared against the C++ oracle on new public cases. The first 4719 requests
and their replies are unchanged.

## Findings and corrections

1. **Ambient unwinding (P1).** `~sentry` tests `!uncaught_exception()`. The model only
   skipped a destructor when its own `Result` flow exited early, so an insertion made from a
   destructor during another exception's unwinding could falsely terminate. Corrected:
   `CallContext { uncaught_exception }` is an explicit argument of `insert_wx_size`,
   `insert_wx_point` and `OStream::flush`, and reaches every main sentry and, through
   `StreamHost::flush_tie( context )`, the tied stream's flush and its sentry. The adapters
   take `unwinding: true` and the C++ adapter runs the real operator in a destructor while
   `std::runtime_error( "outer" )` propagates (`std::uncaught_exceptions() == 1` checked).
   The unwinding generator (130 cases, `*-unwinding-*`): the probe configuration (good,
   `unitbuf`, failing `sync()`, `badbit` in the mask) writes the whole output with no sync
   and stays good (`point-unwinding-would-terminate-w0`), where the same request without the
   context terminates after the first literal (`point-unwinding-false-would-terminate-w0`).
2. **Mutable facet `ios_base&` (P2).** `_M_insert` passes the stream itself as the facet's
   `ios_base&`; the hook received only a preformatted field, and the stock padding path had
   been removed on the assumption that width is always zero there. Corrected: the hook is
   `do_put_long( out, &mut IosBase, fill, value )`, with mutable flags, width and precision
   of the stream and its `numpunct`, and returns an `OstreambufIterator`. The stock body
   `stock_do_put_long` is the whole `_M_insert_int<long>` for any `long`: base from
   `io.flags()` (two's complement for negative non-decimal values), grouping, sign or
   prefix, `__pad` (left, internal after a sign or `0x`, otherwise before), `io.width( 0 )`,
   and a write that a failed iterator skips. The adapters script a facet that sets width,
   precision and flags before and after delegating, passes its own fill and value, and
   delegates zero to two times. The facet generators (448 cases and 64 fixed-seed random
   ones, `*-facet-*`): `point-facet-probe` gives ` x="****+7" y="****+8"`, precision 42,
   `showpos`, width 0.
3. **Forced unwind (P2).** `__ostream_insert`, `_M_insert` and `flush` catch
   `__forced_unwind&` first, set `badbit` and always rethrow. The host can now classify its
   exception (`StreamHost::is_forced_unwind`); `caught` rethrows a forced unwind whatever
   the mask. The rethrow counts as uncaught (`__cxa_rethrow` increments the count and
   `__cxa_begin_catch` never decrements it for a foreign exception), so the destructors it
   passes do nothing. A forced unwind out of the libstdc++ 14 `noexcept` destructor's sync
   terminates; the GCC 15 destructor's `catch( ... )` swallows it and glibc aborts
   (`InsertError::Abort`, observed only in the diagnostic GCC 16 run). The C++ adapter
   cancels the insertion's own thread with `pthread_cancel( pthread_self() )` and
   `pthread_testcancel()` inside the scripted host operation. The cancellation generator
   (262 cases, `*-cancel-*`): with an empty mask the overflow's cancellation ends the thread
   with `badbit` only (`point-cancel-overflow-00-mask-none`).
4. **Null `rdbuf()` (P2).** `basic_ios::init( nullptr )` sets `badbit`, `clear` ORs it in
   when there is no buffer, the destructor tests `rdbuf()` before syncing, and `flush()`
   returns before its sentry. Corrected: `StreamHost::has_rdbuf`, consulted by
   `OStream::new`, `clear`, `~sentry` and `flush`; the adapters construct the stream or the
   tied stream on `nullptr`. The null-buffer generator (86 cases, `*-null-rdbuf-*`): after
   `clear( goodbit )`, `width( 9 )` and `unitbuf`, the insertion returns with
   `badbit|failbit`, width 9 and no write or sync
   (`point-null-rdbuf-good-mask-none-unitbuf`); a tied stream without a buffer is never
   synced.

The fifth finding (an allocating derived `Clone`) was corrected at `4aaae34988`.

## Signal-safe abort reporting (review of `e601ac070b`)

The independent review of `e601ac070b` found one new P2 problem. The adapter reported a
GCC 15+ abort from a `SIGABRT` handler, which invoked a `std::function`, read the streams
through standard-library members, built JSON and called `dump()`. None of that is
signal-safe ([support.signal]), so the handler's invocation was undefined. Its 38 `abort`
replies (for example `point-cancel-sync-0-mask-none-good`) were therefore not oracle
evidence, even though they agreed with Rust. The libstdc++ 14 path reports through the
synchronous terminate handler and was not affected.

Corrected: no reply is built in a signal handler.

- The child's `SIGABRT` handler only calls `std::_Exit( 5 )`.
- Both streams are constructed in an anonymous `MAP_SHARED` mapping (`ABORT_RECORD`), so
  their `ios_base`/`basic_ios` members outlive the child.
- Before a scripted host operation cancels the thread, it serializes the reply's host
  fields in ordinary context: bytes, `sync_calls`, trace, setup results and the tie's sync
  count. From the cancellation, only the runtime runs until the abort: the forced unwind
  passes `pubsync()` into the destructor's `catch( ... )`, and `__cxa_end_catch` calls
  glibc's `unwind_cleanup`. A host call after the snapshot marks it stale.
- After `waitpid`, the oracle process checks four things: the dedicated status, an empty
  reply pipe, glibc's `FATAL: exception not rethrown` on the child's captured stderr (a
  memfd, copied to the oracle's stderr), and a snapshot that is present, fresh and within
  its bound. It then parses the snapshot.
- The oracle process then replaces `state`, `exceptions`, `flags`, `width`, `precision` and
  `fill` (and the tie's `state` and `exceptions`) with the ended child's stream images. It
  reads libstdc++'s `_M_streambuf_state`, `_M_exception`, `_M_flags`, `_M_width`,
  `_M_precision`, `_M_fill` and `_M_fill_init`. The code names them through an explicit
  instantiation, which may name private members. It finds their offsets on a probe stream
  of the same complete type.

The reply format and the Rust `InsertError::Abort` model are unchanged: the observable data
is the same, now read outside signal context. The images matter. In a temporary diagnostic
build, the snapshot state was good in all 38 aborts. The image showed the destructor's
`badbit` on the main stream in 30 and on the tied stream in 8. Every other field was equal.
That build was not committed, and the rebuilt binary's hash returned to the one below.

The snapshot is serialized JSON and so allocates (like the terminate reporter, it is not
allocation-safe; a failure is a harness failure). Reading another process's stream image
relies on the oracle's pinned libstdc++ layout, which is outside the C++ object model. It
does not run in a signal handler.

## Provenance correction

The earlier local evidence stage was exported by `port/oracle/prepare.py` in the Windows
worktree with Git's `core.autocrlf=true`: `git archive` applies the checkout end-of-line
conversion to `text=auto` files, so the archive held CRLF files (reproduced here: 917 of 918
files differ from the raw blobs). The audit finding's description of it as a raw-blob
export was wrong. This correction's stage was exported with `core.autocrlf=false` and
`core.eol=lf` (through `GIT_CONFIG_COUNT`), and every one of the 918 files equals
`git cat-file blob <pin>:<path>` byte for byte. The hosted job now performs the same check
on its own stage before building (on Linux the export has no conversion). `prepare.py` is
shared infrastructure and is not changed here.

## Evidence

Local WSL Ubuntu 26.04, oracle rebuilt for the signal-safe reporter by `build.sh` (all 16
ctest checks pass). It used the same raw-blob stage: source manifest sha256
`6f1c4c850c90f5b6e7d84902bab092e646bb36e253bf2b188fd7ccbfdbcf84be`, archive
`b6825dc24218ceda2d5ba42d5952a614f515b7ebf84f5ec150609837318f0ff6`, 918 files, none
differing from the raw blobs. Build inputs and runtimes:

- adapter `shims/core_wx_stl_compat.cpp` sha256
  `504e4b9d791835413d913a2b4771b3ca08dcedef984341f8482e550ea50e81ee`;
- oracle binary sha256 `f0114bee4fbb336cd49fdcd5afa4deec76f3f94d157834e0d9a0f0fd866131ec`;
- compiler `g++ (Ubuntu 15.2.0-16ubuntu1) 15.2.0`, `libc6 2.43-2ubuntu2.7`,
  `libwxbase3.2-1t64 3.2.9+dfsg-1`;
- corpus sha256 `c38969e66b3b4ad482c65fda60d7b6a01f13964bfbfd4fe84f020e0a6939a0f5` (5709
  cases), controls sha256 `852f1b6ae8febd2d8f56a6593675ac241acc940acde3455e0956f9280eec1e33`
  (217).

Results:

- Genuine Ubuntu 24.04 `libstdc++6_14.2.0-4ubuntu2~24.04.1` (`libstdc++.so.6.0.33` sha256
  `1fd75fe70354a416d75aef22bcae68c47bd25d20e2d0568c30b1a9838cf62f11`, loaded through
  `LD_LIBRARY_PATH`, confirmed by `ldd`): native replies sha256
  `1f31a95a1a47797e389b918d973fa1ca2ac31712718ed66710561a532d18f73a`. They are
  byte-identical to the Rust shim with the default reference model for all 5709 (2337
  returned, 679 ios_failure, 383 terminate, 165 runtime_error, 165 forced_unwind, 1980 other
  operations). They are unchanged from the `e601ac070b` adapter's replies, whose first 4719
  equal the earlier recorded replies (sha256 `8607e44c...`). No abort occurs on this runtime.
- Local `libstdc++6 16-20260322-1ubuntu1`: native replies sha256
  `8701aa1734e52ded39d3ef842bbec266850e00b71cef50b44903783e8dff33b2`. They are
  byte-identical to the Rust shim with `--sentry-flush=when-good` for all 5709 (2581
  returned, 778 ios_failure, 167 runtime_error, 165 forced_unwind, 38 abort, 1980 other).
  All 38 aborts took the new path: each ended with the handler's exit status, with glibc's
  `FATAL: exception not rethrown` captured (38 times in the run's stderr), and with a fresh
  snapshot. They are the only `abort` replies counted as evidence. They are byte-identical
  to the replies of the replaced reporter, but those earlier replies were produced by
  undefined behaviour and are not evidence.
- 217 controls with continuations pass on the oracle under both runtimes and on the Rust
  shim. Import-free wasm32 replays of all 5709 against each new native reply set pass (WSL
  Node 22.22.1). The Rust implementation and the corpus are unchanged.

This is not target evidence: the compiler, glibc and wxWidgets differ from the reference
image. The hosted `rust-wx-stl-compat-live` and `rust-port` runs on ubuntu-24.04 (GCC 13.3,
glibc 2.39, libstdc++ 14.2) are the target comparison. The unit stays partial with no golden
until they and a fresh independent review complete.
