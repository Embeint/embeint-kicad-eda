# Circle source port: deferral owners, wxCHECK effects and the tan-tan adapter guard

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

## wxCHECK_MSG assertion effects

`ConstructFromTanTanPt` has seven `wxCHECK_MSG( cond, *this, msg )` checks (circle.cpp lines
102, 115, 142, 157, 162, 172 and 177). With wx 3.2 a failed check expands to
`wxFAIL_COND_MSG( #cond, msg )`: when `wxTheAssertHandler` is set it calls
`wxOnAssert( __FILE__, __LINE__, __FUNCTION__, "\"cond\"", msg )` (the condition is
stringified twice) and `wxTrap()` if the handler requests it, then `return *this`. `-DNDEBUG`
does not remove it (`wxDEBUG_LEVEL` stays 1). The circle keeps the members assigned before the
check; for the parallel branch that is the new radius with `Center = aP`. The hosted oracle's
default handler printed 39 such assertions for the 107 corpus requests (17 at line 102, 22 at
line 142), e.g. `circle.cpp(102): assert ""possibleCenters.size() > 0"" failed in
ConstructFromTanTanPt(): No solutions exist!`. The oracle reply records the returned circle;
it does not record the assertion.

The Rust body calls `Callees::wx_check_failed( &WxCheck )` at each site with the exact line,
function, condition and message, then returns `Ok(self)` only if that host effect returns.
The production `Pinned` has no wx host and returns `Unported { upstream: "wxOnAssert",
unit: <this unit> }`, because the expansion is this unit's own body and no unit ports the wx
assertion handler. `&mut self` keeps the partial mutation on either path, as C++ does. These
deferrals stay behind the earlier `SEG::Square` deferral, so no public case reaches them yet.
In-crate tests check `tan_tan_pt-hand-2` (center [-40, 60], radius 25 after the line 102
check, as the hosted oracle returned) with a recording host and with the deferral, and the
line 115 check leaving the circle unchanged. They are continuation tests, not oracle evidence.

The accepted `VECTOR3::Normalize` port has the same omission in its own unit; see
`vector3-normalize-assertion-effect.md`. It is not a precedent for this unit.

## Adapter guard for ConstructFromTanTanPt

The adapter first accepted a sufficient domain (lines non-degenerate, coordinates within 2^16,
`ApproxParallel` true or the lines' sine at least 1/8). That set was not arithmetic-safe: the
independent review traced lines `(0,0)-(65536,1)` and `(0,0)-(1,65536)` with point
`(32766,-32768)` to `actTanA = (INT_MIN, -32768)` and then `INT_MIN - 1` in
`PerpendicularSeg`; a follow-up UBSan sweep of the genuine sources found lines
`(-64561,-62774)-(61438,63223)`, `(42568,-43794)-(-24787,13026)` and point `(-6633,29240)`.
The adapter reported success for both.

The domain is replaced by `guardTanTanPt`, which walks the pinned body operation by operation
in evaluation order, following its control flow: `ApproxParallel` (SquaredLength's `A - B`,
the canonical line and `sgn( det ) * dsq`, `std::abs( d1_sq - d2_sq )`), `LineProject`,
`Center`, the lambdas' differences, `ParallelSeg`, `IntersectLine` (the existing guard),
`IntersectLines` (int64 cross products, the collinear midpoint sum, the rescaled sum),
`CalcArcMid` with its `RotatePoint` (both differences, the 90/180/270 degree negations, the
sums with the centre), `LineDistance`, `PerpendicularSeg` and the wxCHECK returns. Each value a
later operation or branch depends on is taken from the genuine function after its own guard.
There is no remaining domain restriction: degenerate lines and full-range coordinates are
accepted when defined.

Diagnostic exactness check (local WSL, GCC 15.2 / glibc 2.43; not target evidence): the
genuine circle, seg, trigo, eda_angle and util sources built with
`-fsanitize=undefined,float-cast-overflow -fno-sanitize-recover=all`, the adapter guard
included verbatim, and each request's genuine call run in a forked child. Over the 107 corpus
requests, the tan-tan controls and fixed-seed families (full-range random lines with parallel,
degenerate, shared-point and limit-valued variants, and the review's near-parallel homothety
family at 2^16 and 2^30 scale), the guard refused exactly the requests whose child died, with
no accepted undefined and no refused defined request. All 107 public cases are accepted and
their oracle replies are unchanged.

The public tan-tan selection keeps its earlier fixed-seed choice; the generator no longer
presents it as an adapter domain. The two review repros and an `ApproxParallel` SquaredLength
overflow are separate controls. Rust defers at `SEG::Square` before each of them, so they are
checked as `rust_unported` for that owner and must become ordinary matching controls once SEG
and its callees are wired.

## Undefined after a Rust deferral

Four `Intersect( const CIRCLE& )` controls overflow the `int` solution sum after `RotatePoint`.
The oracle refuses them; Rust defers to the RotatePoint owner before reaching the sum. They are
checked as such (`rust_unported`) and must become ordinary matching controls once RotatePoint is
wired. The three tan-tan controls above are classified the same way for `SEG::Square`.

## Toolchains

The local comparison used GCC 15.2 / glibc 2.43 and is diagnostic. The owned double sites
(`aP - Center`, `+ Center`) have a single possible NaN operand, so their NaN result does not
depend on operand order; two-NaN sites inside the inlined `Resize` defer through the vector
unit. The hosted GCC 13 / glibc 2.39 / wx 3.2.4 run of `rust-circle-live` is the reference.
