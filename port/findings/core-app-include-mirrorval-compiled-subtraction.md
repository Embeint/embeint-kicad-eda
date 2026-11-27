# MIRRORVAL<double> is compiled as a subtraction

Unit: `include.core.application.pure.p001`. Location: `include/core/mirror.h`, `MIRRORVAL`
(`return -( aPoint - aMirrorRef ) + aMirrorRef;`), instantiated for `double` directly and
through `MIRROR( VECTOR2D&, ..., FLIP_DIRECTION )`.

## Trigger

A NaN operand. For every non-NaN pair, `r + -d` and `r - d` are the same IEEE operation
(signed zeros included), so GCC without fast-math may compile `-( a - r ) + r` as
`r - ( a - r )`. The two forms differ only in which NaN survives and in its sign.

## Evidence

The first port evaluated the source form (`fp::sub`, a sign-bit negation, `fp::add` with the
negated difference as the left operand). Against the pinned oracle (local GCC 15.2.0, glibc
2.43, `-O3 -fno-fast-math -ffp-contract=off`, x86-64 SSE2) 419 of 3,599 public cases
differed, all `double` `mirror_val`, `mirror` and `VECTOR2D` `mirror_vector` cases with a NaN
operand or an `inf - inf` difference:

- `aMirrorRef` NaN: the oracle returns `aMirrorRef` quieted, sign kept (for example point
  `0`, ref `fff4000000000abc` gives `fffc000000000abc`; the source form gives
  `7ffc000000000abc`).
- `aPoint` NaN, `aMirrorRef` not: the oracle returns `aPoint` quieted with its sign
  unchanged; the source form flips it.
- `inf - inf`: the oracle returns the x86 indefinite NaN `fff8000000000000` unflipped.

Every difference is the behaviour of `aMirrorRef - ( aPoint - aMirrorRef )` with x86's NaN
rules (first NaN source operand, quieted; indefinite for an invalid operation). The port now
evaluates that form (`kicad_math::mirror`), and all 3,599 cases are equal.

## Confirmation state

Confirmed on the local GCC 15 build. Like the ellipse operand-order findings, this is a
property of the compiled reference, not of the C++ source: another compiler or version may
choose differently. The hosted GCC 13 run (`rust-core-app-include-live.yml`) must confirm it;
a difference there is a failure to record, not a comparison to relax. Not a defect in KiCad.
