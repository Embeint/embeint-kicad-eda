# Bind generator refreshes to the original source snapshot

Independent review of extractor PR #39 at
`d2bdddb904116348e474389dd577cb99ac0c8790` confirmed that the published artifacts
used the correct pinned source, but found a generic P2 in the refresh command.
Supplying a foreign root or deleting a source header/Markdown input erased valid
Ninja dependencies while the unchanged build fingerprint still passed. The pure
`apply` command accepted that capture, and the real drift reporter then missed a
Markdown-only change for a direct tracked consumer or a build-header consumer.

The refresh now requires the original parse inventory and the pinned
generator/configure input audit. The physical source root must equal the
recorded reconciliation and parse root. Capture checks every tracked C/C++ file
and every audited generator input before and after querying Ninja, using the
original parse's declared CRLF-to-LF-normalized SHA-256 policy. Ninja source nodes
outside that recorded snapshot fail explicitly. This includes Markdown, schemas,
scripts and generated source-tree headers, beyond the C/C++ inventory alone.

The capture records the complete source snapshot, its hash, the input-audit hash,
the parse inventory hash and pins. Pure application reconstructs that expected
identity from the same immutable inputs and rejects foreign or changed capture
identities. It does not read WSL source paths when replayed on Windows. External
tools and the stored frozen inputs are also checked again after capture; the
earlier documentation's unqualified before/after claim did not hold for those
checks at the reviewed head.

Real-Ninja regressions cover the intact source, foreign root, missing and stale
Markdown/header/C++ files, forged capture source identities, unrecorded source
nodes, and source/tool/frozen-copy changes after the actual query routine. The
matching-libclang21 suite passes all 36 tests. Windows passes 25 tests, with 11
libclang/Ninja tool skips. An additional read-only check of the real pinned
snapshot verifies 9,655 distinct source files and every source-tree Ninja target;
no source target lacks a recorded hash. Snapshot hash:
`3c0d96516f2c20ca5e7883aa7a549ed0e412a190b27af60012a4edfb0621122e`.

This changes metadata guards, not AST extraction, source bytes, compile flags,
unit scope or numerical comparison. The previously published capture remains
historical evidence under its original code identity. A new guarded capture and
application must publish their own identities before final review or acceptance;
the original V5 parse/runtime/cache identities remain unchanged.
