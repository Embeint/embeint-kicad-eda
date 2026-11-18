# IPC/MCP review and promotion record

Date: 2026-11-18. Baseline: `4f5a35311f`, the Embeint `main` bootstrap containing
the existing IPC/MCP work, including `3c05b0ecba` live PCB editor extensions.
Three subagents separately reviewed native IPC handlers, the Python client/MCP
adapter, and MCP coverage. This record covers the resulting bounded fixes and
support foundation; it is not a complete electrical-design or release sign-off.

## Findings addressed

1. **P1: batches admitted settings and other nontransactional work.** The generic
   batch path now allows only explicitly audited transactional edits. Settings,
   editor replacement, arbitrary actions, async work and file/export operations
   cannot be bundled under a false rollback guarantee. Command descriptions expose
   `batchable`; selection/zoom effects are explicitly outside design rollback.

2. **P1: batch steps could target a different document.** Missing top-level
   document/board/header targets now inherit the transaction document. Explicit
   mismatches are rejected before `BeginCommit`. Cross-sheet operations currently
   require separate batches; exact document equality is intentional.

3. **P1: partial and failed acknowledgements could be reported as committed.**
   Validation covers core create/update/delete item results, board transform item
   statuses, symbol update results and expected acknowledgement counts. Rejected
   or incomplete responses cause the item transaction to be dropped.

4. **P1: rejected routes could undo a later manual edit.** Verification no longer
   calls global undo. It removes the exact newly created route UUIDs in a separate
   undoable cleanup. An unconfirmed cleanup reports the UUIDs for inspection.
   If DRC itself fails or times out, its task may still be running, so the unverified
   route remains and the error explicitly identifies that state and its UUIDs.

5. **P1: truncating DRC presentation hid route violations after 200 findings.**
   Internal route checks request the full report. The default presentation remains
   bounded, reports the total count and marks truncation.

6. **P2: route acceptance ignored manufacturing findings.** New copper is checked
   for clearance, keepout and manufacturing findings, including track width, drill
   and via constraints. The response states its scope; this does not assert that
   every board-wide or connectivity finding has been resolved.

7. **P1: project/document opening could prompt to discard, switch or recover work.**
   Manager IPC now refuses project switches, hidden/busy existing editors, incorrect
   native filenames, locks and pending autosave recovery. Resolving the active
   project no longer reloads it. Existing editor windows are preserved on refusal.
   This addresses known unsafe entry conditions, with the loader limitation below.

8. **P2: invalid direct board transforms could leak an implicit commit.** The
   transform handler creates its commit only after finding an item it can modify.
   Empty/all-invalid requests no longer create a transaction that blocks undo or
   later `BeginCommit` calls.

## Validation

- The standard offline suite passes **123 tests**, including the prior unit cases,
  actual MCP stdio protocol tests and 34 new safety regression cases. Tests use
  synthetic replies or a deliberately nonexistent socket, not the open design.
- Both changed native translation units compile in the existing WSL build:
  `kicad.cpp.o` and `api_handler_board.cpp.o`. Relevant build headers were checked
  against this checkout before copying those two source files. No installation,
  linking or running-editor replacement was performed.
- The MCP contracts workflow generates protocol modules from a clean checkout and
  runs the offline suite on relevant pull requests and integration/product pushes.

## Remaining gates

**Fully noninteractive document loading remains a priority gap.** Even after the
new entry guards, `OpenProjectFiles` can show GUI error/upgrade dialogs for corrupt,
unsupported or failing hierarchical documents. A dedicated no-dialog loader path
needs native integration tests; do not claim the guarded opener is universally
noninteractive.

The new native guards and lazy-commit behavior still need runtime tests against a
disposable application instance. A full application build/link and the existing
guarded live suite were not run during this review because the installed development
application was open on a customer project. No tests modified that project, and
the installed application was not stopped or replaced.

Live capability negotiation, complete workflow coverage, clean-machine installation,
and save/reopen qualification remain delivery gates in
[MCP_SUPPORT_PLAN.md](MCP_SUPPORT_PLAN.md). Rust and WebAssembly follow those gates.
