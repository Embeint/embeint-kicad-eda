# Desktop IPC operation decisions (P1.1)

This matrix audits native **registrations**, their implementation paths and the
local MCP adapter. It is a first coverage inventory, not desktop qualification.
The audited starting revision is `a3e9347feb96bf5a6c8c33d87211282b49969661`.
No KiCad process, IPC socket or customer design is used by the audit or its tests.

The maintained matrix is [operation_decisions.json](operation_decisions.json).
Each key is an explicit `API_HANDLER_CLASS/fully.qualified.Request` decision.
[operation_coverage.py](operation_coverage.py) joins these decisions with the
current C++ registrations, inherited handlers and Python generic registry. The
checkout contains **209 owner/request registrations**, **165 distinct requests**
and **399 contextual rows** across manager, schematic, PCB, symbol and footprint.
These are inventory counts, not counts of qualified operations.

## Inspect the complete matrix

Use an environment with `requirements.txt` installed, then run from the repository
root:

```sh
python tools/live_schematic_mcp/generate_proto.py
python tools/live_schematic_mcp/operation_coverage.py
python tools/live_schematic_mcp/operation_coverage.py --format json > /tmp/kicad-operation-matrix.json
python tools/live_schematic_mcp/operation_coverage.py --format markdown > /tmp/kicad-operation-matrix.md
python -m pytest -q -p no:cacheprovider tools/live_schematic_mcp
```

The Markdown view groups every operation by surface and includes native request,
handler method, registration and implementation locations, decision, adapter
mapping, document requirements, registration conditions, disk effects, undo and
offline test references. JSON additionally includes response types, the
`allowed_during_task` registration flag and structured surface metadata. Output
is deterministic for the same checkout and registry. Views are generated on
request so separately reviewed adapter additions do not leave a second committed
snapshot of the registry stale.

## Meaning of a decision

| Decision | Meaning in this inventory |
| --- | --- |
| `supported` | Selected for the adapter-facing desktop scope, with a native implementation path. Source evidence is sufficient for this inclusion decision; it does **not** establish tested native/live behavior, undo correctness or release support. Consult the implementation and evidence columns. |
| `deferred` | Explicitly excluded from the qualified desktop scope pending implementation, adapter review or a safe workflow contract. Any existing generic mapping requires an explicit `adapter_exposure` explanation for that context. |
| `internal` | Client transaction ownership (`BeginCommit`/`EndCommit`) or debug/test use (`InjectDrcError`), rather than an ordinary user operation. |
| `interactive` | Requires GUI interaction or invokes arbitrary actions; generic registry presence does not establish an unattended contract. |

`implementation` distinguishes an implemented path, a conditional path and a
known stub. `live_support_verified` is **false for every row**. There are no native
build IDs or observed live outcomes in this matrix. The P0/P1 build, disposable
project, failure/undo, save/reopen and platform gates remain open.

Generic mappings come from `commands.COMMANDS` at audit time, including its kind
and declared response. The executor is `live_schematic_execute`; the command name
can target any eligible document type, despite the tool's historical name.
Registry kind is never used to infer disk or undo semantics. Batch eligibility
is separately curated by the adapter and is not promised by this matrix.

`typed_tools` records curated primary entry points, including conditional tool
options and export formats; it is not an exhaustive transitive call graph of
every workflow helper. Empty means **none recorded**, not proof that no indirect
route exists. Shared EDITOR/BOARD rows filter typed entry points to the target
surface: a PCB-only tool is not presented as a footprint tool just because its
native request is inherited. Offline test references are named synthetic adapter
tests, not per-editor native coverage. Empty evidence lists are intentional.

## Native paths and important limits

| Surface or provider | Registration and inheritance evidence |
| --- | --- |
| Manager | `kicad/kicad.cpp` registers COMMON and DESIGN_BLOCK LIBRARIES, and installs only the desktop `OpenDocument` callback. Opening is restricted to the manager's loaded project and may create/save a missing board. |
| Schematic | `eeschema/sch_edit_frame.cpp` registers SCH, which inherits EDITOR. Standalone mode adds common/design-block providers. |
| PCB | `pcbnew/pcb_edit_frame.cpp` registers PCB, which inherits BOARD then EDITOR. Standalone mode adds common/design-block providers. |
| Symbol | `eeschema/symbol_editor/symbol_edit_frame.cpp` registers SYMBOL, which inherits EDITOR. |
| Footprint | `pcbnew/footprint_edit_frame.cpp` registers FOOTPRINT, which inherits BOARD then EDITOR. An inherited request's `board` field still needs a footprint document identity. |
| Libraries | `API_HANDLER_SCH_LIBRARIES` and `API_HANDLER_FP_LIBRARIES` inherit LIBRARIES, specializing adapters and packing through virtual hooks. `eeschema/eeschema.cpp` and `pcbnew/pcbnew.cpp` attach them as kifaces load. Request library type/scope and provider readiness govern dispatch; these are session providers, not document handlers. |

Every `registerHandler` call in `common`, `include`, `eeschema`, `pcbnew` and
`kicad` C++ sources/headers is scanned. Declarations alone do not add matrix rows.
The audit resolves request and response names against locally generated protocol
descriptors, follows `API_HANDLER` class inheritance and locates registered
method definitions, including inherited methods and task implementations in
separate source files. Source paths and line numbers are computed on each run.
These facts are static evidence, not observations from a running application.

Concrete source-derived limits captured in the decisions include:

- `ParseAndCreateItemsFromString` validates the document/busy state and returns
  an empty response. It does not parse or create items; it is a deferred stub.
- Schematic and PCB `SaveDocumentAs`, plus `LoadAllLibraries`, reject GUI mode.
  The latter registers only on the DESIGN_BLOCK dispatcher, not on the symbol or
  footprint library subclass instances. They are deferred for desktop use.
- `GetTitleBlockInfo`, `SetTitleBlockInfo` and `GetPageLayout` are registered in
  EDITOR but their default hooks reject symbol/footprint documents. Contextual
  overrides keep those rows deferred.
- `CloseDocument`, `CloseAllDocuments` and `CreateDocument` require common-handler
  callbacks that the desktop manager does not install. CLI/headless callbacks
  are a separate scope.
- `SetTextVariables` saves the project immediately; `SetCustomDesignRules` writes
  `.kicad_dru` immediately. Neither is an undoable editor commit, despite the
  generic registry's `edit` kind. Library table edits also write immediately.
- `RevertDocument` discards unsaved state. Opening library items/replacing editor
  content needs separate unsaved-state and prompt qualification. Native PCB
  embedded-file mutations remain deferred. The audited standalone PCB embedded-file
  read is selected for inclusion; its independent adapter is supplied by PR #2.
  Batch use remains deferred until matching-build qualification passes.
- `RunAction` and `InteractiveMoveItems` are interactive. Debug DRC injection is
  internal. They are not silently counted as unattended supported commands.
- `GetDocumentRevision` (EDITOR) and `GetOperationStatus` (COMMON) are the read
  side of the revision and reconciliation contract in [REVISIONS.md](REVISIONS.md).
  Expected-revision and operation-id checks themselves are request-header fields
  enforced around every EDITOR registration, so they add no rows of their own.
- Library-item rendering requires a GUI frame and writes/removes temporary image
  files. The source library is not saved by rendering.

Undo categories describe source structure: `editor_commit` participates in a
native client commit; `standalone` uses an independent undo/tool mechanism;
`history` consumes shared undo/redo history; `none` has no editor undo;
`not_applicable` is read-only/serialization/validation; `unknown` needs further
audit or qualification. None is evidence of verified rollback or disk recovery.
In particular, netlist import remains `unknown`, and variant undo differs between
the schematic GUI and PCB paths.

## Maintaining the gate

[test_operation_coverage.py](test_operation_coverage.py) runs in the existing
offline pytest invocation in both `scripts/run_tests.sh` and MCP contract CI.
CI path filters cover all scanned native trees, including registration attachment
and inheritance changes outside the original API directories.

When adding a native registration, add an intentional owner/request decision in
the JSON file, with implementation state, document identity, registration
condition, disk effects, undo category, rationale, and explicit typed-tool/test
lists (empty lists are allowed and visible). Add surface metadata when adding a
new handler class. Do not mark a stub supported or add test names as a substitute
for running or writing the appropriate tests. Preserve the distinction between
synthetic tests, static code traces and actual native/live evidence.

The check rejects missing/stale decisions, duplicate keys/registrations, missing
handler implementations, unknown handler ancestry/surfaces, invalid decision
fields, stale typed-tool/test references and native/adapter response mismatches.
A generic adapter for a non-supported contextual decision also fails unless that
context has an explicit `adapter_exposure` reason. Legacy interactive, destructive
and unsupported-context mappings have visible explanations; these are exceptions,
not runtime safety enforcement or automatic promotion to supported status.

This is a conservative source scanner, not a C++ compiler: unrecognized
`registerHandler<...>` syntax and ambiguous protobuf name suffixes fail closed
and require extending the scanner/tests. Constructor conditions and virtual-hook
semantics are reviewed decisions, not inferred capabilities. The gate does not
prove callback readiness, conditional compilation on every platform, all indirect
disk effects, semantic correctness of a maintainer's decision, or that a different
installed KiCad binary matches this checkout.
