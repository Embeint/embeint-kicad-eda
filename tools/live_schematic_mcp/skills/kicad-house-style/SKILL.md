---
name: kicad-house-style
description: Apply Embeint conventions when capturing or reviewing schematics, selecting parts, or creating symbols, footprints and boards with KiCad. Use for design work, not general fork development.
metadata:
  version: "1.3.0"
---

# Embeint KiCad house style

Load the reference for the task at hand. These are Embeint house conventions;
the user's current design requirements and existing authorization take precedence.

| Task | Reference |
| --- | --- |
| Capture or tidy sheets, hierarchy and pages | [schematic-style.md](schematic-style.md) |
| Select parts, fields, symbols, footprints and models | [parts-and-libraries.md](parts-and-libraries.md) |
| Review pin planning, power paths and interfaces | [design-review.md](design-review.md) |
| Set up, place, route or export a board | [pcb.md](pcb.md) |
| Drive live IPC and verify edits and persistence | [live-workflow.md](live-workflow.md) |
| Plan and reconcile a small generated schematic block | [block-helpers.md](block-helpers.md) |
| Configure local libraries or optional part selector | [workstation.md](workstation.md) |

Start by identifying the intended project and document. Reuse authorization already
given; coordinate with other editor users before switching a shared session or
discarding unrelated work. Preserve manual edits and keep changes scoped to the task.

Draw dashed, titled functional blocks with pin-to-pin Manhattan wiring inside;
use labels between blocks and hierarchical labels with matching parent sheet pins
across sheets. Wire the root sheet between sheet pins. Use power symbols for rails.

Default to 0402 multi-sourced passives with actual ratings, complete manufacturer
part numbers and verified alternates. Avoid Analog Devices, Linear and Maxim parts.
Keep symbol fields, footprints and embedded models complete before placement.

Verify the framed page, containment, ERC and intended REF.PIN connectivity before
an explicit save. Verify saved contents; timestamps alone do not prove persistence.
Report unresolved findings and distinguish offline evidence from live acceptance.
