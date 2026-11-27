<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# libs/sexpr source: AsString reference details, adapter domain and bounds

Unit: `libs.sexpr.application.pure.p001` (`libs/sexpr/sexpr.cpp` at
`4cf939778df04d2b747bb2c4714fc5014a7565c7`). The header unit
`libs.sexpr.include.sexpr.application.pure.p001` depends on it for `doScan` and
`doAddChildren`; with this port its 620 previously deferred cases compare (0 deferrals).

## Reference details the port reproduces

- `std::string::max_size()` of the reference libstdc++ is `min( PTRDIFF_MAX, allocator
  max_size() ) - 1` = 2^63 - 2 on LP64 (GCC 12 and later), reported by the pinned oracle itself
  (`std::string().max_size()` in the adapter guard). An earlier draft of this port assumed the
  older `( max - 1 ) / 2` = 2^62 - 1; the oracle comparison rejected it on 14 cases at the
  boundary (fixed in this PR before any publication).
- `AsString` indents with `result.append( aLevel * 2, ' ' )` in `size_t` arithmetic: the
  product wraps modulo 2^64 (level 2^63 indents by 0, level 2^63 + 1 by 2), and `append`
  throws `std::length_error( "basic_string::_M_replace_aux" )` when the count exceeds
  `max_size()` minus the frame's current `result`, which already holds the leading newline at
  a nonzero level. So level 2^62 - 1 throws (2^63 - 2 > 2^63 - 3) although its count equals
  `max_size()`; the corpus has that boundary.
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
  on the machine), an estimated output above 64 MiB, or an estimated copy work above 2^30
  bytes. The recursive reference builds each list's text in its own frame and appends it to
  every enclosing frame, so a chain of depth d copies on the order of d^3 bytes: depth 4,000
  would copy about 40 GB. Depth 1,000 is compared; depth 1,300 and 4,000 are controls.
- `invalid path`: a getter path through an atom, out of range or to a null child (a request
  error, not a library call).

The Rust library itself is linear (explicit stacks for `AsString` and `Drop`); the bounds only
keep the reference evaluation finite and machine-independent.

## Not represented

Shared or self-referencing children (double delete), the implicit `SEXPR_LIST` copy, and the
`else` branches of `doScan`/`doAddChildren` (unreachable with the closed argument enums). A
non-list `AddChild` leaks its child in the reference; the port drops it (unobservable).

## Status

Local diagnostic evidence only (GCC 15.2 / glibc 2.43); the hosted GCC 13.3 / glibc 2.39
`rust-sexpr-live` run is the reference. Both units are `partial` with zero deferrals until that
run and independent review qualify them and reference goldens are recorded.
