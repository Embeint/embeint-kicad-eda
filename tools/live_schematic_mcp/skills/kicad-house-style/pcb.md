# Boards

## Page and title block

- Use the same page rules as schematics: `live_pcb_page` must report `fits: true`. Draw the outline inside
  the drawing area, clear of the title block.
- Before saving, `live_pcb_page` should also report `overlap_free: true` (no silkscreen or fab
  texts overlapping on the same layer); DRC covers silkscreen against copper and other silk.
- Use the Embeint title block (`live_pcb_set_page`), the same fields as the schematic.
- **Origin and grid:** use the project mechanical requirements; resolve missing requirements
  before final placement rather than assuming an undocumented house standard.

## Order of work

1. **Footprints come from the schematic** (`live_pcb_update_from_schematic`), never placed by hand. Fix
   footprint problems in the library or the schematic, then update again.
2. **Board setup before routing:**
   - stackup (`live_pcb_set_stackup`);
   - design rules (`live_pcb_design_rules`, custom rules in `live_pcb_custom_rules`);
   - net classes with the default track width, clearance and via size (`live_pcb_netclasses`);
   - RF/antenna nets (`live_pcb_rf_net`);
   - keepouts (`live_pcb_add_keepout`, e.g. around an antenna or under a module's legs).
3. **Placement:**
   - connectors and mechanical parts first;
   - decoupling caps at their pins;
   - short critical loops: buck/boost hot loops, crystal, USB pair.
4. **Routing:** `live_pcb_route` / `live_pcb_route_net` route around other copper, keepouts and the edge, and
   check the current route result and DRC. If a route fails, verify the resulting
   geometry/connectivity instead of assuming rollback succeeded.
   - Never draw tracks that cross or crowd other nets; `live_pcb_add_tracks` refuses such a path.
   - Route RF feeds first, on one layer without vias; then critical nets (USB pair, clocks); then the rest.
   - Leave pour-connected nets (GND) to zones and refill them.
5. **Finish clean:**
   - `live_pcb_check` clean (clearance, silkscreen, courtyard, connectivity);
   - `live_pcb_drc` with schematic parity clean;
   - check the rendered view (`live_pcb_view`, `live_pcb_view_3d`) before saving.
6. **Outputs:** save the board and schematic, then run `live_pcb_fab_package`. It refuses unsaved
   designs and DRC errors (with schematic parity), and writes the project's `Outputs/`: Gerbers (with
   drill, drill map and report, plus `<project>-gerbers.zip` for the fab), PnP (CSV), BOM (orderable
   CSV), Schematic (PDF), STEP and Assembly (fab layers PDF), with a `manifest.json`. Add `ipc2581`,
   `odb` or `ipcd356` to `outputs` if the fab or assembler asks for them.
   - Read `warnings` before sending anything: plotted copper layers against the board, parts without
     an MPN, and BOM parts with no placement.
   - Change one output with `options` (for example `{"position": {"use_drill_place_file_origin":
     true}}` if the assembler wants the drill/place origin) rather than exporting it by hand. Use
     `live_pcb_export` or `live_schematic_export` for one-off files.
   - Re-run with `overwrite=true` after a change. It removes every file the previous package's
     manifest lists and refuses if the output folders hold anything else, so keep your own files
     (quotes, enclosure models) out of them. If the design changes while the package is exported,
     the call fails and removes what it wrote: save, then run it again.

## Castellated modules

- Use the Embeint castellated footprints (pads on the board edge, `pad_prop_castellated`).
- **Keep the underside free of parts** (single-sided assembly) and tent all bottom vias.
- Tell the base-board designer about anything that protrudes below. USB-C shell stakes, for example, need
  holes or a cut-out on the base board, or the connector can overhang its edge.
- Provide a matching base-board land pattern footprint and a pad table.
