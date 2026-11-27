<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# libs/sexpr port audit

Pin `4cf939778df04d2b747bb2c4714fc5014a7565c7`. Portable code: `rust/kicad-sexpr`. Reference
adapter: `port/oracle/shims/libs_sexpr.cpp`. Corpus generator: `generate_cases.py` (seed
`0x5E592026`); controls: `port/oracle/fixtures/sexpr-controls.jsonl`.

## Header unit `libs.sexpr.include.sexpr.application.pure.p001`

Every owned USR is listed with its Rust body, the operation that checks it and what remains.
The `sexpr.cpp` bodies these reach (`doScan`, `doAddChildren`) are ported by the source unit
below, so no case defers.

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
| `SEXPR_LIST( const Args&... )` | `SexprList::from_args` | `construct_list` variadic (160) | none |
| `SEXPR_LIST::AddChildren` | `SexprList::add_children` | `add_children` (160) | none |
| `SEXPR_LIST::Scan` | `SexprList::scan` | `scan` (300) and 44 undefined-domain controls (refused on both sides) | none |
| `SEXPR_SCAN_ARG` (8 constructors) | `ScanArg::*` | each kind in `scan` (values written back and compared); types by unit tests | none |
| `SEXPR_CHILDREN_ARG` (7 constructors) | `ChildrenArg::*` | each kind in `add_children`/variadic (resulting trees compared); types by unit tests | none |

`sexpr_parser.h` has no inline body. Co-ported from the source unit: the `SEXPR` base
constructors (`DEFAULT_LINE_NUMBER`, stored line) and `~SEXPR_LIST` (iterative `Drop`).

Not ported because unrepresentable: the implicit `SEXPR_LIST` copy constructor (shares child
pointers, double delete), shared or self-referencing children, a null `const char*`, and an
out-of-range `SEXPR_SCAN_ARG::Type`/`SEXPR_CHILDREN_ARG::Type` (the closed Rust enums make
`doScan`'s and `doAddChildren`'s `else` branches unreachable).

## Source unit `libs.sexpr.application.pure.p001`

`kicad_sexpr::sexpr_cpp`; shim operations in `src/source_ops.rs` (`getter`, `insert`,
`extract`) plus the shared `scan`, `add_children` and `construct_list`. Bounds and refusals:
`port/findings/sexpr-source-port-and-asstring-bounds.md`.

| Upstream (owned USRs) | Rust | Checked by | Gap |
| --- | --- | --- | --- |
| `SEXPR( SEXPR_TYPE )`, `SEXPR( SEXPR_TYPE, size_t )` | `DEFAULT_LINE_NUMBER`, stored line (co-ported by the header unit) | every decoded node's line; `GetSymbol` message | none |
| `GetChildren`, `GetNumberOfChildren`, `GetList` | `SexprOps` | lists (with null children, 2,000 wide): dumps, count, identity; every atom type throws | none |
| `GetChild` | `SexprOps::get_child` | every index incl. null children, identity; atoms throw; past the end refused (controls) | none |
| `AddChild` | `SexprOps::add_child` | lists (tree, null, 200-deep chain, nested); atoms throw | none |
| `GetString`, `GetSymbol` | `SexprOps` | every special string; mismatches incl. the line-number message (lines -1, `INT32_MIN`, ...) | none |
| `GetLongInteger`, `GetInteger` | `SexprOps` | special and random integers; `int32_t` truncation | none |
| `GetDouble`, `GetFloat` | `SexprOps`, `double_to_float` | special bits (NaN payloads, subnormals, overflow), integers, 300 random | none |
| `AsString` | `as_string` | 2,409 requests: atoms (1,500 random doubles, decision points), lists, upstream `AsStringList` example, levels incl. the default argument, wraps and `length_error` (41) | requests above the resource bounds are refused by both adapters (controls) |
| `~SEXPR_LIST` | iterative `Drop` | every request; trees to depth 4,000 | unobservable |
| 9 `operator<<` | `SexprList::insert_*` | `insert` (407): each overload, chains, returned list, resulting tree, float NaN promotion | none |
| 7 `operator>>` | `SexprList::extract_*` | `extract` (544): each overload against each node type, accepted chains, exceptions, values, `m_inStreamChild` | past the end or null refused (controls) |
| `doScan` | `SexprList::do_scan` | `scan` (300 here, 300 header): reads, `INVALID_TYPE` stop, `STRING` skip, `STRING_COMP` mismatch | `INT` on an integer is undefined (refused); `else` unreachable |
| `doAddChildren` | `SexprList::do_add_children` | `add_children` (160 + 160), variadic (80 + 160) | `else` unreachable |

## Evidence (local, diagnostic)

GCC 15.2.0, glibc 2.43, rustc 1.99.1 on WSL, tested commit `cf5d83211b` (Rust and corpus;
the oracle adapter is unchanged since `69056891dd`); the hosted GCC 13.3 / glibc 2.39 run of
`rust-sexpr-live` is the reference.

- `kicad-difftest compare --partial`, header unit: 1,372 cases, 1,372 compared exactly,
  0 deferred (previously 620), 0 failures.
- Source unit: 6,240 cases, 6,240 compared exactly, 0 deferred, 0 failures.
- `compare_controls.py`: 97 controls (48 header, 49 source) equal on both programs; none
  defers.
- Native/wasm replay of all 7,709 requests in import-free wasm32: identical.
