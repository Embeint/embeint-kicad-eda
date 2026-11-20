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

`live_schematic_create_items` covers the whole transaction contract in
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
15000); both servers read it. Register it next to the Python server, not instead of it: it has 8 of
the 115 tools.

## How parity is checked

`python rust/parity/mcp_parity.py --build [--live] [--report out.json]` runs with the MCP's Python
environment. It starts both servers as real MCP stdio subprocesses and drives them with the same
MCP client.

- **Schema.** Every Rust tool is advertised with the Python tool's name, description, input schema
  and output schema. The `initialize` protocol version and capabilities are the same, and
  `tools.json` must match the Python server (`--write-schemas` refreshes it). Python 3.13 and later
  strip docstring indentation when compiling, so the Python server's own descriptions differ in
  whitespace between interpreters. Descriptions are therefore compared, and stored, as
  `inspect.cleandoc` gives them. Schemas are compared exactly.
- **Manifest.** Each Rust command entry equals `commands.describe()`. Its `schema_sha256` is
  Python's hash of the same inventory: `canonical_json` reproduces `json.dumps(sort_keys=True)`,
  including the ASCII escaping.
- **Codec.** 101 fixed cases go through `embeint-mcp --parity` and through the Python functions it
  ports, and must match exactly. They cover protobuf JSON in both directions (well-formed and
  malformed: unknown fields, bad integers, enums, oneofs, maps, `Any`, `FieldMask`), `parse_items`,
  the document checks, endpoint URL validation for both platforms, Windows pipe candidates and the
  temp-directory rule. Refusals must match too, message included.
- **Discovery.** `instance_discovery.py` and `embeint-mcp --discover` must give the same inventory
  on the host's real endpoints.
- **Protocol fixtures.** There are 62 scenarios, each served by a disposable NNG REP0 peer that
  answers with real protobuf envelopes. The requests the peer receives must be identical from both
  servers, and so must the tool results, text included. Client names and generated operation ids
  are normalised. The scenarios cover:
  - every read, with the editor closed, an old KiCad, status errors, a wrong reply type, a lost
    reply, and token pinning after the first reply;
  - the document checks;
  - the transaction: commit, rejected items, a partial acknowledgement, a refused step, an
    unconfirmed rollback, a lost step reply, stale revision, duplicate operation, and a conflict at
    commit;
  - every lost-reply path for `BeginCommit` and `EndCommit`, including in-progress commits settled
    by a drop, a lost drop, and a lost or refused status query;
  - argument validation;
  - endpoint discovery: no editor, one instance, a `api-<pid>` endpoint, two instances
    (ambiguous), and a pinned instance that has gone.

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

## Results (2026-11-20)

| Host | Python | Rust | Fixtures | Codec | Live |
| --- | --- | --- | --- | --- | --- |
| Windows 11, x86_64 MSVC | 3.12.0 | 1.99.0 | 62/62 identical | 101/101, refusal texts included | 45/45 checks |
| WSL Ubuntu, x86_64 | 3.14.4 | 1.99.1 | 62/62 identical | 101/101, refusal texts included | 45/45 checks |

The Windows live run used the native build installed from `main` (`C:\kb\install`, 10.99.0), started
with `run_kicad_windows.ps1`. It read 11 items of 7 types, including two placed `Device` symbols.
Two small changes came after it: the server now ignores JSON-RPC responses, and the harness removes
its scratch directories. Neither changes a tool path. The offline suites were run again on both hosts
after them. A repeat Windows live run was interrupted: something asked the test KiCad to close, and
its "Save Changes?" dialog made the API answer `AS_BUSY`.

The Linux live run came after those changes. It used the WSL build in `~/kicad-live/install`
(10.99.0), started with `run_kicad_wsl.sh` under a private `TMPDIR`, so other MCP servers in WSL
could not find it. It read 10 items of 7 types, and the lost replies were real KiCad replies over a
Unix socket.

The schema check compares the 8 ported tools against the Python server's 115. The unit tests
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
- **A connection closed before the reply** is the same error class as a lost reply, and is
  reconciled the same way. The Rust message says the connection was closed. pynng waits for the
  timeout and then reports a timeout.
- **`-0.0`.** It is printed as Python prints it. When a request *sent* to KiCad contains `-0.0`
  in a double field, `prost-reflect` sends it as 0.0, the field's default.
- **Lone surrogate escapes** in a request are a JSON-RPC parse error. The Python server reports
  them as a tool error ("Unpaired surrogate").
- **Hosts.** Endpoint discovery runs on Linux and Windows. Other hosts use the default endpoint, as
  in Python.

## Next steps

- Move further tools over in groups, each with fixtures that both servers pass. Start with the
  other item edits (`update_items`, `delete_items`), which reuse the transaction, then the PCB
  reads.
- Making the native server the default needs every tool to have moved, or a front end that routes
  the tools that have not moved to Python.
- Run the live comparison as part of the release qualification, against the KiCad build the
  package ships with.

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
  `Cargo.lock` hash, the crates with their licences, and the native server's version, tools and
  schema hash. Its version carries the source revision (`0.1.0+rust.g<revision>`), as the Python
  package's does.
- **Check.** `check_package.py` verifies the hashes and notices. It then runs the native server from
  an empty directory with the same restricted environment as the Python server. The native server
  must initialise, offer exactly the recorded tools with the same schemas as the bundled Python
  server, report the recorded manifest hash, refuse invalid arguments and unknown tools, and exit
  cleanly on EOF. The layout runner must answer, and the frozen server must find it.

`build_windows.ps1` passes `--rust` when cargo is on PATH, so `install\mcp` gets both servers.

`.github/workflows/release-mcp.yml` builds and checks the package on Windows x86_64, Linux x86_64
and Linux arm64, after the parity suite has passed on that platform. Each platform's archive, check
report, parity report and `build-info.json` are uploaded as artifacts. A tag `mcp-v*`, or a manual
run with `release_tag`, creates a **draft** GitHub release with the archives and `SHA256SUMS`, for a
person to review and publish. KiCad itself is not built there: a KiCad build does not fit a hosted
runner. The package is installed beside the Embeint KiCad build it is meant for; a manual run can
record that version with `application_version`.

On 2026-11-20 a Windows package built this way passed `check_package.py`, including the native
checks. It held 30 linked crates and both servers' 8 shared tools with identical schemas.

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
