<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Clipper2 engine p001 cleanup observation review

The Sol 6.1 review of PR #76 at `96c1c83d806078ca37898ee849084d0c348919dc`
found a native adapter lifetime defect. An open path can create an active edge in
both AEL and SEL. ClipperBase::CleanUp deletes the AEL but intentionally leaves sel_
unchanged. The native adapter then unconditionally registers reachable edges for its
state observation. Following the deleted SEL could register a destroyed edge again;
the adapter's later orphan cleanup could delete it a second time.

The C++ cleanup operation is defined; the adapter's deleted-list observation is not
supported. Both shims now refuse that observation explicitly with `invalid input:
deleted list observation` after forgetting the deleted handles and before following
SEL. The pinned engine source and portable engine algorithms are unchanged. This is
a harness limitation, not an upstream undefined-behaviour claim or a successful
comparison of this lifecycle. The controls cover direct CleanUp and Clear; draining
SEL before cleanup is a separate successful corpus case. No existing successful
case is removed or relaxed.

Local validation after the change: 907 public cases, 881 exact results and 26 owned
deferrals; 49 refusal controls and 49 subsequent successful requests on each process.
The rebuilt native oracle audit verified 105 compilations and strict FP flags at pin
4cf939778df04d2b747bb2c4714fc5014a7565c7. Native execution used the existing local WSL
build, not hosted reference CI. The import-free wasm corpus/control replay agrees
with fresh native Rust replies under Node v22.22.1. Shim tests, fmt and clippy pass.

Other review limitations remain explicit: many engine steps use a blanket coordinate
check across all stored paths/rings, which excludes defined cases, even unrelated
stored data. A small ring translated to x=3e18 can have safe Area intermediates but
is outside that harness domain. P001 qualification still needs operation-specific
preflight guards before claiming the full defined domain. The adapter executes actual
pinned source in a second namespace compilation; it does not call p001 bodies from
the already-linked native archive. Hosted comparison and a completed independent
review are still required; the unit remains partial.
