# Porting Embeint KiCad to Rust: plan

Status: reviewed 2026-11-21. The phase 0 harness and extractor and its three layer-0 pilots
are implemented and merged into `rust`; phase 0 is accepted only when the strict gate passes in
hosted CI (see [Next step](#next-step)). The review's answers and the recommendations it
left standing are recorded under [Decisions](#decisions). This plan extends the
[fork roadmap](../tools/live_schematic_mcp/EMBEINT_FORK_ROADMAP.md), which calls broad engine
replacement "a multi-year program", and the [native MCP parity plan](MCP_PARITY_PLAN.md), which
is the first finished example of the method below on a small scale. The native MCP serves the
whole Python surface and matches it on every offline fixture; its live qualification, the
Python-free package and the wider function audit (criteria 2 to 5 of that plan) continue on
`main` alongside phases 0 to 4, and must be finished before phase 5 uses the live suites as its
gate.

## Branches

The plan and the native MCP work land on `main`. The port itself (the unit records in a
top-level `port/` directory, the harness and the new crates in `rust/`) is developed on a
long-lived `rust` branch, created from `main` when phase 0 starts: port PRs target `rust`,
`main` is merged into `rust` regularly (never rebased: many agents share it), and `rust` merges
back into `main` when the port is done, or earlier for parts that are ready to ship on their
own, such as the phase 4 CLI. The work is public, like the rest of this repository, including
the progress site.

The repository's branch rules (the root `AGENTS.md`, `tools/live_schematic_mcp/AGENTS.md` and
`CONTRIBUTING.md`) name only `main`, `live-schematic-ipc` and `master`; phase 0 adds the `rust`
branch to them when it creates it. Upstream reaches `rust` only through `main`.

## Goal

A Rust implementation of the whole KiCad application: the project manager, the schematic and
symbol editors, the PCB and footprint editors, the 3D viewer, Gerber viewer, drawing sheet
editor, calculator and simulator, the file formats, the checks (ERC, DRC), the manufacturing
outputs and the IPC API. It reads and writes the same files as KiCad, gives the same results
for the same designs, and serves the same IPC API, so the MCP servers and the end-to-end suites
drive either application unchanged. The same application, from the same code, runs on the
desktop and as WebAssembly in a browser, as SolidCraft does.

C++ KiCad stays the reference and the shipped product until the Rust application passes every
gate below on the test corpus.

## The method, taken from solidcore

[solidcore](https://github.com/tangency-labs/solidcore) is the Rust port of NVIDIA's SMLib
B-rep kernel inside SolidCraft. Its 260 units (about 388,000 upstream lines) were all ported by
a coordinated agent swarm between the repository's creation on 12 October 2026 and the
23 October status snapshot; its deferred paths have been wired since. What made that work, and
what we reuse:

| solidcore | Here |
| --- | --- |
| A call graph of the C++ (static calls plus every override of a virtual call) from the compile database | The same, from KiCad's `compile_commands.json` (both the WSL and Windows builds have one) |
| Units of about 2,000 upstream lines, ordered in topological layers; cycles broken by a weighted feedback-arc set, the dropped calls recorded as back-edges | The same. KiCad's item/board/tool core is one large cycle too |
| One `plan/units/<id>.toml` per unit: upstream files, Rust module, `depends_on`, `soft_depends_on`, layer, status `todo/claimed/partial/green/blocked` | The same schema, in `port/units/` |
| A call into an unported unit returns `Err(Unported { upstream, unit })` with a `// UNPORTED(<unit>)` comment; cases that reach one are *deferred*; a unit with deferred cases is `partial`, never `green` | The same. No panics, no faked results |
| Difftest: the C++ built with fixed flags (`-ffp-contract=off`, no fast-math), shims run each unit's cases on both sides, JSON out, compared exactly | The same for headless units, through a `kicad-oracle` binary linked against KiCad's own libraries (as `qa/` already does) |
| Goldens: every unit's cases and results committed per unit; any change must be blessed with numbers and pictures | The same, per unit, once a unit's difftest is green |
| Records are directories of per-entry files (`findings/`, `upstream-issues/`), never one shared file; claims are draft PRs titled `port: <unit>` | The same |
| A progress site generated on every merge | The same, public on GitHub Pages |
| No `unsafe`, no panics on bad input, deterministic results, no global state, builds for `wasm32-unknown-unknown` | The same for every portable crate |

SolidCraft's application shows the shape of the result: layered crates, an engine where every
edit is a command, an egui/wgpu front end that can be swapped, a headless CLI, an MCP server
that runs headless or bridged to the app, and a web build.

## What is different about KiCad

- **It is an application, not a library.** About a third of KiCad is user interface: wxWidgets
  frames, 301 wxFormBuilder dialogs (`.fbp`), widgets, the tool framework's interactive loops and
  the OpenGL/Cairo renderer. That part cannot be ported line by line to a different toolkit. It
  is re-implemented against a specification taken from the C++ (the `.fbp` files list every
  field of every dialog; `TOOL_ACTION`s list every command), and tested through behaviour.
- **The file formats are the contract.** A board written by the Rust application must load in
  KiCad and round-trip byte for byte. That gives an oracle solidcore did not have:
  `kicad-cli` loads and writes every format and produces every output.
- **The IPC API is an application-level oracle.** It serialises every item of an open document.
  Our MCP parity harness already compares two implementations through it. Pointed at a Rust
  engine that serves the same protocol, the existing `e2e_*` suites and `mcp_parity.py --live`
  become acceptance tests for the whole engine.
- **Upstream moves daily.** SMLib was pinned at one release. KiCad `master` changes every day and
  the fork merges it. The port pins one upstream commit per wave and re-baselines on a cadence;
  the call-graph tool reports which ported functions changed since the pin.
- **Licence.** A port of KiCad is a derived work: GPL-3.0-or-later, like the rest of this
  repository. solidcore (Apache-2.0) is compatible and can replace OpenCascade for STEP and 3D.
- **wxWidgets types run through the "headless" code.** `wxString`, `wxFileName`, `wxPoint` and
  `wxConfig` appear in the document model. The call graph classifies every function by its wx
  dependencies so the plan knows which units are pure, which need a thin replacement (strings,
  paths), and which belong to the UI.

## Size

Measured at fork source `3cd2c9afc6d2749b7d3a79fd1deaa86ac51cb3a7` and upstream pin
`4cf939778df04d2b747bb2c4714fc5014a7565c7`. The tracked inventory is unchanged at
9,401 C/C++ files. Corrected extraction additionally
freezes 178 generated build inputs under explicit
logical `@build/` locations, backed by committed source bytes and generation
provenance. Counts are physical lines, with tracked CRLF normalized to LF.

| Area | Lines |
| --- | ---: |
| Tracked application and generated C/C++ | 1,946,361 |
| Tracked application / generated | 1,836,426 / 109,935 |
| Tracked pcbnew / common / eeschema | 656,560 / 448,496 / 440,108 |
| Additional generated application build inputs | 424,513 |
| Generator tooling probes (separate) | 1,883 |
| Tracked QA (separate) | 340,033 |
| Tracked third-party (separate) | 1,059,199 |

The corrected graph yields **1,801 units**, owning **2,380,947
physical lines**: tracked application/generated sources, additional generated
application inputs, and Clipper2; QA and other third-party/tooling sources remain
separately inventoried. There are **392 topological layers**,
**9,548 back-edges** (dropped weight 106,538)
and 46,894 hard dependency edges.

| Propagated wx classification | Units | Owned physical lines |
| --- | ---: | ---: |
| pure | 553 | 666,688 |
| thin-replacement | 101 | 53,714 |
| UI | 1,147 | 1,660,545 |

Coverage remains three separate measurements: raw **2,462/2,471**
successful parses (four stale/deleted exclusions and five other failures), reviewed
reconciled **2,507/2,511**, and corrected
**2,507/2,511**. Both reconciled runs retain
four fmt GCC-module failures. No historical AST result was relabelled as corrected.
The corrected graph owns 37,838 generated-build
definition entries that the reviewed graph omitted. Exact counts, diagnostics and
independent hashes are in [the measurement](../port/extractor/generated/measurement.json).

These are scope/coupling measurements, not a measured implementation rate or
literal UI effort. 282 units exceed the 2,000-line target,
retaining function boundaries and conservative enclosing ranges. wx-pure does not
establish portability; I/O, globals and supporting types still require audit.
64 units remain blocked. Implicit lifetime paths,
indirect/dependent calls and inactive platforms/configurations remain explicit
limits. The measurement itself ports no unit. The three layer-0 pilots (MD5, IEC60664 and
the resistor calculator) were then implemented against the corrected records, re-confirming
the [pilot scopes](../port/extractor/PILOTS.md), and are merged into `rust`. Extraction parsing
and all-configuration coverage remain incomplete: four parse failures and 63 uncompiled
application sources.

## Architecture of the Rust application

Layered like SolidCraft. Portable crates contain no filesystem, sockets, processes or GUI
types; they build for wasm.

| Layer | Crates (working names) | Contents |
| --- | --- | --- |
| L0 | `kicad-math`, `kicad-geom` | Integer-nanometre vectors and boxes, `SEG`, `SHAPE_*`, `SHAPE_POLY_SET`, Clipper2, arcs, approximations |
| L1 | `kicad-core` | KIID, units, LIB_ID, strings and paths, the property system, text variables, expressions (`libeval`), fonts |
| L2 | `kicad-sch`, `kicad-pcb`, `kicad-sheet` | The document models: symbols, sheets, footprints, pads, tracks, zones, the drawing sheet |
| L3 | `kicad-io` | The s-expression lexer and every KiCad format, project and library tables; later the importers |
| L4 | `kicad-connect`, `kicad-erc`, `kicad-drc`, `kicad-fill`, `kicad-router` | Connectivity and netlists, ERC, the DRC engine and rule language, the zone filler, the push-and-shove router |
| L5 | `kicad-plot`, `kicad-export` | Plotters (Gerber, drill, PDF, SVG, DXF), placement, BOM, netlists, IPC-2581, ODB++, IPC-D-356, STEP through solidcore |
| L6 | `kicad-engine` | Sessions, commits and undo, selection, every tool action as a command, jobs |
| L7 | `kicad-api`, `embeint-mcp` | The IPC API server (same protobuf contract) and the existing native MCP |
| L8 | `kicad-gpu` | Every picture: wgpu pipelines for the schematic, symbol, board, footprint and drawing-sheet canvases and the 3D view, used by the editors, the CLI and MCP screenshots alike |
| L9 | `kicad-ui` | The egui front end: one application for desktop and browser (see below) |
| apps | `kicad-rs`, `kicad-rs-cli`, `kicad-rs-web` | Desktop application (eframe native), headless CLI (the `kicad-cli` replacement), browser build (eframe web runner, built with trunk) |

## The user interface, following SolidCraft

SolidCraft's GUI is the model, because it already runs the same application on the desktop and
in a browser. How it is built (checked against `tangency-labs/solidcraft` at `bd2dc3fade`):

- **The UI owns no design data.** `solidcraft-ui-egui` is "a thin shell over
  `solidcraft_engine::Session`": panels read engine state and act only through engine commands.
  Toolbar buttons, the command palette, scripts, the control channel, MCP and the browser's
  JavaScript API all run the same commands with the same undo history.
- **Drawing lives outside the UI.** The `solidcraft-gpu` crate holds every wgpu pipeline; the UI
  hands them egui's device and render pass through an `egui_wgpu` paint callback. Headless CLI
  snapshots and MCP screenshots use the same pipelines, so a picture in a test is the picture
  on screen. The scene is retained in GPU buffers; egui redraws only the chrome.
- **One application, two hosts.** The desktop app (`apps/solidcraft`, about 10,000 lines) and
  the browser app (`apps/solidcraft-web`, about 3,400 lines) both run the same `SolveApp`. The
  browser host adds what a browser needs: the origin private file system (OPFS) behind virtual
  `/opfs/` paths, File System Access import and export, a service worker for offline use and
  updates, settings shared between tabs, launch URLs, an embed mode and keys left to the
  browser. Platform differences inside the UI are `cfg(target_arch = "wasm32")` branches.
- **Rendering backends:** wgpu, so WebGPU in the browser with a WebGL2 fallback (`?webgl`
  forces it), and Vulkan, DX12 or Metal on the desktop.
- **A UI kit with tokens.** Every dialog, panel, card and menu uses shared controls and theme
  tokens (light and dark), and simple dialogs are typed records generated by a macro
  (`dialog_descriptor.rs`: command, title, selection inputs, fields with defaults).
- **The UI can be driven and inspected.** The control channel publishes UI handles (`ui.at`,
  `ui.click`, `ui.inspect`, `ui.screenshot`), so agents and tests drive the real interface. In
  the browser, `window.solidcraft` exposes the same methods, and `cargo xtask web-api-test` checks
  a built site in headless Chrome.
- **Strict code rules:** `forbid(unsafe_code)` and no `unwrap`, `expect` or `panic` in the UI.
  egui, egui-wgpu, winit and AccessKit are vendored where fixes were needed.

Applied to KiCad:

- **Every `TOOL_ACTION` becomes an engine command** with its name, hotkey, icon and menu text as
  metadata. Interactive tools (route, drag, draw, move) are state machines in the engine that the
  UI feeds pointer events, so they can be tested headless, through the IPC API and in the UI.
- **Dialogs are generated, then finished by hand.** The 301 `.fbp` files list every field of every
  dialog; a generator turns each into a typed descriptor (command, fields, types, defaults,
  validators) for the UI kit, and a person finishes the layout. The descriptor is the dialog's
  specification and its test fixture.
- **The canvases are retained GPU scenes** in `kicad-gpu`, rebuilt incrementally from the
  engine's change notifications, like KiCad's GAL view items. egui draws the panels, property
  grids, tables and menus around them.
- **The application serves the IPC API and a control channel with UI handles**, so the
  existing `e2e_*` suites and MCP drive both the engine and the interface, natively and in the
  browser (`window.kicad`).

SolidCraft's UI crate is about 300,000 lines of Rust including its tests, for one modelling
application. KiCad has five editors and several smaller tools, so the UI is the largest single
part of the port and is sized accordingly below.

## WebAssembly and the browser

The goal is the same KiCad application in a browser, from the same code, with no server
required for local work. That is a constraint from the first phase, not a final port:

- **Every crate except the two app shells builds for `wasm32-unknown-unknown`,** checked in CI
  from phase 0. That rules out filesystem, sockets, processes and threads in portable crates:
  they take bytes and a file-system trait, and the hosts supply native paths or OPFS.
- **Threads.** KiCad fills zones, runs DRC and builds connectivity on a thread pool. Portable
  crates take an executor: a thread pool natively, and in the browser serial at first, then wasm
  threads (which need cross-origin isolation headers) or workers. As in SolidCraft, the first
  browser build runs the engine on the main thread; long jobs move to a worker broker later.
- **Projects are folders.** A KiCad project is several files plus library tables. The browser
  host opens a folder with the File System Access API where the browser has it (Chromium), and
  otherwise imports and exports a zip of the project into OPFS.
- **Libraries.** KiCad's official symbols, footprints and 3D models are too large to bundle. The
  browser fetches them from a static, versioned library site on demand and caches them in OPFS;
  project-local libraries come with the project. Database libraries (ODBC) are desktop only;
  HTTP libraries work in both.
- **Fonts.** The stroke font is data and ships with the app. A browser cannot list system fonts,
  so outline fonts come from a bundled set or from fonts the user adds.
- **Renderer.** The canvases must work on WebGL2 as well as WebGPU, so they cannot depend on
  compute shaders; compute is an optional accelerator, as in SolidCraft.
- **What stays desktop only at first:** the simulator (ngspice is C; a wasm build of it is
  possible later), database libraries, Git integration, and anything else needing native
  libraries. Each is a capability the application reports, like SolidCraft's
  `render.backends`, so the UI and agents know what this host can do.
- **Agents in the browser** use `window.kicad`, the same commands as the IPC API, not a second
  protocol.

The browser build is the independent browser document engine that roadmap gate 5 puts last.
That gate's earlier step, a browser edit reaching the live desktop model through the gateway, is
separate from the port and stays on the roadmap.

## Oracles

Every port step is decided by comparison with C++ KiCad, at three levels:

1. **Unit difftest.** `kicad-oracle` links the same libraries the `qa/` executables already link
   (`kimath`, `common`, `pcbcommon`, `connectivity`, `pnsrouter`, the pcbnew and eeschema kiface
   objects). Each unit gets a shim that takes JSON cases,
   calls the C++ function and writes JSON results. The Rust runner does the same, and the
   results must be identical. Cases come from `qa/`, fixed-seed generators (degenerate, boundary
   and huge/tiny inputs) and the corpus.
2. **Files and outputs.** The corpus is the 428 boards and 366 schematics in `qa/data`, the
   demos, and KiCad's official symbol and footprint libraries at a pinned commit. Only public
   test data is used, at every level: no Embeint design enters the corpus, the goldens, the
   cases or the progress site. Gates:
   - load and save is byte-identical to `kicad-cli pcb upgrade` / `sch upgrade`;
   - ERC and DRC reports equal `kicad-cli sch erc` / `pcb drc` JSON;
   - Gerber, drill, placement and netlist files are identical, after normalising timestamps;
   - PDF, SVG and renders pass an image comparison with a stated tolerance.

   The fab package checks written for PR #24 already read Gerber job files, Excellon,
   placements and BOMs.
3. **The application.** The Rust engine serves the IPC API. The existing `e2e_*` suites,
   `live_parity.py` and `mcp_parity.py --live` run against it and against C++ KiCad on the same
   disposable project, and the documents must end up identical item by item.

A golden proves a result unchanged, not correct; the C++ is the reference for correctness until
a finding records a defect in it.

## Phases

Each phase ends at a gate that is checked in CI, not estimated.

0. **Harness.** Create the `rust` branch and pin the upstream commit. Add the west manifest
   for solidcore (see [External code](#external-code)). Build the call-graph extractor on the WSL
   compile database (clang), the unit splitter, layering and back-edges, and the unit files.
   Build `kicad-oracle` with fixed compiler flags on Linux in WSL (its own build tree, with a
   capped job count on the shared machine), the goldens runner and the public progress site on
   GitHub Pages; add CI for fmt, clippy, tests, the wasm check and goldens. Write
   `port/AGENTS.md`: the rules above, adapted. Pilot three layer-0 units end to end before opening
   the work to a swarm.
   *Gate: three units green against the oracle in CI; every unit file generated and valid.*
1. **Foundation.** `kimath` and geometry, Clipper2, core types, the s-expression lexer
   (keyword tables generated from the 13 `.keywords` files by a build script), fonts.
   *Gate: all L0/L1 units green or partial.*
2. **Documents and formats.** Both document models and every KiCad format, the project file
   and the library tables. *Gate: the whole corpus round-trips byte for byte; the item dump of
   every corpus document equals KiCad's over the IPC API.*
3. **Checks.** Connectivity and netlists, ERC, DRC with the rule language, zone filling,
   stackup. *Gate: ERC and DRC reports and filled zones identical on the corpus.*
4. **Outputs and the CLI.** Plotters and exporters, jobsets and `kicad-rs-cli`. This is the
   first thing Embeint can ship from the port: a headless, Python-free CLI for manufacturing
   outputs, which the MCP can also use without a running editor. *Gate: `kicad-cli` output
   parity on the corpus; the fab package verifies the same files; the same outputs generated
   in a browser test page match the native ones.*
5. **Headless editing.** Commits and undo, selection, every tool action as a command, the
   router, update-from-schematic and the IPC API server, including this fork's own handlers.
   *Gate: the end-to-end suites and live MCP parity pass against the Rust engine.*
6. **Rendering and the viewer.** `kicad-gpu` pipelines for schematic, symbol, board, footprint
   and drawing sheet, and the 3D view on solidcore's tessellation, on WebGPU, WebGL2 and the
   desktop backends. The first application is a read-only viewer: the egui shell with the
   project tree, the canvases, layer controls, selection and properties, natively and in the
   browser (OPFS, folder or zip open, the library site). This can start once phase 2 has the
   documents, in parallel with phases 3 to 5. *Gate: renders match `live_*_view` output within
   the stated tolerance, on every backend; the corpus opens in the browser build.*
7. **The editors.** Editor by editor on the same shell, desktop and browser together: project
   manager, schematic and symbol editors, PCB and footprint editors, then Gerber viewer,
   drawing sheet editor and calculator, and the simulator on the desktop (ngspice behind its
   interface). Dialogs come from descriptors generated from the `.fbp` files. Importers (Altium,
   Eagle, CADSTAR, EasyEDA, PADS and the rest) run as a parallel track from phase 2. *Gate per
   editor: its tool actions and dialogs covered by behaviour tests through the control channel,
   natively and in headless Chrome; corpus projects edited, saved and reopened in C++ KiCad.*
8. **Browser hardening.** Long jobs (zone fill, DRC, outputs) in a worker, wasm threads where
   the hosting allows cross-origin isolation, offline use and updates through a service worker,
   storage quotas, and installation as a web app.
9. **Cutover.** Ship the Rust application beside C++ KiCad, and the browser build on a static
   site. Make it the default once it passes every gate on the corpus and Embeint has used it
   for its own work for an agreed period.

## Rules for the work

From solidcore's `AGENTS.md`, adapted:

- Edit only your unit's `port/units/<id>.toml`. Every finding or upstream defect is a new file.
- Port every branch, default argument and special case the C++ has. A function that stops at the
  first unported call is not ported: the stub replaces only that call, and the rest of the
  algorithm follows it.
- Never loosen a comparison, drop a case or bless a changed golden without the cases, their old
  and new values and the reason.
- Coordinates stay integer nanometres; floating-point code matches the oracle's flags.
- Every crate but the app shells builds for `wasm32-unknown-unknown`; none touches the
  filesystem, sockets, processes or threads directly. `forbid(unsafe_code)` and no `unwrap`,
  `expect` or `panic` throughout, the UI included, as in SolidCraft.
- The UI holds no document state and changes nothing except through engine commands.
- Claims are draft PRs. Under this repository's rules (`AGENTS.md`), nothing goes to upstream
  KiCad.

## Effort

solidcore's 388,000 lines were all ported in under two weeks of coordinated agents, with the
harness built first, and wiring the deferred paths took longer than the porting. KiCad's
headless part is about three times that size, so phases 1 to 4 are plausibly two to four months
once phase 0 works. Phase 5 adds one to two months. The user interface (phases 6 and 7) has no
mechanical method and is the long pole. SolidCraft's UI is about 300,000 lines for one
modelling application, and KiCad's five editors are at least comparable: six to twelve months or
more for editors Embeint would use daily, with the read-only viewer, in the browser too, much
earlier. Because the browser
is a host of the same application, it adds weeks of host work (storage, libraries, workers), not
a second UI. These figures assume swarm throughput similar to solidcore's and will be
re-estimated from the phase 0 pilot.

## External code

solidcore comes in with [west](https://docs.zephyrproject.org/latest/develop/west/), from a
manifest on the `rust` branch that pins `tangency-labs/solidcore` to a commit; the port's crates
use it as path dependencies into the west workspace, so CI and contributors run `west update`
before `cargo build`. Upgrades are a manifest change in a PR.
west is a development tool (Python); it is not part of anything the port ships.

solidcore is patched inside SolidCraft, not in its own repository: SolidCraft vendors it with
`git subtree` under `solidcore/`, that tree is the source of truth, and SolidCraft's
`solidcore-mirror.yml` workflow pushes every commit that touches it to `tangency-labs/solidcore`
as a fast-forward, with a `solidcraft: <sha>` trailer. The standalone repository therefore has
the same kernel, a few minutes behind, and builds on its own (Apache-2.0, Rust 1.85 like this
workspace). Consequences:

- Pin solidcore commits, never a branch, and read the trailer to know which SolidCraft commit a
  pin corresponds to.
- A fix the port needs in solidcore is made as a SolidCraft PR to `solidcore/`; it reaches the
  standalone repository through the mirror. A PR to `tangency-labs/solidcore` itself would make the
  two diverge, and the mirror, which only fast-forwards, would stop.
- SolidCraft's patched egui and winit live in its `vendor/` directory, which is not mirrored.
  They are not needed before phase 6; whether to take them from a pinned SolidCraft checkout in
  the same west manifest, or ask for them to be mirrored like solidcore, is decided then.

## Decisions

Reviewed 2026-11-21. Items 10 to 14 were decided in review. Items 1 to 9 were not discussed;
they stand as recommendations, with the amendments noted, and phase 0 proceeds on them unless
they are changed.

1. **Scope of "full".** Every application, with the importers as a parallel track and ngspice
   kept as a C library rather than ported. The SWIG Python API is not ported (upstream is
   replacing it with the IPC API, which is ported); Python action plugins and footprint wizards
   are replaced by Rust plugins (item 13).
2. **Faithful port for the headless code, re-implementation for the UI.** Byte- and bit-identical
   results are the gate for the first; behaviour tests for the second.
3. **UI toolkit:** egui and wgpu through eframe, as SolidCraft uses, with SolidCraft's
   structure: an engine that owns everything, a UI that only reads state and runs commands, a
   separate GPU crate for every picture, and one application for desktop and browser. Its
   vendored fixes and UI-kit approach can be shared.
4. **Browser targets:** Chromium-based browsers fully (WebGPU, folder access); Firefox and Safari
   with WebGL2 and zip import and export. The browser build is a static site that needs no
   server for local work; libraries come from a static, versioned library site.
5. **Location:** this repository's `rust/` workspace, with the unit records in a top-level
   `port/`, next to the C++ that is the reading reference and the oracle, on the `rust` branch (see
   [Branches](#branches)). Upstream has no `rust/` or `port/`, so merges cannot conflict.
6. **Upstream pin:** the upstream commit of the fork's current base, re-baselined each quarter
   with the call-graph drift report deciding which units need re-checking.
7. **Transition:** no in-process bridge into the C++ application by default. The Rust engine
   meets C++ KiCad at the file formats and the IPC API. A `cxx` bridge is added only where a
   measured need justifies it.
8. **STEP and 3D:** solidcore instead of OpenCascade, pulled in with west
   ([External code](#external-code)).
9. **Who works on it:** a coordinated agent swarm like solidcore's, with a lead agent splitting
   the work and the Embeint team reviewing gates and blessed goldens.
10. **Branches:** `main` for the plan and the native MCP; a long-lived `rust` branch for the
    port until it is done ([Branches](#branches)).
11. **Public:** the port, its records and the progress site are public, like the repository.
12. **Platform:** the port and its oracle are built and compared on Linux, in WSL, for now.
    Windows (MSVC) builds and comparisons are a later decision.
13. **Plugins:** Python action plugins and footprint wizards are replaced by Rust plugins.
    Recommended shape, decided with the plugin interface in phase 5: SolidCraft's model, where a
    plugin is a WebAssembly component (`craft-plugin`, `craft-plugin-wasm` on wasmtime) with a
    manifest and permissions that runs engine commands. SolidCraft runs such plugins only in
    native x86-64 and AArch64 builds today; its browser build refuses them, so running plugins
    in the browser is open work for phase 5. KiCad's built-in footprint wizards are ported as
    engine commands.
14. **Test data only:** the corpus, cases, goldens and published results use public test data
    only (`qa/data`, the demos, KiCad's libraries, generated cases); no Embeint design is used.

## Risks

- **Upstream drift** while the port runs. Mitigated by the pin, the drift report and quarterly
  re-baselining. Some features will lag C++ KiCad.
- **Bit-identical floating point.** Fills, arcs and approximations must match. solidcore met
  this with fixed compiler flags and exact comparison. Text shaping (FreeType/HarfBuzz against
  Rust equivalents) may need a stated tolerance.
- **Hidden UI coupling** in the document and tool code. The call graph's wx classification
  measures it before the plan is fixed.
- **User expectations of the UI.** A re-implemented editor will differ from KiCad in details
  users rely on. Embeint's daily workflows, reproduced on corpus projects, define the acceptance
  tests for phase 7.
- **Lost Python plugins and footprint wizards.** Third-party Python plugins stop working; users
  must port them to the Rust plugin interface (decision 13) or drive the application over the
  IPC API instead. Plugins in the browser need a WebAssembly component runtime that SolidCraft
  does not have yet; until one exists, plugins are a desktop capability.
- **solidcore's history comes through SolidCraft.** A stalled mirror workflow leaves the
  standalone repository behind SolidCraft; pins record the SolidCraft commit so this is visible.
- **Browser limits.** Main-thread work blocks the page until jobs move to workers; OPFS quotas
  limit large projects and 3D libraries; folder access is Chromium-only; WebGL2 lacks compute.
  Each has a fallback above, but large boards in the browser need measuring early (phase 6).
- **Dense tables in egui.** KiCad's symbol fields table, net inspector and DRC lists hold
  thousands of rows. egui can virtualise them, but they need performance tests on the corpus.
- **Scale of review.** solidcore's lesson is that ported code passed CI yet still departed from
  upstream behaviour; its pre-ready checklist (default arguments, dropped parameters, special
  cases, status handling, generated case coverage) is part of `port/AGENTS.md`.

## Next step

Historical: the plan review set phase 0 on the `rust` branch to start with the call-graph
extractor on the WSL build. Its unit list with sizes, layers, cycle back-edges and wx
classification has since replaced the estimates above with counts, and the three layer-0
pilots are merged.

Now: the phase 0 integration switches `PHASE0_REQUIRED` on, and its strict run in hosted CI is
the acceptance authority for the phase 0 gate. Once that passes, phase 1 opens as described in
[Phases](#phases). The known extraction coverage gaps stay reported separately.
