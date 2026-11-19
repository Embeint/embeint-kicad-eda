# Schematic capture acceptance

`capture_acceptance.py` extends the explicit-save workflow in `e2e_persistence.py`
and the item normalization in `qualification/persistence.py`. It owns a new
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
an installed native prefix and its retained four-binary SHA-256 record. The
Windows/WSL source record identifies the native d880 base plus the exact four-file
loader patch separately from the adapter Git revision and source hashes. The
driver verifies the selected binaries against that record before starting and
after finishing. It records the measured application version, loaded modules,
current adapter revision/dirty state, source hashes and adapter schema manifest.
It does not rebuild or relabel the retained binary.

On WSL, use a private mount namespace for X11 rather than WSLg's shared socket
directory. Run from the T3 checkout, choosing a **new** output directory:

```sh
python tools/live_schematic_mcp/generate_proto.py
bwrap --bind / / --tmpfs /tmp --dev-bind /dev /dev -- \
  /path/to/existing/venv/bin/python \
  tools/live_schematic_mcp/qualification/capture_acceptance.py \
  --install /path/to/retained/install-patched \
  --native-hashes /path/to/retained/patched-application-sha256.txt \
  --native-source-record tools/live_schematic_mcp/qualification/evidence/windows-wsl-2026-11-18.json \
  --library-root /path/to/kicad/share \
  --git-executable '/mnt/c/Program Files/Git/cmd/git.exe' \
  --output /path/to/new/evidence
```

If the retained build needs private dependency libraries, pass its existing
`LD_LIBRARY_PATH` into the namespace. No dependencies are installed or modified.
Windows-owned worktree `.git` pointers require Windows Git under WSL; ordinary
Linux checkouts use the default `git`. Optional `--helper-probe /path/block_example.py`
runs the coordinated helper's isolated probe serially on the empty test sheet,
before capture, and records its commit and file hashes. A failed/unknown probe
stops the run without mutation replay.

The driver records exact failed-batch rollback, transaction undo/redo, no implicit
saves, actual schematic-to-PCB transfer, pin/pad parity, page fits and distinct
framed renders. Guarded `UpdateItems` probes label duplicate multi-unit pin IDs
and missing library nicknames as `reproduced` or `historical_not_reproduced` for
this exact binary and request shape. Each probe is undone and its restoration
checked; absence of reproduction is not evidence for other versions or paths.
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
python -m pytest tools/live_schematic_mcp/test_connectivity_intent.py
```

These tests include the ERC-zero ESD split, a missing hidden stacked pad, merged
nets/connected NCs, duplicate identities across units, unexpected pins, native
reference mapping, board mismatches and a changed native editor hash.
