<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Circle partial review: assertion effects and coverage corrections

Addendum to circle-deferrals-and-adapter-domain.md. The initial independent
Sol 6.1 review of head 48e156721a stopped on a provider content flag; it is not
an approval or a completed review. Its reported evidence is retained as partial.

The reviewer independently reproduced the hosted C++ output hash with the
downloaded GCC 13.3 binary under glibc 2.39, the Rust output hash and all controls.
It confirmed an assertion call for corpus case tan_tan_pt-hand-2. A temporary
Rust continuation test returned success without the assertion host effect.
Source contains seven wxCHECK_MSG sites. This is an unresolved representation
gap behind genuine missing SEG callees, not a reason to swallow assertions or
invent a replacement result. A faithful host/assertion interface and genuine
observable evidence must precede readiness.

A reported 45,928-request tan-tan sweep exceeded the claimed 2^24 intermediate
bound. The reviewer did not establish an arithmetic failure from that fact, but
it invalidates the sufficiency argument. The adapter restrictions remain
harness limits; complete safety of accepted requests still needs an audit.

Two bounded parent corrections are made:

- Test-only Transcribed::seg_center now follows A + (B-A)/2 using accepted
  VECTOR2I division, including KiROUND half-away-from-zero behavior. The previous
  integer division truncated odd halves. Regression inputs cover both signs
  and reversed endpoints. This does not port SEG::Center or change production.
- The generator's equal-radius remainder search emitted zero cases. Given
  d <= 2*r, its trunc(d/2) cannot exceed r. Removing the no-output search leaves
  every public request unchanged. No negative-remainder coverage is claimed.

All ten scoped circle tests, the deterministic generator check (4,978 public
cases and 81 controls), and formatting pass after these bounded corrections.
No comparison, corpus, refusal or golden is changed to clear the unresolved
assertion or adapter-safety findings. The PR remains draft/partial without a
golden. A new independent review must finish on the corrected concrete head;
new hosted checks do not retroactively validate missing source behavior.

The fresh follow-up review of 6b36ef0440 was also stopped by the same provider
content flag before a final verdict. It independently reported the same seven
missing assertion effects and no additional discrepancy in the other compared
body groups, but that partial progress is not approval. The remaining stale
2^24 sufficiency claim in the adapter's source comment is now corrected too;
guard code and numerical behavior are unchanged. No third review retry is
started. A completed independent review and substantive findings resolution
remain prerequisites to readiness.

## Resolution of the completed review (head 48e156721a) on the next head

The completed Sol 6.1 review requested changes for an admitted overflow (P1),
the missing assertion effects (P2), the odd-half test helper (P3, already fixed
at 6b36ef0440) and the audit's evidence accounting. A follow-up UBSan sweep of
0aa3752c16 found a second admitted overflow. The changes:

- P1: the sufficient domain is removed. `guardTanTanPt` guards every integer
  operation of the pinned body and its SEG, CalcArcMid and RotatePoint callees,
  in evaluation order. Both review repros and an ApproxParallel SquaredLength
  overflow are controls (`rust_unported` at SEG::Square). A local diagnostic
  UBSan check found the guard's refusals equal to the genuine sources' undefined
  evaluations over the corpus, controls and fixed-seed families; all 107 public
  tan-tan cases stay accepted with unchanged replies.
- P2: each of the seven sites calls a typed host callee with the exact
  wxOnAssert arguments before `return *this`; production defers as `wxOnAssert`
  and the partial mutation is kept. See the updated main finding.
- Accounting: seven sites everywhere; the signed-zero centre cases now give a
  zero centre component each signed zero (twelve public inputs change, replies
  unchanged); the negative-remainder branch of `Intersect( CIRCLE )` is recorded
  as unreachable on defined evaluations (AUDIT.md), not as missing fixtures.
- The `VECTOR3::Normalize` omission is a separate finding,
  `vector3-normalize-assertion-effect.md`, for its own owner.

The unit remains partial and draft without a golden. A fresh independent review
of the new head is required; target evidence is the hosted run on that head.
