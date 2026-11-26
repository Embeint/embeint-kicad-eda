<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# TRANSFORM::MapAngles always reports a swap

Reference: KiCad `4cf939778df04d2b747bb2c4714fc5014a7565c7`,
`libs/kimath/src/transform.cpp:76-118` (`TRANSFORM::MapAngles`), with `EDA_ANGLE::Invert`
and `EDA_ANGLE::Normalize` in `libs/kimath/include/geometry/eda_angle.h`.

Trigger: the swap test is

```cpp
static const EDA_ANGLE epsilon( 0.1, DEGREES_T );
...
residualError.Normalize();
if( residualError > epsilon || residualError < epsilon.Invert().Normalize() )
```

`epsilon.Invert()` is -0.1 degrees and `Normalize()` maps it to 359.9 degrees, not to a
negative tolerance. `residualError` has just been normalized into [0, 360) (a non-finite
residual folds to 0). Every value in [0, 360) is either greater than 0.1 or less than
359.9, so the condition is always true: the two mapped angles are always swapped and the
function always returns `true`, whatever the matrix and angles. The apparent intent (swap
only when the transform reverses the sweep direction, i.e. a residual outside
[-0.1, 0.1] degrees) is not what the code evaluates.

Reproduction: `TRANSFORM().MapAngles( &a1, &a2 )` with `a1 = 10 deg`, `a2 = 80 deg` (the
identity matrix, which preserves the sweep) returns `true` and leaves `a1 = -280 deg`,
`a2 = 10 deg`.

Evidence: all 5,208 `map_angles` cases of the public corpus for unit
`libs.kimath.src.transform.application.ui.p001` (every orientation, degenerate and
non-unimodular matrices, edge, random and arbitrary-bit angles including NaN and
infinities) return `"swap": true` from the genuine pinned C++ (`kicad-oracle` built from
`port/oracle/shims/kimath_src_transform.cpp`, glibc 2.39 SSE2 runtime). The Rust port
`kicad_hash::transform_source::Transform::map_angles` evaluates the same expression and
matches every case exactly. The behaviour is ported as written; it is not corrected and no
comparison exemption is taken.

Confirmation state: confirmed by source analysis and by the live oracle over the corpus.
KiCad application impact (which callers rely on the returned flag or the swapped order) is
not assessed here; callers are in eeschema, outside the ported units. Nothing has been
reported upstream.
