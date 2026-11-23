<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# kimath header hash: BEZIER<double> results where two distinct NaN payloads meet

Unit `libs.kimath.include.application.pure.p001`, PR #51. Reference: `BEZIER<double>::PointAt`,
`Split` and `SubCurve` (`libs/kimath/include/bezier_curves.h:110-191`) with the free `VECTOR2`
operators (`libs/kimath/include/math/vector2d.h:461-545`) at pin
`4cf939778df04d2b747bb2c4714fc5014a7565c7`. Status: **open, needs a parent decision**; the 38 cases
below remain failing corpus cases.

## Trigger

A `double` addition, subtraction, multiplication or division whose two operands are NaNs with
different sign or payload, for example `aT * ( C1 - Start )` with a NaN `aT` and a NaN control
point, or `( t_m1_3 * Start ) + ( 3.0 * aT * t_m1_2 * C1 )` with NaNs in `Start` and `C1`.

IEEE 754 leaves open which input NaN the result carries. C++ adds nothing. On x86-64 an SSE
instruction returns its first source operand (quieted), so the result depends on which C++
operand the compiler places first. That is a property of the generated code, not of the KiCad
source.

## Evidence (live oracle, native WSL x86-64, GCC 15.2, libstdc++ 16)

Every other NaN result matched exactly: a single NaN operand is returned quieted with its sign
and payload, and an invalid operation without NaN operands (`inf - inf`, `0 * inf`) gives the
negative indefinite NaN `0xfff8000000000000`. The Rust port spells these rules out (private `fp`
modules of `kicad-math` and `kicad-hash`) and keeps the left operand when both are NaNs.

The cases that differ, all `BEZIER<double>`:

- `bezier-f64-nan-curve-0-split-0` .. `-7`, `bezier-f64-nan-curve-0-sub-curve-0`, `-1`, `-2`, `-4`;
- `bezier-f64-nan-curve-1-point-at-0` .. `-3`, `bezier-f64-nan-curve-1-split-0` .. `-7`,
  `bezier-f64-nan-curve-1-sub-curve-0` .. `-4`;
- `bezier-f64-random-007`, `-023`, `-031`, `-059`, `-071`, `-091`, `-095`, `-111`, `-119`.

Each differs only in the payload or sign of NaN components. `bezier-f64-nan-curve-*` were
written to measure this choice; the random cases draw control points from a pool with several
NaN payloads.

No consistent rule reproduces the oracle. A bit-level model of the evaluation, with the
two-NaN choice fitted over all 615 `BEZIER<double>` cases:

| Model | Cases still differing |
| --- | ---: |
| left operand for every operation (the port) | 38 |
| best fixed choice per operation kind (`+`: right; `-`, `*`, `/`: left) | 22 |
| best fixed choice per source site (four `PointAt` products, three `PointAt` sums, the `lerp` difference, product and sum, the `SubCurve` division; 2,048 combinations) | 11 |

Within one inlined `Split` the choice changes from statement to statement. In
`bezier-f64-nan-curve-0-split-6` (`aT` = `0x7ff80000000000c3`), the first five
`a + aT * ( b - a )` statements keep the control point's payload where both are NaNs. The last
one (`shared`) keeps `aT`'s payload, returning `0x7ff80000000000c3` for both components where every
per-site model predicts `0x7ff8000000000044` and `0xfff8000000000033`.

## Consequence

The exact comparison of these 38 cases measures GCC's operand order for this build of the
adapter. It does not measure the port's faithfulness to the KiCad source. Another compiler or
version (hosted CI uses GCC 13.3), or a change to the adapter's inlining, can choose differently.
The cases are kept in the corpus unchanged and are reported as failures. Nothing is normalized,
dropped or blessed, so no corrected golden can be blessed for the unit until this is decided.

## Options for the parent

1. Treat an operation that receives two distinct NaN payloads as outside the compared domain: an
   unspecified result rather than undefined behaviour. Both programs would detect it before
   evaluating and refuse it, as they refuse the undefined `BEZIER<int>` evaluations. This needs a
   reviewed domain decision because the inputs are valid C++.
2. Derive a per-statement model from the compiled oracle's instruction operand order, and
   qualify it on every oracle compiler in use (GCC 13 hosted, GCC 15 WSL). It is fragile: any
   compiler or adapter change can invalidate it.
3. Keep the exact comparison as is and accept that the unit cannot be blessed.

Next step: a parent decision between these options. Until then the unit's record stays
`partial` and its PR stays draft.
