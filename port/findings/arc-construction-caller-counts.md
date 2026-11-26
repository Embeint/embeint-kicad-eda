<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Arc construction caller test counts after finite norm wiring

PR #85 head `eaa34442d1dcf74ed373d2d48be6499f7bfbc1d9` failed
`layout-parity` in run 45735176879, job 139626572992. The failure was in
the full workspace's release tests, before the layout parity comparison:
`kicad-util-box-shim::tests::every_public_case_returns_or_explicitly_defers`.

The finite VECTOR2<double> norm wiring resolves five util/box deferrals.
The test observed 14,329 results and 1,602 deferrals, but its old assertion
still expected 14,324 and 1,607. The arc source audit already records those
new caller counts and a successful local live comparison. This is a stale
assertion after dependency wiring, not evidence of a layout output mismatch.

The assertion now requires the new exact counts. It still rejects every
non-result reply unless it is an explicit Unported error; no cases, guards,
comparisons or golden values are removed or relaxed. The scoped arc workflow
now runs the vector and util/box caller tests as well as their existing live
comparisons, so this regression is checked before the full workspace workflow.

The caller test correction does not change runtime behavior. Hosted checks on
the final corrected head remain required, including the original failed
workflow. The existing non-reference local oracle does not substitute for
target-host qualification.

After correction, eight release tests pass across arc-construction, vector and
util/box shims. Fmt and util/box clippy with warnings denied pass. A fresh replay
of all 15,931 util/box cases against the existing local genuine oracle
(`f49977363148d66e74e981881cd52dc2701fcb3bc2683034702d9ac5955944b7`)
and a rebuilt Windows Rust shim gives 14,329 exact results, 1,602 explicit
deferrals and zero failures. The oracle remains GCC 15/glibc 2.43 with the CI
SSE2 tunables; these local checks do not certify the target runtime.
