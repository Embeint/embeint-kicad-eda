<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Math util source audit

Owned unit: `libs.kimath.src.math.util.application.ui.p001`, all 182 lines of
`libs/kimath/src/math/util.cpp`, five extracted USRs. Reference pin:
`4cf939778df04d2b747bb2c4714fc5014a7565c7`. Accepted starting base:
`origin/rust` = `eb4cd4724ff6f5fa2119c86248c1a4beb4847451`, fetched and verified
before edits on 2026-11-24. Open claims were searched by exact title, `util.cpp`
and `RoundedHypotWide`; only unrelated PRs #50 and #58 targeted `rust`.
No open topic branch is included.

Read in full: pinned util.cpp, util.h and wide_int.h; contribution/port/MCP rules;
port-pr-review skill; native registry/protocol, source-pin audit and live CI gate.
The non-MSVC `KI_UINT128` is actual `unsigned __int128`, not an arbitrary-precision
integer or a floating approximation. The MSVC alias is `std::_Unsigned128` from
the installed STL; that implementation has not been executed here.

| Owned symbol | Implementation and checked observable | Remaining gap |
| --- | --- | --- |
| `RoundedHypotWide(uint64_t,uint64_t)` | Canonical `kicad_math::math_util::rounded_hypot_wide`; both squares, unsigned-128 modular sum, 64 descending candidate bits, exact floor-root test and unsigned-64 modular increment. Corpus includes full-u64 inputs. | Native evidence is the selected Linux branch only; MSVC STL branch uncertified. |
| `rescale<int>` | `rescale_i32`; signed product, sign XOR, signed denominator/2 truncation before adjustment, division toward zero, low-32-bit quotient. Amplification, negatives, halfways, odd divisors, extrema and narrowing retained. | Other compiler/platform conversion contracts uncertified. |
| `rescale<int64_t>` | `rescale_i64_int128`; exact checked signed-128 product/adjustment/division, low-64-bit signed result. | MSVC `_mul128`/`_div128`/SEH and no-int128 fallback bodies deliberately unported. |
| `kimathLogDebug` | Typed `Unported` with this unit as owner, transported through the native/shim boundary. | Actual wx level query, printf ABI/formatting and conditional warning side effects. No native log-hook execution claimed. |
| `kimathLogOverflow` | Typed `Unported` with this unit as owner; no asserted success. | Actual wx string conversion/formatting and build-dependent assertion effects. No native assertion execution claimed. |

## Integer-domain audit

Each u64 square fits u128. Their sum can overflow: use `wrapping_add`, preserving
C++ unsigned arithmetic for **every** u64 pair, including values outside the
header's stated magnitudes <= 2^63. The source comment `n <= 2^127` describes its
usual callers; it is not a wrapper refusal. For the wrapped n, every candidate
square fits u128 and the final floor f satisfies f^2 <= n < (f+1)^2 (the upper
bound is mathematical when f is u64::MAX). Thus subtraction is safe. When
`n - f^2 > f`, the final unsigned increment may wrap. For x = u64::MAX,
y = 2^32 - 1 returns u64::MAX; y = 2^32 returns 0. With both arguments u64::MAX,
the u128 sum wraps and the returned root is u64::MAX - 1. These values are source
semantics, not a claim about the mathematical hypotenuse outside the header domain.
No float square root, saturation or upper-domain restriction is introduced.

For i32 operands, product magnitude <= 2^62 and rounding magnitude <= 2^30;
native int64 intermediates cannot overflow. The shared Rust i128 calculation is
therefore identical to the int64 calculation before narrowing. For i64 operands,
product magnitude <= 2^126 and rounding magnitude <= 2^62, safely inside i128.
Division cannot reach signed MIN / -1 in either native intermediate width.
Denominator zero is refused before native division, including 0*0/0. It is the
only arithmetic undefined-input guard on the selected native branches.

Signed denominator/2 and final division both truncate toward zero. For a negative
quotient the body subtracts denominator/2; for a nonnegative quotient it adds it.
This preserves negative-denominator behavior and halfway rounding away from zero.
A zero product with a negative denominator also follows the exact XOR branch.
The header's `numerator <= denominator` prose is not enforced by the specialization;
no ratio, result-range or sign restriction is added. Results beyond int32/int64 are
narrowed to low signed bits by `as`, matching the selected Linux C++20 conversion
contract; native corpus cases exercise that conversion rather than rejecting it.
All arithmetic that could fail in shared signed intermediates uses checked methods;
the typed `IntermediateOverflow` branch is unreachable for the exposed widths.

## Other native profiles: not interchangeable

MSVC x64 uses signed multiply into two words, adds signed rounding words with carry,
then `_div128`. A quotient overflow can raise `EXCEPTION_INT_OVERFLOW`, call
`kimathLogDebug` and **return 0 after logging**. That is not the int128 branch's
low-bit narrowing result. `-aDenominator` in the rounding expression also needs
special care for INT64_MIN. Neither an SEH reproduction nor that profile's input
refusals have been implemented or executed; no MSVC equivalence is asserted.

The fallback takes signed `abs` (undefined for INT64_MIN), has signed-expression
overflow risks in its small-b/c path, and uses a manual word multiply/divide in
its other path. Its unsigned final multiplication and signed narrowing require
a separate profile audit and native tests. It is not exposed under an int128
name or silently routed through the present implementation.

## Host boundary and continuation contract

The portable crate has no I/O, state, wx types, global log level, assertion handler
or unsafe variadic ABI. `LogArgument` is only typed transport for a future host;
it does not implement printf parsing or claim all C vararg types are supported.
Both public logging entrypoints currently return `MathUtilError::Unported` with
the exact source symbol. The oracle adapter makes the same explicit deferral
without calling wx; those controls prove transport only, not logging parity.

A future host bridge must do these steps, preserving order and effects:

1. For debug, query `wxLog::IsLevelEnabled(wxLOG_Debug,
   wxString::FromAscii(wxLOG_COMPONENT))` on the host. If disabled, return only
   after that actual query, without formatting/emitting anything. If enabled,
   convert typed arguments to validated C ABI arguments and call the equivalent
   of `wxVLogWarning` with the original ASCII format and arguments. Despite its
   name, this routine emits a **warning** when the debug gate is enabled.
2. For overflow, construct the type name with `wxString(aTypeName)` and preserve
   `wxString::Format("\n\nOverflow converting value %f to %s.", value, typeName)`.
   Invoke `wxFAIL_MSG` under the actual assertion build/host policy, including
   handlers that continue or abort. Debug/release policy, locale, nonfinite input
   formatting, character conversion and embedded-NUL handling need native coverage.
3. A caller must propagate `Unported` until the host is available. If an actual
   host assertion returns, header `KiCheckedCast` and `KiROUND` must then execute
   their clamp/return continuations. If the future MSVC overflow logger returns,
   `rescale<int64_t>` must then return 0. Those callers/branches are not implemented
   by pretending that the present deferral succeeded.

No util.h helper body was co-ported: `UnsignedAbs`, `RoundedHypot`, `ct_sqrt`,
`isqrt`, `KiROUND`, `KiCheckedCast`, generic rescale, sign and equals remain with
`libs.kimath.include.math.application.ui.p001`. Its record/status/ownership are
unchanged. Existing vector/hash implementations are unchanged.

## Downstream handoff

PR #58 can wire its exact-deferred source calls to `rounded_hypot_wide`,
`rescale_i32` and `rescale_i64_int128` after parent integration. This PR does not
edit vector.rs or claim the header RoundedHypot helper. Header callers needing
that helper still need owner-approved narrow-path/unsigned-abs/root support;
calling the source port does not certify those caller continuations. The parent
must rerun newly enabled vector cases, preserving any remaining header-owned gaps.
In PR #58's general Resize path, the concrete source-owned deferral is its
`rescale_i64` helper. Adapt that helper to `rescale_i64_int128`, preserving the
vector's error conversion and subsequent sqrt/round/sign reconstruction. A newly
reached KiROUND call remains header-owned and may still defer; do not report it
as a successful complete Resize solely because rescale is now available.
PR #50 and other dependency integration remain parent work.
