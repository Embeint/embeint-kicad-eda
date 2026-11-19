# Instance discovery and startup diagnostics (Linux/WSL and Windows)

This is the first bounded P1.3 slice. It covers the current checkout's Python
adapter and local IPC: filesystem sockets in Linux/WSL and named pipes on Windows
(see [Windows named pipes](#windows-named-pipes)). It is not a clean-machine
installer, an application compatibility certification, or macOS qualification.

## Offline startup checks

From the repository root, using the same interpreter/environment as the MCP server:

```sh
python tools/live_schematic_mcp/diagnose.py
```

The JSON report checks Python dependency imports, generated protocol files and
client imports, and lists matching socket paths without dialing any of them. It
works with the Python standard library alone when dependencies or generated
modules are absent. Fix missing dependencies with that interpreter's package
installer and `tools/live_schematic_mcp/requirements.txt`, then run:

```sh
python tools/live_schematic_mcp/generate_proto.py
```

Native shared libraries must also be available. For example, the Linux ARM64
`pynng` wheel needs `libatomic.so.1`; Debian/Ubuntu provide it in `libatomic1`.
The diagnostic reports this known import failure with a specific dependency action.

Generated modules must come from this checkout and use the MCP interpreter. A
successful import verifies local runtime availability, not live application
compatibility. The CLI prints a report and exits zero even when it finds problems;
invalid CLI arguments exit nonzero. Inspect the reported states and actions.

## Endpoint naming and selection

`KICAD_API_SERVER::StandardSocketPath` and `Start` in
`common/api/api_server.cpp` use `<temp>/kicad/api.sock`. When occupied, the
server falls back to `api-<process ID>.sock`. KiCad manages stale paths using
`api.lock`; the adapter never locks, unlinks or cleans up these files.

The adapter preserves its existing Python temporary-directory default (normally
`/tmp/kicad` on Linux). Use matching temp-directory environments for KiCad and MCP;
if they differ, pass an explicit endpoint. Discovery does not recursively scan
other directories, inspect process command lines, read logs or open designs.
Custom C++ socket overrides require explicit selection.

An explicit constructor socket or `KICAD_API_SOCKET` always takes precedence and
bypasses discovery. `KICAD_API_TOKEN` is used only with such an explicit endpoint;
automatic selection obtains and pins the responding instance's token. URLs must be
absolute local `ipc:///path` filesystem endpoints;
TCP, WebSocket, relative and query-bearing URLs are rejected without echoing them.

Without explicit selection, the Linux adapter waits until the first editor request
to inspect the socket directory. A single socket candidate (standard or PID name)
can be selected. Multiple candidates are reported as ambiguous by offline inventory.
Immediately before the first editor request, automatic selection checks local stream
connections without sending any NNG or application request bytes. Refused listeners
and paths that disappeared are excluded; timeouts, permission errors and unknown
conditions remain candidates. Multiple remaining candidates require explicit selection.
Stale socket files are preserved. Regular files and symlink entries are not
automatic socket candidates; unreadable entries prevent selection. A symlink socket
directory requires explicit selection. If no candidates exist, the request tries the
`api.sock` default and can time out normally; the default is not pinned, so the next
request looks again and finds a KiCad started in the meantime.

The first endpoint found stays pinned for that client. Existing instance-token
handling guards subsequent requests after an initial successful reply. The client
does not switch endpoints or retry a mutation after failure. NNG request resends
are disabled with `NNG_DURATION_INFINITE` (`resend_time=-1`), including on disconnect.
See the [NNG request protocol implementation](https://github.com/nanomsg/nng/blob/v1.8.0/src/sp/protocol/reqrep0/req.c#L221-L246).
After a successful reply, a request-free local listener check detects a dead pinned
endpoint before sending; its error includes recovery instructions. NNG negotiation
itself remains asynchronous so a peer that accepts connections without completing
its handshake still respects the configured send/receive timeouts. A restarted
application can have a different token or project; the
client deliberately does not silently switch endpoints, clear identity or replay
an operation. Restart MCP with an explicit endpoint after identifying the intended
project. Windows discovery is described in the next section; on other hosts (macOS)
the prior default path behavior is retained.

## Windows named pipes

On Windows, NNG serves the same `ipc://<temp>\kicad\api.sock` URL as the local named
pipe `\\.\pipe\<temp>\kicad\api.sock`. The pipe exists only while a server holds it,
so there are no stale endpoints, and KiCad's `api-<pid>.sock` fallback never triggers
(it tests for a file): a second KiCad with the API enabled runs without an API.

- Accepted URLs are `ipc://X:\...` drive paths of plain components. Relative, UNC
  (`\\server\pipe`), device (`\\.\`, `\\?\`), forward-slash, `.`/`..` and
  trailing-dot/space components are rejected without echoing the URL, because Windows
  normalises `\\.\pipe\` names and such components could redirect the name to a
  network pipe.
- The temp directory is derived as KiCad derives it (`wxFileName::GetTempDir`): the
  first of `TMPDIR`, `TMP` and `TEMP` that is set, else `GetTempPath()`. Python's
  `tempfile`, which prefers `TEMP` to `TMP`, is only a last resort.
- Discovery lists `\\.\pipe\` (enumeration never connects) and keeps only
  `api(-<n>)?.sock` pipes whose name is under this user's temp `kicad` directory.
  Directories are compared case-insensitively after resolving short (8.3) names, so
  `C:\Users\LONGNA~1\...` and `C:\Users\LongName\...` match. Pipes that name other
  directories are never candidates.
- Pipes are selected by name only; their owner is not verified. The pipe namespace is
  machine-wide, so another local account could create a pipe with the expected name
  before KiCad starts and receive the MCP's requests. Use the MCP on machines whose
  other local accounts you trust. Checking the owner (`GetNamedPipeServerProcessId`
  and the server's token) needs a handle to the pipe, i.e. a connection to it, which
  discovery deliberately never makes; it is left as a follow-up.
- `run_kicad_windows.ps1` runs this discovery (`python instance_discovery.py` prints
  the inventory as JSON, ignoring `KICAD_API_SOCKET`) in the environment the new
  KiCad inherits, and refuses to start while any candidate holds the endpoint.
- Liveness before the first request, and the dead-pinned-endpoint check, use the same
  listing: absence proves no listener; nothing is dialed.
- Explicit selection uses the URL literally. Pipe names are exact strings, so use the
  endpoint that discovery, `diagnose.py` or `scripts\wait_ready.ps1` reports.

`test_windows_discovery.py` covers validation, discovery and selection with a
substituted pipe listing on every platform, and with real disposable NNG named-pipe
peers (two instances, probes, MCP stdio, the launcher's refusal) on Windows. Native evidence comes from the
live suites run against the Windows build with `scripts\run_tests.ps1 -Live`.

## Opt-in read-only probes

Only when reading the intended instances is appropriate, run inside Linux/WSL:

```sh
python tools/live_schematic_mcp/diagnose.py --probe --timeout-ms 500
python tools/live_schematic_mcp/diagnose.py --probe --socket ipc:///tmp/kicad/api-123.sock
```

`--socket` overrides `KICAD_API_SOCKET`. Without either, probes inspect all matching
socket candidates in deterministic filename order. Each responding endpoint gets
at most five fixed reads: `GetVersion`, then `GetOpenDocuments` separately for
schematic, PCB, symbol and footprint documents. Responses include project/document
identity when the relevant editor handles the query. A manager without open editors
may have no document handler; this is not proof that a project is absent. Version is
informational: even an old version's successful read is recorded, and a new version
with missing handlers is reported as unsupported for those requests.

`AS_UNHANDLED` means the editor may be closed or the request unsupported; diagnostics
report `editor_not_open_or_unsupported`. Only `AS_UNIMPLEMENTED` is labelled unsupported.
Busy/not-ready/token errors remain unverified. Malformed envelopes and wrong reply
types report incompatible responses. An unresponsive socket is **possibly** stale,
busy or inaccessible; timeouts do not prove staleness. Inspect the intended instance,
IPC preferences and permissions. No fallback or cleanup occurs, and probing does
not resolve automatic ambiguity. Select the intended path explicitly before edits.

Each send and receive has a timeout of 1–5000 ms (500 ms by default); the total is
bounded per request and scales with candidate count. Requests are not retried.
An explicit endpoint uses `KICAD_API_TOKEN` when configured; discovery probes start
with an empty token independently for each endpoint and pin the returned token for
that endpoint's later reads. A changed token stops that probe. Tokens, peer error
text and exception text are excluded from output; known tokens are also redacted
from document/version strings. Reports include project paths, so review them before
sharing. KiCad's own optional API debug logging is outside this CLI's control.

No probe opens an editor, starts a task, runs a command supplied by the caller,
edits a document, saves or connects to a network transport. Neither offline nor
opt-in reports claim a negotiated capability set or mutation support.

## Validation boundaries

`test_instance_discovery.py` covers deterministic discovery, explicit overrides,
stale/non-socket entries, ambiguity, deferred selection and no mutation retries,
including a peer that disconnects and rebinds the same endpoint after receiving a request.
`test_diagnostics.py` uses disposable local NNG peers with generated protobuf
envelopes to model two projects, stale endpoints, version/command differences,
token pinning, malformed responses and runtime failures. It also exercises the
stdlib-only CLI with missing modules and a real MCP stdio handshake with two inert
socket candidates. These are synthetic adapter/protocol tests, not native KiCad
integration evidence. Existing offline tests remain part of the required suite:

```sh
KICAD_LIVE_PYTHON=/path/to/python bash tools/live_schematic_mcp/scripts/run_tests.sh
```

On 2026-11-18, combined adapter revision
`c6e511abf1863ec6dfa33939674aaaec61d54696` passed a real two-project run with upstream
KiCad `10.99.0-unknown-4fe1ca2060~190~ubuntu24.04.1`, Linux ARM64, Ubuntu 24.04 and
Xvfb in a network-disabled container. Each application used separate disposable
projects/configuration. Diagnostics identified both projects; automatic selection
refused two live listeners. After killing the second application's process group,
its stale socket was preserved and reported unresponsive. A fresh automatic client
excluded that refused listener and selected the first project, while a client
already pinned to the dead instance failed without switching projects. The same
run exercised native embedded-file reads through actual MCP stdio. Its schema
SHA-256 was `0dfe9020f6c9377221971c6a7194761d18ee05398165570fcd25eb4f22b2c8cd`.

That is bounded upstream-binary evidence, not an Embeint native build or WSL
certification. Matching-fork builds, installation, application-restart/token
reconciliation, disconnects during real mutations and broader platform qualification
remain separate release gates.
