<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# E-series decade scaling overflows before normalization

State: reference behaviour confirmed locally; whether the fork should change it
is unconfirmed. This is a fork-local record, not an upstream submission or an
exact-comparison exemption. The port preserves the behaviour.

At pin `4cf939778df04d2b747bb2c4714fc5014a7565c7`,
`pcb_calculator/eseries.cpp:96` evaluates
`decadeMultiplier * seriesValue / decadeBase` in that order. For E1 and exponent
307, all inputs and the mathematical normalized value (1e307) are finite, but
the intermediate multiplication by 100 overflows. The actual returned value is
positive infinity, IEEE754 bits `9218868437227405312`.

Reproduction: run the registered resistor shim operation
`{"op":"series","series":0,"decades":[307]}` through the real `resistor-reference`
archive. The committed corpus's `series-0` case includes this exponent and every
other representable decade; the same condition occurs for larger E-series.
The portable implementation retains multiplication before division. It does
not reorder the expression to obtain a finite answer.

Observed with GCC 15.2.0/glibc 2.43, both fixed FP flags enabled. Initial real
oracle executable SHA-256:
`77b34831fe8a6e2bdd555ccc079d53d3fc9cdfa8251cbc890411eb54a6353783`.
All 194 prepared cases agreed locally after the portable math compatibility
change. Final baseline evidence and extractor scope are separate integration
dependencies; no green unit status is asserted by this finding.
