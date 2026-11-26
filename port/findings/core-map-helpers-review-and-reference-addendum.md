<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Map helper review fixes and reference correction

Unit: `libs.core.include.core.application.thin.replacement.p001`, PR #78.
Independent Sol 6.1 review of `9d4c88718110419e5de8948006fb43943d58d526`
found two implementation defects and a reference-description error.

The missing-key branch of `get_def_or_value_init` returned its temporary default
directly, skipping the copy that the C++ return from a const reference performs.
It now calls `TryClone` like the explicit-default overload. A custom value whose
copy increments a counter verifies both default forms return one copy; a
fallible copy verifies the allocation error propagates.

`WxString::from_utf8` checked its temporary buffer, including the terminator,
against the logical string limit. It now checks the decoded length against
`MAX_UNITS` and separately reserves length plus one. A boundary test accepts
exactly 16,777,216 ASCII units, agrees with `from_units`, and refuses one more.
The reference has no `MAX_UNITS` fence; this remains an explicit port limitation.

The first baseline's reason text claimed wxWidgets 3.2.4, but its nested oracle
provenance truthfully records wx 3.2.9. The independently checked binary SHA-256
is `b52c653214bc993834d30f4aef11c29ea2160da915376b6ad848f86707c6c193`;
local `wx-config --version` also reports 3.2.9. The intended Ubuntu 24.04 target
uses wx 3.2.4; reading its sources does not certify the linked 3.2.9 binary as
that target. No source-equivalence qualification is claimed here.

The existing golden and its evidence are preserved unchanged as historical
artifacts. This addendum corrects the false version in their prose; the old
baseline establishes only agreement with its actual recorded local binary.
The unit is partial with zero deferred cases pending a fresh exact comparison
on the target runner with its own provenance. Hosted success must be assessed
on the final corrected head before any green promotion.

The generic copy and capacity boundary tests exercise behavior absent from the
3,084-case serialized corpus. The corpus and its golden are unchanged. Windows
16-bit `wchar_t`, other template instantiations and strings beyond the explicit
port bound remain outside the qualified contract.

After the fixes, the Windows native shim and that same local C++ oracle return
all 3,084 corpus replies identically, also matching the unchanged golden
(reply SHA-256 `f7103e18b3969d9821cad7e364c16fbb085ced4cd05d065935d0d8185a7c4691`).
Both processes pass 336 identical refusal controls and 336 successful followups.
Import-free wasm32 replays all 3,084 fresh C++ replies under Node 22.22.1.
Fmt, clippy with warnings denied, seven library tests, two corpus/control tests
and the generator check pass. These are local results, not target qualification.
