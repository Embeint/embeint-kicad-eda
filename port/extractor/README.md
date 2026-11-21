# Phase 0 call graph and source accounting

The reference upstream is `4cf939778df04d2b747bb2c4714fc5014a7565c7`.
The source snapshot is the fork commit in `generated/inventory.json` provenance.
`~/kicad-live/build/compile_commands.json` and its build tree are read-only inputs.
Do not run a build there. Analysis runs against a separate source snapshot, using
the database's existing generated headers and unity files. A clang VFS overlay
remaps absolute source includes in those unity files to the snapshot.

The tools are GPL-3.0-or-later. Generation uses Python >=3.11 and the `clang==21.1.7`
Python package with libclang 21 (Apache-2.0 WITH LLVM-exception, GPL compatible).
Validation, splitting and drift use only Python's standard library and Git. No
new shipped or Rust dependency is introduced.

## Reproduce

First create a source manifest **with Windows Git** from the intended commit,
listing every tracked C/C++ source/header (`.c`, `.cc`, `.cpp`, `.cxx`, `.h`, `.hh`,
`.hpp`, `.hxx`, `.m`, `.mm`) as repository-relative paths. The JSON object has
`source_commit`, `upstream_pin` and `files` keys. Copy that exact checkout to a
separate WSL directory, preserving bytes; do not use WSL Git in a Windows worktree.
The committed manifest and provenance make the set reproducible.

```powershell
python port/extractor/source_manifest.py --output C:/Users/KarlVonRichter/.t3/phase0-source-manifest.json
```

```sh
python3 -m venv ~/kicad-port-extractor-venv
~/kicad-port-extractor-venv/bin/pip install clang==21.1.7
mkdir ~/kicad-port.build-lock  # serialize with the oracle/build worker
trap 'rmdir ~/kicad-port.build-lock' EXIT
~/kicad-port-extractor-venv/bin/python port/tools/extract_graph.py \
  --compile-commands ~/kicad-live/build/compile_commands.json \
  --source-root ~/kicad-port/extract-source \
  --original-source-root ~/kicad-live/src \
  --manifest ~/kicad-port/source-manifest.json \
  --cache ~/kicad-port/extract-cache --jobs 8
python3 port/tools/split_units.py
python3 port/tools/validate_units.py
python3 port/tools/drift_report.py --output drift.json
python3 -m unittest discover -s port/extractor -p 'test_*.py'
```

Use a script file for multi-step WSL calls from PowerShell; nested native command
quotes are not preserved reliably. The extractor rejects more than eight workers.
It does not compile/link KiCad or alter the build tree. Each command has a cached
result keyed by command, extractor and source inventory hashes. Successful and
failed commands, including worker errors, remain visible. A missing source or
unreadable manifest fails extraction instead of silently reducing its scope.
`--match` and `--limit` are diagnostic subsets and explicitly set
`compile_database_complete=false`.

## Outputs and limits

`generated/graph.json.gz` is deterministic gzip (mtime zero) of the full JSON graph.
The small inventory and splitter report remain plain `inventory.json` and
`units.json`. `extractor.model.read_json` accepts either `.json` or `.json.gz`.
The graph includes definitions keyed by clang USR, source ranges, normalized body
hashes, static/virtual/override edges, override relationships, unresolved call
sites, definition variants and per-command status/diagnostics/include provenance.
Definitions repeated in headers are deduplicated by USR; configuration variants
are recorded and their wx dependencies/calls are unioned conservatively. Primary
ranges are chosen deterministically by command identity. SHA-256 source hashes
normalize CRLF to LF so Windows and Linux checkouts validate identically; offsets
refer to the original snapshot bytes.
Source paths are lexically normalized before inventory matching. When libclang
returns a null extent for an abbreviated constrained template, its nearest valid
enclosing declaration supplies a conservative range, tagged
`range_kind=enclosing-declaration`; `definition_line` retains the declaration's
own location. Its fingerprint and drift scope cover that wider range. The
measurement reports how many definitions needed this recovery rather than
inventing token offsets or dropping them.
Separate executables can define the same USR at different source locations
(KiCad has three observed `main` bodies). Additional locations get stable
`@definition:<hash>` keys, retain the original `clang_usr`, and appear in
`definition_copies`. Calls and overrides conservatively reach every location;
each body therefore has a unit owner instead of disappearing behind one primary.

Virtual calls expand transitively to **all observed overrides**, including
uninstantiated derived classes, without restricting them to runtime reachability.
Explicitly qualified virtual calls are conservatively expanded too. Function
pointers, dependent calls without a clang target and implicit compiler-generated
calls are a stated analysis limit; unresolved expressions are retained. The
graph covers the active preprocessor branches in this Linux database. Other
platforms and inactive branches are inventoried, not claimed parsed.
Implicit template instances resolve to the primary body at the same source
location, so their static calls reach its owning unit. Explicit specializations
retain their distinct bodies. `template_aliases` preserves the instance-to-body
USR mapping. Missing compilation sources are reported without calling clang.

Per-TU caches accept both historical `.json` and compressed `.json.gz` records.
New entries publish atomically after compression. On a space-constrained host,
`python3 port/extractor/compact_cache.py --cache <cache> --output <audit.json>`
losslessly compresses complete entries older than 60 seconds, retaining the exact
original bytes and SHA-256 in the audit. Do not run an older reader afterward
without restoring the JSON companions or retaining its gzip-aware resume wrapper.

wx coupling is classified per definition and propagated through every edge
**before** cycle removal. The thin replacement whitelist in `model.py` includes
strings, paths, value types, configuration and streams; other observed wx types
are conservatively UI. Inherited wx bases and owned wx fields count too. This is
a dependency classification, not a proof of portability: files, sockets, threads,
global state and other non-wx coupling still require the porter's audit.

The inventory distinguishes application, generated, QA and third-party files.
Build-generated compilation inputs are separately tagged in translation-unit
records. Every physical line of the tracked application/generated sources, plus
Clipper2, belongs to a unit range. QA and other third-party files remain explicitly
inventoried: their library replacement/retention decisions are not implementation
units. No missing configuration or parse failure disappears from accounting.

Each unit targets 2,000 physical upstream lines; a function is never split, so a
single larger function or conservative enclosing range can exceed that. Units group directory, wx classification
and source category; math implementation files retain independent units to make
small foundation pilots possible. `est_lines` is the **measured physical range
sum**, including declarations/comments, rather than an estimated port effort or a
sum of overlapping template/inline function extents. IDs and order are stable for
identical inputs. Upstream edits may change subsequent split boundaries; existing
claims/status are preserved, and stale unit removals require reconciliation.

Cross-unit edge weight is ten for each distinct static/base virtual function pair
and one for each expanded override pair. The deterministic greedy feedback-arc
heuristic retains descending-weight edges unless they close a cycle. It is not
an optimal feedback-arc solution. Every dropped unit edge retains its weight and
original calls in `units.json`; those edges become `soft_depends_on`. Hard edges
form a DAG with leaves at zero and caller layer `1 + max(dependency layers)`.

## Unit schema and validation

The core schema follows solidcore at
`7a588fa284765a83823293ae20b84da8356ab479` (SolidCraft mirror trailer
`93d5492ef295a1124664d36e967f58a81a76d7d2`):

`id`, `upstream`, `rust_module`, `depends_on`, `soft_depends_on`, `est_lines`,
`layer`, `tests`, `status`, `claimed_by`, `claimed_at`, `notes`.

Extensions are `schema_version=1`, `wx_class` (`pure`, `thin-replacement`, `UI`),
`source_category`, `functions` (graph definition keys: clang USRs with the
documented suffix for additional same-USR bodies), `source_ranges` (inclusive
`path:start:end`) and `coverage_gaps` (source paths). Missing coverage is initially
`blocked`, otherwise status is `todo`. Supported port states are `todo`, `claimed`,
`partial`, `green`, `blocked`. Green requires recorded tests and no coverage gaps;
the goldens/oracle gate independently verifies the evidence. `rust_module` names
the intended module and does not claim an implementation already exists.
Units with unparsed source ranges are conservatively classified UI, so missing
platform coverage cannot inflate measured pure-line totals.

Run **`python port/tools/validate_units.py` from the repository root**, exit zero
only on valid records. It checks schema/types, references, ownership, measured
ranges, source hashes, classification, cycles/layers, and back-edge accounting.
`--json` is machine-readable. `--skip-source-hashes` validates historical records
after upstream edits; the drift tool then identifies re-checks.

Provenance separates `inventory_complete`, `parse_complete`,
`compile_database_complete`, `all_configurations_complete`, and `gate_complete`.
`gate_complete` is **only the extractor diagnostic** (all WSL commands parsed and
sources accounted for); `--require-complete` enforces it. It is not the plan's
phase-0 gate (three live oracle units green in CI and every unit record valid).
`all_configurations_complete=false` remains visible without adding an all-platform
phase-0 requirement. Parse failures make extraction exit nonzero and stay in the
provenance even if partial definitions are available.

## Raw database reconciliation and evidence

The original live database remains an independent diagnostic input. Keep its
hash, command statuses, diagnostics and graph even if a source has since been
deleted. A manifest- and configure-proven deletion is `stale-deleted/excluded`,
never a successful parse. Retain any historical cached parse failure as evidence;
do not ask clang to parse an absent file again. A missing source that is not
proven deleted remains a failure.

Regenerate the current compile database with **CMake configure only**, in a
private pinned source/build tree, copying options from the existing CMake cache.
`reconcile_database.py` records the exact options, configure command, source pin,
both database hashes, added sources and proven deletions. It does not edit the
raw database, invent commands for new sources or compile the application.
`prepare_generated_inputs.py` can then invoke only Ninja's actual
`CUSTOM_COMMAND` C/C++ source/header targets and their generator prerequisites in
the private tree, capped at eight jobs. It records its target list and log. This
is separate from configure and does not build/install KiCad applications or
libraries. Verify all generated compilation inputs exist afterward.

Run the reconciled database with a separate cache and output directory. Only
that measured graph generates the final unit records. Newly discovered sources
without successful coverage stay blocked. Preserve the raw output separately;
its coverage cannot be replaced with reconciled counts.

With the shared build lock held, for an existing byte-equivalent source snapshot:

```sh
python3 port/extractor/reconcile_database.py \
  --raw-build ~/kicad-live/build --raw-source ~/kicad-live/src \
  --source-root ~/kicad-port/extract-source \
  --build-root ~/kicad-port/extract-build \
  --manifest ~/kicad-port/source-manifest.json \
  --output ~/kicad-port/reconciliation
python3 port/extractor/prepare_generated_inputs.py \
  --build-root ~/kicad-port/extract-build \
  --output ~/kicad-port/reconciliation
~/kicad-port-extractor-venv/bin/python port/tools/extract_graph.py \
  --compile-commands ~/kicad-port/extract-build/compile_commands.json \
  --source-root ~/kicad-port/extract-source \
  --original-source-root ~/kicad-port/extract-source \
  --manifest ~/kicad-port/source-manifest.json \
  --cache ~/kicad-port/reconciled-cache \
  --output ~/kicad-port/reconciled-output --jobs 8
python3 port/tools/extraction_report.py \
  --raw-output ~/kicad-port/raw-output \
  --reconciled-output ~/kicad-port/reconciled-output \
  --reconciliation ~/kicad-port/reconciliation/reconciliation.json \
  --output port/extractor/generated/measurement.json
```

The measurement tool verifies database and inventory identities, reports raw and
reconciled coverage separately, measures layers/back-edges/wx spans and lists
covered pure L0 candidates with exact ranges. A candidate still needs a source,
type, portability and oracle audit before recommendation; wx-pure alone does not
exclude I/O, global state or platform coupling. Header-only declarations are
accounted physical lines, not evidence that their interfaces are portable.

## Drift

`drift_report.py` compares the upstream pin to a committed `--target` (default
HEAD), considering units marked `partial` or `green`. It maps extraction ranges
back to the pin before intersecting diff hunks, so fork edits already present at
extraction do not shift the comparison incorrectly. Deletions/renames, changes
outside recorded bodies, owned declarations and changed translation-unit include
inputs trigger conservative reviews. Direct changes and dependency reviews are
separate in JSON. An empty report with zero ported units proves no parity.
`--include-todo` audits the whole planned graph; `--fail-on-drift` makes affected
units nonzero, while tool/input errors return two. Working-tree edits and includes
in uncompiled configurations remain outside this committed-source comparison.
