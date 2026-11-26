# Circle source port: deferral owners, wxCHECK returns and the tan-tan adapter domain

Unit: `libs.kimath.src.geometry.circle.application.ui.p001` (PR #95).

## Deferral owners

- `CIRCLE::ConstructFromTanTanPt` cannot produce a Rust result yet: its first call,
  `SEG::ApproxParallel`, reaches the accepted SEG port's `SEG::Square` deferral (owner
  `libs.kimath.include.geometry.application.ui.p003`). The rest of the body is implemented and
  calls `SEG::Center`, `SEG::IntersectLines` (same header owner) and `CalcArcMid`
  (`libs.kimath.src.trigo.application.ui.p001`), each a typed deferral.
- `CIRCLE::Intersect( const CIRCLE& )` defers at `RotatePoint( VECTOR2I&, const EDA_ANGLE& )`
  (trigo.h, `libs.kimath.include.application.ui.p001`, the owner the accepted BOX2 port already
  names for this call) whenever intersection points exist. Empty results compare exactly.
- Wiring either owner should rerun this unit's corpus: 107 tan-tan and 193 intersect cases are
  deferred with retained oracle replies.

## wxCHECK_MSG returns

`ConstructFromTanTanPt` has five `wxCHECK_MSG( ..., *this, ... )` checks. In the oracle process
(no wxApp) a failed check prints the assertion to stderr and returns, so the reply is the circle
as mutated up to that point (for the parallel branch, the new radius with `Center = aP`). Of the
107 corpus requests, 39 printed such an assertion locally. The Rust body returns `Ok(self)` at
the same points, as the accepted `VECTOR3::Normalize` port does; the host assertion handler is not
modelled.

## Adapter domain for ConstructFromTanTanPt

Guarding every operation of `ConstructFromTanTanPt` (including the trigo and SEG callees) in the
adapter would re-implement the function. The adapter instead accepts a sufficient domain: lines
non-degenerate, coordinates within 2^16, and either the pinned `ApproxParallel` true or an angle
whose sine is at least 1/8. Requests outside it are refused as a harness limit. No Rust result
depends on this yet, since every tan-tan case defers before any arithmetic.

## Undefined after a Rust deferral

Four `Intersect( const CIRCLE& )` controls overflow the `int` solution sum after `RotatePoint`.
The oracle refuses them; Rust defers to the RotatePoint owner before reaching the sum. They are
checked as such (`rust_unported`) and must become ordinary matching controls once RotatePoint is
wired.

## Toolchains

The local comparison used GCC 15.2 / glibc 2.43 and is diagnostic. The owned double sites
(`aP - Center`, `+ Center`) have a single possible NaN operand, so their NaN result does not
depend on operand order; two-NaN sites inside the inlined `Resize` defer through the vector
unit. The hosted GCC 13 / glibc 2.39 / wx 3.2.4 run of `rust-circle-live` is the reference.
