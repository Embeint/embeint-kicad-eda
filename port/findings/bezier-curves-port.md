<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# bezier_curves.cpp port: compiled sites, domain, budgets and callers

Unit `libs.kimath.src.bezier_curves.application.ui.p001` (`libs/kimath/src/bezier_curves.cpp` at
`4cf939778df04d2b747bb2c4714fc5014a7565c7`), ported as `kicad_hash::bezier_curves`. Status
**partial**: 38 public cases defer at the wx assertion host.

## Reference object and derivation

The order was read from the locally built diagnostic oracle: extracted Ubuntu
`x86_64-linux-gnu-g++-13 (Ubuntu 13.3.0-6ubuntu2~24.04.1) 13.3.0`, `-O3 -DNDEBUG -fno-fast-math
-ffp-contract=off` (the oracle's flags), baseline x86-64 (SSE2), from this branch's adapter:
`reference/kimath/CMakeFiles/kimath.dir/src/bezier_curves.cpp.o` sha256
`eb0cf47d49f0e9b9cc1c5094dc35bd9a4c4c7f925adc825d9067b25c732680d2`, linked into `kicad-oracle`
sha256 `dcb891fae7ee704bbc3b84109d930c9a614755a0a572a2106432b2059e0b734a`. The object's undefined
libm symbols are `hypot`, `llround`, `pow`, `sincos`, `sqrt` (the errno path of `std::sqrt`) and
`tan`; `std::ceil` is inlined (exact). The two `TransformEllipseToBeziers` instantiations are weak
in the object; the linked binary uses the archive's copies (no adapter instantiates them).

Sites (offsets in the function), `op dst, src` in Intel order of the C++ operands:

| Function | Offsets | Order |
| --- | --- | --- |
| `BEZIER_POLY::eval` | +0x04..+0x26 | `omt = 1.0 - t`; `omt2 = omt * omt` |
| | +0x118..+0x15a (three points) | `omt + omt` for `2.0 * omt`; `( omt + omt ) * t`; `t * t`; `P0 * omt2`; `c1 * P1`; `term0 + term1`; `P2 * t2`; `sum + term2` |
| | +0x98..+0x10f (four points) | `t * t`; `t * 3.0`; `omt2 * omt`; `t * t2`; `t2 * 3.0`; `3t * omt2`; `3t2 * omt`; `omt3 * P0`; `a * P1`; `term1 + term0`; `P2 * b`; `sum + term2`; `P3 * t3`; `sum + term3` |
| `BEZIER_POLY::subdivide` | +0xa0..+0x13b (four points) | `( b - a ) * t` then `+ a` for `left_ctrl1`, `tmp`, `left_ctrl2`, `right_ctrl2`, `right_ctrl1`; `shared = t * ( rc1 - lc2 )` then `+ lc2`; stores left 0..3, right 2, 3, 1, 0 |
| | +0x140..+0x1d8 (three points) | `left[1] = ( P1 - P0 ) * t + P0`; `left[2] = eval( t )` (call); `right[1] = ( P2 - P1 ) * t + P1`; `right[0] = left[2]` |
| `BEZIER_POLY::thirdControlPointDeviation` | +0x23..+0x3f | `dy = p1.y - p0.y`; `dx = p1.x - p0.x`; `dy * dy + dx * dx` (swapped); `1e-6 > len_sq` returns `+0.0` |
| | +0x50..+0x8e | `sqrtsd`; `p0.x * p1.y`; `p1.x * p0.y`; `r = dy / len`; `s = ( p0.x - p1.x ) / len`; `u`; `r * p2.x`; `s * p2.y`; `s p2.y + r p2.x` (swapped); `+ u`; `andpd` |
| `TransformEllipseToBeziers<double>` | +0x3c..+0xc3 | `E - S`; `comisd` (NaN skips the `- 360`); `xorpd`; `- 360`; `* pi/180`; `/ (pi/2)` (folded); inline `ceil`; `cvttsd2si` |
| | +0xec..+0x148 | `arc / n`; `sincos( theta )`; `theta * 0.25`; `tan`; `4/3 * tan`; `sin * k`; `k sin + cos` (swapped); `cos * k`; `sin - k cos` |
| | +0x22c..+0x500 (per point) | `S * pi/180`; `i * inc`; `inc_i - S_rad`; `/ (pi/180)`; `xorpd`; `RotatePoint`; `radii * point` (mulpd, radii the destination); `xorpd` rotation; `RotatePoint`; `Center + point` (addpd, Center the destination) |

Of these only some can be observed: two distinct NaNs must meet. In `eval`, `subdivide` and
`thirdControlPointDeviation` (reached directly through the adapter's private entries) they can
everywhere; in `TransformEllipseToBeziers<double>` only at the `Center + point` sum (the rotated
unit point is finite, so the radius product has at most one NaN; `k`, `sin` and `cos` are finite
whenever the curve count is defined). The Rust writes every site as compiled anyway.

Inlined copies in `GetPoly`'s helpers are not bound: with finite control points every NaN of
`getQuadPoly`'s loop is the default NaN `0xfff8000000000000` (a NaN or infinite control point or a
NaN error makes `x0`, `x2` or the error a NaN, so `n` is undefined and refused), and the cubic path
outputs only copies of control points and points that passed `isFlat`, whose comparisons a NaN
always fails (an infinite end point makes `inv_delta_sq` zero and the products NaN).

`rust/kicad-bezier-curves-shim/check_sites.py` disassembles the four bound functions from an
oracle binary and compares their instructions (addresses, displacements and names removed) with
`reference-sites.txt`; the live workflow runs it on the hosted oracle before the comparison.

## Domain and budgets

The pinned code is undefined for: `getQuadPoly`'s `int n = std::ceil( ... )` and
`TransformEllipseToBeziers`' `static_cast<int>( std::ceil( ... ) )` of a NaN or a value beyond
`int` ([conv.fpint]); unchecked `m_ctrlPts` subscripts outside the vectors in the private members;
three-point `isFlat`'s `std::max( t, 0.0 )` and `std::min` of a NaN ([alg.min.max]); and
`ELLIPSE<int>`'s conversion of a NaN point, which cannot happen (a NaN rotation folds to zero in
`EDA_ANGLE::Normalize`, and every other intermediate is finite). These are refused on both sides.

`recursiveSegmentation` never terminates on a curve holding a NaN (every comparison fails and both
halves keep a NaN; its stack grows until allocation fails), and a tiny error makes it run for an
impractically long time. The adapter therefore runs `GetPoly` only within a budget of 2^20 loop
iterations (getQuadPoly's `n` plus one per iteration of recursiveSegmentation's and
cubicParabolicApprox's loops), counted by a guard that repeats the pinned control flow before the
genuine call; its comparisons equal the genuine ones (a NaN's payload never changes a comparison),
and the genuine output size is checked against the guard's. `TransformEllipseToBeziers` runs only
for at most 4096 curves. Rust counts in the same places and refuses with the same message; the
portable API takes the limits as arguments, since unbounded growth would exhaust a host. The
generator classifies exactly: it replays the cubic path with IEEE operations and a correctly rounded
integer `sqrt`, and bounds getQuadPoly's `n` (which needs glibc's `pow` and `hypot`) with rational
intervals.

## Assertions

`wxASSERT( false )` in `isFlat`, `eval`, `subdivide` and `GetPoly( VECTOR2D )` (a count other than
three or four) calls `wxOnAssert( __FILE__, line, function, "false", NULL )` (lines 154, 205, 321,
600). The adapter records each call and lets the process's default handler return (wx 3.2 without an
application logs to stderr), so the genuine continuation follows; Rust's production host defers as
`wxOnAssert` naming this unit, and crate tests check each continuation with a returning host. 38
public cases defer this way.

## Callers

`git grep -n "libs.kimath.src.bezier_curves"` finds no accepted `UNPORTED` deferral naming this
unit: 0 before, 0 after. The header unit's `BEZIER_POLY( std::vector<VECTOR2D> )` and `BEZIER<T>`
bodies never call into this file. The pinned callers of `BEZIER_POLY` and
`TransformEllipseToBeziers` (EDA_SHAPE, the GAL backends, plotters, the EasyEDA, Altium and IPC-2581
importers, teardrops, the font outline decomposer, the drawing-sheet importer, the arc tangent seed)
are all unported. No corpus of another unit changes.

## Toolchain notes

Local runs used glibc 2.43's libm (hosted `GLIBC_TUNABLES`), whose `pow`, `hypot`, `tan` and
`sincos` are not the locked 2.39 bodies the Rust uses; every compared reply agreed regardless. The
hosted `rust-bezier-curves-live` run on glibc 2.39 is the reference.
