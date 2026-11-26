<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Unix system-info hosted reference evidence

PR #90 head `ab2d7b683c9ea101540adc084492c989b23c4161`, tree `c67da78ea16ecb47a668eb55a5dea5c0d2bce694`.
A fresh independent Sol 6.1 review found no findings or material objections.
The six pinned Unix bodies retain their false/no-mutation/empty behavior; no
host observations are invented. Every C++ output field is initialized before
observation. The Rust fixtures do not claim C++ uninitialized default values.

[Scoped hosted run 45852710376](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45852710376) passed on GCC 13.3,
glibc 2.39 and wxWidgets 3.2.4. Its generated PR merge has the accepted
`98a12c1123` base and reviewed head as parents, and its tree exactly equals the
reviewed tree. The parent downloaded and hashed its artifacts and verified
this binding. The hosted `rust-port` gate also passed.

- 126 cases compare exactly, zero deferred/failed/unprocessed.
- C++ and Rust reply-stream hashes both equal
  `b470406aea31b9622f9a44fc16083ef2a341dcaca560e601de9e48e10aa69e34`.
- All 67 refusal controls match on both processes, each followed by a successful
  continuation. The complete import-free wasm corpus agrees with fresh C++.
- The audit uses 917 pinned source files. Independent review verified reused
  local-oracle source/flags as well as constructors, references, lifetimes,
  output kinds, co-port ownership, guard tests and the serialization correction.

The unit stays partial without a golden. Uninitialized C++ numeric defaults
are excluded from observation. Generic allocation failure, other platform
sources and unbounded fixture domains are not qualified here.

The isolated developer runner remains offline. Its queued run 45852706498 was
cancelled and replaced with the existing hosted fallback
[45858418153](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45858418153)
on the same reviewed head. That fallback was pending at this recording; it
provides ordinary developer preflight, separate from the completed target
reference comparison. Repository settings and required checks are unchanged.

[verification.json](verification.json) retains source/tree/merge bindings,
actual target versions, comparison/binary hashes, artifact and full-log hashes,
relevant control/wasm log lines, review disposition and fallback identity.
