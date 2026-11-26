<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Ellipse after full hypot integration

The accepted full hypot wrapper supplies the general VECTOR2<double> norm used
by the double major-axis constructor. The 36 earlier norm deferrals now reach
existing ellipse continuations. No ellipse arithmetic changes, including the
independently reviewed target GCC 13 ry NaN operand order.

A diagnostic replay of the original 2,239 public requests on locked GCC 13 /
glibc 2.39 gives 2,124 defined exact comparisons, 100 logging deferrals and 15
identical guarded refusals. All nondeferred C++ and Rust replies match exactly,
including the state and successful steps before each refusal. The old generator
explicitly assumed the general norm deferred and returned false from its
NaN-radius classifier; that assumption became stale after dependency wiring.

Each of the 15 reaches the existing GetAngleAtPoint NaN-radius std::max guard
at step index 3, under the unit's existing reviewed undefined-domain policy
(port/findings/ellipse-compiled-nan-sites-and-undefined-domain.md). The generator
now classifies the full norm's NaN outcome without using a host transcendental
function or producing an expected ellipse result.

The requests move to the controls file with their original IDs and inputs.
Nothing is discarded: the combined public/control request map remains exactly
2,473 programs. There are now 2,224 public cases (2,124 compared, 100 logging
deferrals) and 249 controls. The public test accepts only the math-util logging
owner for a deferral. The existing 234 controls are retained. No golden changes.

Moved request IDs (every refusal is at step 3):

- `major-f64-13-7`.
- `major-f64-13-8`.
- `major-f64-14-7`.
- `major-f64-14-8`.
- `major-f64-15-0`.
- `major-f64-15-1`.
- `major-f64-15-2`.
- `major-f64-15-3`.
- `major-f64-15-4`.
- `major-f64-15-5`.
- `major-f64-15-6`.
- `major-f64-15-7`.
- `major-f64-15-8`.
- `major-f64-16-7`.
- `major-f64-16-8`.

Before publication, the complete joint tree is replayed against genuine pinned
C++ with all registered units, affected Rust tests, refusal controls and portable
builds. Exact reports and the original-cohort diagnostic are retained with the
manual integration evidence. The unit remains partial for its logging dependency
and separately reviewed golden work.

The direct double major-axis constructor test now verifies radii (5.0, 2.5)
for the (3, 4) vector and ratio 0.5. Its earlier expectation of a vector-owner
Unported error was stale after dependency integration. Runtime ellipse code
before the test module is byte-identical to the reviewed target-order repair.
