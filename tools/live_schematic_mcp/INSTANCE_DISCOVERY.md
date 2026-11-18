# Linux/WSL instance discovery and startup diagnostics

This is the first bounded P1.3 slice. It covers the current checkout's Python
adapter and local filesystem IPC in Linux/WSL. It is not a clean-machine installer,
an application compatibility certification, or Windows/macOS qualification.

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
bypasses discovery. URLs must be absolute local `ipc:///path` filesystem endpoints;
TCP, WebSocket, relative and query-bearing URLs are rejected without echoing them.

Without explicit selection, the Linux adapter waits until the first editor request
to inspect the socket directory. A single socket candidate (standard or PID name)
can be selected. Multiple socket candidates are ambiguous, **even if one may be
stale**, and no editor request is sent. Regular files and symlink entries are not
automatic socket candidates; unreadable entries prevent selection. A symlink socket
directory requires explicit selection. If no candidates exist, the existing
`api.sock` default remains in effect and a later request can time out normally.

The first selected endpoint stays pinned for that client. Existing instance-token
handling guards subsequent requests after an initial successful reply. The client
does not switch endpoints or retry a mutation after failure. NNG request resends
are disabled with `NNG_DURATION_INFINITE` (`resend_time=-1`), including on disconnect.
See the [NNG request protocol implementation](https://github.com/nanomsg/nng/blob/v1.8.0/src/sp/protocol/reqrep0/req.c#L221-L246).
Restart the
MCP with an explicit endpoint to change instances. Non-Linux automatic discovery is
not enabled by this slice; the prior default path behavior is retained there.

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

`AS_UNHANDLED` and `AS_UNIMPLEMENTED` mean unsupported for the attempted request.
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

Native Linux/WSL application builds, real two-project sessions, installation,
disconnect reconciliation and platform qualification remain separate release gates.
