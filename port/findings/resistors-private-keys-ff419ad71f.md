<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Resistor adapters: serde private marker keys

Tested clean runtime source: `ff419ad71f51044b81b8b7b9b0b63970e6de3942`.
It merges prior PR #44 head `084115f06cb1cb73fe6bc04f90f715d3384cfb3e` with
`rust` `1a2fb924d539d63a68f5696191907dc2aa6d6370`, which contains merged #48
(runtime fix `50e3f24874ea430770d59e10aa3db095dec4f757`, see
[the shared finding](native-request-reserved-object-coercion.md)). This note and
its evidence were committed afterwards; they do not change the tested source.

## Defect and adoption

Before #48, `kicad_difftest::request::parse` deserialized each line as
`serde_json::Value`. That deserializer treats object keys
`$serde_json::private::Number` and `$serde_json::private::RawValue` as internal
markers, so an ordinary user object could become a number, string, array or
identity, or a valid line could turn into a null-identity parse refusal. The
pinned C++ adapter treats these as ordinary keys. The resistor shim calls the
shared parser, so the merge alone adopts the fix: no resistor adapter,
portable, comparator or shared-helper code changed here.

## New default regressions

`port/oracle/fixtures/resistor-private-keys.requests.jsonl` contains **889
literal request envelopes**. The complete replies in
`resistor-private-keys.replies.jsonl` were recorded from the actual pinned C++
oracle; no reply was written by hand. The wires were authored by
[`fixture-authoring.py`](resistors-private-keys-ff419ad71f/fixture-authoring.py)
and are committed literally.

- Both marker keys carrying `"0"`, `"1"`, `"-0"`, full and overflowing unsigned
  strings, `"1e1000"`, `"bad"`, an encoded unit string, encoded arrays/objects,
  invalid JSON `"{"`, empty string, `0`, `true`, `null`, `[]`, `{}` and nested
  marker objects; combined keys, keys beside other members, escaped spellings
  (`$...`, `RawValue`, escaped colons) and six near-miss spellings.
- This full set of 55 objects at each of: a format bits element, collector
  `target_bits`, `unit`, `case` and the input root.
- Twelve representative objects at each of 39 further positions: operation,
  bits arrays and elements, resistance value/name/parts/clear, calculator actions,
  action op, unsigned `set_series`, `new` target, exclude/find values, solution
  level, build series, custom `build_values` vectors, series selector and decades,
  collector candidates and nested candidate fields, reader text/source/parser
  indices and bind reader, helper first/text/symbol.
- Marker keys as extra envelope, input, resistance, action and candidate members,
  including the envelope's first member.
- Accepted positions: unknown units carrying marker inputs, and names spelled as
  marker keys, encoded marker objects and numeric/overflow strings.
- Plain numeric, overflow, JSON-looking and marker-key strings in bits, target
  and series positions stay strings and keep their typed refusal.
- Duplicate marker keys, including escaped duplicates, stay null-identity
  refusals; literal overflow inside a marker object stays a parse refusal, while
  finite tokens keep their integer `-0`, full-u64 and fallback semantics.
- 64 successful replies (51 interleaved recovery probes, names, literal `-0` and
  full-u64, the empty-collector result and a calculator state) separate the 825
  refusals. These are adapter controls, not registered difftest cases; the
  1,050-case corpus is unchanged.

They run by default in three places:

| Guard | Programs |
| --- | --- |
| Canonical CTest `oracle-resistor-private-keys` (`--byte-identical`, both fixture hashes pinned) | Actual C++ oracle |
| `rust/kicad-resistors-shim/tests/private_keys.rs` (process replay, exact trees) plus one focused unit test | Built shim, debug and release |
| `tests/requests.py` (71 original cases unchanged as a byte prefix, then the 889) | C++, Rust debug and release |

The collector-target body from `1dd31a7493189ca694bc09020a3a84bf9fb6082e` and
its eight-envelope CTest `oracle-resistor-inputs` are unchanged.

## Negative control

The same requests through the retained pre-#48 shims (`cc9b410f...` debug,
`61fa3cd9...` release; the actual binaries recorded for `084115f06c`, executed
read only) differ structurally from C++ on **459 of 889** lines: 101 coerced into
results (for example a collector target `{"$serde_json::private::Number":"0"}`
returned the `Empty solution collector` result instead of
`expected u64 IEEE754 bits`), 302 became null-identity parse refusals and 56
returned a different refusal. The per-position table is in
[`prefix-observation.txt`](resistors-private-keys-ff419ad71f/prefix-observation.txt).
In the proof run both old shims fail the new guard
([`negative-control.txt`](resistors-private-keys-ff419ad71f/negative-control.txt)).

## Proof run

Own Linux root `~/kicad-port/pilot-resistors-private-keys`, from a clean LF
`git archive` of the runtime SHA and an oracle stage exported with
`core.autocrlf=false` (`overlay_dirty_at_export=false`). Prior roots were only
read. Heavy steps held the shared mkdir lock with traps, at most eight jobs.

| Check | Result |
| --- | --- |
| Oracle audit and CTest | 70 compilations, 913 pinned exports, strict FP flags; **5/5 CTests** |
| Workspace fmt, clippy (warnings denied) and tests | Passed; **117** tests, zero failed or ignored |
| Release tests, `kicad-resistors-shim` and `kicad-difftest` | **24** passed, including the new process replay |
| New complete guards | 889/889 byte-identical C++; 889/889 Rust debug and release |
| Collector refusal script | Seven refusals and the empty result, all three programs |
| `requests.py` | **960** complete replies agree across C++, Rust debug and release |
| Corrected source validator | Valid, no errors, **1,801** records |
| Corpus regeneration | Byte-identical, 1,050 cases `58dd1186...` |
| Registered live check (debug shim, default 60-second budget) | **1,050/1,050**, zero deferred |
| Release `kicad-difftest compare` | **1,050/1,050**, zero deferred |
| Portable wasm builds | embeint-layout, embeint-web, kicad-resistors |

Phase 0 remains **INCOMPLETE, 0/3 live green layer-0 units**;
`PHASE0_REQUIRED=false`. The record stays claimed; no unit status, ownership or
graph field changed.

| Artifact | SHA-256 |
| --- | --- |
| Clean source archive | `6955dae57aa4ea61e164b4eb89f8949ff1f380abfad22cab6461d905129af0a4` |
| Oracle reference archive (LF) | `c962971d0e4b42174ea5932756f8a358a403aa20e3e464ba5231b9fc48e9a06e` |
| Source manifest | `00726fa79a76410def76c1da63bf52dad4e5e605c91089f95fa7006fc7da2f44` |
| Actual GCC 15.2 Release oracle | `b0b167cb40bfcb3b2f8fa6791f36742bdab8a560d791acccb875b105ce9b632f` |
| Rust debug shim | `a14a2fa7d54e81a547298d4f5d4a414a8b9c7b720138c32a5dd2e75d72c0d9b9` |
| Rust release shim | `e5c6ba211c2c72001396da3026636e60a175dbf3b05c13cb3cbdf6e6c21a7046` |
| Oracle provenance | `87e1c222c8884911690a6088c428804b8cebfc88ee671d333ed4d01c64c7f86d` |
| Golden / C++ 1,050 output | `8d1cc9e8c280b349294f9d512e450586c892a3170881afbba1c1b78de1a107a2` |
| Rust 1,050 output (debug and release) | `5fb5dfb6938e6a7b55c99416e46334b7e5f05ceb9821d053c442fa4dabc36441` |
| Private-key requests / C++ replies | `854ef879...` / `579853934d...` |

The oracle hash differs from the prior `582917da...` because the overlay changed
and the reference archive is now exported with LF line endings, as hosted Linux
CI does; the prior stage was exported through a CRLF-converting checkout. The
1,050 C++ outputs are identical. The source manifest is committed byte-exact
(CRLF, written by `prepare.py` on Windows) so it matches the provenance hash.
Rust 1.99.1.

The first proof attempt in `proof-ff419ad71f` passed every check but its final
summary step raised `NameError` (an ordering bug in the driver). It is kept
unchanged as [`failed-attempt-1.log`](resistors-private-keys-ff419ad71f/failed-attempt-1.log)
and is not proof. The full rerun `proof-ff419ad71f-r2` is the
[`proof.log`](resistors-private-keys-ff419ad71f/proof.log) above, driven by the
retained `proof.sh`. Summary: [JSON](resistors-private-keys-ff419ad71f.json);
reports, provenance, manifest and Rust replies are in the adjacent directory.

Compiler qualification remains **GCC 13/15 Release x86-64**. GCC O0 and Clang 18
differ on 234 NaN payload cases; compiler-independent NaN parity is not claimed.
Goldens, cases, immutable evidence, portable algorithms and the lockfile are
unchanged and nothing was reblessed. Final #39 graph acceptance, independent
review and hosted CI remain holds.
