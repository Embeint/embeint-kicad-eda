# Native request parser integration validation

Tested code: `f777c57baaf1813f210e3ce022db553b301c5dd6`, with a clean oracle
export (`overlay_dirty_at_export=false`). The source integrates exact reviewed
PR #45 commit `8f5f45729c733d8f1695dda9ee6d953f9bb4d400` using merge commit
`bf8c796362`. The parent subsequently merged #45 after hosted CI and independent
review into `origin/rust` at `f72e08a883509c0bf451b00905718a9a83249fb1`; its helper
contents are unchanged. That ancestry update was not substituted for the actual
tested source identity.

The clean 824-file pin and 60 compile commands pass the unchanged source,
archive-link and floating-point audit. All five CTests pass. Both actual
processes pass 106 shared complete refusal envelopes, including original kind,
identity and message, plus accepted integer `-0`, signed-zero bits, unsigned
finite bit patterns, maximum step count, EOF without newline and recovery.
Raw literal fixtures cover `1e1000`, negative overflow, 400-digit integers,
overflow in nested/unknown/identity fields, finite integers beyond 64 bits,
underflow and floats. Domain failures remain distinct from parse failures.

Affected workspace formatting, native shim/difftest clippy with warnings denied,
native tests (including the real pinned C++ request fixture), portable engine
wasm build, full unit/source validation and all seven policy tests pass.
The policy tests print synthetic phase-zero COMPLETE/INCOMPLETE examples;
those are test fixtures, not a project qualification claim.

The full existing live `check` passes all **14,190 cases**, zero failures or
deferrals, against the actual pinned oracle and committed golden. No propose or
bless ran. The portable source, corpus, every golden/evidence file and all parent
0659b51c validation artifacts are unchanged from
`424ebe0e81558bdfe6b5a6b9700384fa2da1239b`; `retained-artifacts.json` records their
SHA-256 hashes after the committed diff check. Only notes change in the owned unit
record; canonical scope/dependencies/layer/USRs remain untouched.

`validate.sh` records the private Linux tree, detached resumable workflow and
commands. It waits for and acquires the real shared mkdir lock, releases only its
own lock and caps compiles at eight. The retained parent build was copied into a
separate `~/kicad-port/iec-request-parser` tree; parent logs, runners and evidence
were preserved. `job.log`, `live-check.json`, `tested-oracle-provenance.json` and
`binary.sha256` record actual results and executable identity.

Golden policy additionally runs against committed HEAD with both intended bases
`0ab6a0e8635421998a6471d1001fbf7b44c81e71` and
`a7ec77beffd3b2e5d60409423fa28b6c116c6e00` after this validation artifact is
committed. The draft PR's corrected #39 scope/layer hold, independent review and
hosted CI remain merge requirements. Phase zero remains INCOMPLETE.
