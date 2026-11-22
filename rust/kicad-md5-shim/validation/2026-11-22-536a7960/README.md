# Adapter input fix validation

Tested source: `536a79601d3116e8a30befacc07127ef05b052f1`, clean overlay export.
All 822 pinned reference files and 59 compilations passed the original source,
linkage and floating-point audit. Four oracle CTests, affected Rust formatting,
clippy and eight Rust tests, plus eight complete refusal replies per adapter pass.

All 5,973 cases pass live compare, propose, bless and check at the unchanged
60-second budget, including the real 512 MiB carry case. Every original 5,972
golden reply is byte-identical. The one new case preserves literal JSON integer
`-0`, hashes the byte zero, and checks the reference's unusual formatted digest.
No hash-wrapper body, comparison tolerance or original evidence was changed.

The new immutable evidence enumerates the complete initial baseline against the
absent golden in both intended PR bases. Reports and actual provenance retain
binary identities. The command script reproduces this private workstation WSL
job, rather than providing a portable project entrypoint. Committed-HEAD golden
policy is a separate required check.

Open merge holds: adopt the reviewed shared native request parser for numeric
overflow envelopes; reconfirm corrected PR #39 ownership/scope/layer; pass hosted
CI and parent review. This pass does not certify the three-pilot phase-zero gate.
