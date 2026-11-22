<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# MD5 native request overflow boundary

The MD5 native adapter previously used the arbitrary-precision exact comparator
parser as request validation. A raw `1e1000` or 400-digit integer therefore
survived JSON parsing on the Rust side and reached envelope/domain validation.
The pinned nlohmann parser rejects the same token before identities are read:
the complete reply is `invalid_request`, null unit/case, and message
`invalid JSON or duplicate key`. This applies inside nested or unknown fields
and numeric identity fields too.

The adapter now calls `kicad_difftest::request::parse` before reading identities.
Its independently reviewed source is PR #45 commit
`8f5f45729c733d8f1695dda9ee6d953f9bb4d400`, merged into this topic with
`056dc882ae`. Parsing errors map to the exact complete reference envelope.
The shared exact result comparator is unchanged. No global C++ protocol or
portable MD5 body is modified.

Finite out-of-range integers, including the full u64 token, finite integers
beyond 64 bits and underflow/floating tokens, remain parseable and reach the
actual domain or envelope checks. Integer `-0` remains a successful zero byte;
float `-0.0` remains a domain refusal. Unknown-unit requests with finite numeric
payloads remain unknown-unit replies, whereas numeric overflow rejects parsing
before an unknown unit can be dispatched.

`port/oracle/test_md5_inputs.py` builds raw strings, retaining literal number
tokens. It asserts 78 complete refusal envelopes and a complete signed-zero
success per actual adapter. CTest runs the real C++ process; the explicit dual
process command checks both C++ and Rust against the same expected replies.
Native Rust tests assert complete envelopes as well. Successful corpus/goldens,
all parent evidence and canonical unit accounting remain unchanged; corrected
graph ownership/scope/layer confirmation and hosted CI remain merge holds.

The actual tested code and new validation evidence are recorded in the matching
`rust/kicad-md5-shim/validation/2026-11-22-8259981d/` entry after execution.
