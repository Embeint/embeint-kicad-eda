# Embeint KiCad Rust components

The first shared Rust component from roadmap delivery gate 3
([EMBEINT_FORK_ROADMAP.md](../tools/live_schematic_mcp/EMBEINT_FORK_ROADMAP.md)). It is a port of
the symbol pin layout that runs natively and in a browser. **The Python implementation stays
authoritative.** The MCP does not call this code. Parity has been measured but has not yet been
reviewed.

| Crate | Purpose |
| --- | --- |
| `embeint-layout` | Portable library: `layout_box` from `symbol_layout.py`, JSON in and JSON out. No filesystem, sockets, processes or GUI types. |
| `embeint-web` | Thin WebAssembly binding (`cdylib`): a three-function C-style ABI plus `js/embeint_layout.mjs`, which loads it in a browser, a Web Worker or Node.js without a binding generator. The ABI packs a 32-bit pointer into its return value, so the functions are only exported on 32-bit targets such as wasm32; a native build exports nothing. |
| `embeint-layout-cli` | Native runner (`embeint-layout` binary): one JSON request per stdin line, used by the parity harness. Process I/O lives here so the library stays portable. |
| `parity/` | `parity.py` (harness and Python reference adapter), `cases.json` (fixed examples), `run_wasm.mjs` (Node runner). |

The workspace is a top-level `rust/` directory, not part of `tools/live_schematic_mcp`. The
roadmap's later crates (`embeint-design`, `embeint-ipc`, `embeint-mcp`) belong to the product, not
to the Python MCP tool. Upstream KiCad has no `rust/` directory, so upstream merges cannot conflict
with it. It is also outside KiCad's CMake build and outside the directory where pytest collects
`test_*.py`.

## The ported operation

`layout_box(pins, arrange, group_gap)` is the one function in `symbol_layout.py` that the MCP
calls. `live_symbol_add_pins` (server.py) calls `client.symbol_add_pins`, which calls `layout_box`
and turns its body rectangle and pin positions into `CreateItems`. The other functions in that
file (`default_side`, `group_rank`, `is_ground` and so on) are its helpers and are ported as part
of it. The function is deterministic and independent of the UI. It is also the first candidate
the roadmap names.

### JSON boundary

Request: `{"pins": [...], "arrange": "auto" | "as_given", "group_gap": 0..6}`. `arrange` and
`group_gap` are optional and default to `"auto"` and `2`. These are the MCP tool's parameters, and
the pins are passed exactly as the agent sends them. A request with any other shape returns a
`RequestError`: a request that is not a JSON object, an unknown field, `pins` that is not a list
of objects, an `arrange` that is not a string, or a `group_gap` that is not an integer (booleans
and `2.0` are rejected). `parse_request` in Rust and `reference()` in `parity.py` apply these
checks in the same order, with the same messages. As with Python's `json`, an integer literal with
more than 4,300 digits anywhere in the request makes it invalid JSON. That is CPython's default
int/str conversion limit; the harness sets it explicitly, so `PYTHONINTMAXSTRDIGITS` cannot change
the reference.

Response: the Python result as JSON, `{"body": [x1, y1, x2, y2], "pins": [...]}` in millimetres.
On failure it is `{"error": {"type": ..., "message": ...}}`, where `type` names the Python
exception (`ValueError`, or `KeyError`/`TypeError` for gap sides and types that the Python code
does not validate) and `message` is Python's text.

The output matches Python in full, not just in the coordinates:

- Unknown pin fields (`shape`, `unit` and others) are passed through.
- Key order is the same, including `setdefault` keeping a caller's `orientation` or `length`.
- Visible pin lengths are floats and hidden pin lengths are the integer `0`.
- Python's `str()`, truth tests and `str.strip()` are applied to non-string field values.
- The `repr()` of a pin in the "needs a number" error matches Python's.

Coordinates are integer nanometres inside the crate. Each one is written to JSON as an exact
decimal (`-7.62`, `0.0`) built from the integer with no float arithmetic, so parsing it back gives
the same nanometre count. Python rounds to 4 decimals, which is 100 nm. Every layout coordinate is
a multiple of 1.27 mm, so no rounding is lost.

### Multi-unit symbols

`layout_box` has no notion of units. A `unit` field passes through untouched, and duplicate pin
numbers are rejected across the whole request, even when the pins belong to different units. The
fixtures check that Rust behaves the same way, both for one request per unit (dual op-amp units A
and B, plus a power unit) and for all units in one request. Unit-aware layout would be a change to
the Python behaviour first.

## Running it

You need a stable Rust toolchain (1.85 or later) and Python 3. The WebAssembly run also needs the
`wasm32-unknown-unknown` target and Node.js.

```bash
rustup target add wasm32-unknown-unknown             # once
cd rust && cargo test --locked --release && cd ..    # unit tests
node --test rust/embeint-web/js/embeint_layout.test.mjs   # JavaScript loader tests
python rust/parity/parity.py --build                 # build, then compare (WASM skipped without node)
python rust/parity/parity.py --build --require-wasm --bench 200 --report /tmp/report.json
```

`--build` runs `cargo build --release` for the native runner and for the wasm32 module, and uses
the artefact paths that cargo reports for that build. A failed build therefore never falls back
to an older module left in the target directory (or restored from a CI cache). The harness then
sends every request through all three implementations and compares the responses structurally.
Key order, `int` versus `float` and each float's exact value all count. It exits non-zero on any
difference. With `--require-wasm`, it also exits non-zero when the WASM build fails or Node.js is
missing; without it, a failed WASM build only skips WASM. Without `--build`, it refuses an
artefact older than any Rust source. The corpus is the 64 cases in `cases.json`, a generated
256-pin symbol, 7 cases around Python's integer digit limit, 500 seeded pseudo-random pin lists
(`--generated N`), and 300 seeded pin lists with values of any JSON type in their fields
(`--typed N`).

CI runs `.github/workflows/rust-layout.yml` when `rust/**` or `symbol_layout.py` changes. Its
actions are pinned to commit SHAs and its Rust toolchain to an explicit version (`RUST_TOOLCHAIN`),
so a new stable release cannot fail unrelated pull requests with new lints. It runs rustfmt,
clippy with `-D warnings` (native and wasm32), `cargo check` with the declared minimum Rust 1.85.0
(native and wasm32), the unit tests, the loader tests, and the parity run with `--require-wasm`.
It is a separate job from the MCP unit tests (`mcp.yml`), which neither need nor wait for Rust.

## Parity results (2026-11-20)

Every run reports 872/872 identical for each implementation, and the harness exited 0 each time.

| Host | Python | Rust | Native | WASM (Node 22.22.1) |
| --- | --- | --- | --- | --- |
| WSL Ubuntu, x86_64 | 3.14.4 | 1.99.1 | 872/872 identical | 872/872 identical |
| Windows 11, x86_64 MSVC | 3.12.0 | 1.99.0 | 872/872 identical | not run (no Node.js on the host) |

The corpus is the 72 fixed cases, the 500 seeded cases and the 300 seeded typed cases. It covers
the following:

- **Ordinary layouts.** LDO and buck regulators, explicit gaps, top/bottom centring, long names,
  ground and feedback classification, inversion marks, pass-through fields, non-string values,
  and Unicode and whitespace.
- **Malformed inputs (39 cases).** Bad types and sides, missing or duplicate numbers, gap-side
  `KeyError`/`TypeError` cases, `group_gap` and `arrange` errors, wrong request shapes, invalid
  JSON, and `NaN`.
- **Multi-unit symbols (5 cases).**
- **Python's integer digit limit (7 cases).** Integers of 4,300 digits pass through; 4,301 digits
  anywhere in the request, signed or not, are invalid JSON; long floats are not limited.
- **Values of any JSON type (300 seeded cases).** Null, booleans, integers (including ones beyond
  64 bits), floats, strings, lists and objects in every pin field the layout reads or writes. 130
  of them lay out; the rest are `ValueError`, `TypeError` and `KeyError` responses, including
  Python `repr()`s of nested values.

There were 284 error responses in all.

The harness catches real differences. Each of these deliberate edits to the Rust code failed the
run:

- a hidden pin length written as a float;
- the `!` mark not removed;
- the order inside a group not stable;
- a wrong float `repr()` exponent threshold;
- `strip()` missing U+001C to U+001F.

One further edit made no observable difference, and is semantically equivalent: skipping `strip()`
for non-string numbers, whose `str()` never has outer whitespace. A first run showed that no
fixture depended on the `!` mark, so fixtures for it were added.

### Responsiveness

The table times each call with JSON in and JSON out: the best of 5 rounds of 200 calls over the
27 valid fixed cases, in microseconds. The host was shared with other builds (load average about 5
on 32 threads), so treat the ratios as indicative.

| WSL run | Median | Slowest (256 pins) | Compared with the Python adapter |
| --- | ---: | ---: | ---: |
| Python adapter (`json.loads` + `layout_box` + `json.dumps`) | 45.7 | 1354 | 1.0x |
| Python `layout_box` alone (as the MCP calls it) | 23.0 | 926 | |
| Rust native | 12.8 | 745 | 3.6x |
| Rust WASM in Node | 25.8 | 960 | 1.8x |

The Windows run gave a Python adapter median of 91.0 µs, a native median of 30.0 µs (3.0x), and
1246 µs natively for the 256-pin case.

A whole-corpus run, including process start, took 43 ms natively and 226 ms in Node. Most of the
Node time is startup and WebAssembly compilation. The release module is 234 KB.

Every case is far below interactive limits, and even 256 pins takes under 1 ms. For the MCP the
gain is small: a typical symbol takes about 23 µs in Python, against milliseconds for the IPC round
trip. The value of the port is reuse in the browser, not speed.

## Dependencies

The only direct dependency is `serde_json`, with `std`, `preserve_order` (which keeps Python's key
order) and `arbitrary_precision` (which keeps number literals exact, for Python's `int`/`float`
distinction). Licences are listed with the option used under GPL-3.0-or-later. Every licence is
compatible with GPLv3.

| Crate | Licence |
| --- | --- |
| serde_json 1.0.154, serde 1.0.229, serde_core 1.0.229, itoa 1.0.19, hashbrown 0.17.1 | MIT OR Apache-2.0 |
| indexmap 2.14.2, equivalent 1.0.3 | Apache-2.0 OR MIT |
| memchr 2.8.3 | Unlicense OR MIT |
| zmij 1.0.23 | MIT |
| foldhash 0.2.0 | Zlib |
| Build-time only (serde_derive proc macro): serde_derive 1.0.229, syn 3.0.10, quote 1.0.47, proc-macro2 1.0.107 | MIT OR Apache-2.0 |
| Build-time only: unicode-ident 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 |

The JavaScript loader, the Node runner and the harness use only the platform's standard library.

## Limitations and next steps

- **Not wired into the MCP.** Switching would need a review of these results, then an opt-in path
  in `client.symbol_add_pins` (for example, a native module or the CLI), with Python kept as the
  fallback.
- **Python `repr()` of unassigned code points.** Rust has no Unicode category tables, so it treats
  unassigned code points as printable, where Python escapes them. This only affects the "needs a
  number" message for a pin whose strings contain such characters. `str.upper()` follows the Unicode
  version of each runtime.
- **JSON parsers.** Python's `json` accepts lone surrogate escapes and nests deeper than
  serde_json's limit of 128 levels. Such requests are outside the tested contract.
- **Python 3.14 messages.** Python 3.14 words "unhashable type" errors differently. The reference
  adapter reduces them to the `unhashable type: '...'` part.
- **WASM traps.** A trap (there should be none, because the layout code does not panic on input)
  loses the module instance. `js/embeint_layout.mjs` then marks it broken: the trapping call
  rethrows the trap, every later call throws `EmbeintLayoutBrokenError` without entering the
  module, and `layout.broken` is true. It does not re-instantiate by itself, because synchronous
  instantiation is restricted on a browser's main thread; call `loadEmbeintLayout` again, with
  the bytes or a compiled `WebAssembly.Module`.
- **Python's integer digit limit.** The Rust side hard-codes CPython's default of 4,300 digits. A
  Python process configured with a different limit (`sys.set_int_max_str_digits`) would accept or
  reject other requests.
- **Pin length.** `pin_length` is not part of the boundary, because the MCP never passes it.
