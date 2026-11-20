# MCP support delivery plan

Priority update, 2026-11-20: the Rust migration has started. Complete native Rust
parity with every Python MCP operation and resource, and audit and close remaining
KiCad function gaps, before browser work. Python forwarding does not satisfy parity.
See the [native parity plan](../../rust/MCP_PARITY_PLAN.md). The P0/P1 qualification
criteria below still apply; their original baseline is historical context. This plan
does not claim that every KiCad feature is exposed or validated today.

## Baseline and meaning of full support

The reviewed baseline has 110 typed MCP tools and 144 generic command registry
entries spanning schematics, boards, symbols, footprints, libraries, checks and
exports. These are adapter registrations; they are not a negotiated capability
list from a running editor. Some commands have document-specific support or lack
undo. The baseline's 86 unit cases did not provide an actual MCP stdio handshake
or a hosted pull-request check.

Full support means every operation in the documented supported desktop workflows
has an explicit contract, a discoverable schema, an implemented editor path and
appropriate failure/round-trip tests. Protobuf declarations alone do not qualify.
Internal transaction primitives, interactive actions, debug commands and operations
that discard unsaved work require an explicit inclusion decision, not automatic
export to increase a command count.

## P0: reliable MCP foundation and reviewed main

### Implemented foundation in this change

- Add `live_mcp_capabilities` and the JSON resource
  `embeint://mcp/capabilities`. They enumerate the loaded tools and local command
  schemas, resolve request/reply descriptors and provide a deterministic schema
  fingerprint. Neither contacts KiCad or claims negotiated live support.
- Add real stdio protocol tests: initialize, discover tools/resources, call the
  offline inventory and existing command-describe tool, read the capability
  resource, and verify invalid/unknown calls return MCP errors.
- Validate generated tool JSON Schemas and all curated command request/reply types.
- Run protocol generation plus unit/protocol tests on Ubuntu for relevant pull
  requests and pushes to `main` and `live-schematic-ipc`.
- Make the local test runner stop when protocol generation fails, rather than
  potentially testing stale generated modules.

Acceptance: a clean checkout installs the listed Python dependencies, generates its
own protocols and passes unit and stdio tests without KiCad or a local API socket.
Existing tool names and argument shapes remain compatible; the additional inventory
tool makes the current count 111. Counts must be computed from registrations.

### Remaining P0 release gates

1. Resolve concrete IPC/MCP review findings and add regression cases. Validate
   document identity, mutation status, transaction ownership and unsupported batch
   operations before touching the document. A rejected batch must not report
   success or claim rollback that was not confirmed.
   Exclude settings edits and other operations outside the editor commit from
   atomic batches; reject requests aimed at a different document. Routing must
   compare DRC findings against a baseline rather than erase unrelated violations.
   Library/document opening must report busy or unsaved-state conditions rather
   than opening modal dialogs that a headless client cannot answer.
2. Run a full native build and guarded live tests against disposable documents.
   Exercise hierarchical sheet creation, pin connectivity, annotation, symbol and
   footprint libraries, schematic-to-board transfer, ERC/DRC and exports. Verify
   failed edits, undo, explicit saves and save/reopen preserve expected content.
3. Add per-workflow live support results keyed to the actual application build and
   schema fingerprint. Report missing editor commands as unsupported, not inferred
   from the application version. Keep local inventory and observed support distinct.

Acceptance: review findings are tracked to fixes or explicit deferrals; required
hosted checks pass; build and disposable-project live logs identify the tested
commit and application version. Unit/protocol success alone is not native sign-off.

## P1: complete desktop workflow contracts and installation

1. Build a maintained operation matrix for project manager, schematic, PCB, symbol
   and footprint editors. Record typed tool or generic-command mapping, document
   requirements, implementation status, disk effects, undo support and test evidence.
   Add a check that requires an explicit coverage decision for new registered IPC
   handlers instead of silently assuming every declaration is supported.
2. Audit currently omitted registry operations: library-item opening, parsing native
   item strings, loading libraries and PCB embedded-file operations. Keep
   `BeginCommit`/`EndCommit` under client transaction ownership. Treat debug DRC
   injection separately. Add only operations with clear semantics and tests.
3. Provide reproducible application/MCP installation, instance discovery and startup
   diagnostics on the first supported platform. Ship dependencies or an explicit
   installer so users do not need the development checkout. Test two open projects,
   stale sockets, missing protocol modules and incompatible application versions.
4. Add deterministic fixtures for all supported export formats and project/library
   save/reopen behavior. Test the exact KiCad 10.0.6 target separately; development
   `10.99` files must not be advertised as backward-compatible without evidence.
5. Qualify disconnects and concurrent manual edits. Define revision checks and an
   operation-result reconciliation contract before enabling mutation retries. A
   timeout leaves an unknown outcome until reconciled.

Acceptance: every supported operation has an implemented editor path and test
evidence in the matrix; clean-machine installation and representative end-to-end
projects pass. Unsupported operations and platform limits are documented.

## P2: broader platform and client support, then portable engines

- Qualify native Windows and macOS builds and client setup, with platform-specific
  IPC transport, packaging and isolated configuration checks.
- Add document/context resources and useful workflow prompts when their update and
  invalidation semantics are defined. MCP permits a tools-only server; optional
  protocol features do not substitute for correct editing and validation.
- Decide whether to offer a remote browser gateway. Remote sessions need a separate
  authentication and project-access design; KiCad's instance token is not evidence
  of an authentication system.
- After P0/P1 gates pass, port one deterministic layout operation to Rust and
  compare Python/native/WASM results. Migrate the MCP executable incrementally while
  retaining existing tool contracts and native KiCad validation.

Acceptance: each new host/client combination passes the same protocol and workflow
contracts. Portable components must demonstrate behavior parity before replacing
the desktop implementation.

See [the fork roadmap](EMBEINT_FORK_ROADMAP.md) for the longer-term architecture.
