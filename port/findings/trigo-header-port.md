<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# trigo.h inline bodies: caller wiring, loop bound and operand order

Unit: `libs.kimath.include.application.ui.p001` (`libs/kimath/include/trigo.h` at
`4cf939778df04d2b747bb2c4714fc5014a7565c7`). This file is also the addendum to
`port/findings/trigo-source-port.md`, `port/findings/circle-deferrals-and-adapter-domain.md` and
`port/findings/shape-headers-p003-port.md`, whose header-unit deferrals it removes.

## Caller deferrals before and after

Counts from the Rust shims and the local diagnostic oracle (below) on accepted `origin/rust`
(eef1c186ac) and on this branch. Every former deferral is now compared exactly, with 0 failures.

| Caller unit | Call site | Cases before -> after | Controls before -> after |
| --- | --- | --- | --- |
| `libs.kimath.src.trigo.application.ui.p001` | `CalcArcMid`: `RotatePoint( VECTOR2I&, const VECTOR2I&, EDA_ANGLE )` | 311 -> 0 | 2 -> 0 (now refused identically) |
| same | chord/angle `CalcArcCenter`: `RotatePoint( VECTOR2D&, EDA_ANGLE )` | 214 -> 0 | none |
| `libs.kimath.src.geometry.circle.application.ui.p001` | `Intersect( CIRCLE )`: `RotatePoint( VECTOR2I&, EDA_ANGLE )` | 193 -> 0 | 4 -> 0 |
| same | `ConstructFromTanTanPt` through `CalcArcMid` | 70 -> 0 (48 complete; 22 reach CIRCLE's own failing `wxCHECK_MSG` and defer at its `wxOnAssert`) | 2 -> 0 |
| `libs.kimath.include.math.application.ui.p001` (BOX2) | rotated `Intersects`, `GetBoundingBoxRotated` | 78 -> 0 | none |
| `libs.kimath.include.geometry.application.ui.p003` | `SHAPE_CIRCLE::Rotate` | 34 -> 0, plus 28 continuation cases | 12 continuation controls added |

Totals per caller corpus, before -> after: trigo.cpp 5372 -> 5897 compared of 5987 (deferred 615
-> 90: 56 `kimathLogOverflow`, 34 distinct-NaN vector sums); CIRCLE 4636 -> 4877 of 4978 (342 ->
101: 39 `wxOnAssert`, 56 vector, 6 `kimathLogOverflow`); BOX2 14527 -> 14605 of 16066 (1539 ->
1461); shape headers 4006 -> 4068 of 4227 -> 4255 (221 -> 187). The vector and geometry corpora,
which the BOX2 workflow reruns as callers, are unchanged (0 failures).

The trigo.cpp `HeaderCallees` seam and the CIRCLE test-only cardinal `RotatePoint` and forwarding
callees are removed: the production paths now call `kicad_math::trigo_header`, so the crate tests
of those continuations use the same code the shims run. No `UNPORTED` tag names this unit any more.

The shape header generator kept every `SHAPE_CIRCLE::Rotate` as a script's last call because the
rotated centre was not computable. It now replays a cardinal rotation exactly (an integral multiple
of 90 degrees, or a non-finite angle, which `foldToTwoPeriods` makes 0) and continues the script on
the rotated circle; a rotation that needs sin/cos still ends its script. The new cases use their
own seed, so every earlier case is byte-identical.

## Template loops and the adapter's bound

`NormalizeAnglePos`, `NORMALIZE_ANGLE_POS` and `NormalizeAngle180` are instantiated for `int` and
`long long` only. GCC 13.3 inlines them as loops (one pass per 3600), so a `long long` near its
limits would run about 10^15 passes. The adapter refuses a `long long` beyond `3600 * 2^24` as
"outside the verified domain"; this is a harness bound, recorded as 15 controls, not an undefined
domain. The Rust port evaluates the loops in closed form for every value; crate tests compare it
with the literal loops, including the `int` limits (a few hundred thousand passes).

## Compiled operand order

For double results the GCC 13.3 objects were disassembled: the adapter's `DEG2RAD`, `RAD2DEG` and
`RAD2DECIDEG` are `mulsd const, %xmm0; divsd const, %xmm0` with the input as destination (source
order, no reassociation; the constants are never NaN). The wrappers compile to calls of the
out-of-line trigo.cpp overloads. The newly reachable tail of the chord/angle `CalcArcCenter`
(trigo.cpp.o, `CalcArcCenter(VECTOR2<double> const&, VECTOR2<double> const&, EDA_ANGLE const&)`
+0x6c7..+0x6e4) calls `RotatePoint(double*, double*, EDA_ANGLE const&)` out of line and then sums
`addpd 0xa0(%rsp), %xmm4; addpd 0x90(%rsp), %xmm4` with `start` in `%xmm4`: `( start + vc ) +
vec2` in source order, as the Rust `Vector2D` addition (`fp::add( lhs, rhs )`) evaluates it.

## Local evidence

Oracle built from the committed blobs with the extracted Ubuntu GCC 13.3 toolchain in WSL
(`port/oracle/build.sh`, ctest 17/17), run on the glibc 2.43 host with the hosted
`GLIBC_TUNABLES` and `fma` bound to the hash-checked locked glibc 2.39 libm by the trigo unit's
diagnostic preload. `kicad-difftest compare --partial` and each unit's `compare_controls.py`:
0 failures and every control as recorded for this unit, trigo.cpp, CIRCLE, BOX2, the shape
headers, vector and geometry. Every generator touched (`kicad-trigo-header-shim`,
`kicad-shape-headers-shim`) and every caller generator gives `--check`-identical output on
Windows and Linux. The hosted live workflows (GCC 13.3, glibc 2.39) are the reference.
