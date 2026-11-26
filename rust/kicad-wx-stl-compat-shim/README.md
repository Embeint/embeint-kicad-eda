<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# kicad-wx-stl-compat-shim

Native JSONL adapter for unit `libs.core.application.thin.replacement.p001`
(`libs/core/wx_stl_compat.cpp`). It mirrors `port/oracle/shims/core_wx_stl_compat.cpp`
field for field, including the validation order and every refusal message, and calls
`kicad-wx-stl-compat` for each result. The audit is in [AUDIT.md](AUDIT.md).

```text
kicad-wx-stl-compat-shim [--sentry-flush=unconditional|when-good]
```

Without an argument (and as registered for the difftest) the stream model uses the
reference runtime's sentry destructor (libstdc++ 14). `--sentry-flush=when-good` selects
the GCC 15 and later destructor, for comparing against an oracle linked to such a runtime
(the local WSL host's GCC 16 `libstdc++`); that comparison is diagnostic evidence only.

## Operations

Every request's `input` has an `op` and exactly the fields listed. Points and sizes are
`[x, y]`, two JSON integers in the 32-bit `int` range.

| `op` | Fields | Calls | Result |
| --- | --- | --- | --- |
| `less_point` | `a`, `b` | `std::less<wxPoint>()( a, b )` | `{"less": bool}` |
| `point_map` | `entries`: at most 256 `[[x, y], int]` | `std::map<wxPoint, int>::emplace` of each, in order | `{"keys": [[x, y]...], "values": [...]}` in map order |
| `to_wx_string` | `ctype`: `"C"` or `"C.UTF-8"`; `view`: lowercase hex bytes (at most 4096) or `null` | `TowxString( view )` with `setlocale( LC_CTYPE, ctype )` | `{"units": [wchar_t...]}` |
| `insert_size` | `value`, `stream` | `operator<<( out, wxSize( x, y ) )` | the stream after the call, below |
| `insert_point` | `value`, `stream` | `operator<<( out, wxPoint( x, y ) )` | the stream after the call, below |

`view: null` is a `std::string_view` with a null `data()`; `""` is a non-null empty view.

### The stream

The `stream` object configures the caller's `std::ostream`. Its ten required fields are
`state` and `exceptions` (lists of `bad eof fail`), `flags` (a list of `fmtflags` names:
`boolalpha dec fixed hex internal left oct right scientific showbase showpoint showpos
skipws unitbuf uppercase`), `width` and `precision` (64-bit integers), `fill` and
`thousands_sep` (bytes), `grouping` (at most 16 bytes), `capacity` (the buffer accepts this
many bytes, then `overflow` returns `eof()`; `null`: unbounded) and `sync_fails` (`sync()`
returns -1). An optional eleventh field, `host`, scripts the program around the stream:

| `host` field | Value | Effect |
| --- | --- | --- |
| `overflow_throw` | `null` or an index up to 2^20 | the buffer's `overflow` call with this 0-based index throws `std::runtime_error("overflow")` |
| `sync_throw` | `null` or an index up to 64 | the buffer's `sync()` call with this index throws `std::runtime_error("sync")` |
| `num_put` | `null` or `{"call": index up to 64, "throw": "before" or "after"}` | a `num_put<char>` facet whose `do_put( long )` call with this index throws `std::runtime_error("num_put")` before or after the runtime's own `num_put<char>::do_put` |
| `tie` | `null` or `{"state", "exceptions", "flags", "sync_fails", "sync_throw"}` | `out.tie( &tied )`: a stream on an unbounded buffer with those settings, whose `sync()` throws `std::runtime_error("tie_sync")` at `sync_throw` |

The adapter builds, in this order: the buffer; the tied stream (`flags`, `clear( state )`,
`exceptions( mask )`); `imbue` of the classic locale with a `numpunct<char>` returning
`thousands_sep` and `grouping` (and the scripted `num_put`); `flags`, `width`, `precision`,
`fill`; `tie`; `clear( state )`; `exceptions( mask )`. An `exceptions( mask )` that throws
because the state intersects the mask is caught: the mask stays set and the insertion runs.

The C++ adapter runs each insertion in a forked child, so that a `std::terminate` (the
libstdc++ 14 sentry destructor when `sync()` throws, or fails with `badbit` in the mask) is
observed and reported rather than ending the oracle.

An insertion's result reports `bytes` (lowercase hex of everything written), `state`,
`exceptions`, `flags`, `width`, `precision`, `fill`, `sync_calls` (calls of the buffer's
`sync()`, throwing ones included), `outcome` and `same_stream` (the operator returned its
argument; `true` when it did not return). `outcome` is `returned`, `ios_failure`
(`ios_base::failure` propagated), `runtime_error` (a scripted exception propagated; `what`
gives its message), `bad_alloc` or `terminate` (the state is the one reached when
`std::terminate` was called). `setup_failure: true` is added when setup's
`exceptions( mask )` threw. With `host`, the result adds `events`, the trace of host calls
as `[kind, n, detail]` (`n`: bytes in the main buffer at the time): `["eof", n, byte]`,
`["throw", n, "overflow" | "sync" | "num_put" | "tie_sync"]`, `["sync", n, result]`,
`["tie_sync", n, result]`, `["do_put", n, value]`; and, with a tie, `tie`: its `state`,
`exceptions`, `sync_calls` and `setup_failure`.

## Refusals

Malformed fields are refused with `invalid_input` and one of: `expected string: op`,
`unknown operation`, `unexpected or missing field`, `expected point`,
`component out of range`, `expected entries`, `expected entry`, `value out of range`,
`expected ctype`, `expected view`, `unexpected or missing stream field`,
`expected iostate`, `expected fmtflags`, `expected streamsize`, `expected byte`,
`expected grouping`, `expected capacity`, `expected sync_fails`,
`unexpected or missing host field`, `expected overflow_throw`, `expected sync_throw`,
`expected num_put`, `unexpected or missing tie field`.

After parsing, one configuration is refused before any call: `unbounded width exceeds
limit` (`width` above 2^20 with an unbounded buffer; the reference would allocate all the
padding; a bounded buffer stops the padding at its capacity, so any width is accepted
there). `event limit exceeded` would refuse a trace longer than 64 events; an insertion
logs at most 30.

`port/oracle/fixtures/wx-stl-compat-controls.jsonl` lists 140 such requests with their
exact refusal; `port/oracle/test_wx_stl_compat_inputs.py` replays them against either
program, each followed by successful requests.

## Corpus

`generate_cases.py` writes the public fixed-seed corpus
(`port/cases/libs.core.application.thin.replacement.p001.jsonl`, 4719 requests: the first
corpus's 3329 unchanged and first, then 1390 for the termination boundary, setup failures
and the scripted host paths) and the controls fixture; `--check` verifies both.
`tests/corpus.rs` checks the Rust side's counts and invariants; `examples/corpus_parity.rs`
with `wasm-test.mjs` replays the whole corpus in import-free wasm32 against this revision's
native C++ replies (`KICAD_WX_STL_COMPAT_WASM_REFERENCE`).
