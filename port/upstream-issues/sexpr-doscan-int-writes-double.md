<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# SEXPR_LIST::doScan writes a double through an int32_t* scan target

Reference: KiCad `4cf939778df04d2b747bb2c4714fc5014a7565c7`, `libs/sexpr/sexpr.cpp:443-446`
(`SEXPR_LIST::doScan`) and `libs/sexpr/include/sexpr/sexpr.h:159-160`
(`SEXPR_SCAN_ARG( int32_t* )`).

Trigger: `SEXPR_LIST::Scan` with an `int32_t*` argument (type `INT`) whose child is an integer
atom. The constructor stores the pointer in the union member `u.int_value`; `doScan`'s `INT`
branch then executes `*arg.u.dbl_value = child->GetInteger();`, reading the inactive `double*`
union member and storing an eight-byte `double` into the four-byte `int32_t` object. That is an
out-of-bounds store (and a strict-aliasing violation), so the reference result is undefined.
When the child is not an integer, `GetInteger()` throws `INVALID_TYPE_EXCEPTION` first, which
`doScan` catches to return the argument's index: that path is defined and is compared.

Reproduction: `SEXPR_LIST list; list << int32_t( 5 ); int32_t value = 0; list.Scan( &value );`
(`port/oracle/fixtures/sexpr-controls.jsonl`, `control-scan-int-writes-double` and the
generated `control-scan-*` requests that reach an `INT` argument on an integer child).

Evidence and confirmation state: a source-level defect, not an observed failure; no undefined
execution is used as parity evidence. The oracle adapter's scan guard refuses each such
request before calling `Scan` (`undefined sexpr operation`). The Rust port of `doScan`
(unit `libs.sexpr.application.pure.p001`) returns `SexprError::Undefined` on the same branch;
until that port lands the Rust header port defers every scan to it. No caller in the pinned
tree outside `libs/sexpr` uses `Scan` (or `AddChildren`), so production code does not reach
it.

This fork-local record does not change the reference implementation or the port's unit scope
and is not an upstream submission.
