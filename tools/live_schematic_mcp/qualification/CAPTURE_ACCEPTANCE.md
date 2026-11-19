# Schematic capture acceptance

`capture_acceptance.py` reuses the owned-process lifecycle, explicit saves and MCP
stdio check in `qualification/run.py` and the item normalization in
`qualification/persistence.py`; it follows the same explicit-save principle as
`e2e_persistence.py` but does not import or run that test. It owns a new
project, settings directory, IPC namespace, Xvfb display and manager processes.
It never attaches to a discovered editor. All project parts are deliberately
synthetic test models, with explicit electrical types, fields and stock test
footprints. They are not manufacturer-qualified device models.

The root power block supplies two child sheets, MCU and Interface, through real
parent pins and hierarchical labels. Inside the blocks, Manhattan wires connect
pins directly, with explicit junctions at T joins. The MCU has a hidden stacked
ground pad, a reserved NC and an intentionally unconnected reset. A two-unit
buffer exercises physical reference U2 versus displayed U2A/U2B. The interface
has a six-pin ESD model with USBLC6 flow-through topology: 1/6 and 3/4 require
external bridges. Manufacturer and MPN fields and footprints belong to the
project-local `Acceptance` library. No client design is used.
Stock native VDD/GND power symbols identify rails. Their typed, nonphysical pins
are excluded from the physical pin/pad intent; complete item snapshots retain
the power symbols and their identities.

## Electrical intent

`connectivity_intent.py` compares a complete declaration of `REF.PIN` sets against
native `GetSymbolPins` and netlist observations. It retains individual identities
and duplicate counts. It reports missing/extra pins, split/merged nets, duplicate
pin identities, disconnected pins and displaced stacked pins. Multi-unit display
suffixes are mapped through native item UUIDs to physical references, never
removed with a string heuristic. Pin types can be checked independently.

Intent is a partition of physical pins, independent of native net naming. A
no-connect declaration is a singleton partition: the native generated
`unconnected-(...)` name is permitted, but connection to another pin is not.
Board parity independently checks every numbered pad, including hidden stacked
pads, extra pads, missing pads, duplicate footprints and pad net assignments.
Multiple physical pads with the same number must all match the schematic net.

The negative ESD regression removes both external bridge routes. Every ESD end
still connects to a connector or MCU pin, so ERC can report zero while DM and DP
are split. The connectivity oracle must reject both splits; undo must restore the
exact native pin/net state. ERC findings, excluded findings and disabled-category
counts are retained as returned. The driver does not change ERC/DRC severity or
exclusions. DRC results describe an unrouted test board, not manufacturing sign-off.

## Owned native run

Generate protocols from this checkout and use an existing compatible venv. Select
a native prefix and the retained `native-provenance.json` that lists **every**
file of that install with its SHA-256, the d880 source revision, the exact
four-file loader patch blobs and the retained source archive. Hashes and source
attribution come from that one record, and the archive beside it must match the
hash it names. Before launch and again after all owned processes stop, the driver
verifies all manifest files and the private dependency libraries on
`LD_LIBRARY_PATH`. For the seed manager and the reopened manager, it reads
`/proc/<pid>/maps` after their editors have run. Every loaded install module
must appear in the manifest with identical bytes, every private dependency must
be unchanged and a deleted mapped file fails the run. System libraries are
hashed and recorded. The driver records the measured application version,
adapter revision and dirty state, executed source bytes, checkout-independent
Git blob ids and the adapter schema manifest. It checks the adapter source and
worktree status again at the end, and does not rebuild or relabel the retained
binary.

KiCad embeds its install prefix in `libkicommon`, the library that carries the
API code every editor shares. The same build installed under two prefixes
therefore differs there even when `kicad` and both kifaces are byte-identical.
Run the staged package at the prefix its manifest describes. On WSL, mount it at
`/opt/embeint-kicad` inside a private namespace that also replaces WSLg's shared
X11 socket directory. Run from the T3 checkout, choosing a **new** output
directory outside `/tmp`:

```sh
python tools/live_schematic_mcp/generate_proto.py
R=/path/to/retained/d880-windows-wsl
bwrap --bind / / --tmpfs /tmp --tmpfs /opt \
  --ro-bind "$R/stage-package/opt/embeint-kicad" /opt/embeint-kicad \
  --dev-bind /dev /dev -- \
  /path/to/existing/venv/bin/python \
  tools/live_schematic_mcp/qualification/capture_acceptance.py \
  --install /opt/embeint-kicad \
  --native-provenance "$R/artifacts/native-provenance.json" \
  --library-root /path/to/kicad/share \
  --git-executable '/mnt/c/Program Files/Git/cmd/git.exe' \
  --output /path/to/new/evidence
```

If the retained build needs private dependency libraries, pass its existing
`LD_LIBRARY_PATH` into the namespace. No dependencies are installed or modified.
Xvfb starts inside the cleanup scope with a display-allocation deadline. The
source MCP stdio checks have explicit time limits. Manager, display, project and
config cleanup steps run independently, and any cleanup failure fails the
receipt. The driver exits non-zero unless the result is `passed`.
Windows-owned worktree `.git` pointers require Windows Git under WSL; ordinary
Linux checkouts use the default `git`. Optional `--helper-probe /path/block_example.py`
runs the coordinated helper's isolated probe serially on the empty test sheet,
before capture, and records its commit and file hashes. A failed/unknown probe
stops the run without mutation replay.

The driver records exact failed-batch rollback, transaction undo/redo, no implicit
saves, actual schematic-to-PCB transfer, pin/pad parity, page fits and distinct
framed renders. Guarded `UpdateItems` probes send each placed buffer unit's
complete read-back definition, with only the value changed, as a separate
request, and record the unit and symbol id. A probe counts only when the re-read
value and serialized items show that the update took effect. Duplicate
multi-unit pin IDs and missing library nicknames are labelled `reproduced`, or
`historical_not_reproduced` only when every unit's update demonstrably applied;
otherwise the label is `inconclusive`. Observations are stored before restore
assertions. A rejected update is recorded without an undo, because the client
already dropped its commit. Every committed probe is undone and the exact items
and pins are compared. Absence of reproduction is not evidence for other
versions or paths. The negative ESD probe likewise restores exact items, not
only pins.
Source MCP stdio checks cover bundled skill initialization/tool/resource delivery
and an actual create/undo operation against the owned native editor.

After explicit schematic/board saves, the driver stops only its manager process
group, starts another manager for the same owned project, reopens both editors,
and compares normalized complete items, hierarchy, net membership, pin identities,
board parity and saved-file hashes. Reports, snapshots, framed PNGs, project and
settings are retained in the new output directory. Failed attempts remain separate.

`capture_persistence.py` extends the shared normalization only for inactive-unit
library pin UUIDs (not stored as placed pins), library-child collection order,
and board-level outline parents equal to the verified native board root. Active
and common pin identities, unit metadata, geometry and pad-to-footprint parents
remain exact. The original raw evidence established that inactive pin UUIDs are
absent from the saved schematic while every active pin UUID is present. Caption
multiline state and marker dimensions are set explicitly at creation rather than
discarded during comparison.

This is **scripted source/native fixture evidence**. Installed/frozen distribution
checks and independent fresh-agent skill evaluation are separate receipts. The
driver always labels independent fresh-agent evaluation pending.

## Offline regressions

```sh
python -m pytest tools/live_schematic_mcp/test_connectivity_intent.py \
  tools/live_schematic_mcp/test_capture_runtime.py
```

These tests include the ERC-zero ESD split, a missing hidden stacked pad, merged
nets/connected NCs, duplicate identities across units, unexpected pins, native
reference mapping, and board mismatches, including duplicate schematic pins and
unknown footprints. The runtime tests reject a libkicommon from another prefix
with identical editor binaries, a source archive that differs from its record, an
unlisted or deleted loaded module and a changed private dependency. They also
cover a stalled partial display line and cleanup that continues after one step
fails. Against a fake two-unit editor, they check that an ignored update is
`inconclusive`, that a rejected update is never undone and that a dropped
nickname is `reproduced` per unit.

## Recorded Windows/WSL acceptance, 2026-11-19

The [bounded receipt](evidence/capture-windows-wsl-2026-11-19.json) identifies the
clean tested adapter commit `d644f74209e4bd736fe5b2d5a2b2417baa9ceb35`, actual
source bytes, retained native binaries, loaded modules and schema independently.
The native version remains `10.99.0-qualification-d880-loader-patch-dirty`:
d880 plus the four recorded loader source blobs, without a new C++ build.

All 281 offline tests passed on that committed source, including 13 focused
intent/persistence regressions. The new owned native run passed three-sheet
capture, all 28 physical pins, hidden stacking, multi-unit identities, fields and
footprints, exact rollback/undo, seven-footprint transfer and complete pad parity,
all four page-fit/framed-render checks, bundled source MCP entrypoints and an
actual MCP edit/undo. Explicit save, owned manager stop/restart/reopen, normalized
complete snapshots, active pin identities, native nets and saved hashes matched.

Baseline ERC and the intentionally split ESD case each reported **zero findings**,
zero excluded findings and four default-disabled check categories. The independent
oracle rejected both split signal nets. The historical full-definition value-update
probes did not reproduce duplicate active pin IDs or lost library nicknames on
this exact native build/request shape. DRC retained **18 unrouted connections**,
zero excluded findings and five default-disabled categories; rules were unchanged.

Raw new evidence is in
`/home/karlvonrichter/embeint-qualification/capture-acceptance-2026-11-19-16`,
copied to `C:/Users/KarlVonRichter/.t3/scratch/capture-acceptance-evidence/run16`.
Earlier failed attempts remain separate. Save-file hashes cover schematic/PCB
files; project preferences and local history are not compared. The coordinated
helper probe's exact source commit/bytes and successful no-save cleanup are in
the receipt. Installed-distribution qualification and independent fresh-agent
evaluation remain separate; the latter is pending a separate evaluator.
