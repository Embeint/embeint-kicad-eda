# Agent rules for the Rust port

Read this entire file, the root `AGENTS.md`, `CONTRIBUTING.md`,
`tools/live_schematic_mcp/AGENTS.md`, `rust/PORT_PLAN.md`, `rust/MCP.md` and
`rust/MCP_PARITY_PLAN.md` before working. The plan's method, oracles, phase gates,
external-code decisions and public-data rule govern this port. Read the owned
unit, its pinned upstream sources, fixtures and existing findings before editing.

## Scope and references

Port topic branches start from `rust` and PRs target `rust`. `main` continues the
product and native MCP work; upstream reaches `rust` through `main`. Never commit
to `master`, force-push shared branches, or regard an automatic merge as tested.
The root branch-rule documentation is maintained separately; do not change it
incidentally in a port PR. Do not send issues, comments, merge requests or any
other material to upstream KiCad on the user's behalf.

Use `port/pins.toml` as the canonical revision record and follow `port/README.md`
for west setup. solidcore is obtained at a full commit, never a moving branch.
Its commit's `solidcraft:` trailer identifies the SolidCraft source revision.
Fixes to solidcore originate in SolidCraft's `solidcore/` subtree and flow through
the mirror, not PRs to the standalone mirror. Such external publication requires
its own authorization. Do not edit ignored dependencies as a substitute for a
reviewed pin change. Prefer existing Rust crates over reimplementing libraries;
record dependencies and GPL-compatible licences.

The workflow is adapted from the pinned solidcore unit schema, porting checklist,
shims, goldens and progress generator. Its current tolerance and changed-result
correctness policies do not apply here. KiCad C++ remains the behavioural oracle.
Headless results must match exactly, byte for byte and bit for bit. Do not add a
tolerance, copy solidcore's `1e-9` rule, or silently replace a reference result with
a supposedly better result. Record a suspected reference defect for review; a
confirmed defect is not an automatic comparison exemption. Later UI/render
acceptance criteria require their own stated plan gate; they cannot weaken the
phase 0 unit comparison.

## Claims, ownership and records

Claim a unit with a scoped draft PR titled exactly `port: <unit>`. Search open
claims by symbols and paths as well as title. The earliest open overlapping claim
wins. State owned functions, edit paths, dependencies, evidence and remaining work
in the PR. Agree boundaries before changing another worker's functions or shared
interfaces; take independent work while that is unresolved. Do not reserve work
through edits to a central claim list.

Edit only owned `port/units/<id>.toml` records and implementation/runner files
agreed in the claim. The solidcore core schema is retained as top-level keys:
`id`, `upstream`, `rust_module`, `depends_on`, `soft_depends_on`, `est_lines`,
`layer`, `tests`, `status`, `claimed_by`, `claimed_at`, `notes`. The filename stem
equals `id`; upstream paths are relative to the pinned KiCad tree. Port statuses
are `todo`, `claimed`, `partial`, `green`, and `blocked`. Tools read units in sorted
ID order. The extractor documents its extensions: `schema_version = 1`, `wx_class`
(`pure`, `thin-replacement`, `UI`), `source_category` (`application`, `generated`,
`thirdparty`), `functions` (clang USRs), `source_ranges` (inclusive 1-based
`path:start:end`) and `coverage_gaps` (paths). Preserve core fields and meanings;
document additional extensions with their owning tool. A claimed record requires
an owner and timestamp; green requires nonempty tests and no coverage gaps.

Run `python port/tools/validate_units.py` once the extractor is integrated. Default
validation can accept an honestly incomplete graph; `--require-complete` also
rejects incomplete WSL command processing or parsing. Preserve separate provenance
flags for inventory, parsing, compile database and all configurations. A Linux-only
graph has `all_configurations_complete = false`; `gate_complete` is the WSL
extractor completeness diagnostic, separate from the plan's phase 0 gate.
Uncompiled application sources remain accounted for by blocked records. Valid
TOML and a green pilot do not establish all-configuration completeness.

Each finding is a new `port/findings/<slug>.md`; each reference defect is a new
`port/upstream-issues/<slug>.md` with location, trigger, reproduction, evidence and
confirmation state. A merged record is immutable: add an addendum file for an
update. Never create shared append files such as `findings.md`, `upstream-issues.md`
or a shared registry/log. Cases and goldens are per unit. Generate progress from
the records; do not hand-edit generated reports or count stored goldens as current
CI evidence.

## Faithful implementation and deferrals

Port every upstream branch, default argument, parameter, special case, status and
operation order. Coordinates remain integer nanometres. Match the oracle's fixed
floating-point flags: `-ffp-contract=off`, no fast-math; preserve expression order
and avoid reassociation or explicit fused multiply-add that changes results.

A missing dependency call returns `Err(Unported { upstream, unit })` and carries
`// UNPORTED(<unit>): <C++ symbol>`. The tag names the actual owning unit. Never
fake a result, panic, or turn a failure into an empty/default success. The stub
replaces only the unavailable call: implement the upstream continuation and
surrounding algorithm too. Name any deliberately missing owned body or continuation
in the PR, even when existing fixtures cannot reach it.

`depends_on` units that are `green` or `partial` satisfy scheduling;
`soft_depends_on` and recorded cycle back-edges do not block a claim. This does not
make an unavailable function implemented: inspect the callee, use a real call
where available, and retain explicit deferrals elsewhere. Coordinate minimum type
co-ports with the owner and record their symbols and Rust paths in that unit's
notes. When wiring a dependency, find its `UNPORTED` callers, rerun them and add
cases that reach their previously blocked continuations.

A case reaching `Unported` is **deferred**, distinct from mismatches, harness
failures and legitimate upstream error statuses. A unit with passing non-deferred
cases and any deferred cases is `partial`, never `green`. All-deferred cases prove
no successful operation. `green` requires nonempty checked evidence and zero
deferrals or failures; it does not itself prove every branch has coverage. A
blocked gate is `blocked` with a per-entry finding and a concrete next step.

## Cases, protocol and goldens

Only public test data may enter the corpus, cases, goldens or site: `qa/data`, demos,
KiCad's official libraries at recorded pins, and fixed-seed generated cases. No
Embeint design or customer data is allowed. Keep tests on disposable projects.

Per-unit input is `port/cases/<id>.jsonl`; expected output is
`port/goldens/<id>.jsonl`. The shared shim protocol is one JSON object per line:

```json
{"unit":"<id>","case":"<case-id>","input":{}}
{"unit":"<id>","case":"<case-id>","result":{}}
{"unit":"<id>","case":"<case-id>","error":{"kind":"unported","upstream":"<C++ symbol>","unit":"<callee-id>"}}
```

The last two lines illustrate alternative replies. Keep stdout for protocol data
and diagnostics on stderr. Compare exact values, statuses, ordering and case
identities; no tolerance or dropping cases. Failure is distinct from deferral.
Every case runs on the C++ oracle and Rust shim. Goldens follow successful exact
comparison. A golden proves unchanged output, not correctness or completeness.
Never bless a mismatch to make CI green. A justified golden change lists case IDs,
old and new values, the reference evidence and reason, with before/after pictures
where the result is visual. Never relax comparison or hide a broken case.

## Portable code and architecture

Portable crates build for `wasm32-unknown-unknown`: no filesystem, sockets,
processes, threads or GUI types. Hosts supply I/O and execution. Use
`#![forbid(unsafe_code)]`; no `unwrap`, `expect`, `panic!`, `todo!` or
`unimplemented!`. Use checked arithmetic and input bounds, typed errors and bounded
input-driven recursion. Avoid panic-prone indexing/allocation from untrusted
counts. No global, mutable static or thread-local state. Results must be
deterministic, independent of clocks, addresses, hash iteration or earlier calls;
use stable ordering and fixed-seed generators. The same input must yield the same
native and wasm result. Do not apply portable restrictions to native harness/host
I/O by pretending that code is portable.

Engine commands own edits and undo. UI holds no document state and mutates only
through those commands. Preserve existing source comments unless directly affected;
add comments for nontrivial reasoning. No emojis. Never manually edit generated
files, including `_base.cpp/.h`; `.wxfb` edits must be resaved in wxFormBuilder.
Contributions are GPL-3.0-or-later and reused code must be compatible.

## Before marking ready

Keep the PR draft until these checks and a separate review are complete. Phase 0
foundation and infrastructure do not claim the three-pilot gate by themselves.

1. Audit every owned upstream function and branch, including code after deferred
   calls. List exact missing symbols/continuations and affected callers.
2. Check every default argument, dropped or hard-coded parameter, special case,
   return/status handling and mutation order against the pinned C++.
3. Ensure the runner constructs every actual kind/type the implementation branches
   on. Stand-ins and skipped/absent entities do not cover successful operations.
4. Check input contracts against the reference's defined domain. Exercise genuine
   successful results as well as errors/no-ops; do not broaden guards to hide a gap.
5. Generate degenerate, boundary, tolerance-limit, huge/tiny and sub-domain cases
   with fixed seeds. Check observable values/state, not only success statuses.
6. Bound extreme counts, derivative orders, file-header allocations and recursion;
   verify no panic on caller-controlled input and preserve reference behaviour for
   valid input.
7. Check `UNPORTED` tags and ownership, deferred/failure counts, dependency wiring,
   co-port records and the unit's honest `partial`/`green` status. Deferrals never
   become green through exclusion or blessing.
8. Run fmt, clippy with warnings denied, relevant tests, portable wasm checks,
   exact difftests and affected goldens. Include the tested commit, commands,
   case IDs, result counts and unresolved failures in the PR. CI is the merge gate.
9. Provide an evidence table: operation/upstream symbols; implemented body/branches;
   successful operation and observable result checked; remaining gap. Distinguish
   successful work, legitimate refusal/no-op and blocked/harness-limited paths.
   Rerun callers after dependency wiring and keep newly exposed failures visible.

## Machine and Git rules

On the shared WSL host cap compiles at `-j8` (including `ninja -j8`). Serialize
heavy builds with the directory lock `~/kicad-port.build-lock`: acquire with
`mkdir`, release only your own acquired lock, and do not remove another worker's.
`~/kicad-live` belongs to other work: read its `build/compile_commands.json` only;
never build there. Build the oracle in its own Linux tree from an rsync copy,
not `/mnt/c`. Install west in a venv. Do not install over `C:\kb\install` or stop
a KiCad you did not start. Stop only owned processes by PID.

Run Git for a Windows worktree from Windows, never WSL. When invoking WSL from
PowerShell use script files for multi-step commands so native argument quoting
does not strip inner quotes. Git Bash rewrites leading `/mnt/c` arguments; use
PowerShell for those paths. Never use `pkill -f` (it can kill its own shell).
Shell scripts keep LF endings.

Stage explicit owned paths only. Author commits as
`Karl Von Richter <karl@embeint.com>` (check `git config user.email`, use `--author`
if needed). Subject is `area(type): summary`, plain English and lower case after
the colon; the body explains why. Required trailers, in this order:

```text
Assisted-by: Codex
Signed-off-by: Karl Von Richter <karl@embeint.com>
```

For example: `port(feat): pin the phase zero references`. Authorized publication
is to the Embeint fork only. Leave draft PRs against `rust` for parent review;
never merge or watch them unless separately instructed. Stop and report changes
to plan decisions, money or repository settings (except explicitly authorized
Pages enablement). Do not delegate unless explicitly assigned to coordinate.
