# Exact KiCad unit difftest

This native harness feeds **the same case-file bytes** to two external programs,
checks their JSONL protocol, and compares every reply exactly. It uses filesystem,
processes and a bounded process timeout; it is not a portable engine crate.

From the repository root, after the foundation's west bootstrap:

```sh
cargo build --locked --manifest-path rust/Cargo.toml -p kicad-difftest
rust/target/debug/kicad-difftest compare --unit UNIT \
  --oracle /absolute/path/kicad-oracle --rust /absolute/path/unit-shim
rust/target/debug/kicad-difftest check --unit UNIT \
  --oracle /absolute/path/kicad-oracle --rust /absolute/path/unit-shim
```

`compare` checks live agreement. `check` also requires
`port/goldens/UNIT.jsonl`. Neither command writes a golden. `--root PATH` selects
another repository root. `--oracle-arg ARG` and `--rust-arg ARG` are repeatable;
arguments are passed directly without a shell. Programs must be executable file
paths, including when selected from PATH. `--timeout-secs N` defaults to 60 per
program. `--report PATH` records green, partial, failed or deferred status as JSON
with one outcome per case (see [Reports](#reports)); stdout repeats the counts
without the outcomes. By default every failure and deferral exits nonzero. `--partial` is the
explicit exception for units declared partial (see [Partial units](#partial-units)).
`--help` lists the commands.

## Protocol and exactness

Input is `port/cases/UNIT.jsonl`. Its requests have exactly three keys:

```json
{"unit":"UNIT","case":"unique-id","input":{}}
```

Each program reads stdin through EOF and emits exactly one reply per request,
in request order, with no diagnostics on stdout:

```json
{"unit":"UNIT","case":"unique-id","result":{"value":1}}
{"unit":"UNIT","case":"unique-id","error":{"kind":"unported","upstream":"function","unit":"dependency"}}
```

These are alternative replies. By default every `error` envelope fails the run.
`unported` and `deferred` are reported as deferred; other kinds are failures, including the
oracle's `invalid_request`, `invalid_input`, `unknown_unit`, `execution_error`.
Equal errors do not pass. An upstream operation's legitimate return/status value
belongs in `result`, where it is compared along with every other value. A nonzero
process exit fails even if stdout contains apparently correct results.

Missing, additional, duplicate, reordered and wrong-unit cases fail. Empty files,
blank lines, duplicate JSON object keys, invalid JSON and extra envelope keys fail.
The JSON must be UTF-8. JSON object order, insignificant whitespace and equivalent
string escapes do not affect equality. Array order and number **tokens** do:
`1`, `1.0`, `1e0`, `1E+0`, `-0`, `0`, `-0.0`, `0.0` are distinct. Integers and
decimals of any size are retained without conversion through `f64`. There are no
tolerances, numeric coercions, truncation, array summaries or ignored fields.

Bare NaN and Infinity are invalid JSON. The current oracle rejects non-finite
results with `execution_error`; those cases cannot pass or be blessed. If future
units need successful non-finite values, agree on explicit tagged JSON on both
sides (for example `{"$float":"nan"}` and signed infinity tags). The comparator
preserves those strings exactly and does not silently turn non-finites into null.

## Partial units

A unit with passing non-deferred cases and deferred cases is `partial`, never
`green`. `compare --partial` and `check --partial` report such a run without
weakening any comparison:

- Every case runs on both programs. Every oracle reply must be a `result`; an
  oracle error of any kind, including `unported`, fails. A case the Rust shim
  defers therefore always retains a real C++ reference outcome.
- Every Rust `result` is compared exactly with the oracle. Mismatches, identity
  faults, invalid JSON, nonzero exits, timeouts and non-deferral error kinds fail
  exactly as in the default mode.
- A Rust deferral must be exactly
  `{"kind":"unported"|"deferred","upstream":"<C++ symbol>","unit":"<owner id>"}`
  with a nonempty symbol and a valid unit ID. Any other shape is a failure.
- `check --partial` compares **every** oracle reply with the committed golden,
  including the replies of Rust-deferred cases, so a stale golden fails.
- At least one real exact comparison is required. All-deferred runs report
  `deferred` and exit nonzero: they prove no successful operation.

The report's `status` is `partial` (exit 0); its deferred outcomes carry `upstream`,
the owning `unit`, the oracle's reference reply and the Rust reply. With no
deferrals the status is `green`; `mode` stays `partial` so the caller still knows the
unit was declared partial. `--partial` is refused (exit 2) for `propose` and `bless`:
goldens follow a green, zero-deferral comparison, and no Rust error envelope or
omitted case is ever blessed. Without `--partial`, `compare` and `check` are unchanged.

CI passes `--partial` only for a unit whose record declares `status = "partial"`;
there is no global switch. See [Pilot registration and CI staging](#pilot-registration-and-ci-staging).

## Reports

Both programs always run, and every corpus case receives exactly one entry in
`outcomes`, in corpus order, whether or not the run succeeds. One fault therefore
never erases the outcomes of the other cases:

| `outcome` | Meaning | Extra fields |
| --- | --- | --- |
| `compared` | Rust's result equals the oracle's result exactly | `oracle` only in a failed run |
| `deferred` | well-formed Rust deferral; the oracle returned a result | `upstream`, `unit`, `oracle`, `rust` |
| `failed` | mismatch, error reply on either side (including an oracle deferral), malformed, misplaced or missing reply | `reason`, and `oracle`/`rust` lines when present |
| `unprocessed` | the program never started, or stopped (nonzero exit, timeout) before replying to this case | `reason`, and the other side's lines when present |

Each program's stdout is captured however the run ends. Replies written before a
nonzero exit or a timeout kill are still parsed, compared and checked against the
golden, and keep their reference lines; a truncated final line is a malformed
reply. The process fault itself (start failure, nonzero exit, timeout, non-UTF-8
stdout) is always listed in `run_faults` with up to 64 KiB of stderr and always
fails the run, so no recovered reply can make a faulted run pass. Timeouts still
kill and reap the program; stdout is never truncated.

Replies are aligned by position, as the protocol requires; a reply at the wrong
position fails the case expected there. Lines beyond the corpus are a run fault
with their count; the first 16 of each program are kept in `extra_replies` (side,
line number, raw text up to 4096 bytes, `truncated`). `cases`, `compared`, `deferred`, `failures` and `unprocessed` count
the outcomes. An accepted report lists compared cases by ID only, since the agreed
results equal the golden; a failed report includes every available oracle reply so
the reference results stay recoverable. Failures take precedence over deferrals in
`status`: `failed`, then `deferred` (all-deferred, or any deferral without
`--partial`), then `partial` or `green`. A deferral is never counted as a
success, and an error is never counted as a deferral.

`golden_checked` is `true` exactly for `check`. `golden.status` is `matched`,
`failed` (with `error` or `mismatched_cases` listing case, golden and oracle lines),
`incomplete` (with `unchecked_cases` whose oracle result was unavailable), or
`not_requested` for `compare`. Output hashes cover every captured stdout byte,
including output before a fault; they are null only when a program never ran.
The report is built from the final outcome of the action: if a green comparison is
followed by a failed `propose` or `bless` step (for example a missing
`--provenance`), that failure is a run fault and the report keeps the full
reference evidence of a failed run.
Only a corpus that cannot be read or validated leaves `outcomes` absent, with the
reason in `error`.

## Native shim request parsing

Native adapters should use `kicad_difftest::request::parse(line)` before reading
`unit` or `case`, replacing their separate `exact::parse` and
`serde_json::from_str::<Value>` calls. It returns `Result<serde_json::Value,
String>` with the existing arbitrary-precision representation, including integer
`-0` and the full `u64` domain. Envelope and unit-specific validation follow it.
On any parse error, reply with null identities and the returned message:

```json
{"unit":null,"case":null,"error":{"kind":"invalid_request","message":"invalid JSON or duplicate key"}}
```

This helper uses the exact parser for syntax, duplicate keys and bounded nesting,
then checks number tokens against the pinned nlohmann parser's finite `double`
fallback. For example, `1e1000` or a 400-digit integer anywhere in the request
fails before identity extraction, even in an unknown field. Finite integers
outside `i64`/`u64` remain parseable for subsequent domain validation. Subnormals,
underflow to zero and finite exponents also remain parseable. Conversion to `f64`
is validation only; it does not replace the returned numbers with rounded values.
Strings and object keys that look like numbers are unaffected. User objects are
constructed directly from the validated exact tree: keys such as
`$serde_json::private::Number` and `$serde_json::private::RawValue` remain ordinary
keys, including when their string values look like encoded JSON. They cannot
coerce an object into a number, string, array or envelope identity.

The explicit tree conversion retains the previous request recursion bound. At most
**127 simultaneously open objects/arrays** are accepted, counting the root and
empty containers. Scalars do not consume that budget. The C++ transport checks
both container-start events at nlohmann's zero-based depth 127, before extracting
identities. Excessive nesting anywhere (including identities and unknown fields)
therefore returns the same complete parse-error envelope above. This is native
transport validation, independent of a pilot's operation domain; the exact result
comparator retains its existing separate bound and numeric-token semantics.

For the MD5 wire shape with `input.extra` containing `n` arrays around `0`, the
root and `input` objects consume two levels: `n=125` parses and `n=126` is refused.
The sibling `steps` array does not add to that path's depth. Below the bound,
normal envelope and adapter validation still run and retain parsed identities.

`exact::parse`, `records` and comparison still retain arbitrary-precision tokens,
including overflow tokens; they do not use the request helper. This native I/O
crate is not a portable engine dependency. Pilot adapter adoption is separate
from adding this shared API.

`cargo test --locked --manifest-path rust/Cargo.toml -p kicad-difftest -j4` includes
a small C++ fixture compiled against the repository's nlohmann header and actual
`port/oracle/protocol.cpp`. It requires a GCC/Clang-compatible C++17 compiler
(`CXX`, default `c++`), without any KiCad library build. It checks parse acceptance,
null-identity error envelopes, integer/float zero classification, integer fallback,
underflow and both sides of the exact binary64 overflow rounding midpoint.
Nesting regressions compare complete identities, error kinds and messages across
object, array and alternating roots; scalar and empty-container leaves; input,
identity and unknown fields; identity order; siblings and escaped duplicate keys.
The MD5 reproducer's wire shape is tested against the real transport with an empty
registry, so accepted requests produce `unknown_unit` here. This fixture does not
execute MD5 or establish pilot-operation coverage.

## Reviewed golden updates

First build the live oracle using `port/oracle/README.md`. Capture its provenance:

```sh
/absolute/path/kicad-oracle --provenance > oracle-provenance.json
rust/target/debug/kicad-difftest propose --unit UNIT \
  --oracle /absolute/path/kicad-oracle --rust /absolute/path/unit-shim \
  --provenance oracle-provenance.json --approval proposal.json
# Inspect every change, fill proposal.json's nonempty reason, add visual evidence
# to provenance when applicable, then propose again if provenance changed.
mkdir -p port/goldens/evidence
rust/target/debug/kicad-difftest bless --unit UNIT \
  --oracle /absolute/path/kicad-oracle --rust /absolute/path/unit-shim \
  --provenance oracle-provenance.json --approval proposal.json \
  --evidence port/goldens/evidence/UNIT-baseline.json
```

`propose` runs live difftest before writing a new approval file. Both `propose`
and `bless` also invoke the oracle with its supplied arguments plus `--provenance`
and require the supplied build record to match that live output. Evidence includes
case IDs, old/new complete reply lines as strings (preserving numbers), old/new
indexes, case/golden SHA-256 hashes, both executable hashes and arguments, and
the supplied oracle build record. The first baseline records null old values and
oracle provenance too. The oracle record should contain its pinned source hashes,
overlay identity, compiler and build/compile/link commands and flags; use the
oracle's own `--provenance`, rather than an invented record.

`bless` reruns both programs and rejects a stale or incomplete approval. It writes
a new evidence artifact durably before replacing the golden. Existing evidence
files are never overwritten. Unchanged replies keep their existing text. Commit
the evidence together with the golden; describe case IDs, old/new values and
reason in the PR, and attach before/after pictures for visual results. No first
baseline or changed golden is permitted before live oracle agreement. Evidence
is a review artifact, not cryptographic attestation of who approved it.

CI's `python port/tools/check_goldens_policy.py --base BASE` requires changed
goldens to have newly added matching evidence files. It checks the exact base
and head values and case corpus, including added/removed/reordered cases. Whole
unit retirement is an explicit review decision; this tool refuses golden-file
deletion. With more than one local blessing, consolidate evidence against the PR
base before publication. Golden history does not establish current correctness;
CI reruns the live oracle on every registered case.

Keep case, golden and evidence files as LF UTF-8. Newline conversion after a
blessing changes the recorded byte hashes and requires a new approval. Committed
evidence is immutable: add a new artifact for a subsequent blessing.

## Pilot registration and CI staging

Keep the solidcore unit schema intact. The optional unit-record extension below
selects a native shim without hardcoded pilot names:

```toml
[difftest]
rust_package = "my-unit-shim"
rust_binary = "my-unit-shim"
rust_args = [] # optional direct arguments, in order
```

Its corpus and golden paths follow the unit filename. Each case file must have a
valid generated unit record and this registration. Every registered corpus runs
live against the freshly built own-library oracle and its committed golden;
deferrals fail, and a green unit without a corpus fails. Orphan goldens fail.

The one exception is a record declaring `status = "partial"`: it runs with
`--partial`, as `check` when its golden exists and otherwise as live `compare`
(goldens follow a green difftest). Green, claimed, todo and blocked records always
run strict `check`, so a green unit stays zero-deferral. The policy then checks the
runner's report rather than trusting the exit code alone. It parses counts without
numeric coercion: a fraction, exponent, negative or non-canonical integer, boolean,
`NaN` or `Infinity` fails, so `1e-400`, `0.0` or `true` cannot pass as a count. It
then requires that unit, mode and action match; `golden_checked` is the boolean for
the action and `golden` is `matched` for `check` or `not_requested` for `compare`;
zero failed or unprocessed cases, no run faults and no extra replies; outcomes for
every corpus case exactly once, in order, only `compared` or `deferred`, with
matching counts; at least one compared case; a status that agrees with the
deferrals; and every deferral owned by an existing unit record, carrying the
oracle's result reply for that case and, as `rust`, a string that parses exactly
(no duplicate keys, `NaN` or trailing data) to the deferral envelope for the same
unit and case, with only `kind` (`unported` or `deferred`), `upstream` and `unit`,
whose upstream and owner equal the reported outcome's. An error of another kind,
a result, another case's or owner's reply, or missing or mistyped evidence fails. A previous report is deleted before each run.
The shim should be a separate native crate; mark it in its Cargo manifest:

```toml
[package.metadata.kicad]
portable = false
```

`--wasm` discovers every workspace crate as portable by default, except the four
existing native adapters/harnesses listed in the policy script and crates with
the explicit native metadata above. That includes `embeint-layout`,
`embeint-web`, and future pilot libraries. No pilot library may opt out.

The workflow runs on PRs targeting `rust` and pushes to `rust`. It performs the
foundation bootstrap and explicit `west update` before Cargo when the foundation
is present. An infrastructure checkout before that dependency lands reports the
missing foundation and uses no external dependency. It runs workspace fmt,
clippy with warnings denied, tests, every portable crate's wasm build, golden
review policy, default `validate_units.py`, and all available live cases. When
the oracle exists, it builds only pinned KiCad libraries, using the oracle
worker's Ubuntu 24.04 contract and capped jobs. There is no whole-KiCad build or
compiled-result cache. Provenance and per-unit reports are uploaded even on failure.

`difftest-reports/phase0.json` adds `live_partial_units`, `partial_deferred_cases`,
`partial_units_without_deferrals` (declared partial, but its corpus deferred
nothing: audit before promoting) and per-unit `units` entries with the declared
status, mode, action, result and counts. Partial units never count towards
`live_green_units` or the phase-0 gate.

`PHASE0_REQUIRED=false` is the bootstrap state: successful infrastructure checks
summarize **phase0 INCOMPLETE**, including when no pilots exist. The final
third-pilot/integration PR must set it to `true`, which enables
`--require-phase0`. The strict gate requires at least three green units with live
oracle/golden checks and all generated valid unit records with exhaustive source
accounting. Record validity does not imply complete parsing/configuration
coverage. All extractor provenance flags and gaps are reported separately;
`all_configurations_complete=false` stays visible and is not silently added as a
phase-0 requirement. The extractor's separate `--require-complete` mode remains
available for the broader extraction qualification.

## Validation and dependencies

```sh
cargo fmt --manifest-path rust/Cargo.toml -p kicad-difftest --check
cargo clippy --locked --manifest-path rust/Cargo.toml -p kicad-difftest --all-targets -- -D warnings
cargo test --locked --manifest-path rust/Cargo.toml -p kicad-difftest
python -m unittest discover -s rust/kicad-difftest/tests -p test_policy.py -v
```

The native process-fault tests use Python 3 to inject crashes, timeouts, malformed
output, mismatches, identity faults, errors and deferrals, whole-run or per case
(`tests/peer.py`). `tests/partial.rs` covers partial reporting; the policy suite
also feeds reports of the built runner, when present, to the CI report check. Python is not a runtime
dependency of the Rust harness. Tests cover numeric tokens beyond `f64`, negative
zero/non-finite handling, missing/duplicate/changed goldens, reviewed baseline
evidence, portable-crate discovery and non-vacuous staging.

This crate is GPL-3.0-or-later. It reuses existing locked workspace versions:
serde 1.0.229, serde_json 1.0.154, sha2 0.10.9 and tempfile 3.27.0, all
MIT OR Apache-2.0 and GPL-compatible. `raw_value` adds no new package. Their
transitive dependencies are already recorded in the workspace lockfile and
`rust/MCP.md`; this native development harness is not part of the shipped engine.
