Read-only independent review of PR93

Verdict: changes requested. Keep partial, draft and without a golden. The corrections resolve several earlier findings, but documented gaps still include missing defined behavior in the stream interface and exception control flow. Pending hosted checks were not polled; their outcome is not inferred here.

Reviewed head: 058165f437c59b26afc27423e651c7f1f7fcb2b3
Reviewed tree: fa6a296b581f473209dedb24e41d785c61aed1ec
Checkout: C:/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-37e15dd1-a871ca6069ce01b0
Branch: t3/port-wx-stl-compat
Accepted base: 70888b572202033a6522f2f271b10cc908f74dfe
Earlier reviewed head: 6f272c818852adf3d640aee315d1f4756d86eaa8
The checkout remained clean and the head/branch remained unchanged. No tracked edits, commits, pushes, GitHub comments/reviews, watches, delegation or branch changes. All Git operations ran on native Windows. Small native C++ probe builds used an acquired ~/kicad-port.build-lock, released only by their owning scripts; no build used ~/kicad-live.

Findings

1. P1: ambient exception propagation is missing, causing false termination and truncated output.
   Path: rust/kicad-wx-stl-compat/src/ostream.rs:545 (OStream::sentry_exit; also OStream's fields at 413).
   GCC14's sentry destructor tests !uncaught_exception(). The Rust implementation suppresses its destructor work only when its own Result flow exits early. There is no explicit caller-supplied context for an exception already propagating before this insertion.
   Independent reproduction links the actual saved pinned wx_stl_compat.cpp.o, calls the real operator<< from a destructor during outer runtime_error unwinding, and uses a good unitbuf stream with a failing sync and badbit in the mask. With genuine libstdc++14 loaded, C++ writes the complete ` x="1" y="2"`, calls sync zero times, leaves state good, and preserves the outer exception. Current portable code with those stream settings returns Terminate after ` x="`, calls sync once and sets badbit. This is a source-contract departure, not just absent corpus coverage. Add explicit execution context and propagate it through main and tied sentries; preserve actual source behavior without replacing sync with a fake no-op.

2. P2: the num_put host hook cannot observe/mutate the caller's ios_base; integer padding was removed on an invalid invariant.
   Paths: rust/kicad-wx-stl-compat/src/ostream.rs:252, :631, :665.
   C++ passes ios_base& and fill into num_put::put/do_put. StockPut instead contains only an already formatted borrowed field and the numeric value. The callback cannot mutate OStream's width, precision, flags, state or mask. The comment that no host operation can change width describes a restriction introduced by this API, not the source contract.
   Independent real-library probe installs a facet whose do_put(long) sets io.width(6), io.precision(42), and showpos, then delegates through the classic runtime facet's virtual put. With fill '*', C++ writes ` x="****+7" y="****+8"`, keeps precision 42/showpos, and ends with width 0. The existing portable hook cannot implement those state changes. Host code can write arbitrary bytes already, so non-stock bytes alone are an evidence limit; mutable ios access and ensuing stock padding are the missing functionality. Preserve those mutations and operation ordering with an explicit safe stream context; do not manufacture matching output in a fixed script.

3. P2: forced_unwind is swallowed as an ordinary host exception.
   Path: rust/kicad-wx-stl-compat/src/ostream.rs:515 (also insertion/flush callers).
   The audited libstdc++ __ostream_insert, _M_insert and flush have a distinct catch(__forced_unwind&) that sets badbit and unconditionally rethrows. caught() rethrows every Host(E) only if exceptions.bad is set; no host control-flow classification exists for forced unwind.
   Independent native pthread cancellation probe calls the real KiCad insertion with mask empty and a buffer whose overflow reaches pthread_testcancel. With genuine libstdc++14: cancelled=1, returned=0, overflow_calls=1, bad=1, failbit=0. The corresponding current portable host-error path with mask empty returns success and ends bad|fail. Documenting cancellation as a gap does not implement this compiled catch branch. The host must convey the distinguished unwind without adding portable thread/process/global machinery.

4. P2: StreamBuf retains an infallible heap-cloning operation.
   Path: rust/kicad-wx-stl-compat/src/ostream.rs:282 (derive Clone on StreamBuf, containing bytes: Vec<u8>; also reached through OStream<StreamBuf>'s derive at 412).
   Cloning a populated buffer calls Vec::clone and may abort on allocation failure. try_reserve in overflow does not cover this public operation, so the claim that every portable allocation is fallible remains false. Remove the allocating Clone or expose a typed fallible clone. This is a static source finding; no deliberate OOM was induced.

5. P2: nullable rdbuf is missing from the stream state model.
   Paths: rust/kicad-wx-stl-compat/src/ostream.rs:493, :545, :705; StreamHost at 225.
   basic_ios::clear must OR badbit into the supplied state when rdbuf()==nullptr; sentry destruction checks rdbuf before pubsync, and flush returns before constructing a sentry when it is null. StreamHost has no null-buffer state, clear assigns the requested bits verbatim, and the later paths always act as though a buffer exists.
   Independent call of the real KiCad operator on std::ostream(nullptr), after clear(goodbit), width(9), unitbuf: bad remains set after clear; insertion returns with bad|fail and width still 9. A host whose overflow returns eof is a real failing buffer and does not model null rdbuf. Add explicit nullable-buffer state/behavior, including the tied flush continuation, rather than a dummy sink.

Disposition of previous findings

- Missing defined throwing buffer/num_put/sync/tie paths: substantially corrected for ordinary exceptions and non-mutating hooks. xsputn is overridable and fallible; overflow preserves partial bytes; caught() preserves the ordinary thrown value, state and appropriate skipped width reset. A separate Rust Box<Token> probe confirms identical pointer identity after propagation. The runtime, mutable-ios and null-buffer findings above remain.
- Overbroad termination guard: corrected for normal insertion/unwinding caused by that insertion. State/mask overlap is caught at setup and continued; the 264 padding/capacity boundary cases genuinely call the source and expose the first-write/first-destructor boundary. Ambient outer unwinding remains incorrect (finding 1).
- Infallible sink/format allocations: overflow now uses typed BadAlloc via try_reserve; stock int scratch and grouping storage are fixed; converter reservations are fallible, with the second pass bounded before push. The 32-byte field is sufficient for the owned 32-bit int domain, including unsigned octal, grouping and prefix. The first 32 non-NUL grouping bytes suffice for at most 11 digits. Derived heap Clone remains (finding 4). Runtime locale/cache bad_alloc was not induced; the generic ordinary exception value can represent bad_alloc, so missing live allocation evidence alone is not an additional false-result finding.
- Licence/attribution: corrected. Source notices and AUDIT identify libstdc++, glibc and wx derivation, upstream owners/licences and GPL-compatible distribution decisions. The accepted WxString, WxPoint, WxSize and FmtFlags implementations are unchanged. No upstream interactions occurred.

Reference integrity and harness audit

- All four owned compiled bodies and the header/macro configuration were checked against KiCad pin 4cf939778df04d2b747bb2c4714fc5014a7565c7. Less ordering, map ordering/first emplace, explicit glibc C/UTF-8 conversion, preserved embedded/trailing NULs and decoder failures show no new problem in the supported locale domain. The KiCad wxString hash omission is scoped correctly to the audited inactive macro configuration.
- The exact saved adapter, registry and CMake bytes equal the corresponding HEAD Git blobs. Adapter SHA256: cd16608d44d437eba8528960b880f48711c7e0099e0378c3ce8d9e4b2ad551cb. Saved oracle SHA256: b8aa4f454cefa8c60a9ea2a1639799c07fe45a9b6df544cb163f79e9a9ec44bc.
- Independently inspected symbols in both compiled adapter objects, kicad-oracle and oracle-dispatch-test. None defines/instantiates num_put<char>::_M_insert_int or __use_cache<__numpunct_cache<char>>. The owned compiled object imports std::ostream::operator<<(int), __ostream_insert<char>, wxString::ConvertStr and wxConvLibc; the adapter relocations call the out-of-line TowxString and wxPoint/wxSize operators. The scripted facet retains a reference to the classic runtime facet and invokes virtual put; its lifetime is valid. The NUL-grouping cases agree with the runtime's twin-facet behavior. No private formatter/cache interposer was found.
- Relative to the accepted base, the adapter is appended after all accepted objects in both oracle targets. Registry/workspace additions are scoped; the dispatch-test omission is fixed. Clock wraps, existing source/test lists, accepted implementations, pins, global rust-port workflow and goldens are unchanged. Cargo.lock adds only the two local packages, with no dependency version substitutions.
- Every insertion genuinely runs in a forked child. The handler callback is installed while its captured streams remain alive; successful termination replies originate from std::terminate rather than a precomputed guard. Parent reads while child writes (avoiding pipe-capacity deadlock), retries EINTR and checks child status before parsing. The bounded input/event schema bounds successful report size. No invented termination outcomes were found for tested cases.
- Limitation: ReportTermination (adapter line 481) calls a JSON-building closure and WriteAndExit calls dump (line 455); both allocate and may throw. It is not an allocation-safe terminate handler. Under exhausted allocation, reporting itself can fail and parent will see child failure, not valid termination-state evidence. This limits bad_alloc qualification; current successful runtime_error/sync termination reports remain genuine. fork failure also does not close the newly opened descriptors. Neither resource path was fault-injected.
- Important provenance correction: all 918 saved reference files match the saved manifest. Comparing each manifest entry with native-Windows Git cat-file raw pinned blobs found 880 CRLF-export differences, with zero differences other than EOL conversion; 38 files match raw. Thus that saved artifact is not byte-for-byte a raw-blob export as described. This is a local evidence caveat in unchanged export infrastructure, not evidence of changed C++ answers. No fresh target qualification is inferred from it.
- The corpus is exactly the unchanged raw 3329-line old corpus prefix plus 1390 new inputs (4719); SHA256 f22ace8504af6bb5c2ffcb25c87b81f96e3d794e06b45fefdc37f2ec18725242. There are 140 separate malformed/resource controls. No substituted reference replies or new golden were found.

Independent checks actually rerun

- cargo test --locked --manifest-path rust/Cargo.toml -p kicad-wx-stl-compat -p kicad-wx-stl-compat-shim -j2: 16 portable tests and 4 corpus/control tests pass.
- Scoped cargo fmt --check, clippy --all-targets -D warnings and portable wasm32 cargo check: pass.
- generate_cases.py --check: 4719 cases and 140 controls exactly match.
- Built current native Windows shim. Fresh execution of the saved genuine-source oracle compared byte-for-byte with current Windows output for all 4719 inputs under each runtime profile, not against stored answers. libstdc++14 loaded from root/usr/lib/x86_64-linux-gnu was confirmed by ldd; glibc/wx/compiler remain local. Default reply SHA256 8607e44c6b789b83f80d83ceb731c663dd3ccf9d336ddfdabd73da320af6efcc; WhenGood c893a5eeae00db33ea71a70b77a26d5f7a07b67531f036b7c291bca8442f86dc. Default outcomes: 303 terminate, 138 runtime_error, 617 ios_failure, 1681 returned insertions, 1980 other operation results.
- test_wx_stl_compat_inputs.py: 140 controls each followed by successful insertion and conversion on current Windows Rust and on the saved oracle under both runtimes.
- Rebuilt both release wasm examples from current Rust source and current byte-equal native replies; replayed with Node v22.22.1 in WSL. Both complete 4719-case import-free wasm runs pass. Native Windows Node was unavailable; this was resolved by using WSL Node, not by skipping replay.
- Real-library C++ probes and independent current-port Rust probes described above: ambient unwind, mutable facet, forced unwind, null buffer, ordinary exception pointer identity. Probe sources and executables are alongside this report.
- Windows kicad-difftest: 28 tests pass, four fail. partial::large_stderr_is_bounded_on_clean_failed_and_timed_out_runs fails because the peer's text-mode stderr print emits CRLF: omitted count 4128791 versus expected 4128790. The three request_cpp tests fail because the native `c++` program is absent. All kicad-difftest sources/tests, protocol.cpp/h and nlohmann source are exactly identical to the accepted base (git diff --quiet); lockfile dependency versions are unchanged. These are reproduced accepted-source/platform limitations, not assumed-equivalent PR regressions.
- git diff --check passes; final HEAD, tree, branch and clean status reverified.

Not rerun / evidence limits

No full new oracle rebuild or entire Linux/Windows workspace rerun was claimed. Saved binaries were used only after binding their current adapter/source manifests and inspecting symbols; they remain the local GCC15-header/glibc2.43/wx3.2.9 stack. Neither loading the genuine libstdc++14 package into it nor replaying the old 3329 answers is new-head target-stack qualification. No hosted run was polled. Parent must collect the actual fresh Ubuntu24.04 checks independently.

Nested acyclic ties can be expressed by composing StreamHost::flush_tie with OStream::flush, although the adapter/corpus exercises one level only. Tie cycles are undefined under the standard's cycle prohibition and should not be implemented using invented successful continuations. Arbitrary facet output bytes can already be supplied by StreamHost; mutable ios state is the actual missing facet contract. Other locale/build configurations and runtime allocation-failure coverage remain explicit limits. The proven failures above cannot be resolved by honest partial documentation alone.

Readiness remains blocked by the findings and by uncollected target evidence. No ready/green/golden transition is recommended.

PR https://github.com/Embeint/embeint-kicad-eda/pull/93, head 058165f437c59b26afc27423e651c7f1f7fcb2b3, checks and reviews target checks not polled; independent review requests changes.
