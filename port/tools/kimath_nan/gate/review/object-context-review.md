<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Object context review, 2026-11-24

Reviewer: Codex, parent review of the object-mode implementation supplied by the
separate implementation lane. This records this reviewer's own assessment; it
does not attribute clearance to the earlier Sol reviewers.

Reviewed source: `8520d10138869acf746f4eb4757825e6e9dfa918` and its unchanged
matrix object `97c5f4527c06760a405a5c2f11b99e1ed1524c9b14c8893cbfdc694f580feeb8`.
The accepted link digest is
`3f640aed3a1cd89b131ee02108346b266ebad9b108b2b12d52bfc4b8a5911601`.

## Evidence examined

Native run 45376487021 used Linux x86-64, Python 3.12.3 and the installed locked
GCC 13.3/glibc 2.39 closure. Its actual oracle hashes to
`ed3c6c617cb6822ed46b4a6450a9c5b2489643e79a2ac6c73c4eaeaffcea482c`.
The complete artifact ZIP hash is
`907878d1dcbb47a16ced7d80b9d2a17b7b771fe72b16cbff4231862aba30b303`.
The parent verified that ZIP, every member CRC and all 981 captured-file hashes.
The native gate reports 30 schema, 39 eligibility, 40 mechanical context,
5 scope and 44 arithmetic checks passed; only the pending named-review row
failed. Installed closure verification was executed, not simulated. Image and
no-cache rebuild identity matched before the gate.

The source review covered the object/ELF readers, relocation proof, address
translation, complete path listings, frame and register checks, callee contracts,
ordered extraction and emission boundary. Earlier objections led to the recorded
FP-state decoder, fixed closure verifier, recursive schemas, emission, execution
mapping and exact version-chain fixes. The original measured source, graph and
provider facts remain unchanged. The REGISTRY::Add range equivalence proves the
same original digest preimage, code bytes and relocations, rather than accepting
new code. The matrix-before-hash link order restores the measured helper bodies;
the rejected hash-selected alternatives are not covered by this clearance.

## Manual context assessment

- **A1, operation routing:** the source `Shared<double>` branches return a
  non-null reply for each recognized shared operation. Only its unmatched/null
  path reaches the remaining DoubleOps dispatch. The bound object and its
  unchanged path bytes preserve that compiled routing and the `%rsi -> %rbx`
  operation binding. The dispatch literals and equality-call targets are checked
  from actual read-only data and proven code. Path feasibility is a manual source
  and compiled-context assessment, not a general symbolic proof of Shared.
- **A2, request memory and callees:** JSON lookups, Double/Elements, Matrix and
  Vector parsers read const request fields. Bit transport uses integer parsing
  and bit-casts; reply helpers serialize those bits in row/member order. Their
  documented caller writes concern only declared frame results or local/heap
  objects. String destruction frees owned local storage. The exact bound helper
  bodies and locked runtime support this const-correctness trust boundary;
  arbitrary new helper implementations are not authorized by their names alone.
- **A3, scalar kind:** the source passes KIND unchanged to DoubleOps. The bound
  `%r15` prologue and every pre-window path preserve it. Each scalar branch is
  checked against its actual comparison and the reviewed disposition. Integer
  scalar conversion and the double-scalar sites remain separate.
- **A4, frame/calls:** the reviewed frame is `0x708` bytes. The executor confines
  accesses to that frame, models the CALL return-address write, invalidates
  callee-owned memory, clobbers ABI caller-saved registers, and permits only the
  explicit callee contracts. The analysed Inverse leaf has only its declared
  non-overlapping input/output regions and cannot acquire a stack alias.
- **A5, exceptions:** DoubleOps has no catch that resumes arithmetic after a
  throwing parser/helper. A throwing path unwinds without deriving a window
  result. Cleanup/runtime behavior remains a reviewed toolchain assumption;
  unwind tables are not machine-proved by this gate.

## Decision and limits

Clear the named context row for this object and link digest, with the stated
callee contracts and A1-A5 assumptions. This authorizes a full gate rerun and,
after a full PASS, initial-baseline generation from an exact live comparison.
It does not itself make a gate PASS, qualify all matrix behavior, approve another
COMDAT selection, prove unwind metadata, certify other rounding modes or a second
host, or adopt a universal reference environment. The old frozen whole-binary
route remains distinct. Source/graph/toolchain drift continues to fail closed.
