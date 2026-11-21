# Native Rust MCP parity and KiCad function coverage

Priority updated 2026-11-20: complete native Rust MCP parity before browser work.
Every supported operation must execute through Rust and the KiCad C++ engine.
Python remains a development reference for comparison; forwarding tools to a Python
server, embedding Python, or launching Python helpers does not satisfy this target.
KiCad's native engines and CLI may remain execution dependencies.

## Current state (2026-11-21)

Every Python tool, resource, resource template and generic command is implemented natively:
`surface_coverage.py --require-complete` passes (116 tools, 2 resources, 1 template, 147
commands; nothing missing or different) and all 641 offline fixtures compare identical on
Windows and in CI on Linux x86_64 and arm64. The remaining 97 tools were ported in six areas
(PRs #25 to #31); [MCP.md](MCP.md) records the results, the known differences and the
dependencies.

Against the completion criteria below:

1. **Python MCP parity**: met for advertisement and for every scripted scenario; the known
   differences are listed in MCP.md.
2. **Native execution**: the native server runs no Python. The installed package still ships the
   Python server beside it and has not been qualified with Python absent.
3. **Behavioural evidence per operation**: offline, every tool has fixtures (success, refusals,
   KiCad errors, lost replies and reconciliation for edits); live evidence is still limited to
   the first 19 tools and the manufacturing tools.
4. **Live workflow qualification**: in progress across every area, on a disposable project.
5. **All KiCad functions have a coverage decision**: not started beyond the IPC operation audit.

The previous inventory (2026-11-20, 19 of 116 tools) is in the git history of this file.

## Completion criteria

1. **Python MCP parity is the minimum.** Preserve every tool name, input and output
   schema, default, unit, validation rule, result content type and error contract.
   Include resources, resource templates, prompts if added, bundled skill content
   and hashes, generic command discovery/execution, and batch semantics. Additional
   Rust functionality is allowed. Inventory equality alone is insufficient.
2. **Native execution throughout.** Port geometry, routing, generators, layout,
   impedance calculations, process management and resource delivery as needed.
   Qualify the installed Rust server without Python or the Python MCP bundle
   available. Preserve dependency licences and package provenance.
3. **Behavioral evidence per operation.** Compare protocol requests and responses,
   validation failures, document identity and unsupported-editor behavior. Mutations
   need undo, rollback, partial-failure, revision, disconnect and timeout-reconciliation
   coverage. Never blindly resend a timed-out edit. Keep saves explicit and do not
   claim atomic rollback for settings or filesystem changes outside editor commits.
4. **Live workflow qualification.** Exercise hierarchical schematics, connectivity,
   ERC, library editing, schematic-to-board transfer, placement, routing, rules,
   zones, DRC, rendering, 3D, exports, and save/reopen on disposable projects.
   Record the source revision, application build, schema fingerprint and host.
   Qualify Windows and Linux against the actual shipped application and package.
5. **All KiCad functions have an explicit coverage decision.** Extend the feature
   inventory beyond existing IPC registrations, using menu/tool actions, CLI jobs,
   editor settings and workflows. Include the 3D viewer, GerbView, drawing-sheet
   editor, image converter, calculator, footprint assignment and remaining project
   management workflows. Map each function to a tested Rust tool, identify a missing
   C++ IPC handler, or record why an internal/interactive action needs a different
   automation contract. Deferred features remain gaps; they do not count as supported.
   An unqualified claim of all-function support requires closing those gaps.

## Decisions (2026-11-21)

- **Python libraries are replaced by existing Rust crates, not re-implemented.** shapely (GEOS)
  becomes `geo` with `rstar`, numpy plain vectors, zstandard `ruzstd`, PyYAML `yaml-rust2`,
  subprocess `tokio::process`. Each crate's licence must be compatible with GPL-3.0-or-later and
  is recorded in [MCP.md](MCP.md#dependencies).
- **Routing is compared by behaviour.** `geo` does not reproduce GEOS's polygons, so a point near
  a clearance boundary can fall differently and a route can differ by a grid step. For
  `live_pcb_route`, `live_pcb_route_net`, `live_pcb_check_path` and the check in
  `live_pcb_add_tracks`, both servers must agree on every fixture about success or refusal and the
  conflicts reported, both routes must pass KiCad's DRC (live), and route lengths may differ within
  a tolerance stated in the scenarios. Every other tool stays an exact comparison.
- **Areas.** Each group of tools is one module in `embeint-mcp/src/tools/` with its scenarios in
  `parity/scenarios/`: shared (skills and their resources, editor grid), schematic (with the page
  and undo tools both editors use), symbol (with every library table tool), footprint (with
  embedded files and `live_kicad_cli`), board, and board design (rules, stackup, RF, checks and
  routing, after board).

## Implementation order

- **Coverage and contracts:** keep a complete MCP surface report alongside the
  existing subset parity results; inventory desktop functions beyond IPC and track
  each missing operation, its owner and acceptance evidence.
- **Shared command infrastructure:** generic command schemas, describe/list,
  execute and batch now run natively with the existing transaction restrictions;
  complete typed item CRUD,
  undo/redo, document opening, explicit saving and schema inspection for all editors.
- **Schematic workflows:** hierarchy, symbols, pins, connections, annotation, page
  layout, selection, rendering, netlist, ERC and exports.
- **Symbols, footprints and libraries:** library tables/search/open/create/save,
  properties, pin layout, footprint generation, embedded files and model workflows.
- **PCB workflows:** reads and edits, schematic transfer, placement, stackup, rules,
  net classes, RF calculations, routing geometry, zones, checks, views and exports.
- **Resources and packaging:** bundled skills and content manifests, CLI execution,
  diagnostics and a Python-free installed runtime. These can move earlier where
  another group needs them.
- **Remaining KiCad features:** implement the missing editor interfaces discovered
  by the wider function audit, then qualify each workflow through Rust.

Each group needs fixtures and live evidence where applicable. Update/delete are
an early implementation slice, not the completion milestone. Keep the Python server
available as the reference until replacement qualification passes. Browser work
follows this native parity and function-coverage milestone.

## Reproducible coverage checks

With the MCP development interpreter and a built Rust binary, from the repository root:

```text
python rust/parity/surface_coverage.py --report mcp-surface-coverage.json
python rust/parity/surface_coverage.py --require-complete
python rust/parity/mcp_parity.py --build --live --report mcp-parity-report.json
python tools/live_schematic_mcp/operation_coverage.py --format json
```

`surface_coverage.py` initializes both real MCP servers and compares every advertised
tool, resource, template, prompt and capability-registry command. It lists missing,
different and additional entries without maintaining a hand-selected port list.
It does not contact KiCad. Migration CI reports missing entries and fails on changed
shared definitions; `--require-complete` additionally fails on any missing entry.
The strict check is expected to fail until migration is complete. A passing inventory
does not establish native execution or behavioral parity; the other criteria still
apply. The existing `mcp_parity.py` fixtures currently qualify only the ported subset.

See [MCP.md](MCP.md) for current implementation evidence and
[OPERATION_MATRIX.md](../tools/live_schematic_mcp/OPERATION_MATRIX.md) for the
existing IPC coverage decisions.
