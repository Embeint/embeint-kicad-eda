<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Full hypot after accepted arc construction

Accepted arc PR #85 wires the already qualified finite hypot body. Hypot PR #88
qualifies and supplies the full glibc 2.39 wrapper, including infinities and NaNs.
Their overlapping norm call is resolved to the full wrapper. Every byte outside
the f64 norm2 method agrees between the two vector source files and their ancestor.
No hypot backend arithmetic or public corpus input is changed.

The util/box assertion is identical on both branches (14329 results, 1602 deferrals);
only its comment differed. Both historical owning-unit notes are retained.

The 20 non-finite tangent requests in arc construction now reach its existing
continuation. Its expected deferral count becomes zero. The generator's coverage
mirror models the full wrapper's finite/non-finite outcomes and stops requiring
the removed deferral branch. It still only labels coverage, never oracle answers.
Every case ID and corpus byte is preserved. The unit remains partial and no golden
is added. Ellipse's separate 36 hypot deferrals are checked when that source topic
is integrated on this accepted dependency.

Before publication, the joint tree must pass native exact comparison on the
locked GCC 13/glibc 2.39 runtime, controls, affected tests and portable builds.
The resulting reports are retained with the manual integration evidence.
