<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# libs/sexpr headers: scope, representation, adapter domain and deferrals

Unit: `libs.sexpr.include.sexpr.application.pure.p001` (`isexprable.h`, `sexpr.h`,
`sexpr_exception.h`, `sexpr_parser.h` at `4cf939778df04d2b747bb2c4714fc5014a7565c7`).

## Scope

All 48 owned USRs are inline bodies of `sexpr.h`, `isexprable.h` and `sexpr_exception.h`.
`sexpr_parser.h` declares `SEXPR::PARSER` only; `Parse`, `ParseFromFile`, `GetFileContents` and
`parseString` are bodies of `sexpr_parser.cpp` in `libs.sexpr.application.ui.p001` and are not
ported here. The text parser (number, string, escape, comment and line-number handling) and
its file-reading entry points therefore remain unported; when that unit is ported its file
entry points become a typed host boundary.

The parser unit is the only producer of `PARSE_EXCEPTION` and of nodes carrying a parsed line
number. The header port provides the exception class (`ParseException`) and every
line-number constructor, which the corpus exercises directly.

## Representation

- `std::string` is bytes (`Vec<u8>`); embedded NULs are kept, and the `const char*`
  constructors take a `CStr`, ending at the first NUL as the C++ does.
- `size_t` is `u64` on every target. The derived constructors take `int aLineNumber` and pass
  it to the `size_t` base parameter: the conversion is modular, so line `-1` is
  18446744073709551615. A `usize` would differ on wasm32.
- Children are `Option<Box<Sexpr>>`: null children are representable (the default
  `ISEXPRABLE::SerializeSEXPR` returns `nullptr` and `operator<<` adds it), shared or
  self-referencing children are not (the reference would delete them twice). The implicit
  `SEXPR_LIST` copy constructor, which shares child pointers, is not provided.
- `SEXPR`'s class hierarchy is the `Node` trait over `Sexpr` and each concrete node type.
- C++ overload resolution has no Rust equivalent; each overload is a named constructor:
  `ScanArg::{int, long_int, double, string, sexpr_string, const_string, string_value,
  c_str}`, `ChildrenArg::{int, long_int, double, string, c_str, out_string, sexpr}`, and the
  non-const `AsSymbol`/`AsString` are `as_symbol_in`/`as_string_in`. A single `int` selects
  `SEXPR_LIST( int )` over the variadic template (`with_line_number`); the template is
  `from_args`.
- `what()` returns `msg.c_str()`: the message up to its first NUL. Both exception types
  report that prefix.

## Header include order

The pinned `isexprable.h` is not self-contained when it is the first sexpr header included:
it includes `sexpr.h`, whose `SEXPR::GetList` and `SEXPR_LIST` friend declarations then
precede the `SEXPR_LIST` and `ISEXPRABLE` declarations they name, and compilation fails. The
adapter includes `sexpr.h` first, as `sexpr.cpp` does. This affects only compilation order,
not behaviour.

## Adapter domain

Trees are flat pre-order arrays, so their depth is not limited by the 127-container JSON
transport. Requests are bounded identically on both sides: 20,000 nodes per tree, depth
4,000 and strings of 65,536 bytes, keeping the recursive `~SEXPR_LIST` (and, in the source
unit, `AsString`) within the reference's stack. `Scan`/`AddChildren` and the variadic
constructor are instantiated for 1 to 16 arguments; the C++ templates need at least one.

`Scan` reaches `doScan`, which the adapter guards: a scan is refused (`undefined sexpr
operation`) when the replayed control flow reaches `GetChild( i )` past the last child, a
null child, or the `INT` branch on an integer child, which writes a `double` through the
`int32_t*` (port/upstream-issues/sexpr-doscan-int-writes-double.md). The generator applies the
same replay to keep these requests out of the public corpus; 44 of them are controls.

## Deferrals in this revision

`Scan`, `AddChildren` and the variadic constructor call `doScan`/`doAddChildren` in
`sexpr.cpp` (unit `libs.sexpr.application.pure.p001`). Until that unit is ported they return
`Unported`; 620 public cases (300 scan, 160 add_children, 160 variadic constructor) defer
with retained oracle replies, and 44 scan controls are recorded with `rust_unported`. Every
other operation (752 cases) compares exactly. Wiring the source unit should rerun this corpus.

Minimum co-ports from the source unit (ownership unchanged, recorded in its notes): the two
`SEXPR` base constructors (line 1, or the given `size_t`) and `~SEXPR_LIST` (an iterative
`Drop`, so deep trees do not exhaust the native stack).
