<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# ELLIPSE: target compiler order of GetPointAtAngle's ry

Unit: `libs.kimath.src.geometry.ellipse.application.ui.p001`
(`libs/kimath/src/geometry/ellipse.cpp` at pin `4cf939778df04d2b747bb2c4714fc5014a7565c7`).
Addendum to `ellipse-compiled-nan-sites-and-undefined-domain.md`, which recorded a local
GCC 15.2 build. Not a reference defect.

Reference: the oracle built by hosted CI on Ubuntu 24.04 (GCC 13.3, glibc 2.39) for PR
head `5c42b69677`, run 45732444817, job 139617508389, artifact
`rust-port-difftest-evidence`, binary sha256
`9b4dce98ce61384dd70ab2d2cf9f21afe11987a00330a8b26ce97cb2659e57b5`. This is the target
binary; the local GCC 15 binary is not the reference.

## Observation

That run processed all 2239 public cases: 2092 compared, 136 deferred, 11 failed
(`radii-f64-rot7-defaults`, `radii-f64-rot7-both`, `radii-f64-rot11-start`,
`radii-f64-rot11-both`, `random-f64-34`, `-258`, `-307`, `-363`, `-382`, `-418`, `-507`).
Each difference is the second coordinate of a `GetPointAtAngle` result (directly or as an
arc end point) when both products of `ry` are NaN, and only the NaN sign or payload
differs. No finite output differs. Examples: 270 degree rotation with an infinite angle
gives `7ff8000000000000` (the port gave `fff8000000000000`); `random-f64-34` gives
`fff8000000000000` (port `7ffc000000000abc`); `random-f64-382` gives
`fff8000000000123` (port `fff8000000000000`). The dedicated ellipse live workflow on the same
head (run 45732444601, job 139617502795, `GLIBC_TUNABLES` set) failed its public-case
comparison at the same first case, `radii-f64-rot7-defaults`.

## Target disassembly

`objdump -d` of `ELLIPSE<double>::GetPointAtAngle(EDA_ANGLE) const` (0x33b380) in that
binary ends with:

```
33b6f8: mulpd  %xmm3,%xmm5      # xmm5 = cosR * (ex, ey)
33b6fc: shufpd $0x1,%xmm1,%xmm1 # xmm1 = sinR * (ey, ex)
33b704: movapd %xmm5,%xmm0
33b708: subpd  %xmm1,%xmm0      # rx = ex*cosR - ey*sinR
33b70c: addpd  %xmm5,%xmm1      # ry = ex*sinR + ey*cosR
33b710: movsd  %xmm0,%xmm1
33b718: addpd  %xmm0,%xmm1      # (rx, ry) + Center
```

SSE2 returns the first source operand's NaN when both are NaN, so the target keeps the
source order `ex * sinR + ey * cosR` for `ry`. The GCC 15 binary had compiled it as
`ey * cosR + ex * sinR`. The 1.0 and -1.0 `Sin()` branches (copy at 0x33b698, `xorpd`
negation at 0x33b7a7), `sincos`, `rx`, `(rx, ry) + Center` and the major-axis constructor's
`aRatio * MajorRadius` (`mulsd %xmm1,%xmm3` at 0x33b13a) are the same as in the earlier
finding. The hosted comparison of every other site agreed.

## Change

`kicad_math::ellipse::Ellipse::point_at_angle` now computes
`ry = fp::add(sin_r.times(ex), fp::mul(cos_r, ey))`. The unit test
`point_at_angle_ry_keeps_the_sine_term_nan` covers the three shapes above and fails with
the previous order. No case, tolerance or exclusion changes.

## Qualification state

Replaying the hosted report's 2239 oracle replies (their concatenation hashes to the
report's `oracle_output_sha256`) against the corrected Rust shim gives 2103 compared, 136
deferred and 0 failures. That replay reuses the target binary's recorded output; it is
not a fresh oracle run. The downloaded binary run locally still uses the local glibc 2.43
libm, not 2.39. Final qualification is the hosted rust-port workflow
(`.github/workflows/rust-port.yml`, which builds the oracle fresh) and the ellipse live
workflow (`.github/workflows/rust-ellipse-live.yml`) on the corrected head.
