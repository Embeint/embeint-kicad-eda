# Adapter input fix validation

Tested source: `0659b51c6564a98868841f655b4dadc710e15c5a`, clean overlay export.
The pinned 824-file reference and all 60 compile commands passed the original
source, linkage and floating-point audit. Five oracle CTests, affected Rust
formatting/clippy/tests and 38 complete refusal envelopes per adapter passed.

All 14,190 live cases passed compare, propose, bless and check with zero failures
or deferrals. The original 14,189 golden replies remain byte-identical. The one
new case retains literal JSON integer `-0`, storing positive-zero altitude bits.
No algorithm body, comparison tolerance, case budget or original evidence changed.
The new immutable baseline enumerates every initial result against the absent
golden in both intended PR bases, rather than a delta against a local baseline.

The command script records the private workstation WSL reproduction; it is not a
portable project entrypoint. Reports and actual provenance retain binary hashes.
The golden policy must additionally pass against committed HEAD.

Merge holds remain: adopt the reviewed shared native request parser to fix JSON
numeric overflow envelopes, then reconfirm corrected PR #39 scope and layer,
pass hosted CI and parent review. This evidence does not certify those holds
or the three-pilot phase-zero gate.
