# Integration proof checksum presentation correction

This addendum addresses the P3 evidence finding in the independent review of
PR #49 at `3583549ab6e8149ce045fba63020ab6d3c0cf27f`. It adds verifiable copies
and checksum maps without changing any historical artifact, runtime source,
case, golden, unit record or phase-zero setting.

The historical `files.sha256` in each integration proof directory hashes six
CRLF working copies whose committed presentations use LF: `guard-mutations.log`,
`head-identity.json`, `iec-corpus-check.log`, `layer-identity.json`,
`pin-identity.json` and `source-manifest.json`. The other 18 entries already match
the committed bytes. For all six differences, replacing each committed LF with
CRLF reproduces the historical checksum exactly. The old checksum lists remain
unchanged; they do not verify the normalized committed presentations.

Each directory now adds:

- `files.committed.sha256`: hashes of all 24 originally listed artifacts as
  committed, the unchanged historical checksum list, and the new compressed
  original manifest. These 26 entries verify against an exact Git export.
- `source-manifest.original.json.gz`: a lossless compressed copy read directly
  from that proof's retained `oracle-src/reference/source-manifest.json`. Its
  decompressed bytes match both the embedded oracle provenance and the summary's
  original manifest identity. It was not reconstructed from the LF presentation.

The original and committed manifest identities are distinct:

| Tested source | Original manifest SHA-256 | Committed LF presentation SHA-256 |
| --- | --- | --- |
| `d0cf06863c` | `e5c8c1715ce8d9cd2081f50342e941bd31887e1b36871378ed8f7bcee52ab841` | `b22efbee509f32013dd12ace05e05bee5bf34f1a22daffa79cf068a3b70b089f` |
| `3731499b2b` | `c7ec2c6d603af773ae23db84de1c614d8dd179ec97d49a6df8504f06a1171286` | `6941881c0751157954208a866cb52cbca12e4453e1b56b801e69e5a83a98f136` |

Decompressing and normalizing CRLF to LF reproduces the committed manifest
byte for byte; parsed JSON is also equal. The reference files themselves retain
their separately verified 915 raw Git source identities. This presentation
correction does not change those identities or relabel the build provenance.

[The complete checksum map](phase0-integration-checksum-addendum/checks.json)
records all 48 original entries, both digest forms, the six differences per
proof, and the original/compressed manifest identities.

From an exact Git export or LF checkout, verify each proof directory with:

```sh
sha256sum -c files.committed.sha256
gzip -dc source-manifest.original.json.gz | sha256sum
```

The second digest must equal the original manifest identity in that directory's
unchanged `oracle-provenance.json` and `summary.json`, not the digest of
`source-manifest.json`. A Windows checkout that expands text line endings can
instead use an archive exported with both `core.autocrlf=false` and `core.eol=lf`.

Validation checked the staged Git bytes, all 52 sidecar entries in a fresh LF
archive using Linux `sha256sum`, both original manifest digests, their parsed
contents and their exact LF normalization. Every previously committed path keeps
its original Git blob. No oracle or Rust build or numerical comparison was
rerun for this additive evidence correction. The earlier independent review's
retained-binary limits, graph acceptance hold, claimed resistor status and
INCOMPLETE 2/3 phase-zero state remain in force.
