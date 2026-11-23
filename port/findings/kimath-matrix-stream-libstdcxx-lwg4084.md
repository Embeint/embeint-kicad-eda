<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# MATRIX3x3 stream output depends on the runtime libstdc++ version

Unit: `libs.kimath.include.math.application.pure.p001`, `operator<<( std::ostream&, const
MATRIX3x3<T>& )` (`libs/kimath/include/math/matrix3x3.h:417-435`). The operator inserts each
member into the caller's stream, so `num_put` in the runtime libstdc++ formats it.

## Trigger

With `std::ios_base::uppercase | std::ios_base::fixed`, libstdc++ before GCC 15 formats with
`%f`, which prints `inf`/`nan`. GCC 15.1 and later implement LWG 4084 ("std::fixed ignores
std::uppercase") and use `%F`, which prints `INF`/`NAN`. Finite values are unaffected,
because `%f` and `%F` differ only for infinities and NaNs.

## Evidence

`libstdc++-v3/src/c++98/locale_facets.cc`, `__num_base::_S_format_float`, at
`releases/gcc-13.3.0`, `gcc-14.2.0` and `gcc-14.3.0` emits `'f'`. At `releases/gcc-15.1.0` and
`gcc-15.2.0` it emits `__upper ? 'F' : 'f'` under an `_GLIBCXX_RESOLVE_LIB_DEFECTS` 4084
comment. The function is compiled into the shared library, so the runtime package decides:
the hosted ubuntu-24.04 runner has a pre-15 runtime, while the qualified WSL host (Ubuntu
26.04, GCC 15.2) has a post-4084 one. On macOS, libc++ already prints `INF`.

## Port and corpus

The port (`kicad_math::stream`) implements the pre-4084 rule (`fixed` always uses `%f`). That
matches the runtime of the hosted CI gate and the GCC 13.3 compiler the hosted oracle
records. The corpus keeps 28 `stream-state-*` cases that combine `fixed`, `uppercase` and
infinite or NaN members: `fixed-uppercase-*` and `fixed-all-*`. They are expected to pass
against a pre-15 runtime and fail against GCC 15 or later. That difference is reported, not
hidden.

## Decision required (not taken here)

This is the same reference-environment question as the compiler and glibc pins: the oracle
contract should name its libstdc++ runtime. If the reference moves to GCC 15 or later, the
port must switch to `%F` for `uppercase` + `fixed`.
