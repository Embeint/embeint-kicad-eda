<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Reserved serde key envelope integration

Runtime source: `65d5b0a9732551db35a9348d69d7b2ccfa86a1fb`. The clean oracle
export reports that exact revision and `overlay_dirty_at_export=false`. The later
notes commit `ac8ef11e919066aa24b8d432bb6a4e670a00e882` and this evidence change no
runtime body, corpus or output.

## Finding closed

The final independent review of `cfed960` found one P2: serde_json's
`arbitrary_precision`/`raw_value` deserializer treats object keys
`$serde_json::private::Number` and `$serde_json::private::RawValue` as internal
markers. Ordinary user objects with those keys became ids, bytes, strings, arrays or
whole envelopes in the Rust adapter, while the actual C++ oracle refused them as
objects. An invalid marker string could also erase identities as invalid JSON.

Reviewed PR #48 (exact head `1ec85a446741c9903963a7282eedddeea06e34e2`) builds
`Value` directly from the validated exact tree, retaining finite checks, original
numeric tokens and the 127-container bound. It was integrated by normal merge
`1893ac63e94b8c0f808fe123868366ffa3939e4b`. PR #48 has since merged into `rust` by
rebase as `1a2fb924d539d63a68f5696191907dc2aa6d6370`, whose tree is identical to the
reviewed head; integrating that rust head changes no tested source.

## Regressions

`port/oracle/test_md5_inputs.py` (run by the oracle CTest and against both actual
processes) appends 45 complete-envelope checks after the unchanged 154:

- The review's 14 retained reproducers, as literal wire text, expecting the complete
  replies the actual oracle returned to the reviewer.
- Placements at the root (including an encoded envelope), unit/case identities, an
  extra envelope field with an invalid marker string, whole steps, an extra step
  field, `id`, `data` (between valid bytes and after a finalize), `repeat`, `int`
  and `valid` values, `clone` source and `compare` other.
- Multiple reserved keys in one object, keys spelled with JSON escapes
  (`$…`, `RawValue`), numeric/array/null marker values, plain and escaped
  duplicate reserved keys (still refused as duplicates).
- Negative controls where the reserved names are string values or a near-miss key.
- Successful new/bytes/finalize ("abc") requests before, between and after the
  refusals, a successful request whose case is a reserved name, and recovery after
  an invalid-JSON line. Refusals return no partial events.

Expected successful digests are computed with `hashlib` and KiCad's pinned B..G
nibble alphabet; all expectations were validated against the actual freshly built
oracle, not inferred from Rust. With the pre-#48 parser 35 of the 45 new checks
fail; the other 10 are the intended guards and controls. The 154 prior checks
remain a byte prefix of `raw-regression-requests.jsonl` and
`regression-expected.jsonl`. The Rust shim carries the retained reproducers and
the main placements and state guards as two default (not ignored) unit tests.

## Actual local results

- Clean LF `git archive` of the runtime commit (`export-sha256.txt`); MD5 cases,
  golden and changed sources verified byte-for-byte with no CR. Reference archive
  `941b5a74…` is identical to the previous proof's.
- 59 compiler records / 822 pinned files pass source, linkage and fixed
  floating-point audits; four oracle CTests pass, including the new MD5 inputs.
- Affected fmt and warnings-denied clippy pass; 28 Rust tests pass (15 harness
  including the C++ transport probes, 3 MD5 wrapper, 10 MD5 shim), plus portable
  MD5 wasm.
- Each real adapter returns exactly the 194 expected complete refusal envelopes and
  five complete successes; both complete stdout streams are retained.
- The registered CI path validates 1,801 unit records. All 5,973 live cases match
  the oracle and committed golden at the unchanged default 60-second budget,
  including the real 512 MiB carry case: zero failures or deferrals. Oracle and
  Rust output hashes equal the previous proof's. Generator output is byte-identical.
- `check_goldens_policy.py --base` passes against `0ab6a0e8`, `a7ec77be`,
  `19035296` and `4c6c1e38` on the committed head.
- `protected-scope-proof.json` (from `protected_scope.py`): every protected MD5
  field matches the corrected and historical records, existing port metadata is
  preserved, only the owned unit differs from the dependency, and cases, goldens,
  golden evidence, the C++ shim, portable MD5 bodies, the exact comparator and all
  five prior evidence directories are unchanged since `cfed960`.
- Phase zero remains **INCOMPLETE, 1/3 live green L0 pilots**;
  `PHASE0_REQUIRED=false` is unchanged.

| Identity | SHA-256 |
| --- | --- |
| Oracle executable | `ef081cadadaadc81a14b966e492d312872ef3429e20d02ee44ef51af98ead6d1` |
| Rust shim executable | `ed5f93fd81835cf5dcf30b7b0a27bf7dc9eeae01550c74ef5042a3890c4442a6` |
| Harness executable | `5d07329ad8d10ff7363d83d8e72f570e336125eca0500b8cb842354660f3264c` |
| Raw regression requests | `fd7d8399c348c63a6e777c6ee13d96594f6059f8a29ef8065cfa2eb9ce802c30` |
| Expected complete replies | `cb8f38533012e1ae3d88cd384f9fda8279cd187bf22b1547122022e3ec167ebf` |

The first attempt (unpublished runtime `1e48e93`) failed warnings-denied clippy on
a complex test table type after its oracle build and CTests passed;
`attempt1-1e48e93-clippy.log` retains it. The runtime was corrected with a type
alias and the whole job rerun in a fresh root.

The detached job ran in new private Linux root `~/kicad-port/md5-envelope-regressions`
under the real shared mkdir lock with at most eight jobs. It reused the prior
root's solidcore checkout and Cargo cache by copy only and modified no prior tree.
No algorithm, portable body, corpus, golden, immutable evidence or comparator
changed; no proposal or blessing was performed.

**Acceptance remains pending:** independent review of this head, hosted CI, the
parent's final #39 graph acceptance and parent review. Local `green` metadata is
provisional and no phase gate is claimed. Implementation by Claude Opus 5.5
(`claude-opus-5-5`); commits carry `Assisted-by: Codex` per the house rule.
