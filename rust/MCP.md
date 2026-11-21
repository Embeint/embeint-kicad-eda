# Native MCP server (roadmap delivery gate 4)

Gate 4 of [EMBEINT_FORK_ROADMAP.md](../tools/live_schematic_mcp/EMBEINT_FORK_ROADMAP.md) asks for a
bundled Rust MCP that starts with instance discovery and the read-only tools, plus one complete
edit transaction. The old and new servers are to be compared on the same protocol fixtures and on
disposable live projects. Rollback, timeout reconciliation and schema compatibility must pass
before more tools move over. This document records the native server, which now serves the
complete Python surface (all 116 tools, the skill resources and template, and the 147 generic
commands), and its evidence. **The Python server stays the reference and the default until live
qualification of every area is recorded below.** The MCP package bundles the Rust server beside
it (see [Packaging and release](#packaging-and-release)); clients use it only when registered.

## What exists

| Crate | Purpose |
| --- | --- |
| `embeint-ipc` | The adapter to KiCad's IPC API: endpoint discovery (`discovery.rs`, a port of `instance_discovery.py`), NNG REQ0 over local IPC (`nng.rs`), protobuf JSON as Python's `json_format` writes and reads it (`pbjson.rs`), and the client with the revision contract and transactions (`client.rs`, a port of the `send`, revision and transaction code of `client.py`). |
| `embeint-mcp` | The `embeint-mcp` binary: MCP over stdio, the tools below, and the capabilities manifest. `--discover` prints the offline endpoint inventory, as `instance_discovery.py` does. |
| `parity/mcp_parity.py`, `parity/live_parity.py` | The comparison with the Python server. |

The protocol comes from `api/proto` at build time: `build.rs` compiles it with protox, a protobuf
compiler written in Rust, so no `protoc` is needed. Messages are dynamic (`prost-reflect`), built
from the same descriptors as the Python generated code.

NNG is not linked. `nng.rs` speaks NNG's IPC wire protocol directly: an 8-byte header that names
the protocol on connect (REQ0 0x30, REP0 0x31), messages framed by a type byte and a 64-bit
length, and a request id echoed by the reply. It works over Unix sockets and Windows named pipes.
As in the Python client, every request uses a new connection and is never resent.

### Tools

Every Python tool, with its name, schema and results. Each area of tools is a module in
`embeint-mcp/src/tools/` with its fixtures in `parity/scenarios/<area>.py` (the core tools'
fixtures are in `mcp_parity.py` itself):

| Area (module) | Tools | Ported from |
| --- | --- | --- |
| core | capabilities, documents, hierarchy, items, revision and operation status, schematic item create/update/delete, the generic command registry (list, describe, execute, batch), item schemas, exports and the fab package (below) | `server.py`, `client.py`, `commands.py`, `fab_package.py` |
| shared | `live_skill_reference`, the `kicad-skill://` resources and template (skills embedded at build time), `live_editor_grid` | `skills.py`, `board_client.py` |
| schematic | open, place_symbol, view, page, set_page, focus, save, annotate, update_from_library, autoplace_fields, move, undo, symbol_pins, connect_pins, connect_pin_to_net, netlist, erc, select, highlight_nets, and `live_pcb_page`/`set_page`/`undo` (same code) | `client.py` |
| symbol | the 14 `live_symbol_*` tools (pin layout through the `embeint-layout` crate) and every library-table tool, symbol and footprint | `client.py`, `symbol_layout.py` |
| footprint | 13 `live_footprint_*` tools, the embedded-file tools and `live_kicad_cli` | `footprint_client.py`, `footprint_gen.py`, `model_handoff.py`, `kicad_cli.py` |
| board | 25 PCB editor tools and the `BoardTools` helpers | `board_client.py` |
| board design | rules, custom rules, net classes, stackup, RF and impedance, check, check_path, add_tracks, add_keepout, route, route_net (router on the `geo` crate) | `board_design.py`, `board_router.py`, `board_rf.py` |

The first tools, in detail:

| Tool | Kind |
| --- | --- |
| `live_mcp_capabilities` | offline manifest of this server's tools and commands |
| `live_schematic_documents`, `live_pcb_documents` | read |
| `live_schematic_hierarchy`, `live_schematic_items` | read |
| `live_document_revision`, `live_operation_status` | read (revision contract) |
| `live_schematic_create_items` | edit: one commit, with rollback and reconciliation |
| `live_schematic_update_items`, `live_schematic_delete_items` | edit: the same commit and reconciliation contract |
| `live_schematic_list_commands`, `live_schematic_describe_command` | offline curated command registry and descriptor fields |
| `live_schematic_execute` | native dispatch of every curated command, preserving its edit/file/read semantics |
| `live_schematic_batch` | preflight all steps, then one commit; reject mismatched documents and non-transactional edits |
| `live_schematic_item_schema`, `live_pcb_item_schema` | offline protobuf item descriptors and enum choices |
| `live_schematic_export`, `live_pcb_export` | job: one export from the live design, defaults with caller `options` merged over them |
| `live_pcb_fab_package` | job: saved-state and DRC checks, every fab and assembly output, verification of the files, manifest and Gerber zip |

The item edits and batches use the transaction contract in
[REVISIONS.md](../tools/live_schematic_mcp/REVISIONS.md):

- `expected_revision` and `operation_id` are sent on `BeginCommit`, with a 60 s lease.
- A rejected item, a short acknowledgement or a failed step drops the commit. If the drop also
  fails, the call reports the rollback as unconfirmed.
- A lost `EndCommit` reply is reconciled with `GetOperationStatus`. An open commit is settled with
  a drop. A lost `BeginCommit` reply drops the commit it may have opened.
- Results carry `revision` and `operations`.

Arguments are checked as FastMCP checks them, including its decoding of objects sent as JSON text,
and results are encoded as FastMCP encodes them.

### Using it

```bash
cd rust && cargo build --release -p embeint-mcp
claude mcp add kicad-live-rs -- /path/to/rust/target/release/embeint-mcp    # embeint-mcp.exe on Windows
```

From an installed MCP package, register `embeint-mcp` (`embeint-mcp.exe`) from the package folder,
for example `C:\kb\install\mcp\embeint-mcp.exe`.

The server finds KiCad as the Python server does. `KICAD_API_SOCKET` and `KICAD_API_TOKEN` select
an instance explicitly. `KICAD_API_TIMEOUT_MS` sets the reply timeout (1 to 600000 ms, default
15000); both servers read it. It serves all 116 tools; until live qualification of every area is
recorded below, register it beside the Python server rather than in place of it.

## How parity is checked

`python rust/parity/mcp_parity.py --build [--live] [--report out.json]` runs with the MCP's Python
environment. It starts both servers as real MCP stdio subprocesses and drives them with the same
MCP client.

- **Schema.** Every Rust tool is advertised with the Python tool's name, description, input schema
  and output schema, in the Python server's order. The `initialize` protocol version and
  capabilities are the same. `tools.json` holds every Python tool and must match the Python server
  (`--write-schemas` refreshes it); the Rust server advertises the entries it implements, each
  area's tools in `embeint-mcp/src/tools/<area>.rs`, and every advertised tool must be run by a
  fixture beyond argument validation (`parity/scenarios/<area>.py`; `live_mcp_capabilities` is
  compared by the manifest check). Arguments are validated as FastMCP and pydantic 2.14 do
  (integers of any size are kept exactly); NaN and infinite numbers, which Python accepts, are
  refused. Python 3.13 and later
  strip docstring indentation when compiling, so the Python server's own descriptions differ in
  whitespace between interpreters. Descriptions are therefore compared, and stored, as
  `inspect.cleandoc` gives them. Schemas are compared exactly.
- **Manifest.** Each Rust command entry equals `commands.describe()`. Its `schema_sha256` is
  Python's hash of the same inventory: `canonical_json` reproduces `json.dumps(sort_keys=True)`,
  including the ASCII escaping.
- **Codec.** 109 fixed cases go through `embeint-mcp --parity` and through the Python functions it
  ports, and must match exactly. They cover protobuf JSON in both directions (well-formed and
  malformed: unknown fields, bad integers, enums, oneofs, maps, `Any`, `FieldMask`), `parse_items`,
  the document checks, endpoint URL validation for both platforms, Windows pipe candidates and the
  temp-directory rule. Refusals must match too, message included.
- **Discovery.** `instance_discovery.py` and `embeint-mcp --discover` must give the same inventory
  on the host's real endpoints.
- **Protocol fixtures.** Scenarios are served by a disposable NNG REP0 peer that
  answers with real protobuf envelopes. The requests the peer receives must be identical from both
  servers, and so must the tool results, text included. Client names and generated operation ids
  are normalised. The scenarios cover:
  - every read, with the editor closed, an old KiCad, status errors, a wrong reply type, a lost
    reply, and token pinning after the first reply;
  - the document checks;
  - the transaction: commit, rejected items, a partial acknowledgement, a refused step, an
    unconfirmed rollback, a lost step reply, stale revision, duplicate operation, and a conflict at
    commit;
  - lost-reply paths for `BeginCommit` and `EndCommit`, including in-progress commits settled
    by a drop, a lost drop, and a lost or refused status query;
  - argument validation;
  - endpoint discovery: no editor, one instance, a `api-<pid>` endpoint, two instances
    (ambiguous), and a pinned instance that has gone.

  The shared-command expansion repeats the transaction faults for update and delete,
  dispatches every curated registry command against its declared protobuf reply, compares
  command and item-schema discovery, and checks batch refusals, document binding, partial
  acknowledgements and standalone mutation reconciliation. The command registry is compiled
  from `embeint-ipc/commands.json`, regenerated with `--write-schemas` and checked against Python
  on every parity run. The native runtime does not load or execute Python.

  The three validation scenarios compare only the failure and its tool, not pydantic's wording.
- **Live** (`--live`). This runs against a running KiCad on the disposable `live-demo` project
  (`e2e_guard.test_document` refuses anything else).
  - The run first adds wires, a bus, a dashed graphic line, labels of all three kinds, text, a
    no-connect and two library symbols, as undoable steps. The six read tools must then give
    identical results from both servers and leave the revision unchanged. The real items must
    parse back identically.
  - For each server, `create_items` with an expected revision and an operation id commits once:
    `OPS_COMMITTED`, with `revision_after` equal to the result's revision, and one undo removes it.
  - A batch with an item KiCad rejects (an existing id) is rolled back. Nothing changes, no commit
    stays open, and KiCad records `OPS_ROLLED_BACK`.
  - A stale revision is refused, and KiCad records `OPS_REJECTED`.
  - A forwarding proxy then discards one real reply from KiCad. If the `EndCommit` reply is lost,
    the server must reconcile the call as committed, with no resend. If the `BeginCommit` reply is
    lost, the server must drop the commit KiCad opened, and KiCad must record it rolled back.
  - At the end every edit has been undone, no commit is open, and the schematic file has not been
    written.
  - Update, delete and generic batches are checked against complete item snapshots: one undo
    through generic `UndoLastChange` restores every field, partial failures roll back, stale
    revisions fail, and real lost commit replies reconcile without resending edits.
  - The workflows (`parity/live_workflows.py`) then drive both servers, in turn, through the same
    tool sequences in four areas (schematic, libraries, board, shared) and compare them call by
    call; see that module's docstring for what is compared exactly and what by behaviour.

## Live workflows (2026-11-21)

On native Windows (`C:\kb\install` 10.99.0, a fresh disposable `live-demo` started with
`run_kicad_windows.ps1 -Fresh` under a private `TEMP`, Python 3.12.0) `mcp_parity.py --live` passed
**362/362 checks**. The workflows made **230 compared calls of 102 tools**, and every one agreed:
schematic 41 calls, libraries 91, board 84, shared 14. Together with the earlier live checks, 114
of the 116 tools ran live through both servers. Not run live: `live_schematic_save` and
`live_pcb_save` (the schematic must stay unwritten; the harness saves the board once, unchanged,
for kicad-cli). The library save tools ran, into scratch libraries. The same build passed
643/643 fixtures.

- **Exact** for 201 calls; 15 images, 8 route/path/DRC steps and 6 order-free comparisons are the
  rest. Images: valid PNGs of the same size; 2D views and the board's 3D
  view were pixel-identical, the footprint 3D render (kicad-cli) is not repeatable even from one
  server. Routes by behaviour: the same routes (`/VOUT` 15.41 mm, `/VIN` in two legs), path
  conflicts and refusals, and DRC after each route reports no copper clearance violation for
  either server. KiCad's update report, net codes, and the order of ERC/DRC violations and their
  items follow ids generated at random or KiCad's internal item order and are compared order-free.
- **Revision contract**: every edit is undone and redone by KiCad and compared with full item
  snapshots, no commit stays open, and both servers need the same number of undo steps (the
  harness, not the server under test, sends the undo). `live_pcb_place_footprint`,
  `live_pcb_add_zone` and `live_symbol_add_pins` with `replace` take two undo steps in both servers
  (Python's design).
- **Found and fixed in Rust**: a crashed kicad-cli's exit code was printed signed on Windows
  (-1073741819 instead of Python's 3221225477), in `live_kicad_cli` and the render and STEP export
  errors (scenarios `footprint_view_3d_render_crashes`, `footprint_kicad_cli_crash`).
- **KiCad behaviour seen with both servers** (not server differences): undoing a
  `live_footprint_generate` commit opens a modal "Incomplete undo/redo operation: some items not
  found" in the Footprint Editor, which blocks the API (Footprint Editor edits are therefore only
  checked for open commits); redoing an update from the schematic drops pad nets; annotation
  re-places global labels' intersheet references outside the undo step; ERC/DRC markers are not
  undoable; the DRC severities are listed in another order once design rules have been set; a new
  grid size becomes a preset for good; removed libraries stay buffered under their nickname; and
  net classes can only be added through the tools (the harness restores them with `MMM_REPLACE`).

## Results (2026-11-21, complete surface)

The remaining 97 tools were ported in six areas (PRs #26 to #31, after the split in #25), each reviewed by a separate
agent and its findings fixed before merging; CI ran the offline suite on Windows and Linux
(x86_64 and arm64) for every PR.

| Check | Result |
| --- | --- |
| Surface (`surface_coverage.py --require-complete`) | complete: 116 tools, 2 resources, 1 template, 147 commands; 0 missing, 0 different |
| Offline fixtures (`mcp_parity.py`) | 641/641 identical on Windows, plus every per-platform CI run; 109 codec cases |
| Rust | unit tests on Windows and Linux, clippy `-D warnings`, rustfmt, Rust 1.85 check |
| Router | a reviewer drove both routers with the same 1,240 random problems: bit-identical; the fixtures compare routes by behaviour only where they diverge by construction |
| Live | pending (see Next steps) |

Review and porting also found and fixed a `live_kicad_cli` hole in **both** servers: outputs
Windows rewrites (trailing dots and spaces, `::$DATA`, existing 8.3 aliases) could overwrite a
design file or land in a footprint library (PR #31).

## Results (2026-11-20)

### Manufacturing outputs

`live_schematic_export`, `live_pcb_export` and `live_pcb_fab_package` are ported natively
(`embeint-mcp/src/fab.rs`, a port of `tools/live_schematic_mcp/fab_package.py`). The package's
pre-checks, request order, file checks, warnings and result keys are Python's. Its manifest and
the stored (uncompressed), reproducible Gerber zip are byte-identical between the two servers. The
fixture peers write realistic outputs (`fab_fixture.py`: Gerber job file, Excellon, placement
CSV and BOM). The harness compares the hashes of every file each server writes, as well as the
requests and results. The fixtures cover every export kind, options, refusals, the full package,
unsaved and DRC blocks, a failed DRC task, export failures, schematic matching (including KiCad's
trailing separator on the schematic's project path), existing outputs with and without
`overwrite` (which removes only what the previous manifest lists and refuses other files), an
export failure or a design edited during the export (both remove what was written), KiCad's
`JS_WARNING` replies (including a STEP export that warns and writes nothing), and argument
validation. On Windows the full offline run passes 269/269 scenarios.

`live_parity.py` now also runs both servers' export tools and the fab package (checked and
unchecked) on the test board. KiCad stamps Gerber and drill files with the time, so it compares
the placement CSV and BOM bytes and everything else through the tool results. On WSL Ubuntu
(10.99.0 rebuilt with the API job log capture) all four manufacturing checks, the reads,
transactions and lost-reply checks passed, with 266/267 fixtures identical. The one difference,
`batch_invalid_command_types`, predates these tools: Python 3.14 words the unhashable-key
`TypeError` differently from the 3.12 text the Rust server reproduces. On native Windows
(`C:\kb\install` rebuilt with the capture, Python 3.12) all four manufacturing checks passed live
and 269/269 fixtures were identical.

The table below records the original eight-tool slice. The later shared-command expansion
has 16 tools and all 147 generic command definitions. Its native Windows live comparison
passed **100/100 checks**, including update/delete/batch rollback, full-content undo and
lost-reply reconciliation. It used a separate disposable `live-demo`, private configuration
and a private named-pipe endpoint with the installed `C:\kb\install` KiCad 10.99.0 build;
the user's open project was not edited. All test edits were undone, no commit remained
open and the schematic file was not written. Linux live and installed-package qualification
have not been repeated for this expansion. One repeat against a restarted test instance
was interrupted when that KiCad process exited; no matching Windows crash event was found.
It was not counted as a passing run. A fresh disposable project with the process owned by
the test controller then passed all 100 checks on the final binary.

The Windows protocol run passed 244 scenarios and 109 codec cases. Two additional
batch-input regression scenarios also passed, covering empty payload values and invalid command types.
The registry comparison checks every command definition and exercises every generic
dispatch path against a synthetic peer; this is not live qualification of all 147 handlers.
Rust unit tests, clippy with `-D warnings`, formatting and the Rust 1.85 build check pass.
Parity reports identify the native executable by SHA-256, record the source revision and
dirty state, and live reports include the application version and schema fingerprint.

| Host | Python | Rust | Fixtures | Codec | Live |
| --- | --- | --- | --- | --- | --- |
| Windows 11, x86_64 MSVC | 3.12.0 | 1.99.0 | 72/72 identical | 109/109, refusal texts included | 45/45 before review fixes |
| WSL Ubuntu, x86_64 | 3.14.4 | 1.99.1 | 72/72 identical | 109/109, refusal texts included | 46/46 after review fixes |

The Windows live run used the native build installed from `main` (`C:\kb\install`, 10.99.0), started
with `run_kicad_windows.ps1`. It read 11 items of 7 types, including two placed `Device` symbols.
The later review fixes were checked offline on Windows and live on Linux. A repeat Windows live
run was interrupted earlier: something asked the test KiCad to close, and its "Save Changes?"
dialog made the API answer `AS_BUSY`. The Windows live count therefore describes the earlier code.

The Linux live run after the review fixes used the WSL build in `~/kicad-live/install`
(10.99.0), started with `run_kicad_wsl.sh` under a private `TMPDIR`, so other MCP servers in WSL
could not find it. It read 10 items of 7 types, and the lost replies were real KiCad replies over a
Unix socket.

Review regression coverage includes disconnected Windows pipes, decimal/exponent integer strings,
invalid float strings, nested `Any` bounds, token changes after pinning and ten additional
transaction/disconnect scenarios. Codec refusal differences now fail the run. The live harness
records forwarding failures and refuses undo cleanup when other test additions remain.

The original schema check compared 8 ported tools against the Python server's 115. The unit tests
(`cargo test -p embeint-ipc -p embeint-mcp`) pass on both hosts; clippy with `-D warnings` and
`cargo check` with Rust 1.85 pass. CI runs the unit tests and the offline comparison in
`.github/workflows/rust-mcp.yml`. The live comparison needs a KiCad, so it is run by hand.

Responsiveness against the live KiCad on Windows (median of 40 calls, same session):

| | Start to `initialized` | `live_document_revision` | `live_schematic_items` |
| --- | ---: | ---: | ---: |
| Python server | 1261 ms | 15.5 ms | 15.5 ms |
| Rust server | 77 ms | 1.9 ms | 2.1 ms |

## Known differences

- **Server identity.** `serverInfo.version` is `0.1.0+rust` (`0.1.0+rust.g<revision>` in a
  package build); the instructions are the Python server's, word for word. The manifest adds
  `"implementation": "rust"`, reports `"distribution": "source build"` and leaves
  `application_version` and `source_revision` null, which the bundled Python server fills in; its
  limitations say so.
- **Arguments beyond JSON.** NaN and infinite numbers, which Python accepts as text such as
  `"nan"` or `"1e400"`, are refused; integers of any size are kept exactly, as pydantic does.
- **Validation text.** Arguments that fail FastMCP's pydantic validation are refused with an error
  in the same form (`Error executing tool <name>: N validation error(s) for <name>Arguments ...`).
  pydantic's exact wording is not reproduced.
- **JSON-RPC edge cases.** Unknown methods, requests before initialization, and malformed
  `tools/call` envelopes can produce different JSON-RPC error codes than the Python SDK.
  Rust ignores cancellation notifications and finishes calls already received on EOF. Its
  transaction lock serializes calls to KiCad, while pings and tool listings remain responsive;
  Python's synchronous tools block its event loop. These differences are outside the compared
  tool-call contract.
- **Connection failures.** Both servers reconcile a closed connection as a lost reply. Rust also
  treats malformed reply frames as lost replies. A non-retryable failure opening an endpoint
  (such as access denied) fails immediately in Rust; pynng can wait until its timeout.
- **Protobuf edge cases.** Uncommon FieldMask validation errors and some error text for control
  characters or integers beyond 128 bits are not reproduced exactly. Printing nested `Any`
  values is limited to 100 levels in Rust; Python's recursion limit differs. Unknown or malformed
  `Any` payloads are refused rather than returned as opaque data.
- **`-0.0`.** It is printed as Python prints it. When a request *sent* to KiCad contains `-0.0`
  in a double field, `prost-reflect` sends it as 0.0, the field's default.
- **Lone surrogate escapes** in a request are a JSON-RPC parse error. The Python server reports
  them as a tool error ("Unpaired surrogate").
- **Hosts.** Endpoint discovery runs on Linux and Windows. Other hosts use the default endpoint, as
  in Python.
- **Python reprs in messages.** Quoted text in error messages escapes backslashes, quotes and
  ASCII and C1 control characters as Python does, but not the other non-printable Unicode
  characters Python's `repr` escapes.
- **Text encoding.** kicad-cli output and footprint text are read as UTF-8; Python uses the
  locale encoding (cp1252 on Windows), so non-ASCII text can differ there.
- **Skills.** Front matter is read as YAML 1.2 (`yaml-rust2`) where PyYAML reads 1.1; the bundled
  skills use no value the two read differently (a unit test checks). A non-UTF-8 skill file stops
  the build. On Linux, Python reports an over-long skill file name with "File name too long" and
  the checkout's path; the native server serves embedded skills and reports it missing.
- **Routing.** Compared by behaviour (see [MCP_PARITY_PLAN.md](MCP_PARITY_PLAN.md), Decisions).
  The router uses `geo`: convex obstacles follow GEOS's own buffer polygons, but curved or concave
  ones are cleared by true distance where GEOS uses 6-segment chords (up to about 3 um; Python's
  route can pass about 1.6 um inside the real clearance where Rust steps round it). Shapes
  through i_overlay can shift by about 1e-8 mm; invalid pad polygons are cleaned by an overlay
  union rather than `buffer(0)`. With pads far off the board and a tiny grid, Python raises a
  MemoryError where the native server reports numpy's index error.
- **Smaller differences** recorded in the area PRs (#26 to #31): `EMBEINT_LAYOUT_BACKEND` is
  ignored (Python adds a `layout_backend` key only when it is set); `board_layers`' unused `enable`
  argument is not ported; a few type-error, base64 and zstd error texts differ; backslashes in a
  footprint lib_id are inserted literally; the packaged kicad-cli location is tried even when not
  frozen; absurd integers (a priority, size or layer count beyond 64 bits) are refused before KiCad
  is contacted where Python fails after its first requests, with the same message; a placed symbol
  that KiCad returns undecodable reports a different message; Python 3.14 words one unpacking
  error differently from the 3.12 text reproduced. The 60 s ERC deadline is ported but has no
  fixture (it would take 60 s).
- **Python quirks reproduced on purpose** (worth fixing in Python, then in both): an empty id is
  turned into a new random UUID instead of being refused; an unknown pin orientation fails
  connect_pins with "0"; set_page is not atomic; live_schematic_focus does not normalise ids;
  `live_symbol_add_pins` with `replace=true` deletes the old pins in their own commit before
  validating the new ones; route_net reports a renamed footprint's old reference (a process-wide
  cache); vias are only kept from copper that intersects the search area.

## Next steps

- **Live qualification of every area** against the KiCad build the package ships with, both
  servers on disposable projects, routes checked by KiCad's DRC (criteria 3 and 4 of
  [MCP_PARITY_PLAN.md](MCP_PARITY_PLAN.md)). Until it is recorded here, the Python server stays
  the default.
- **A Python-free installed package**: qualify the native server from the installed package with no
  Python available (criterion 2), then make it the default.
- **Shared helpers**: the areas each carry small Python-imitation helpers (rounding, reprs,
  millimetre conversion, UUIDs, temporary folders); they belong in one module.
- **The wider function audit** (criterion 5): features without IPC handlers, menu and tool actions,
  CLI jobs and settings, each with a coverage decision.

`python rust/parity/surface_coverage.py --require-complete` now passes: the advertised surfaces
are equal. That establishes advertisement, not behaviour; the fixtures and the live runs do that.

## Packaging and release

`package_mcp.py --rust` builds `embeint-mcp` and the `embeint-layout` runner from this checkout
(`cargo build --locked`, using cargo's own report of what it built) and puts them beside the frozen
Python server (`tools/live_schematic_mcp/qualification/rust_bundle.py`). The frozen server's
`layout_backend.py` finds the runner there.

- **Notices.** The crates are statically linked, so the licence files of every crate in the
  binaries ship in `licenses/rust/<crate>-<version>/`. The crates come from `cargo tree` with normal
  edges and no proc macros, for the host. The Rust standard library's notices come from the
  toolchain. A crate without a licence file stops the build.
- **Provenance.** `build-info.json` gains `rust`: the binaries' SHA-256, `rustc`, the target, the
  Cargo version, Rust flags and source hashes, `Cargo.lock` hash, the crates with their licences,
  and the native server's version, tools and
  schema hash. Its version carries the source revision (`0.1.0+rust.g<revision>`), as the Python
  package's does.
- **Check.** `check_package.py` verifies the hashes and notices. It then runs the native server from
  an empty directory with the same restricted environment as the Python server. The native server
  must initialise, offer exactly the recorded tools with the same schemas as the bundled Python
  server, report the recorded manifest hash, refuse invalid arguments and unknown tools, and exit
  cleanly on EOF. The layout runner must answer, and the frozen server must find it.
- **Windows runtime.** Packaged MSVC binaries use a statically linked CRT. Both packaging and
  package checking inspect ordinary and delayed PE imports and refuse a VC++ runtime DLL
  dependency, even when that DLL happens to be installed on the build machine. The Windows
  build environment gets the PE reader (`pefile`) from PyInstaller.

`build_windows.ps1` passes `--rust` when cargo is on PATH, so `install\mcp` gets both servers.

`.github/workflows/release-mcp.yml` builds and checks the package on Windows x86_64, Linux x86_64
and Linux arm64, after the parity suite has passed on that platform. Each platform's archive, check
report, parity report and `build-info.json` are uploaded as artifacts. A tag `mcp-v*`, or a manual
run with `release_tag`, creates a **draft** GitHub release with the archives and `SHA256SUMS`, for a
person to review and publish. A manual release builds that existing tag, and the release job
checks all three packages against the tag's commit and version and requires clean Git provenance.
An empty manual `release_tag` only builds and checks. Release builds do not restore the PR build
cache. KiCad itself is not built there: a KiCad build does not fit a hosted
runner. The package is installed beside the Embeint KiCad build it is meant for; a manual run can
record that version with `application_version`.

On 2026-11-20 a Windows package rebuilt after the review fixes passed `check_package.py`, including
the native checks and PE import checks. The previous package fails the new runtime dependency
check, confirming that it detects the missing-DLL problem. The rebuilt package held 30 linked
crates and both servers' 8 shared tools with identical schemas. The Python suite passed 617 tests
(50 skipped); release provenance tests reject mismatched commits, versions and dirty builds.

## Dependencies

All are compatible with GPL-3.0-or-later. These are linked into `embeint-mcp` and the
`embeint-layout` runner, the two binaries the package ships:

| Licence | Crates |
| --- | --- |
| MIT OR Apache-2.0 | base64 0.22.1, block-buffer 0.10.4, cfg-if 1.0.5, chrono 0.4.45, cpufeatures 0.2.17, crypto-common 0.1.7, digest 0.10.7, form_urlencoded 1.2.3, geo 0.32.0, geo-types 0.7.20, getrandom 0.2.17, getrandom 0.3.4, hash32 0.3.1, hashbrown 0.17.1, hashlink 0.12.2, heapless 0.8.0, idna 1.1.0, itoa 1.0.19, log 0.4.34, num-traits 0.2.19, once_cell 1.21.4, percent-encoding 2.3.2, ppv-lite86 0.2.21, prost-reflect 0.16.5, rand 0.8.8, rand_chacha 0.3.1, rand_core 0.6.4, regex 1.13.2, regex-automata 0.4.19, regex-syntax 0.8.12, robust 1.2.0, rstar 0.12.2, serde_core 1.0.229, serde_json 1.0.154, sha2 0.10.9, sif-itree 0.4.1, smallvec 1.16.6, socket2 0.6.5, stable_deref_trait 1.2.1, tempfile 3.27.0, thiserror 2.0.21, typenum 1.20.1, url 2.5.9, windows-link 0.2.1, windows-sys 0.61.2, yaml-rust2 0.13.1 |
| MIT | bytes 1.12.2, float_next_after 1.0.0, generic-array 0.14.7, geographiclib-rs 0.2.7, i_float 1.15.0, i_key_sort 0.6.0, i_overlay 4.0.7, i_shape 1.14.0, i_tree 0.16.0, libm 0.2.16, mio 1.2.5, ruzstd 0.8.1, tokio 1.54.1, twox-hash 2.1.5, zmij 1.0.23 |
| Unicode-3.0 | icu_collections 2.1.1, icu_locale_core 2.1.1, icu_normalizer 2.1.1, icu_normalizer_data 2.1.1, icu_properties 2.1.2, icu_properties_data 2.1.2, icu_provider 2.1.1, litemap 0.8.3, potential_utf 0.1.6, tinystr 0.8.5, writeable 0.6.4, yoke 0.8.4, zerofrom 0.1.8, zerotrie 0.2.5, zerovec 0.11.9 |
| Apache-2.0 OR MIT | equivalent 1.0.3, fastrand 2.5.0, idna_adapter 1.2.1, indexmap 2.14.2, pin-project-lite 0.2.18, utf8_iter 1.1.0, utf_types 1.0.0, uuid 1.18.1 |
| Apache-2.0 | approx 0.5.1, prost 0.14.6, prost-types 0.14.6 |
| Unlicense OR MIT | byteorder 1.5.0, memchr 2.8.3 |
| BSD-2-Clause OR Apache-2.0 OR MIT | zerocopy 0.8.64 |
| MIT/Apache-2.0 | arraydeque 0.5.1 |
| Zlib | foldhash 0.2.0 |

92 crates, from `rust_bundle.linked_crates` on Windows x86_64 (normal edges, no proc macros).

On Linux, `windows-sys` and `windows-link` give way to `rustix`, `linux-raw-sys`, `errno`,
`bitflags`, `libc`, `signal-hook-registry` and `iana-time-zone` (chrono's local time zone). `geo` 0.32.0, `rstar` 0.12.2 and `i_overlay` 4.0.7 publish no licence file; their
upstream texts are kept in `tools/live_schematic_mcp/qualification/notices/rust/`. `ruzstd` is held at
0.8.1 and `uuid` at 1.18.1 because later releases need a newer Rust than the workspace's 1.85.

Build-time only (`build.rs` compiles the protocol): protox 0.9.1, protox-parse 0.9.0, logos,
semver, rustc_version (MIT OR Apache-2.0); miette 7.6.0 (Apache-2.0); aho-corasick (Unlicense OR
MIT); fnv, version_check (MIT/Apache-2.0). regex-automata, regex-syntax and thiserror are used at
build time too and are also linked (above).
