<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# ELLIPSE: compiled NaN sites and undefined domain

Unit: `libs.kimath.src.geometry.ellipse.application.ui.p001`
(`libs/kimath/src/geometry/ellipse.cpp` at pin `4cf939778df04d2b747bb2c4714fc5014a7565c7`).
Confirmation state: confirmed locally against the pinned source built with GCC
15.2 and glibc 2.43 (x86-64 SSE2, `-O3 -fno-fast-math -ffp-contract=off`),
oracle binary sha256
`3d9f83c2d737a9fa139d4c566a95eeefafa4bb4f0da2aafcc9b0ab25d66394e4`. Not a reference defect.
Compiled NaN operand forms remain unqualified against the target GCC 13.3 and
glibc 2.39 until the final-head Ubuntu 24.04 hosted comparison passes. Matching
this local binary does not establish agreement with that target binary.

## Compiled operand forms that decide NaN bits

IEEE 754 fixes every non-NaN result of these expressions, but which NaN survives, and
its sign, follow the compiled instructions. The first live comparison of the source-order port
failed on 36 double cases (for example `random-f64-13` and `major-f64-3-7`). Disassembly of
`ELLIPSE<double>` in the oracle binary shows these forms. `kicad_math::ellipse` now follows
them, and the rerun has zero failures:

| Site | Source | Compiled (first operand wins two NaNs) |
| --- | --- | --- |
| `GetPointAtAngle` cos/sin | `std::cos( t )`, `std::sin( t )` | one `sincos` call (`glibc_trig::sincos`) |
| `GetPointAtAngle` ry | `ex * sinR + ey * cosR` | `ey * cosR + ex * sinR` (packed `addpd`) |
| `GetPointAtAngle` result | `Center + VECTOR2( rx, ry )` | `( rx, ry ) + Center` |
| `Rotation.Sin()` = 1.0 branch | `e * sinR` | `e` (no multiplication; a signalling NaN stays signalling) |
| `Rotation.Sin()` = -1.0 branch | `e * sinR` | `-e` (`xorpd`; a NaN's sign flips) |
| `GetAngleAtPoint` ly | `-dx * sinRot + dy * cosRot` | `dy * cosRot - sinRot * dx` |
| major-axis constructor | `MajorRadius * aRatio` | `aRatio * MajorRadius` |

The 1.0/-1.0 forms apply only to the normalized 90 and 270 degree branches of
`EDA_ANGLE::Sin`; a general glibc `sin` result equal to +/-1 is still multiplied. `cosR` is never
NaN (`EDA_ANGLE::Cos` folds non-finite angles to 0), so the multiplication order with it is not
observable. `Mirror` (`2 * aRef` compiled as `aRef + aRef`) and `GetSubtendedAngle` follow
the source order. For `ELLIPSE<int>` the only NaN sources (non-finite angles) reach
`KiROUND`'s overflow logging, which defers, so these forms are not observable there.

## Undefined domain (refused, separate controls)

The adapter and the port both refuse these before evaluation. They are the 234 requests in
`port/oracle/fixtures/ellipse-controls.jsonl`, checked on both programs by
`rust/kicad-ellipse-shim/compare_controls.py`, and never part of the public corpus:

- `ELLIPSE<int>( aCenter, aMajor, aRatio, ... )`: `NumericType( MajorRadius * aRatio )` with a
  NaN or a value whose truncation is outside `int` ([conv.fpint]); 152 controls.
- `ELLIPSE<int>::Mirror`: `2 * aRef.x - Center.x` (or y) overflowing `int`; 27 controls.
- `ELLIPSE<int>::GetPointAtAngle` (directly or through `GetArcStartPoint`/`GetArcEndPoint`):
  `Center + VECTOR2I( ... )` overflowing `int`; 8 controls with exactly known offsets.
- `ELLIPSE<double>::GetAngleAtPoint` with a NaN radius: `std::max( 1.0, NaN )` violates the
  `Cpp17LessThanComparable` precondition ([alg.min.max]), as for the accepted
  `arc-chord-params-nan-clamp` decision; 47 controls.
