# Collector target refusal boundary

Independent re-review of PR #44 at `c7f31599b52be019f2d90091e4ff64254226fd4b`
found that a malformed collector `target_bits` became `result.exception` in C++
and `error.kind = invalid_input` in Rust. `std::invalid_argument` inherits from
`std::logic_error`, so validating the target inside the collector's exception
handler misclassified the refusal as a legitimate library result.

The C++ adapter now validates the target after candidates and before entering
that handler. Candidate validation order and actual library exception results
are preserved. No pinned library, portable implementation, successful case,
golden or prior immutable evidence changes.

`port/oracle/test_resistor_collector_inputs.py` checks seven complete refusal
envelopes and a valid empty collector's `Empty solution collector` result. It is
registered as a canonical oracle CTest and requires an actual executable. The
existing paired native request suite now includes those same boundaries.
The unchanged Rust process passes the new focused checks; the retained old C++
process provides the failing negative control. Canonical C++ build/CTest and
the unchanged 1,050-case live check must pass in hosted CI before merge.

The independent review otherwise verified native debug/release and executed wasm
against all 1,050 complete results, including a failing poisoned-golden control.
Its real-source GCC 13 Release fixture agreed with the recorded GCC 15 Release
results. GCC without optimization and Clang 18 Release differed on 234 NaN
payload cases: this evidence qualifies the measured GCC Release reference,
rather than compiler-independent NaN semantics. Corrected PR #39 ownership,
dependencies and L0 classification remain separate merge requirements.
