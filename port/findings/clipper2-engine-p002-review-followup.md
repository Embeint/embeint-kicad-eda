# Clipper2 engine/offset independent review follow-up

This addendum supersedes the callback and guard claims in
`clipper2-engine-p002-port.md`. Sol 6.1 reviewed PR #96 at
`6234371e6f9f2eb9ef7ba743eb872287ded1e4b7` against accepted Rust
`70888b572202033a6522f2f271b10cc908f74dfe` and requested changes.

## Unresolved source-contract omission

The callback types cannot report a typed equivalent of a throwing C++ callback.
`ClipperOffset::execute_internal` moves groups out before calling them; caught
native unwinding bypasses restoration. Review reproduced C++ retaining one group
and producing one recovery path, versus Rust retaining zero groups and producing
zero paths. The Z callback proxy similarly removes its callback and loses it on
unwinding: C++ invoked it twice across recovery, Rust once.

These observable differences contradict the earlier finding's claim that nobody
can observe the callback removal. They require an additive fallible callback path
and retention/lifecycle checks. Neither passing corpus results nor partial status
resolves them. This PR remains draft, partial, without a golden, awaiting the
requested implementation route and a fresh independent review of the corrections.

## Bounded harness corrections

- Pinned `OffsetOpenJoined` accesses `norms[0]` on an empty path;
  `OffsetOpenPath` accesses path/normal elements for the start cap. The oracle now
  refuses empty non-Polygon paths during significant offsets with the same
  `undefined behaviour: offset` envelope as Rust. Insignificant offsets still copy
  the paths without entering these bodies; empty Polygon paths remain admitted.
- Pinned `DoGroupOffset` line 518 converts `ceil(abs_delta)` to `int` for a
  non-Round singleton after the `group_delta_ < 1` early exit. The oracle now guards
  exactly that conversion. It inspects groups after duplicate stripping, follows
  default delta mutation across Polygon groups, and handles reversal. With a delta
  callback it checks the actual invoked table entry and path, after reversal;
  unrelated large entries and callbacks retained by later executions are handled
  without a global reduction of the existing delta bound.
- Star input generation no longer calls host `sin` or `cos`. Following the accepted
  arc generator, it evaluates the binary64 input angle using 100-digit Decimal
  arithmetic and 120 Taylor terms, then rounds once to binary64. Only existing
  `big-star` changes; 14 successful boundary cases and 21 separate refusal controls
  are added. Input changes do not supply or alter reference answers or goldens.

At code commit `fba034bbf4f5014268c8ccdba09044b8e191fcc5`, a fresh raw-Git-blob
export and genuine oracle build on the locked GCC 13.3/glibc 2.39/wx 3.2.4 runtime
passed: 1,571 cases, 1,551 exact, 20 genuine include-p002 deferrals, zero failures;
44 matching refusals, each with a successful continuation. All 15 oracle CTests,
focused Rust tests, fmt, clippy, portable checking and import-free wasm replay
passed. Accepted caller results remain engine-p001 901/6 over 907 cases,
include-p001 1,587 exact, rectclip 1,574 exact, with controls 49/83/11. This manual
target evidence does not represent a hosted CI check or independent approval.
