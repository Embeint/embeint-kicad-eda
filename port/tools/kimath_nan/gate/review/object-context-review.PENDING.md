<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Historical object context review request

This is the preserved request from the original source checkpoint. The current
parent assessment is [object-context-review.md](object-context-review.md), bound
by hash in the lock's review record. The pending statements below describe the
original checkpoint and do not override the later review or native run results.

Status: **PENDING**. No reviewer has examined or signed this record. `object.lock.json` carries
`context_review.status = "PENDING"` and `reviewer = null`. The gate row "object context review
signed by a named reviewer for this object and link digest" fails until a named reviewer
replaces both fields after doing the work below. Nobody may fill in a reviewer name on
someone's behalf.

## What it would bind

- Object `kimath_math.cpp.o` sha256 `97c5f4527c06760a405a5c2f11b99e1ed1524c9b14c8893cbfdc694f580feeb8`
  (`GCC: (Ubuntu 13.3.0-6ubuntu2~24.04.1) 13.3.0`).
- Link digest `3f640aed3a1cd89b131ee02108346b266ebad9b108b2b12d52bfc4b8a5911601`. This is the
  SHA-256 over the proven home sections (33) and the exact digests of the 58 units that the
  linker selected from other objects of the same link (`main.cpp.o`, `protocol.cpp.o`,
  `registry.cpp.o`, `iec60664.cpp.o`: nlohmann exception, concat and destroy helpers,
  `_Rb_tree` code and `REGISTRY::Add`). It also covers the 50 dynamic imports and the one
  address-only external (`__dso_handle`).

## Mechanically re-derived against the measured code

`objgate.py derive` ran on the reviewed build (oracle `0bac7567…`, the same object):

- the existing reviewed context bindings were translated through the proven units. These are
  the `%r12` input, `%r15` scalar kind and `%rbx` operation prologue bindings, frame size
  `0x708`, the 7 site windows and branches, the 14 callees and the `operator==` dispatch
  target;
- each site's pre-window path was measured, with a path digest taken over the OBJECT's bytes
  and callee identities taken from the link proof.

The same checks pass unchanged on the fresh hosted build (oracle `6b6629d8…`, newer revision,
different layout): all 40 mechanical context rows and all 44 arithmetic rows. That is a
mechanical result only.

## What a reviewer must still judge (the manual assumptions of the reviewed lock)

- **A1:** routing of `aOp` through the inlined `Shared<double>` bodies (path feasibility is
  reviewed, not proved).
- **A2:** no write through the request pointer. This is mechanically checked on the paths;
  the callee contracts are reviewed.
- **A3:** the remaining reviewed assumptions as written in `eligibility.lock.json`
  `context_review.assumptions`, re-read against the object instead of the old binary.
- **Other objects' code.** Code selected from other objects runs inside the unit (error
  paths, JSON destruction, registration). Whether its reviewed digests are an acceptable
  basis is a review question.

The reviewed lock's citations of private review records are not repeated here. This record
depends only on files in the repository and the gate's own measurements.
