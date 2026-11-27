<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# SegmentIntersectsSegment writes y from the second segment's parameter

Reference: KiCad `4cf939778df04d2b747bb2c4714fc5014a7565c7`,
`libs/kimath/src/trigo.cpp:132-138` (`SegmentIntersectsSegment`).

Trigger: any call with a non-null `aIntersectionPoint` whose two parameters `u_a` and `u_b`
differ. The written point is `a_p1_l1 + ( KiROUND( dX_a * num_a / den ),
KiROUND( dY_a * num_b / den ) )`: x uses `u_a = num_a / den` on the first segment's direction,
but y uses `u_b = num_b / den` on the same direction. The true crossing is
`a_p1_l1 + u_a * ( dX_a, dY_a )` in both coordinates.

Reproduction: segments `( 0, 0 ) -> ( 1, 1 )` and `( 10, 0 ) -> ( 0, 10 )`. Then
`dX_a = dY_a = 1`, `den = -20`, `num_a = -100`, `num_b = -10`; the reference writes
`( 5, KiROUND( 0.5 ) ) = ( 5, 1 )`, while the lines cross at `( 5, 5 )`. The function returns
false here (the crossing is outside the first segment), but the point is written before the
bounds tests, so the value is observable. For segments that do intersect, `( 0, 0 ) -> ( 10, 10 )`
and `( 0, 10 ) -> ( 10, 0 )` give `u_a = u_b = 1/2` and the correct `( 5, 5 )`, which hides the
defect for symmetric inputs.

Confirmation state: source analysis; the Rust port reproduces the reference expression and the
in-crate test `trigo::tests::segments_intersect` records the value. The oracle corpus of
`libs.kimath.src.trigo.application.ui.p001` (cases `ssi-*` with `"point"` requested) compares
the written point with the pinned build. No accepted caller in the port passes an output point
(BOX2::Intersects passes none). This is a fork-local reference finding, not permission to change
KiCad's behaviour, normalize difftest output, or send anything upstream.
