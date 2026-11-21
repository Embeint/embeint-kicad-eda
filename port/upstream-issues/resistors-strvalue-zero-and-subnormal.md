<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Engineering notation can produce NaN text for finite tiny inputs

State: reference behaviour confirmed locally; desired formatting is unconfirmed.
This is a fork-local record only. The port keeps the reference output and does
not exempt any comparison.

At pin `4cf939778df04d2b747bb2c4714fc5014a7565c7`,
`pcb_calculator/resistor_substitution_utils.cpp:230` computes its rounding digit
as `pow(10, floor(log10(value)) - 2)`. For zero, and for sufficiently small
positive subnormals, that digit is zero. Subsequent division/multiplication
produces NaN, which is formatted as `-nanL` by the pinned Linux reference.

Reproduction: operation `{"op":"format","values_bits":[0,1]}` returns
`["-nanL","-nanL"]`; the second input is the smallest positive IEEE754 value.
These inputs are included in `format-boundaries-0`. No out-of-range vector access
or integer conversion is involved. Negative finite inputs separately produce
`nanL`, following the sign of the reference logarithm's NaN. Positive infinity
also produces `-nanL`. Those observations remain part of exact formatting parity.

Observed with GCC 15.2.0/glibc 2.43 and actual pinned fmt, strict FP flags enabled.
Initial real oracle executable SHA-256:
`77b34831fe8a6e2bdd555ccc079d53d3fc9cdfa8251cbc890411eb54a6353783`.
The full prepared corpus compares these strings without normalization. Final
scope and immutable baseline evidence are separate integration dependencies.
