<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# libs/sexpr source: AsString reference details, adapter domain and bounds

Unit: `libs.sexpr.application.pure.p001` (`libs/sexpr/sexpr.cpp` at
`4cf939778df04d2b747bb2c4714fc5014a7565c7`). The header unit
`libs.sexpr.include.sexpr.application.pure.p001` depends on it for `doScan` and
`doAddChildren`; with this port its 620 previously deferred cases compare (0 deferrals).

## Reference details the port reproduces

- `std::string::max_size()` of the hosted reference build (GCC 13.3 libstdc++ headers, C++20,
  LP64) is `( allocator_traits::max_size() - 1 ) / 2` = 2^63 - 1, the C++20 `std::allocator`
  traits giving `size_t( -1 )`. The value is fixed by the headers and language mode, so it is a
  toolchain difference: C++17 gives 2^62 - 1 and GCC 14 and later 2^63 - 2
  (`port/findings/sexpr-string-max-size-toolchain.md`). Two earlier drafts of this port were
  wrong for the target: 2^62 - 1, and then 2^63 - 2 from the local GCC 15 oracle, which the
  hosted run 45946821035 rejected on six cases at level 2^62 - 1.
- `AsString` indents with `result.append( aLevel * 2, ' ' )` in `size_t` arithmetic: the
  product wraps modulo 2^64 (level 2^63 indents by 0, level 2^63 + 1 by 2), and `append`
  throws `std::length_error( "basic_string::_M_replace_aux" )` when the count exceeds
  `max_size()` minus the frame's current `result`, which already holds the leading newline at
  a nonzero level. So level 2^62 - 1 (and its wrapped twin 2^63 + 2^62 - 1) indents by
  2^63 - 2 = `max_size()` - 1, the largest count that still passes, and the reference then fails
  to allocate it; level 2^62 (2^63 + 2^62) throws. The adapters refuse the first at the
  indentation bound below (controls) and compare the second.
- Integers print through `std::stringstream << GetInteger()`: the value is truncated to
  `int32_t` first, and `operator<<( int )` goes through `num_put<char>::do_put( long )` in the
  classic locale; the port calls the accepted `kicad_wx_stl_compat` port of that body.
- Doubles print with fmt 12.2.0 (the pinned `thirdparty/fmt`) `{:.10g}`; when that is
  exponential, `{:.10f}` with trailing zeros and then a trailing point removed. `-0`, `-nan`
  and `-inf` keep the sign bit; ties at the tenth significant digit round to even. The corpus
  checks the power-of-ten decision points within a few ulps and 1,500 random doubles.
- `GetFloat` (`cvtsd2ss`) and the `float` insertion's promotion (`cvtss2sd`) keep NaN sign and
  payload and set the quiet bit; Rust's `as` leaves that unspecified (and wasm may
  canonicalise), so both conversions are explicit.
- `operator>>( std::string& )` accepts a symbol in its type test and then calls `GetString()`,
  which throws `INVALID_TYPE_EXCEPTION`; the target and `m_inStreamChild` are unchanged.
  `operator>>( float& )` and `( double& )` refuse integer atoms although `GetDouble()` would
  accept them.

## Adapter domain

Both adapters (`port/oracle/shims/libs_sexpr.cpp` `runSource`,
`rust/kicad-sexpr-shim/src/source_ops.rs`) refuse, with the same message:

- `undefined sexpr operation`: `GetChild` past the last child (an out-of-range
  `m_children[]`), a null child that `AsString`, an extraction operator or `doScan`
  dereferences, `doScan`'s `INT` branch on an integer child
  (`port/upstream-issues/sexpr-doscan-int-writes-double.md`).
- `as_string output too large`: before the first indentation `append` would refuse, an
  indentation above 1 MiB (a larger one is a multi-exabyte allocation whose `bad_alloc` depends
  on the machine), an output above 64 MiB, or a copy work above 2^30 bytes. The recursive
  reference builds each list's text in its own frame and appends it to every enclosing frame,
  so the work is the sum of every frame's result length: each node's own bytes times its depth.
  Both adapters and the generator count those bytes exactly: a list's newline, indentation,
  parentheses and the separators between its children; a string with its quotes; a symbol as
  it is; an integer or double as the library formats it (the C++ adapter calls `AsString` on
  the atom). A chain of depth d copies on the order of d^3 bytes: depth 4,000 would copy about
  40 GB. Depth 1,000 is compared; depth 1,300 and 4,000 are controls. An earlier draft charged
  every double 400 bytes, which refused practical requests such as a 200-deep chain whose leaf
  holds 19,000 doubles `0.0` (78,398 bytes of output, 13,012,299 of work, estimated at
  1,536,812,299); that chain is now compared, with a 1,000-deep chain of 1,000 short doubles, a
  200-deep chain of 1,000 `-DBL_MAX` doubles (310 bytes each) and a tree whose work is exactly
  2^30. Its twin one byte longer (2^30 + 2) and chains of `-DBL_MAX` doubles at 200 x 19,000
  (1,193,083,299) and 3,000 x 3,000 remain controls.
- `invalid path`: a getter path through an atom, out of range or to a null child (a request
  error, not a library call).

The Rust library itself is linear (explicit stacks for `AsString` and `Drop`); the bounds only
keep the reference evaluation finite and machine-independent.

## Not represented

Shared or self-referencing children (double delete), the implicit `SEXPR_LIST` copy, and the
`else` branches of `doScan`/`doAddChildren` (unreachable with the closed argument enums). A
non-list `AddChild` leaks its child in the reference; the port drops it (unobservable).

## Status

Local evidence with an oracle built from the Ubuntu 24.04 GCC 13.3.0 compiler packages (the
hosted toolchain's headers; local glibc 2.43), plus a GCC 15.2 diagnostic oracle that differs
only on the eight `max_size()` boundary controls; the hosted GCC 13.3 / glibc 2.39
`rust-sexpr-live` run is the reference. Both units are `partial` with zero deferrals until that
run and independent review qualify them and reference goldens are recorded.
