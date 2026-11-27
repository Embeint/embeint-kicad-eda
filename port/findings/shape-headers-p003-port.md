# seg.h and shape header port (p003): caller rewiring, domains and evidence

Unit: `libs.kimath.include.geometry.application.ui.p003` (seg.h, shape.h, shape_arc.h,
shape_circle.h, shape_compound.h, shape_ellipse.h at KiCad 4cf939778df0).

## Caller deferrals removed

The accepted SEG (`libs.kimath.src.geometry.seg.application.ui.p001`) and CIRCLE
(`libs.kimath.src.geometry.circle.application.ui.p001`) ports called this unit through typed
deferrals. They now call the ported bodies:

| Caller | Deferral removed | Corpus effect (public corpora unchanged) |
| --- | --- | --- |
| seg.cpp `ApproxCollinear`, `ApproxParallel`, `ApproxPerpendicular` | `SEG::Square`, `SEG::SquaredLength` | 858 deferred cases -> 0; all 9503 public cases now compare. 197 candidates the generator had dropped as "undefined only after a deferral" are now controls (1690 -> 1887), refused identically. |
| circle.cpp `ConstructFromTanTanPt` | `SEG::Square` (via `ApproxParallel`), `SEG::Center`, `SEG::IntersectLines` | 362 deferred -> 342: the 107 `SEG::Square` cases now give 20 more compared results and stop at `CalcArcMid` (70, trigo.cpp) or the `wxOnAssert` host effect (17). Two tan-tan controls now defer at `CalcArcMid`; `approx-parallel-length` is refused identically by both programs. |

Neither caller's unit record is edited here (they are owned elsewhere); the generators, control
fixtures and shim tests that recorded the old deferral counts were updated in this unit's PR.

## Verified domains for deferred out-of-line calls

The public corpus must hold an oracle result for every case. Cases that defer in Rust still run
out-of-line C++ bodies this unit does not port (shape_arc.cpp, shape_compound.cpp,
shape_collisions.cpp), whose undefined domain the adapter does not replay. Instead of claiming a
guard it cannot prove, the adapter (and the Rust shim, identically) accepts those calls only
inside a stated domain and refuses the rest as "outside the verified domain":

- three-point `SHAPE_ARC`: coordinates and width within 2^20, distinct points, mid-point angle
  sine >= 1/8 (so `update_values`' centre and quadrant points stay below 2^25);
- `SHAPE::Collide( SHAPE* )`: a circle or a compound of circles against a circle, all within
  2^20, clearance in [0, 2^20];
- out-of-line `SHAPE_COMPOUND` members: compounds of circles within 2^20, arguments within 2^20;
- `SHAPE_ARC::BBox`: the default arc with width and clearance within 2^20.

This restricts only deferred evidence; every owned inline body is guarded exactly (no domain).
Those owners must replace the domain with exact guards when they port the bodies.

## Undefined behaviour found

- `SHAPE_CIRCLE::Collide( SEG )` with `aActual`: `(int) sqrt( dist_sq )` is undefined when
  `minDist` is `INT_MIN` and `dist_sq` (< 2^62) rounds to 2^62 as a `double`; e.g. a radius-0
  circle at the origin, clearance `INT_MIN`, point (2147483524, 729778): `dist_sq = 2^62 - 44`.
  The collision itself (`true`) is defined. Recorded as controls.
- `SHAPE_ARC::IsCCW`: the `int64_t` cross product overflows for spans near 2^32 (reachable only
  through shape_arc.cpp constructors; in-crate test).
- `SEG::CanonicalCoefs` cannot overflow for 32-bit points (`qC = A.x * B.y - A.y * B.x`), but
  `SEG::Collinear`'s line values can.

## Local diagnostic evidence (not the reference)

GCC 15.2 / glibc 2.43 oracle (WSL, `~/kicad-port-p003`, ctest 16/16), native Rust (Windows):

| Corpus | Compared | Deferred | Mismatches | Controls |
| --- | --- | --- | --- | --- |
| p003 | 3880 | 221 | 0 | 446/446 identical |
| seg | 9503 | 0 | 0 | 1887/1887 identical |
| circle | 4636 | 342 | 0 | 78 identical + 6 Rust-deferred before the oracle's refusal |

All 4547 p003 requests replay identically in import-free wasm. The hosted workflows
(`rust-shape-headers-live`, `rust-seg-live`, `rust-circle-live`, GCC 13.3 / glibc 2.39) are the
reference evidence.
