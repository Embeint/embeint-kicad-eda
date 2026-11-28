# Linux kiplatform app, environment, io, printing and secrets: port, boundaries and oracle

Unit: `libs.kiplatform.os.unix.application.ui.p001`, the bodies in
`libs/kiplatform/os/unix/{app,environment,io,printing,secrets}.cpp` at KiCad pin
`4cf939778df04d2b747bb2c4714fc5014a7565c7` (byte-identical to the fork's tree). Port:
`rust/kicad-kiplatform-unix`; shim: `rust/kicad-kiplatform-unix-shim`; adapter:
`port/oracle/shims/kiplatform_unix_ui_p001.cpp`.

## Shape of the port

The five files are thin layers over the operating system, GLib/GIO, GTK printing, poppler,
cairo, libsecret and wxWidgets process services. Portable code makes none of those calls.
Each is a method of `kicad_kiplatform_unix::host::UnixHost`, called with the reference
arguments in the reference order; the port is everything around the calls: string
construction and conversion, error handling, fallbacks, loops, arithmetic, and the order of
calls and releases (including `std::unique_ptr` scope ends, such as `cairo_restore` before the
page's `g_object_unref` in `draw_page`).

Host boundaries beyond the C libraries, each process state a native host must supply:

| Boundary | Why it is not ported |
| --- | --- |
| `wxGetEnv`, `wxSetEnv` | the process environment |
| `wxConvLibc`, `wxConvFileName` (`mb_to_wc`, `wc_to_mb`) | the locale and the converter the GTK `wxApp` installs from `G_FILENAME_ENCODING`; only the converter call is the host's, the wx wrappers around it are ported |
| `towlower` (in `wcscasecmp` for `CmpNoCase`) | the locale's case mapping |
| `wxOnAssert`, `wxLogError`, `wxLogTrace` | the assertion handler, the log target and the trace masks |
| `wxGetActiveWindow`, `wxGetTopLevelParent`, `wxWindow::GetHandle` | the GUI's window state |
| `fnmatch` | glibc's locale-dependent matcher (`FNM_CASEFOLD`) |

The deterministic wxWidgets 3.2.4 code the bodies reach is ported in `wx.rs`: `ConvertStr`,
`AsChar`/`AsCharBuf` and the inline constructors (a failed conversion is an empty string; a
`const char*` constructor ends at the first NUL unit; `FromUTF8( std::string )` keeps embedded
NULs), the strict UTF-8 encoder with its `wxFAIL_MSG`, `CmpNoCase` over NUL-delimited chunks,
the Unix `wxFileName::SplitPath`, the `wxURI` scheme/authority parser, `wxURI::Unescape`,
`wxFileName::URLToFileName` and the spool-name pattern as PCRE2 evaluates it (UTF-32, DOTALL,
no `DOLLAR_ENDONLY`, so `$` also matches before a final `"\n"`; an invalid UTF-32 subject is a
match error that logs `"Failed to find match for regular expression: no error"`).
`std::hash<std::string>` is libstdc++'s `_Hash_bytes` (`stdhash.rs`). `printing.cpp`'s
`s_lastSettings` is `Printing::last_settings`, owned by `UnixPlatform`.

wx 3.2.4 behaviours the oracle confirmed and the port reproduces: `ParseH16` advances before
reading, so `[::1]` and `[2001:db8::1]` are not parsed as IPv6 (the reg-name becomes `%5b...`
and the colon ends it); `m_scheme != "file"`, `wxString::assign( char*, n )` (IPv4/IPv6
servers) and `aService + ":"` convert their narrow literals through `wxConvLibc`;
`wxOnAssert` receives the narrow `__FUNCTION__` and condition through `wxConvLibc`;
`wxLogTrace` messages are prefixed `"(KICAD_PRINT) "` and a narrow argument wx cannot convert
prints `(null)`; `what()` of the mapped file's exception is a C string, so the message ends at
its first NUL.

## Interface changes to accepted units (recorded co-port adoption)

- `kicad_kiplatform` (public header unit): `AppPlatform`, `EnvPlatform`, `IoPlatform` and
  `SecretsPlatform` gain an `Error` type and return `Result`; the default-argument methods
  forward it; `TerminateWithCode` returns `Result<Infallible, _>`. The accepted traits could not
  report a deferral, an allocation refusal or a host failure without inventing a value.
  `MappedData::Mapping` carries the address `munmap` receives. The include shim's recording host
  implements the fallible traits with `Infallible` errors and its replies are unchanged.
- `printing.h` (owner `libs.kiplatform.include.application.ui.p001`, todo): minimum co-port
  `kicad_kiplatform::printing::{PrintResult, PrintingPlatform}` (the enum and the declarations
  with `aCurrentPage = 0`); `PrintResultToString` is not taken.
- `WxString` (`kicad-map-helpers`) and its strict decoder are reused unchanged. The strict
  encoder repeats the accepted font port's (`kicad_font::wx_conv::strict_utf8`), whose
  assertion callee is tied to `FontError`; it is the same table with this crate's callee.

## Deferrals (named, typed `Unported`)

- `KIPLATFORM::IO::MAPPED_FILE::readIntoBuffer` (`os/common/io.cpp`), reached when `mmap`
  fails; owner `libs.kiplatform.os.common.application.ui.p001`. The other `os/common` members of
  `IoPlatform` (`AtomicRename`, `CommitTempFile`, `FILE_LOCK`, ...) defer to the same owner.
- `wxFileName::MakeAbsolute` in `GetDocumentsPath` when GLib's data directory is empty or null:
  `Normalize` reads the process working directory and the home directory (tilde); not ported,
  named against this unit. The calls before it (`g_get_home_dir` and its conversion) are made.
- `wxSystemOptions::SetOption( "opengl.egl", 0 )`, compiled only against wx >= 3.3.2 with EGL;
  the reference build (wx 3.2.4, `wxUSE_GLCANVAS_EGL` 0, reported by `build_config`) never
  compiles it.

## Oracle

The unchanged pinned sources (and `os/common/io.cpp`) are the `kiplatform-unix-ui-reference`
archive. Every call they make into libc, GLib/GIO, GTK printing, poppler, cairo, libsecret and
the wx process functions is redirected with `--wrap` (C++ functions by mangled name) to a fake
that is armed only while a case runs; unarmed, the libc, GLib and wx fakes forward to the real
functions, so other adapters are unaffected (their ctest suites pass). libsecret, poppler and
cairo contribute headers only. Each case replaces `wxConvLibcPtr` and `wxConvFileName` with
scripted converters and installs a recording `wxLog` target and assertion handler; the
stateful printing cases and `TerminateWithCode` run in a forked child. A live-workflow step
requires every library and libc call the archive references to be wrapped (`_exit` excepted,
observed through the child's exit status).

glibc declares many wrapped functions `noexcept` (`__THROW`): a refusal thrown through them
terminates the process. Their fakes keep the first refusal, return a failure value, and the
operation raises it when the call returns; the port stops at the same first error. glib's own
internal `free` calls reach `__wrap_free` in this executable, so only the `realpath` buffer
KiCad frees is recorded.

Refused as undefined in the reference (both programs, identical messages): signed overflow in
`TimestampDir` (`st_mtime * 1000` or the running sum, simulated from the script before the
call), a zero divisor when `draw_page` fits a page (`[expr.mul]/4`; detected in the
`poppler_page_get_size` fake during draw-page), and the one-byte `"%"` path for which
`wxURI::Unescape` forms `end - 2` before its buffer. Defined behaviour is compared, including
NaN, infinities, negative zero and subnormal page and context sizes, wrapped `pid_t`, negative
`st_size`, embedded NULs, units above U+10FFFF (asserting) and invalid UTF-8/UTF-32.

The scripted host also models wx's formatting of logged messages. Its model is exact for the
converter rules the corpus uses with logging (`ascii`, `latin1`, `utf8`); a `fail` rule with a
logged error or trace is not in the corpus.

## Evidence (local, diagnostic)

Own WSL tree, extracted Ubuntu `g++-13` 13.3.0, local wx 3.2.9 and glibc 2.43 (the hosted target
is wx 3.2.4 and glibc 2.39; hosted CI is the reference). Oracle sha256
`a1134464089eb3b9d9dc3ff81f1baf12eedfa22298332f38856d216c0c6bbc07`; `kicad-difftest compare
--partial`: 663 cases, 657 compared exactly, 6 deferred, 0 failures, 0 unprocessed (oracle
replies sha256 `91df0cc9…3c4d`, Rust replies `3b191305…f615`). Two oracle runs from different
working directories give identical replies. 58 controls refused identically by both programs, each
followed by a successful request. Import-free wasm32 agrees with every native reply (9
`TerminateWithCode` cases are native only). Full oracle ctest passes. `generate_cases.py
--check` is byte-identical on Windows and Linux; MSRV 1.85 workspace check and wasm32 checks
pass.

## Remaining gaps

- Status `partial`: 6 cases defer; no golden is supplied. A documents-path case whose home
  directory the converter fails on is not in the corpus: the reference's `MakeAbsolute` then
  reads the process working directory, so its retained reply would not be deterministic.
- Hosted qualification on wx 3.2.4 is pending: wx-internal behaviour the corpus depends on
  (`CmpNoCase`, `wxURI`, the PCRE2 messages, log formatting) was observed locally on 3.2.9.
- `gtk_print_operation_get_print_settings` returning null after `set_print_settings` (not
  reachable in GTK) is a host error, not modelled.
- `wxGtkPrintNativeData::GetPrintConfig()` returning null is not reachable through wx; the
  branch is ported but untested.
