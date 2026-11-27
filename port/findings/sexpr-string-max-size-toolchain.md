<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# std::string::max_size() depends on the libstdc++ headers and language mode

Owning unit: `libs.sexpr.application.pure.p001` (`libs/sexpr/sexpr.cpp` at
`4cf939778df04d2b747bb2c4714fc5014a7565c7`). Affected bodies: `SEXPR::AsString`, whose
`result.append( aLevel * 2, ' ' )` throws `std::length_error( "basic_string::_M_replace_aux" )`
when the count exceeds `max_size() - size()`, and both adapters' `AsString` guards, which leave a
request to the library exactly when that append throws.

`basic_string::max_size()` is an inline header function, so the value is fixed by the headers
and `-std` mode the reference is compiled with, not by the runtime `libstdc++.so`:

| Headers, mode (LP64) | Definition | `max_size()` |
| --- | --- | --- |
| GCC 13.3, C++20 (hosted reference: `ubuntu-24.04`, `CMAKE_CXX_STANDARD 20`) | `( allocator_traits::max_size() - 1 ) / 2`; C++20 `allocator_traits<std::allocator>` returns `size_t( -1 )` | 2^63 - 1 |
| GCC 13.3, C++17 | the same, with `std::allocator::max_size()` = `PTRDIFF_MAX` | 2^62 - 1 |
| GCC 15.2, C++17 or C++20 (local WSL diagnostic oracle) | `min( PTRDIFF_MAX, allocator max ) - 1` | 2^63 - 2 |

Measured with a probe printing `std::string().max_size()`, compiled by the Ubuntu 24.04
`g++-13` 13.3.0-6ubuntu2~24.04.1 packages (extracted, not installed) and by the local GCC
15.2.0. A second probe appending to a one-byte string gives `bad_alloc` for 2^63 - 2 bytes
with GCC 13.3 headers (the append passes its length check and fails to allocate) and
`length_error` with GCC 15 headers, on the same runtime library.

The port models the hosted reference: `kicad_sexpr::sexpr_cpp::STRING_MAX_SIZE` = 2^63 - 1,
mirrored by the corpus generator; the C++ adapter's guard reads `std::string().max_size()`
of its own build. Two earlier values were wrong for that target: 2^62 - 1 (the C++17 value)
and 2^63 - 2 (the GCC 15 value, which the hosted `rust-sexpr-live` run 45946821035 rejected
on six cases).

The values differ only where the indentation is 2^63 - 2 at a nonzero level (beside the
frame's newline): level 2^62 - 1 and its wrapped twin 2^63 + 2^62 - 1. On the hosted
reference that indentation fits, so the request reaches the 1 MiB indentation bound and both
adapters refuse it (`as_string output too large`, controls); with GCC 15 headers the library
throws `length_error` there instead. Level 2^62 (and 2^63 + 2^62) throws on every toolchain
and is compared. The guard lets the library run only where the indentation exceeds the
build's own `max_size()` minus the newline, and every listed value is at most 2^63 - 1, so on
any of these toolchains the library throws wherever the guard expects it to.

Consequence for local diagnostics: an oracle built with GCC 14 or later headers disagrees with
the port on the controls at those two levels (the oracle's library throws `length_error`; the
port refuses as the hosted reference does). Every other case and control is independent of the
value. The hosted GCC 13.3 run is the qualifying comparison.
