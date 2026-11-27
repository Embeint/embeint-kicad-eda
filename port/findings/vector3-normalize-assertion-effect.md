<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# VECTOR3::Normalize: failed wxCHECK_MSG omits the assertion effect

Unit: `libs.kimath.include.math.application.ui.p002` (vector headers; accepted port in
`rust/kicad-math/src/vector.rs`). Recorded from the circle review (PR #95); not changed there.

## Location

Pinned `libs/kimath/include/math/vector3.h:160-171`:

```cpp
T norm = EuclideanNorm();
wxCHECK_MSG( norm > T( 0 ), *this, wxT( "Invalid element length 0" ) );
```

`Vector3::normalize` (`vector.rs`, "wxCHECK_MSG returns unchanged self by value for norm <= 0
or unordered") returns `Ok(*self)` when the check fails.

## Trigger and effect

Any vector whose norm is zero, negative or NaN. With wx 3.2 the failed check calls
`wxOnAssert( __FILE__, __LINE__, __FUNCTION__, "\"norm > T( 0 )\"", "Invalid element length 0" )`
when `wxTheAssertHandler` is set (and `wxTrap()` if the handler requests it) before returning
`*this`. `-DNDEBUG` keeps it (`wxDEBUG_LEVEL` 1). The Rust port reproduces the returned value
but not that host effect, and does not defer it.

## Evidence and confirmation

Source reading of the pinned header and wx 3.2.4 `debug.h`. The circle unit's hosted oracle
showed the same macro calling the default handler (printed assertions) under the oracle's
`-DNDEBUG` build. Not separately executed for `Normalize`. Confirmation state: open.

## Next step

The vector unit's owner represents the effect explicitly, for example a typed host callee
that defers (`Unported`) without a host and continues to `return *this` when the host returns,
as the circle unit now does for its seven checks. Until then the returned value alone is not
evidence of equivalence for the failing branch.
