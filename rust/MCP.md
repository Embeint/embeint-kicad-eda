# Native MCP server (roadmap delivery gate 4)

Gate 4 of [EMBEINT_FORK_ROADMAP.md](../tools/live_schematic_mcp/EMBEINT_FORK_ROADMAP.md) asks for a
bundled Rust MCP that starts with instance discovery and the read-only tools, plus one complete
edit transaction. The old and new servers are to be compared on the same protocol fixtures and on
disposable live projects. Rollback, timeout reconciliation and schema compatibility must pass
before more tools move over. This document records the first slice and its evidence. **The Python
server stays the reference and the default.** The MCP package bundles the Rust server beside it
(see [Packaging and release](#packaging-and-release)); clients use it only when registered.

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

These tools have the Python tools' names, schemas and results.

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
15000); both servers read it. Register it next to the Python server: it has 19 of
the 116 tools.

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
  compared by the manifest check). Arguments are validated as FastMCP and pydantic 2.14 do;
  integers beyond 64 bits and NaN or infinite numbers, which Python accepts, are refused. Python 3.13 and later
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

- **Server identity.** `serverInfo.version` is `0.1.0+rust`. The instructions name the subset this
  server implements and leave out the house-style skill, which this server does not serve. The
  manifest adds `"implementation": "rust"` and reports `"distribution": "source build"`. Its
  limitations say that the Python server provides the other tools.
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

## Next steps

The next milestone is **complete native Rust parity**, followed by closing the broader KiCad
function-coverage gaps, before browser work. Python forwarding does not satisfy this milestone.
The implementation order and acceptance criteria are in [MCP_PARITY_PLAN.md](MCP_PARITY_PLAN.md).
Update/delete are an early slice of that work. Making Rust the default requires every Python MCP
operation and resource to have moved and passed qualification, including the generic command
registry, higher-level helpers and installed operation without Python.

`python rust/parity/surface_coverage.py --require-complete` compares the complete advertised
Python and Rust surfaces and currently fails on missing entries. The existing passing parity
fixtures cover only the ported subset. Complete advertisements still require behavioral and
live evidence, including comparison against the KiCad build the package ships with. The wider
function audit must cover features without IPC handlers as well as existing registered commands.

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

All are compatible with GPL-3.0-or-later. These are linked into `embeint-mcp`:

| Crate | Licence |
| --- | --- |
| tokio 1.54.1, mio 1.2.5, bytes 1.12.2, tokio-macros 2.7.2, zmij 1.0.23, generic-array 0.14.7 | MIT |
| prost 0.14.6, prost-derive 0.14.6, prost-types 0.14.6 | Apache-2.0 |
| prost-reflect 0.16.5, serde_json 1.0.154, serde_core 1.0.229, base64 0.22.1, sha2 0.10.9 (with digest, block-buffer, crypto-common, cpufeatures, typenum), getrandom 0.3.4, socket2 0.6.5, libc 0.2.191 (Linux), windows-sys 0.61.2 (Windows), anyhow, cfg-if, either, itertools, itoa, hashbrown, proc-macro2, quote, syn | MIT OR Apache-2.0 |
| uuid 1.18.1, indexmap 2.14.2, equivalent 1.0.3, pin-project-lite 0.2.18 | Apache-2.0 OR MIT |
| memchr 2.8.3 | Unlicense OR MIT |
| foldhash 0.2.0 | Zlib |
| unicode-ident 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 |

Build-time only (`build.rs` compiles the protocol): protox 0.9.1, protox-parse 0.9.0, logos,
regex-automata, regex-syntax, thiserror, semver, rustc_version (MIT OR Apache-2.0); miette 7.6.0
(Apache-2.0); aho-corasick (Unlicense OR MIT); fnv, version_check (MIT/Apache-2.0).

uuid is held at 1.18.1 because later releases need Rust 1.89, above the workspace's declared
minimum of 1.85.
