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
| `libs.kimath.include.geometry.application.ui.p003` | `SHAPE_CIRCLE::Rotate` | 34 -> 0, plus 28 continuation cases | 18 rotate-then controls added: 12 fail after the rotation, 6 before it (below) |

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

Of the twelve controls those continuation scripts produced, six (`7`, `14`, `33`, `34`, `36`,
`37`) reach an undefined call in the two calls before the rotation (for example `14` overflows
`int` in `BBox` with radius 2^30), so they never exercise the continuation; they are kept as
ordinary controls. The six others (`16`, `19`, `20`, `21`, `25`, `28`) fail after the rotation.
Six hand controls `shape-circle-rotate-then-undefined-*` fail only after a cardinal rotation and
because of it: a `Move` or `Collide` difference that overflows only from the rotated centre. The
generator replays each script call by call, checks that their first undefined call follows the
rotation and that the same calls on the unrotated circle are defined, and prints the split.

## Template loops, the double instantiation and the adapter's budget

`NormalizeAnglePos`, `NORMALIZE_ANGLE_POS` and `NormalizeAngle180` are instantiated for `int` and
`long long` in the pinned tree. They are unconstrained templates, so their `double` instantiation
is ported as well (no caller to wire). GCC 13.3 compiles every instantiation as literal loops, one
pass per 3600: for `double` a `comisd` and one `addsd`/`subsd` of 3600.0 per pass, so each pass
rounds (disassembled from a probe compiled with the adapter's flags against the pinned header).
Observed with that probe: `-1e-300` gives `+0.0` (the first loop's last pass rounds to 3600 and
the second loop runs once), `-1.5 * 2^-42` gives `0x40ac1fffffffffff`, `-0.0` and NaNs are
unchanged. The port reproduces every pass exactly: passes are exact up to `2^57` except the one
that changes the sign, which is run literally; above `2^57` the step is constant within a binade
(3584, 4096, or a 3616/3584 tie at ulp 32); above `2^65`, and at infinity, a pass makes no
progress and the loop never terminates, which is undefined ([intro.progress]) and refused by both
programs after testing the first pass.

The adapter runs the compiled loops, so it refuses an angle needing more than 2^24 passes as
"outside the verified domain"; a harness bound, not an undefined domain. The review of 34449e8ff3
found the former magnitude envelope `|a| <= 3600 * 2^24` stricter than that budget: four requests
(`NormalizeAnglePos`/`NORMALIZE_ANGLE_POS` at `3600 * 2^24 + 1`, `NormalizeAngle180` at `+-(3600 *
2^24 + 1)`) need exactly 2^24 passes. Both adapters now count the passes exactly (quotients, in
128-bit integers) and the generator counts them independently with fractions, so the refusal
boundary is the budget itself; those four requests are compared, and each budget boundary is a
case with the next value a control. The Rust port evaluates every terminating angle; crate tests
compare it with the literal loops, including the `int` limits (a few hundred thousand passes) and
`double` angles at the budget (2^24 passes).

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
headers, vector and geometry. After the review corrections: this unit 4693 cases, 4584 compared,
109 deferred, 397 controls; shape headers 4255 cases, 4068 compared, 187 deferred, 484 controls;
the other callers unchanged. Every generator touched (`kicad-trigo-header-shim`,
`kicad-shape-headers-shim`) and every caller generator gives `--check`-identical output on
Windows and Linux. The hosted live workflows (GCC 13.3, glibc 2.39) are the reference.
