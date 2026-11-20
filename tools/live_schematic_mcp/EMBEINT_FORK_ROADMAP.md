# Embeint KiCad fork: native MCP, Rust and WebAssembly

Status: project engineering direction, 2026-11-18. This document records the
architecture and delivery gates; it does not implement a new distribution or Rust
runtime. Estimates below are planning ranges, not measured delivery commitments.

## Decision

Priority update, 2026-11-20: the initial Rust MCP slice exists. The next milestone is
complete native Rust parity with the Python MCP, plus an explicit inventory and
implementation of remaining KiCad function gaps, before browser work. Python
forwarding is not an accepted replacement for native implementations. See the
[native parity plan](../../rust/MCP_PARITY_PLAN.md) for current gaps and acceptance
criteria. The original assessment and estimates below are historical context.

Maintain an Embeint distribution of this fork, with live AI schematic generation
as a supported workflow. Keep KiCad's desktop editors and electrical engines while
introducing Rust behind explicit interfaces. Compile suitable Rust components to
WebAssembly for a browser client.

The first product milestone is an installable desktop application with bundled MCP,
reliable live edits, and tested project compatibility. A browser editor follows once
the shared command and document interfaces have proved useful on desktop.

## What exists today

The checkout already contains a personal fork with an `origin` remote at
`Karl-VonRichter/kicad-source-mirror` and an upstream KiCad remote. The agent guide
identifies `live-schematic-ipc` as the integration branch; this assessment was made
on topic branch `page-layout-check`, commit `3c05b0ecba`.

- C++ handlers extend the schematic, PCB, symbol and footprint editors. Commands
  are declared under `api/proto/` and execute against the live editor models.
- `server.py` exposes MCP over stdio through Python FastMCP. The clients translate
  tools to protobuf messages over KiCad's local NNG IPC transport.
- `client.py` groups supported item edits with `BeginCommit` / `EndCommit`, requests
  rollback on failure and deliberately avoids retrying mutations after timeouts.
- Live netlists, ERC, PCB DRC, hierarchy, library operations, rendering and explicit
  saves provide a useful generation-and-check loop. Some settings changes are
  explicitly outside undo; transactional guarantees must be declared per command.
- WSL scripts build, install, launch and test the development application. The
  scheduled upstream sync merges source changes without building or testing them.
- `cmake/KiCadVersion.cmake` identifies this checkout as `10.99.0-unknown`. It is a
  development-line fork, rather than a demonstrated KiCad 10.0.6-compatible release.

This is a substantial foundation. It is not yet evidence of a supported Embeint
installer, a Rust engine, browser execution, or release-qualified reliability.

## Boundaries to preserve

### Native MCP as part of the product

Ship an `embeint-kicad-mcp` executable with the application. Initially it can package
the existing server; later a Rust implementation can replace tools incrementally.
The client launches it over stdio and it discovers the intended local editor
instance. Shipping MCP as a supported application component removes manual Python
setup without putting JSON-RPC processing on the editor's UI thread.

Keep tool schemas, capability negotiation and structured errors separate from
editor implementation details. Preserve existing `live_*` tools during migration.
Expose the actual application version, supported commands, schema version, active
document and transaction limitations; a successful connection alone is insufficient.
Keep model-provider integration outside the editing engine so different assistants
can use the same application.

The current `kicad_token` identifies a running instance; the protocol comments do
not establish it as authentication. Productize local endpoint permissions and
instance selection explicitly. A remote browser gateway needs its own authenticated
session and project access boundary.

### One authoritative document and command path

Desktop KiCad remains the document authority during the first migration stages.
Rust planning components consume snapshots and return proposed operations. C++
adapters apply those operations through existing handlers and commits, so manual
edits and AI edits share connectivity, undo history and validation.

Define an operation contract with document identity, expected revision, stable
item UUIDs, typed units, transaction scope and structured diagnostics. Expected
revision checks and operation-result lookup are proposed additions, not current
protocol guarantees. A timed-out mutation has an unknown outcome until reconciled;
clients must not blindly resend it. Start with one writer per document. Treat a
cross-editor operation as multiple explicit stages until coordinated rollback is
implemented and tested.

The internal coordinate representation should retain integer nanometres. Use a
lossless encoding at JSON/browser boundaries; JavaScript numbers cannot represent
every 64-bit integer. Human-facing tools may accept millimetres with explicit
conversion and bounds checks.

### Rust components that can also run in a browser

Start with deterministic, UI-independent functions. `symbol_layout.py` is a useful
candidate for the first schematic-oriented port. `board_rf.py` is a small separate
numerical candidate. Routing is a later candidate: `board_router.py` depends on
Shapely/GEOS and NumPy, so moving it requires a geometry backend decision and
clearance-equivalence tests, not just translating its A* loop.

Proposed package responsibilities, to be added when implementation begins:

- `embeint-design`: typed design snapshots, operations and diagnostics.
- `embeint-layout`: symbol placement and schematic block layout algorithms.
- `embeint-ipc`: native protobuf/NNG adapter to the current fork.
- `embeint-mcp`: native MCP executable using the shared operation contract.
- `embeint-web`: WebAssembly bindings for the portable components.

Keep filesystem access, sockets, process launching and GUI types out of portable
crates. Use the existing IPC boundary first; only introduce a narrow C ABI where
profiling or editor integration justifies in-process Rust. Such an ABI needs
explicit ownership, error handling and panic containment.

### WebAssembly and the browser application

First deliver a browser viewer and layout preview that run portable Rust locally.
A subsequent editing client needs rendering, selection, text and font handling,
hierarchy navigation, undo, persistence and library access in addition to algorithms.

A normal browser cannot connect directly to KiCad's local NNG IPC endpoint. During
the hybrid phase, use an authenticated application gateway to desktop KiCad and
keep native ERC, DRC and exports there. Run expensive WASM work in a Web Worker.
A browser-only product needs replacements or supported WASM builds for each native
dependency and an independently validated document engine. WASM compilation alone
does not port wxWidgets, the PCB editor or the manufacturing pipeline.

## Delivery gates

1. **Release foundation.** Record the upstream base and fork patch set. Choose the
   first supported host platform, provide an installer and isolated configuration,
   and bundle MCP startup and diagnostics. Keep automatic upstream integration on
   a development branch; promote a tested commit to a release branch. A clean
   machine must install, open a disposable project, connect MCP, edit, undo, save,
   close and reopen without a development checkout or user-installed Python.

2. **Reliable schematic generation.** Version the operation contract and expose
   capabilities. Exercise hierarchical sheet creation, symbols, pin-to-pin wiring,
   annotations, layout checks and ERC against disposable fixtures. Accept a generated
   example only when its intended netlist matches, page checks pass, validation
   findings are reviewed, and save/reopen preserves the design. Test failed batches,
   editor disconnects and concurrent manual edits. Preserve explicit save behavior.

3. **First shared Rust component.** Port one symbol-layout operation with the same
   inputs and outputs. Run fixed reference examples against Python, native Rust and
   WASM, including malformed inputs and multi-unit symbols. Measure correctness and
   responsiveness before making it the default. Retain the previous implementation
   until parity is established.

4. **Bundled Rust MCP.** Port discovery and read-only tools first, then one complete
   edit transaction. Compare old and new clients against the same protocol fixtures
   and disposable live projects. Verify rollback, timeout reconciliation and schema
   compatibility before migrating the remaining tools. Complete all Python tools,
   resources and generic commands natively, qualify without Python installed, and
   audit and close broader KiCad function gaps before treating this gate as complete.

5. **Browser preview, then editing.** Reuse the Rust layout component in a small
   browser viewer before building interaction and persistence. Demonstrate a browser
   edit reaching the live desktop model through the gateway, with revision checks
   and undo, before attempting an independent browser document engine.

## Compatibility and maintenance

Treat application version, IPC schema and each on-disk format version as separate
contracts. Identify the actual version headers in reference KiCad 10.0.6 files;
do not infer them from the release number. Test opening copies in the fork and
round-tripping them through the exact target release. Include hierarchy, embedded
symbols and models, rules, library tables and relevant project settings.

A development build may write files that an older KiCad cannot read. Publish the
tested compatibility matrix and explicitly report unsupported downgrade cases.
Avoid changing native file formats in the first release. A later Rust parser/writer
needs fixtures for unknown fields, UUIDs, units, hierarchy and unsupported features
before it can replace KiCad's serializers.

Keep native KiCad ERC/DRC as acceptance checks while replacement systems develop.
Passing them establishes rule compliance within configured checks, not complete
electrical design correctness. Preserve renders and expected connectivity alongside
test fixtures so both readability and connectivity regressions can be reviewed.

Use dedicated build infrastructure or sufficiently capable runners for full builds;
the current workflow does not provide that validation. Run protocol and portable
Rust checks separately from slower application integration tests. Upgrade upstream
on an explicit cadence, with a release owner, reproducible build metadata and a
known-good rollback version.

The combined KiCad distribution is governed by GPLv3-or-later, as described in
`LICENSE.README`. Supply the corresponding source and notices for distributed
builds, including compatible Rust additions and dependencies. The project is named
Embeint KiCad and is maintained at
[Embeint/embeint-kicad-eda](https://github.com/Embeint/embeint-kicad-eda). Application
branding and distribution packaging remain implementation work. Keep this fork's
work within its own development and contribution channels.

## Effort and first implementation slice

With one experienced engineer working full time and building on the present code,
allow roughly 4–8 weeks for a one-platform internal desktop distribution with
bundled MCP and a tested schematic workflow. A supported multi-platform release
will take additional packaging, signing and integration work. Budget another 2–4
weeks for a deliberately small Rust/WASM layout proof after its input contract is
fixed; a useful browser schematic editor is more plausibly a 6–12+ month project
for a small team. Broad KiCad engine replacement is a multi-year program. These
ranges depend strongly on scope, fixture coverage and the state of the current
editor extensions; this assessment did not run a reliability audit.

Start with the release foundation and a disposable hierarchical schematic fixture.
Once that workflow is repeatable, implement the symbol-layout Rust/WASM proof.
That sequence gives Embeint a usable maintained application while testing the
proposed shared-engine architecture on a bounded piece of real functionality.
