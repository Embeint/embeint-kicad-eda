<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# libs/sexpr port audit

Pin `4cf939778df04d2b747bb2c4714fc5014a7565c7`. Portable code: `rust/kicad-sexpr`. Reference
adapter: `port/oracle/shims/libs_sexpr.cpp`. Corpus generator: `generate_cases.py` (seed
`0x5E592026`); controls: `port/oracle/fixtures/sexpr-controls.jsonl`.

## Header unit `libs.sexpr.include.sexpr.application.pure.p001`

Every owned USR is listed with its Rust body, the operation that checks it and what remains.
"Deferred" means the Rust body is complete but reaches a `sexpr.cpp` function that returns
`Unported` in this revision; the oracle reply is retained for those cases.

| Upstream (owned USRs) | Rust | Checked by | Gap |
| --- | --- | --- | --- |
| `AsSymbol`/`AsString` (`const std::string&`) | `as_symbol`, `as_string` | `out_string`: flag, bytes, same object | none |
| `AsSymbol`/`AsString` (`std::string&`) | `as_symbol_in`, `as_string_in` | `in_string`: flag, same object, write through | none |
| `PARSE_EXCEPTION` ctor, `what`, dtor | `ParseException::new`, `StdException::what` | `exception`: direct and thrown/caught as `std::exception`, embedded NULs | none |
| `INVALID_TYPE_EXCEPTION` ctor, `what`, dtor | `InvalidTypeException` | as above | none |
| `ISEXPRABLE::SerializeSEXPR`, `DeserializeSEXPR`, dtor | `Isexprable` defaults, `DefaultIsexprable` | `isexprable`: null result, tree unchanged | none |
| `SEXPR::IsList/IsSymbol/IsString/IsDouble/IsInteger`, `GetLineNumber`, `~SEXPR` | `Node` | `predicates`: all five predicates and the line of every node | none |
| `SEXPR_INTEGER/DOUBLE/STRING/SYMBOL` (value) and (value, `int` line) | `new`, `with_line_number` | every decoded tree (line null selects the one-argument constructor); negative and extreme lines | none |
| `SEXPR_LIST()`, `SEXPR_LIST( int )` | `SexprList::new`, `with_line_number` | `construct_list` default/line, every decoded list | none |
| `SEXPR_LIST( const Args&... )` | `SexprList::from_args` | `construct_list` variadic (160) | deferred at `doAddChildren` |
| `SEXPR_LIST::AddChildren` | `SexprList::add_children` | `add_children` (160) | deferred at `doAddChildren` |
| `SEXPR_LIST::Scan` | `SexprList::scan` | `scan` (300) and 44 undefined-domain controls | deferred at `doScan` |
| `SEXPR_SCAN_ARG` (8 constructors) | `ScanArg::*` | each kind in `scan`; types checked by unit tests | values read by `doScan` (deferred) |
| `SEXPR_CHILDREN_ARG` (7 constructors) | `ChildrenArg::*` | each kind in `add_children`/variadic; types by unit tests | values read by `doAddChildren` (deferred) |

`sexpr_parser.h` has no inline body. Co-ported from the source unit: the `SEXPR` base
constructors (`DEFAULT_LINE_NUMBER`, stored line) and `~SEXPR_LIST` (iterative `Drop`).

Not ported because unrepresentable: the implicit `SEXPR_LIST` copy constructor (shares child
pointers, double delete), shared or self-referencing children, a null `const char*`, and an
out-of-range `SEXPR_SCAN_ARG::Type`/`SEXPR_CHILDREN_ARG::Type` (the closed Rust enums make
`doScan`'s and `doAddChildren`'s `else` branches unreachable).

## Evidence (local, diagnostic)

GCC 15.2.0, glibc 2.43, rustc 1.99.1 on WSL; the hosted GCC 13.3 / glibc 2.39 run of
`rust-sexpr-live` is the reference.

- `kicad-difftest compare --partial`: 1,372 cases, 752 compared exactly, 620 deferred
  (`doScan` 300, `doAddChildren` 320), 0 failures.
- `compare_controls.py`: 48 controls (2 tree bounds, 2 argument-count bounds, 44 scans whose
  evaluation is undefined, deferred in Rust) equal on both programs.
- Native/wasm replay of all 1,420 requests in import-free wasm32: identical.
