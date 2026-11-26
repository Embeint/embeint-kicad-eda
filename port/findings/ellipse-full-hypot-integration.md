<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Ellipse after full hypot integration

The accepted full hypot wrapper supplies the general VECTOR2<double> norm used
by the double major-axis constructor. The 36 previously deferred public inputs
now reach the existing ellipse continuation. No ellipse arithmetic or public
corpus bytes change. In particular, the independently reviewed target GCC 13
ry NaN operand order is retained.

The owning shim now expects 2,139 compared results and 100 deferrals among the
same 2,239 requests. Every remaining deferral must name the math-util logging
owner; a vector-owner deferral fails the test. The existing 234 undefined-domain
controls are unchanged. The unit stays partial, with no new golden.

Before publication, the combined tree is compared with the genuine pinned C++
on the locked GCC 13/glibc 2.39 runtime, including the geometry-header caller,
all other registered units, refusal controls and portable builds. Exact reports
are retained in the manual integration evidence. The remaining logging path
belongs to its existing math-util owner.
