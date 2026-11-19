# Live editing and verification

## Scope and execution

- Identify the project and UUID sheet path using document/hierarchy reads. Check
  current capabilities and tool schemas; local registration does not negotiate
  support with a running editor. Reuse existing task authorization, and coordinate
  shared editor use before switching projects or affecting someone else's work.
- Use typed tools or bounded batches for local edits. Larger capture scripts can
  use the fork's `client.LiveSchematic` API with the current generated protocols.
  Read actual placed pin positions before wiring. Preserve stable item identities
  and manual edits: compute targeted changes, or build a new explicitly scoped
  sheet. Do not silently clear an existing sheet and rebuild it.
- Begin/end commits can group edits into an undo step. On failure request supported
  rollback, then read back items and connectivity to verify the resulting state.
  Rollback and undo success must not be assumed from the request alone.
- A timed-out mutation has an unknown outcome. Reconcile with read-only document,
  item and connectivity queries before deciding whether any remaining edit is
  needed. Do not blindly replay it. Stop mutations if state cannot be established.
- For hierarchy, maintain matching child hierarchical labels and parent sheet
  pins, then wire the parent. Preserve the rest of a sheet symbol when updating
  pins. Page and title-block settings apply to each sheet document.

## Evidence for each completed sheet

1. Render the whole page with frame (`live_schematic_view`); fix overlaps and check
   notes, fields, dashed blocks and the title block.
2. Check `live_schematic_page` reports `fits: true`.
3. Run root ERC. Correct electrical causes rather than suppressing warnings or
   relabeling unspecified pins passive without datasheet evidence. Explain remaining
   violations; ERC cleanliness alone is not design sign-off.
4. Export the whole-design netlist and compare intended nets as REF.PIN sets,
   including hidden/stacked pins and parent-child connectivity. Report missing,
   extra, shorted or split nets. Check the current netlist tool's coverage before
   relying on per-sheet reads for a hierarchical design.
5. Save explicitly when authorized by the task. Compare saved schematic contents
   or parsed exports with the intended state; for persistence acceptance, reopen in
   an isolated validation session and compare hierarchy, fields and connectivity.
   A timestamp or success message alone is insufficient. An error or ambiguous save
   remains unresolved until content evidence establishes the outcome.

## Historical 10.99 observations (unverified on the current version)

These are troubleshooting leads from an earlier development build, not current
capability guarantees. Reproduce in a disposable project before applying a workaround.

- Network-mounted saves sometimes reported failure despite file writes. Inspect
  actual contents and modified state; do not infer a successful save from mtimes.
- `UpdateItems` on placed multi-unit symbols was reported to duplicate pin UUIDs,
  and round-tripping items sometimes lost library nicknames. Prefer complete library
  definitions; if a problem occurs inspect affected instances and verify any undo
  or selective relink. Do not relink every symbol based on an empty IPC nickname.
- No-connect marker visibility depended on explicit size; overlapping power-flag
  pins were reported unconnected. Inspect rendered markers and exported nets.
- Large placement replies and item transforms required bounded chunks; transforms
  could merge wire segments. Compare pin connectivity rather than wire IDs and
  measure limits for the current build instead of treating old chunk sizes as safe.
- Project switching on network storage could be slow. Process sleep state is not
  readiness proof; use bounded read-only checks and reconcile timed-out operations.
- Regenerated project symbols may require `ReloadLibrary` followed by selective
  `UpdateSymbolsFromLibrary`. Check current command schemas and resulting fields.
