# Literal escaped MD5 marker keys

Tested source: `da17dc54dc2b2be2b15cb4a2c6c171c62950a5cc`.

Independent review of c5ec29e closed the runtime reserved-object P2 but found a
P3 coverage gap: `reserved-escaped-id`, `reserved-escaped-byte` and
`reserved-escaped-unit` use plain marker keys. Escaped quotes inside their VALUES
do not cover escaped KEYS. Only the old duplicate-key fixture escaped a key;
that refusal happens before object conversion. The nonduplicate escaped-key
claim in the 65d5b0a9 evidence is withdrawn here; that historical evidence and all
199 prior checks remain unchanged.

The default Python guard appends three nonduplicate literal key tokens, using
escaped dollar signs and `Raw\u0056alue`, plus a successful recovery. It checks
their raw spelling and decoded keys. The Rust shim adds three complete-envelope
assertions with literal raw key spellings. Default oracle CTest registration is
unchanged and calls the updated Python script; the Rust test is not ignored.

Fresh checks, private Linux tree, actual shared mkdir lock, at most eight jobs:

- fmt and warnings-denied Clippy pass; all 11 shim tests pass, none ignored.
- Native selected-package shim and portable MD5 wasm build pass.
- Both real adapters match 203 complete replies: 197 refusals and six successes.
  The old 199 requests, expected replies AND actual adapter stdout remain exact
  byte prefixes. The pre-fix helper fails all three new refusals; recovery matches.
- The unchanged exact runner checks all 5,973 cases at the default 60-second
  per-process budget, including the streamed 512 MiB carry case, with zero
  failures/deferrals. Output hashes equal the existing corpus/golden proof.
- The exact LF source export and all Rust/Cargo inputs match the tested Git
  source; production shim code, portable MD5, shared parser/comparator, dependency
  lock, cases, goldens, unit records and historical evidence are unchanged.

The C++ oracle and exact runner are REUSED, with identity and live embedded
provenance checked before execution. Oracle SHA256 is
`ef081cadadaadc81a14b966e492d312872ef3429e20d02ee44ef51af98ead6d1`, from the
qualified 65d5b0a9 build; its original reference-export representation and
provenance are preserved. No new C++ build, four-CTest run, raw-LF reference
export, C++ source/link/FP audit or expanded compiler/architecture qualification is claimed.
Fresh Rust binary identities are in proof.json and registered.json. Earlier full
reference-source/link/FP audit remains prior qualified evidence.

This additive correction does not rebless results or change green/graph/gate
acceptance. Independent review and final hosted CI remain required.
