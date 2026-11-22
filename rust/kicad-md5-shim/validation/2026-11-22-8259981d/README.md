<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Native request parser adoption

Tested source: `8259981dde8c9632c3b1194e8cc9362fe87a45b9`, with a clean source
export and actual embedded oracle provenance. The independently reviewed helper
is PR #45 commit `8f5f45729c733d8f1695dda9ee6d953f9bb4d400`, integrated with
merge `056dc882ae41861d282041812a758d0972db3d89`. Parent subsequently merged #45
into `rust` at `f72e08a883509c0bf451b00905718a9a83249fb1`. Reconciliation merge
`1eede0ef3d98a64e41d0119a375e817fa8cd7962` has an identical tree to the tested
source; it changes ancestry only.

All 59 compilation records and 822 pinned files passed the existing source,
library-linkage and floating-point audit. Four oracle CTests passed, including
the complete MD5 input process test. Affected fmt, warnings-denied clippy, 21
Rust tests (10 MD5/wrapper and 11 harness/request tests), portable MD5 wasm and
the full source/unit validator pass. Raw generator output remains byte-identical.

Each actual C++ and Rust process passes 78 complete refusal envelopes and one
complete successful integer `-0` envelope. `raw-regression-requests.jsonl`
retains the exact wire literals, including `1e1000`, negative overflow, 400-digit
integers, full u64, finite integers beyond 64 bits, 308-digit finite integers,
underflow and floats in nested/unknown/identity fields. Expected replies include
complete identities, error messages and successful state observations.

The existing live `check` matches all 5,973 committed cases at the default
60-second per-program budget, including the real 512 MiB carry request; zero
failures or deferrals. Actual executable hashes and outputs are in
`live-check.json`; full compiler/source/build identity is in
`oracle-provenance.json`. No case, successful reply, golden, prior immutable
evidence, portable body, comparator or global C++ protocol was changed. No
proposal/blessing was performed or needed.

All builds ran in the new private Linux tree
`~/kicad-port/md5-request-parse`, from copies of retained caches. Parent source,
build, runner and evidence trees remained untouched. The detached job acquired
the real shared mkdir lock only after its owner released it, capped compiles at
eight jobs and released only its own locks. `job.log` and `validate.sh` retain
the commands; private runners and caches remain available for resumption.

PR #45's merge hold is resolved. Corrected #39 ownership/scope/layer confirmation,
updated hosted CI and parent review remain explicit holds. Provisional canonical
unit fields are untouched. This result does not establish the phase-zero gate.
