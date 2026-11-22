# Generator capture must invoke its verified executable

Independent review of PR #39 at `2888cd057f` found that the refresh checked the
original manifest's absolute Ninja binary but invoked catalog/query/command
operations through PATH. A different installation or wrapper could therefore
answer the queries while the recorded binary still passed both identity checks.
The reviewer reproduced lost Markdown dependencies by forwarding the calls to
a second real Ninja build graph. This did not establish incorrect dependencies
in the published snapshot: its actual PATH Ninja matched the recorded binary.

Code `c1e8a467da` passes the verified absolute Ninja path to every invocation in
guarded capture and recipe traversal. Missing and relative recorded paths fail
before querying. Standalone recipe callers resolve an absolute executable once.
The freezing CLI passes the absolute tool path it records.

The real-Ninja regression constructs a PATH wrapper whose alternate graph omits
the Markdown edge. A control invokes that wrapper and confirms the omitted edge;
guarded capture then ignores the wrapper and retains the complete dependency.
The same test rejects changing the actual recorded executable after the queries,
and rejects missing/relative recorded identities. Against the earlier published
refresh implementation it fails with `Capture invoked an unrecorded PATH executable`.

Evidence is under
`port/extractor/generated/evidence/verified-ninja/`, including the failing old-code
control, successful fixed suite, fresh real capture, canonical regeneration and
artifact checks. Fresh recipes, catalog and dependency maps equal the previous
publication; AST parse identities, all 1,801 unit records and pilot scopes remain
unchanged. V5, D2 and the earlier source-guard evidence are retained separately.

The implementation correction awaits a new independent review of the published
head. It does not certify numerical pilot parity, accept
the graph for the phase-zero gate or expand compiler/configuration coverage.
