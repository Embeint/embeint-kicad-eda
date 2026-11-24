# Vector arithmetic with one distinct quieted NaN value

This is a measured implementation rationale, not a claim that every floating site is
qualified. Owning unit: `libs.kimath.include.math.application.ui.p002`. Source: pinned
`vector2d.h` / `vector3.h` at `4cf939778df04d2b747bb2c4714fc5014a7565c7`.

The real native comparison on PR58 head `f98485d7c1b28896e59a1bc8ed494e801b29bb36`
([run](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45387261468), embedded
merge revision `50dfd1ee6eec8e94fb819976cd63aba81a9db290`) initially deferred all arithmetic
NaN results. The unchanged report in
`rust/kicad-vector-shim/validation/2026-11-24-f98485d7/` contains the native outcomes for
all 687 such cases. This analysis joined those case identities to the same revision's input
blob and inspected output NaN bits; it was not part of a runtime implementation.

- 535 cases had exactly one distinct quieted NaN bit value among their input components.
  Every observed output NaN had that value, including its sign and payload.
- 68 cases had no NaN input. Every observed output NaN was x86 negative indefinite,
  `fff8000000000000`.
- 84 cases had distinct quieted input NaN values. These do not establish a fixed operand
  preference. GCC operand selection varies with the compiled site; these remain unqualified.

The updated implementation evaluates each real arithmetic operation in source order. A
non-NaN result is returned unchanged. For a NaN result, no NaN operands means the bound
x86 invalid-arithmetic NaN; one NaN operand is quieted and preserved; two operands with the
same quieted bits have one observable result; two **different** quieted NaN values return
precise `Unported` for the vector call. A generated indefinite NaN can itself be one operand
at the next step, so that step still defers if it meets a different NaN. This does not choose
the left operand by default and does not reuse fp.rs's fixed left-NaN policy.

The one-operand sqrt path preserves a unique NaN after quieting. A negative non-NaN sqrt
operand retains an owned qualification deferral. General std::hypot remains deferred: it
has additional NaN/infinity/axis behavior and libm/runtime dependencies that this arithmetic
rule does not cover. Normalize's unordered norm check returns the unchanged original vector,
including signalling bits; it does not expose the temporary norm's NaN payload.

The expanded 9,533-case corpus adds scalar-first/scalar-last arithmetic, division, both live
scalar aliases, and 3D dot/cross/scalar mutations across raw-bit pairs. Quiet/signalling,
positive/negative payloads, zeros, subnormals, finite extremes and infinities remain inputs.
The new library code reads none of this report, case IDs or recorded answers. Every result
must pass a **new** exact hosted comparison; a historical pass does not certify the change.
There is no golden or tolerance. Mixed distinct NaNs remain real native observations and
explicit Rust deferrals rather than rejected inputs or guessed successes.
