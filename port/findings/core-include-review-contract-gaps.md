<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Core header review: unfinished resource and host contracts

Unit: `libs.core.include.core.application.pure.p001`, PR #73. Independent Sol 6.1
source review at `eecececf4294f4be5aa9fc3ae034cbca15e0f9c9` independently replayed
669 header cases, 49 refusal controls and 6,227 existing core cases with exact replies
and no deferrals. Those runs used the existing local native oracle and Windows Rust
binaries; they are not a claim of hosted reference acceptance.

## Confirmed source limitations

- `profile.rs` copies caller-provided names through `to_owned` in constructors,
  checkpoints and timer recording, and allocates infallibly during formatting.
  `sync_queue.rs::push` invokes the caller type's `Clone` before its collection
  reservation. On allocation failure these paths may abort rather than return
  `ProfileError::Allocation` or `AllocationError`. The C++ copies can throw bad_alloc.
  Allocation failure was not injected; the finding is based on source inspection.
- The exclusive-owner portable model does not implement concurrent union-find,
  atomic counters, shared spinlocks or mutex-protected multi-producer/consumer
  queues. A host synchronisation/integration contract is still needed.
- A String sink cannot represent arbitrary ostream formatting flags, flushing or
  stream failures. For example, PROF_COUNTER::Show on a hexadecimal C++ stream
  prints a hexadecimal count; the Rust sink always formats the count as decimal.
- ScopedProfTimer's explicit finish returns the measurement, but ordinary drop
  or early return does not perform the C++ destructor's duration assignment.
  A host scope wrapper is still needed to preserve that lifecycle.

Keep the unit partial after hosted checks pass. The audit and unit notes now name
these as unfinished public contracts rather than merely untested branches. Follow-up
implementation should provide fallible string growth/element-copy boundaries and
resource-failure controls, explicit host synchronisation and stream contracts, and a
scope wrapper exercised through normal completion and early return. No corpus result,
golden or guard is changed to conceal these gaps.
