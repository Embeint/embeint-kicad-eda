# Targeted schematic blocks: supported Python API

Use `tools/live_schematic_mcp/block_helpers.py` for small generated functional blocks
that will be revised. It uses the existing Python client and IPC commits. Read the
loaded MCP capability inventory before using editor operations; this reference does
not add MCP tools. The Python API requires the source installation, its generated
protocol and `requirements.txt` dependencies. The frozen MCP bundles the helper
module and this reference, but does not provide an arbitrary Python execution tool.

## Construct and plan offline

Run from the source distribution, with its prepared Python environment:

```sh
python tools/live_schematic_mcp/block_example.py
python -m pytest -q tools/live_schematic_mcp/test_block_helpers.py
```

The example builds two synthetic resistors, a dashed titled frame, a Manhattan
signal path, a T junction and a hierarchical port. It prints a construction report
and the proposed create/update/delete logical keys without contacting KiCad or
writing circuit files. These synthetic part values are illustrative, not sourced
production components. Use actual ratings, MPNs, alternates and footprints for a
real design, following `parts-and-libraries.md`.

`Block(namespace)` needs one stable UUID per generated block. Persist it with the
block source and manifest; changing it on every run defeats reconciliation.
Logical keys such as `mcu`, `reset-pullup` and `interface/frame` identify items
through revisions. UUIDs derive from namespace + key, not reference designators,
positions, a sheet bounding box or whichever items happen to be selected.

| Operation | Supported boundary |
| --- | --- |
| `frame(key, title, (x1,y1,x2,y2))` | Dashed unfilled rectangle plus title inside it; millimeters. |
| `symbol(key, definition, reference, anchor_pin, at, unit=1)` | Full library definition from `client.library_symbols([lib_id])`; position by the real numbered pin endpoint. Unrotated, unmirrored unit only. Returns unambiguous numbered endpoints. |
| `pin(key, number)` | Endpoint on a constructed symbol. Missing/ambiguous pins are refused; unit and body-style filtering apply, including hidden pins. |
| `wire(key, net, path)` | Explicit Manhattan waypoints; split same-net T/X joins and add junctions. Reject zero/diagonal legs, collinear overlaps and different-net intersections. `net` is a planning name, not an editor net assignment. |
| `port(key, name, at, shape='input', spin='left')` | Hierarchical label at a real wire endpoint. Shapes: input/output/bidi/passive. |
| `sheet(key, name, filename, at, size, page, ports)` | Owned parent sheet with pins `(name, side, edge_offset_mm, shape)`; returns their endpoints. Sides: left/right/top/bottom. |
| `add(key, item)` | Complete supported top-level schematic protobuf; assigns a generated UUID. This is a full replacement request, not a field patch. Preserve required fields. |
| `report()` | Geometry construction report only; not electrical or visual sign-off. |

Library definitions in `GetItems` omit pins: fetch complete definitions through
`library_symbols`. The symbol builder preserves electrical pin types and library
identity. It does not infer a device pinout, choose pins by names, rotate symbols,
assign sourcing data, annotate, or make unspecified pins passive. Inspect the
resulting `GetSymbolPins` endpoints before relying on connections. Use `pin()`
instead of a returned map if a pin could be stacked/ambiguous.

Route waypoints must provide body clearance. There is no automatic obstacle router,
label-on-every-pin scheme, page selection, or whole-sheet layout. Different-net
crossings are conservatively refused even if KiCad could draw a crossing without
a junction. A same-net X join is intentionally connected. Separate generated blocks
with short local labels; connect sheets through matching hierarchical ports and
parent pins. A child sheet is a separate document/block/manifest. Match names and
directions and wire the parent pins explicitly; there is no multi-document commit.

## Apply only owned UUIDs

For an authorized edit, select the exact live document from the client, retain a
single `LiveSchematic` instance, and keep other editor writers idle. An existing
manifest is the ownership receipt from the last successful apply, not an inventory
of every item on the sheet.

```python
from block_helpers import Block, Manifest, apply, plan
from client import LiveSchematic, document

client = LiveSchematic()  # select intended socket through normal discovery/config
doc = document(document_spec)  # exact project + UUID sheet path from the live app
block = Block(persisted_block_uuid)
block.frame('interface', 'Interface', (35, 35, 95, 85))
# Add parts, routes and ports here, using stable keys and real library endpoints.
state = Manifest.from_json(manifest_text) if manifest_text else Manifest.empty(block.namespace, doc)
pending = plan(block, state, doc, client.items(doc).items)
print(pending.report())  # inspect exact create/update/delete keys
state = apply(client, doc, pending)
manifest_text = state.to_json()  # persist explicitly with the block source
```

Planning refuses changed/missing owned items and collisions with unowned UUIDs.
It never adopts selected items, captures everything inside a rectangle, or clears
a sheet. Removing an owned key removes that UUID only. It refuses sheet deletion
and changing a child sheet filename, since manual child content could be discarded.
It limits a revision to 200 changed items and refuses changing an item's type under
the same key. Use a new logical key for a deliberate type replacement.

The manifest is trusted local bookkeeping, not an authorization boundary against
a caller forging receipts. Preserve it durably. Losing a manifest is not permission
to infer ownership from an existing sheet. Inspect and explicitly recover the
known generated UUIDs and last intended state; never recreate blindly.

`apply()` checks the exact document, rereads and compares a fingerprint of all
serialized sheet items before opening the existing client's one-document batch.
Its local client lock covers this read and batch. It checks per-item statuses via
the client, reads again after the commit, verifies UUID membership and unchanged
manual/unchanged owned items, and returns a new manifest. Repeat desired content
is a no-op even when KiCad normalized defaults or omitted definition pins on read.
It records both requested and observed fingerprints; this prevents blind replay.
The manifest is not an electrical acceptance certificate. The editor may normalize
requested content, so inspect actual fields, pins and intended net membership.

The fingerprint is **not an atomic editor revision check**. Other clients and the
user can race after the final precondition read; the client lock coordinates only
this Python client. Post-read checks can detect some races after edits have already
committed, but cannot roll them back safely or catch a writer after the post-read.
`GetItems` does not expose all application state (for example hidden pin definitions,
page settings or another sheet). Use a single writer and explicit validation.
True editor revisions, compare-and-swap commits and operation-result lookup remain
protocol followups; this helper does not implement them.

## Stop on uncertain outcomes

Each apply plan is single-use once a batch is attempted, including rejected batches.
No mutation is retried. A timeout, lost reply, failed rollback or failed post-read
raises `ApplyUncertain`: outcome **unknown/unverified**, not "nothing happened."
The client attempts its existing drop-on-error behavior, but the helper does not
infer successful rollback from an exception. A timeout at EndCommit can follow a
fully committed block. No save occurs implicitly.

After uncertainty, stop writing, inspect the editor and outstanding commit state,
reread the exact target and the plan's generated UUIDs, and compare actual content
with the last confirmed manifest and intended block. Only replan once that state
is understood. If a confirmed rollback restored the previous snapshot, a new plan
can use the previous manifest. If the apply took effect, recovering its manifest
requires explicit inspection of the observed/requested state; there is no automatic
adopt/retry API. Do not rerun a timed-out script with a new namespace. A stale
precondition fails before sending an edit and also requires a fresh read/replan.

## Validate and retain evidence

Check real pin endpoints, intended pin-to-net memberships, ERC, page containment
and the framed render before an explicit save. Preserving a manual item UUID does
not guarantee unchanged connectivity if new wires touch it; check block boundaries
and intended connections to manual circuitry. `report()['valid']` only describes
construction geometry inside this generated block.

The serial native probe is `block_example.py --live`, guarded by an explicit
`KICAD_API_SOCKET`, `KICAD_BLOCK_HELPER_ISOLATED=1`, and `e2e_guard`'s disposable
project restriction. Coordinate with the native runtime owner. It adds two
Device:R instances, one separate synthetic manual note and a block, checks actual
pin positions/nets, a targeted route revision, repeat no-op and stale-plan refusal,
and removes its exact UUIDs on success without saving. Unknown outcomes stop the
probe without speculative cleanup. It never launches/stops an app or changes its
configuration. Record source/schema/binary identity with its output.

For fresh-session evaluation, give a separate evaluator only the installed/bundled
skill entrypoint and a new disposable synthetic project. Ask it to discover this
reference, plan a block, preserve an existing manual item, revise a connection and
report actual validation and limits. Give it no original conversation or expected
answer. A maintainer's scripted probe is not independent-agent evidence; mark the
independent check pending until that evaluator has actually run.
