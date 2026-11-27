<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Addendum: doScan INT branch after the source port

Updates `port/upstream-issues/sexpr-doscan-int-writes-double.md` (not edited, per the record
rule). Confirmation state unchanged: a source-level defect, not an observed failure.

`SEXPR_LIST::doScan` is now ported (unit `libs.sexpr.application.pure.p001`,
`kicad_sexpr::sexpr_cpp`). Its `INT` branch calls `GetInteger()` first: a non-integer child
throws `INVALID_TYPE_EXCEPTION`, which the loop catches to return the index (compared); an
integer child returns `SexprError::Undefined` instead of the eight-byte store, and the Rust shim
refuses the request exactly as the oracle's scan guard does. The header unit's 44 undefined
scan controls and the source unit's own scan controls are therefore no longer Rust deferrals
but identical refusals on both programs (`rust/kicad-sexpr-shim/compare_controls.py`).

This fork-local record does not change the reference implementation or either unit's scope and
is not an upstream submission.
