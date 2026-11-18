# P1.2 omitted-operation audit

Native source baseline: `a3e9347feb96bf5a6c8c33d87211282b49969661`
(Embeint `main`). This audit covers only the omissions below. Source inspection
and synthetic Python tests are not a native build or observed editor behavior.
No native or protocol source changes are made by this work.

| Operation | Decision through generic execute/describe | Reason |
| --- | --- | --- |
| PCB `GetEmbeddedFiles` | Add as `read`, batchable | Concrete target validation and serialization path; no model mutation or disk write in that path |
| `ParseAndCreateItemsFromString` | Defer | PCB/footprint base handler is a stub; schematic has a declaration but no implementation or registration |
| `OpenLibraryItem` | Defer for both library editors | GUI loading can prompt for unsaved changes; response does not establish safe completion |
| `LoadAllLibraries` | Defer | GUI explicitly returns unimplemented; headless loading is not a completion contract |
| PCB `AddEmbeddedFiles` / `SetEmbeddedFiles` | Defer | Mutate storage without an editor commit; collision/replacement and undo need separate qualification |
| `BeginCommit` / `EndCommit` | Keep internal to client | Transaction ownership belongs to the client, not arbitrary execute/batch steps |
| `InjectDrcError` | Keep excluded | Debug mutation, not a production workflow |

## PCB embedded-file read contract

Use `live_schematic_describe_command("GetEmbeddedFiles")` and the existing generic
execute surface. The fully qualified request is
`kiapi.board.commands.GetEmbeddedFiles`; its `board` field is a
`kiapi.common.types.DocumentSpecifier` from the selected instance's open boards.
It returns `kiapi.common.types.EmbeddedFiles`, unlike the existing
`GetEmbeddedFileList` metadata operation for library symbols/footprints.

```json
{
  "board": {
    "type": "DOCTYPE_PCB",
    "board_filename": "live-demo.kicad_pcb",
    "project": {"name": "live-demo", "path": "/absolute/disposable/live-demo/"}
  }
}
```

`API_HANDLER_PCB::handleGetEmbeddedFiles` in
[`api_handler_pcb.cpp`](../../pcbnew/api/api_handler_pcb.cpp) validates the target
then calls `board::PackEmbeddedFiles` in
[`api_pcb_utils.cpp`](../../pcbnew/api/api_pcb_utils.cpp). The packer copies name,
type, stored data and hash. Entries whose `compressedEncodedData` is empty are
omitted, so this is an inventory of stored payloads, not necessarily every map entry.
It does not decompress, validate hashes, extract files or write a cache. No implicit
save, `onModified`, UI action or commit occurs on this read path. No undo is needed.

Native targeting has a specific limit: `validateDocumentInternal` checks PCB type
(`AS_UNHANDLED` otherwise) and the currently open board's **basename**
(`AS_BAD_REQUEST` for a mismatch). It does **not** validate project name/path.
Missing/default targets have the wrong type and are rejected natively. Generic
execute forwards the supplied target; it adds no project-identity guarantee.
Select the intended instance/socket explicitly; same-named boards across projects
are not disambiguated by this native handler. No native identity fix is claimed.

The client requires an `AS_OK` envelope and the correct packed response type.
There is no nested success/status field in `EmbeddedFiles`. Empty success is `{}`
in protobuf JSON (`files` is omitted); populated success has a `files` array.
The `data` field is protobuf **bytes**: protobuf JSON base64 encodes the already
base64-encoded zstd storage bytes. Decode the JSON layer once to recover the
native stored bytes. This operation supplies opaque bytes and a native hash;
the adapter neither decompresses them nor certifies their integrity. No size cap
or streaming is added, so large embedded collections produce large replies.

Batch use follows the existing exact document binder: an omitted target is filled
from the batch document; any supplied different type, filename, project name or
path is rejected before `BeginCommit`. The read can observe prior changes in a
batch, but is not a transactional edit. Its eligibility makes no claim about
rollback for any other handler, nor about isolation from manual/concurrent edits.

## Specific deferrals and response semantics

- **Native parsing:** `API_HANDLER_BOARD::handleParseAndCreateItemsFromString`
  in [`api_handler_board.cpp`](../../pcbnew/api/api_handler_board.cpp) validates
  document and busy state, then returns a default `CreateItemsResponse` without
  parsing or creating anything. `status` is `IRS_UNKNOWN`, not `IRS_OK`.
  This base handler is registered for PCB and footprint contexts. The schematic
  header declaration has no corresponding cpp implementation/registration.
  Thus there are no native creation, disk-effect, undo or batch guarantees to
  advertise. A future implementation must validate every parsed child, preserve
  client commit ownership, report per-item failures and qualify rollback and
  explicit save/reopen in a disposable app before export.
- **Creation statuses:** the shared batch response checker now recognizes
  `ItemRequestStatus` by response descriptor rather than a command-name suffix.
  This also closes the existing `GetItemsById` overall-status gap (its name does
  not end in `Items`); a synthetic batch regression confirms a drop on rejection.
  Synthetic parsed-creation responses exercise default/unknown overall status,
  partial child failure and unknown child codes. `IRS_OK` with zero items is
  explicitly allowed by the proto; a string has no declared input item count
  against which completeness can be checked. Existing create/update/delete
  count checks remain in place. These checks do not implement or enable parsing.
  Generic execute continues to return raw response JSON for existing commands;
  this change does not add automatic transactions or rollback to execute.
- **Library opening:** symbol `handleOpenLibraryItem` calls the GUI context's
  `SYMBOL_EDIT_FRAME::LoadSymbol`; see
  [`symbol_editor.cpp`](../../eeschema/symbol_editor/symbol_editor.cpp), which
  can call `HandleUnsavedChanges` and display load errors. The footprint handler
  preloads then calls the void `LoadFootprintFromLibrary`; see
  [`footprint_editor_utils.cpp`](../../pcbnew/footprint_editor_utils.cpp), which
  can prompt when switching from modified frame-owned content, and may return
  without opening after cancellation/failure. An Empty API reply cannot confirm
  that switch. The request contains type and library identifier, not the current
  document to preserve. Modal/save/discard effects make it ineligible for export
  or batches without further native guards and outcome validation.
- **All-library loading:**
  [`api_handler_libraries.cpp`](../../common/api/api_handler_libraries.cpp)
  registers this dispatcher only on the design-block instance. GUI requests
  return `AS_UNIMPLEMENTED`; headless requests may return `AS_NOT_READY`,
  `AS_BAD_REQUEST`, or `LibraryCommandStatus`. `LCS_OK` for symbol/footprint faces
  means a load was started, not completed successfully (explicit native comment).
  It has no document target, undo or batch contract and can read many libraries
  and initialize faces. No synchronous GUI all-library operation is exported.
- **Embedded mutations:** both PCB handlers validate document/busy state then
  mark the model modified without `getCurrentCommit` or a commit snapshot.
  `Add` first unpacks all input into temporary storage, then clones entries into
  the board; `EMBEDDED_FILES::AddFile(shared_ptr)` silently retains existing names
  on collision. An Empty success has no per-file acknowledgements. `Set` passes
  live storage to `UnpackEmbeddedFiles`, but that function first builds and
  validates a temporary collection (decompression plus `Validate`), and only
  then clears/replaces the destination while retaining its font-embedding flag.
  Invalid payloads return `AS_BAD_REQUEST` before replacement; this is a
  source-derived observation, not live rollback evidence. Successful `Set`
  removes absent files and first-wins duplicate input names. These handlers do
  not explicitly save/extract files in the audited path; later explicit saves
  persist changes. Commit/drop, callbacks/cache effects, duplicate policy,
  references to removed files, and save/reopen behavior remain unqualified.
  Neither mutation is exported, including as a standalone nontransactional edit.

## Evidence and qualification

[`test_registry_audit.py`](test_registry_audit.py) uses generated descriptors,
synthetic replies and a fake transport. It checks discovery/describe registration,
exact request forwarding, empty/populated opaque byte round trips, envelope/reply
rejection without retries, batch target binding and rejection before contact,
creation failure statuses, and continued exclusion of deferred/internal/debug
commands. It also verifies the dedicated live harness refuses a non-test project
or missing explicit isolation. The full offline suite additionally exercises real
MCP stdio discovery and schema validation. None of these runs native C++.

[`e2e_registry_audit.py`](e2e_registry_audit.py) is a manually invoked, guarded
read-only check, pending an isolated native app. It never launches/replaces an app
or opens documents. Start a separate config/socket with a disposable board already
open, then set `KICAD_P12_ISOLATED=1`, `KICAD_API_SOCKET`, and
`KICAD_LIVE_TEST_PROJECT` before invoking it. `KICAD_P12_EXPECT_FILE` requires a
known embedded fixture name; without populated files it reports that coverage as
pending. It checks repeated/batched reads, missing/wrong type/filename failures,
unchanged live serialization/modified state, and unchanged board disk hash, and
prints the native version. Run separately from the shared live runner; do not aim
it at an existing user session. Application build revision and schema fingerprint
must accompany eventual live results. No native build, live execution, undo,
mutation save/reopen or cross-project identity qualification is claimed here.
