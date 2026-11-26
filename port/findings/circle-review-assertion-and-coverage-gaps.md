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
