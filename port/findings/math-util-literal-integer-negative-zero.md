<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Preserve integer negative zero at the math-util input boundary

Independent review at `63ab599c6bb4e727ea567d7ffdbef34296e81f97` found that
`hypot_wide` rejected JSON integer `-0` in either coordinate. The shared Rust
request parser preserves that integer token, for which `Value::as_u64` alone
returns no value. The C++ adapter accepts nonnegative signed integers as well as
unsigned integers. An isolated probe of its actual guard confirmed acceptance
of `-0` and refusal of `-1` and `-0.0`; that probe did not execute full util.cpp.

The Rust shim now falls back to a checked signed-to-unsigned integer conversion.
It accepts `(x:-0,y:4)`, `(x:3,y:-0)` and both integer-zero coordinates, while
still rejecting floating `-0.0` and negative nonzero integers. The arithmetic
implementation and 10,656-case corpus remain unchanged.

Both Rust unit tests and the process controls pass raw JSON strings directly,
so Python integer serialization cannot normalize away the triggering spelling.
The process controls also run as the native oracle's CTest. Local debug/release
validation passed all three raw-token successes and four refusal cases; hosted
native results for the follow-up remain pending. The unit remains partial for
the documented wx and alternative-platform gaps; no golden was produced.
