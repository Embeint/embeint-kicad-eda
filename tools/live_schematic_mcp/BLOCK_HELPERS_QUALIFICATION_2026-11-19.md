# Schematic block helper qualification: 2026-11-19

The helper is a supported Python scripting API over the existing client, with
bundled instructions. It adds no MCP execution tool, native handler or protocol.
The implementation is technically independent of the acceptance fixture; its PR
stacks above acceptance to retain the coordinated evidence and foundation delivery.

## Offline and frozen delivery

Source `4cc55d79a954dec80795094126c5b373a47a80b2` includes the helper, foundation,
skill version 1.1.0 and links from the entrypoint/live workflow. The combined source
suite passed **291 tests**, including **23 helper regressions**, on WSL Python 3.14.
The skill-creator frontmatter/reference validator also passed. The later acceptance
merge is tested separately; these numbers do not describe an untested later head.

After merging acceptance head `61290f6731de8a1d15e3a6f3c7c92cb2ecc57278`, source
`a8fcfd66aa4e080ee1e8a96cb1eb67cd5df42c3d` passed **304 combined offline tests**
in 38.50 seconds with a JUnit receipt. The helper/client/server and bundled skill
files are unchanged from the packaged `4cc55d7` source; this later merge adds the
acceptance files and their evidence. The following helper commit updates this
qualification record only.

The helper regressions exercise the real client's batch/document/status/commit
code over an in-memory editor. They show manual-item preservation, route-only
connectivity revisions, normalized desired/current no-ops, stale owned state,
unowned UUID collisions, a manual edit between plan/apply, document mismatch,
ambiguous pins, T/X splitting, different-net/overlap refusal, sheet ports and
sheet-deletion refusal. Rejection, lost mutation/commit replies, lost rollback and
failed post-read consume the plan without mutation replay. They do not simulate
an atomic editor revision precondition.

The frozen [receipt](qualification/evidence/block-helpers-frozen-2026-11-19.json)
records an actual package built from a Git archive of that source:

| Identity | Value |
| --- | --- |
| Source archive SHA-256 | `f27a41684d9d8deb8b670cad3e1368303ceb6a006bf864ea7c67f4f927ee4341` |
| Frozen executable SHA-256 | `385975eb57daf703b97bd961cfbf75ea2ecf708dbbe06c76b8693820d333b91d` |
| Capability schema SHA-256 | `214ce118ab3b9bfa24fc06818000fb6443c8a4340976875bcc0569f96d9b5115` |
| Skill content SHA-256 | `2e4c83a49dad76f65eca2b28ca58e2e2d83e2cdc263d5aa8851bd6a86ae9b8c2` |
| Discovery | 112 tools; 145 registered commands; skill version 1.1.0 |

`qualification/check_block_helpers.py` runs the existing real stdio package check
from an empty directory with absent source/system-Python paths for the child. It
checks initialization, all bundled skill reads through tools/resources, per-file
and aggregate content hashes against the independently read source, error paths,
and `block_helpers` inclusion in the executable's PYZ archive. It does not execute
the helper inside the frozen server or contact a native editor. The checker itself
runs in the build environment and requires PyInstaller, like the packager.

Reproduce with a new Git archive containing `LICENSE`, `api/proto` and
`tools/live_schematic_mcp`, extracted outside a checkout:

```sh
python -I source/tools/live_schematic_mcp/qualification/package_mcp.py \
  /new/package --source-revision FULL_ARCHIVED_COMMIT
python tools/live_schematic_mcp/qualification/check_block_helpers.py \
  /new/package/embeint-kicad-mcp source/tools/live_schematic_mcp /new/receipt.json \
  --source-archive /path/to/source.tar
```

WSL cannot resolve this Windows worktree's Git pointer directly. The package uses
an archive revision explicitly supplied from Windows Git; its metadata accurately
records `source_dirty: null` and caller-supplied identity. It does not claim WSL Git
verified a clean checkout. The receipt retains raw hashes for both archived and
native-tested helper files. Windows Git archive exported Python with CRLF; the
native-tested worktree had LF. Both normalized byte comparisons passed. Markdown
is LF through the foundation's attributes, so its exact content hashes agree.

## Live scope and independent evaluation

The acceptance owner ran `block_example.run_probe(client, doc)` serially in its
new isolated synthetic WSL runtime. The probe passed: actual Device:R pin endpoints,
intended net membership before/after the route revision, one T junction, manual
item preservation, repeat no-op, stale fingerprint refusal, page containment and
exact UUID cleanup with no save. It performed no app/configuration/process changes.
The [committed acceptance receipt](qualification/evidence/capture-windows-wsl-2026-11-19.json)
retains this result under `helper_probe`; this layer never concurrently used that
app. Acceptance PR #8 is published at `61290f6731de8a1d15e3a6f3c7c92cb2ecc57278`
and merged here without rewriting history.

The final native-tested helper source was
`9dc7a020fc3dcc468c61906997826444fea283f9`; the helper files are unchanged in the
archived package source except export newlines. Native identity is the retained
`d880b3be5dbf30003e4b0069262417d0278bf528` build with its exact four-file loader
patch, measured `10.99.0-qualification-d880-loader-patch-dirty`. This is new scripted
source/native helper evidence on retained binaries, not a native rebuild of this
Python branch or frozen-helper execution evidence. Native hierarchy/intent/ERC,
persistence and PCB results belong to the acceptance layer and its exact receipt.

A fresh stdio discovery session is proven above. **Independent fresh-session
workflow evaluation remains pending a separate controller evaluator.** The required
isolated synthetic workflow and supported scripting/install boundary are documented
in the bundled [helper reference](skills/kicad-house-style/block-helpers.md).
Give that evaluator only installed/bundled skill instructions and a new synthetic
project, with no original conversation. A scripted probe by the implementation or
acceptance owner is not independent-agent evidence.

## Review fixes after qualification

A later code review changed `block_helpers.py`, its tests, the qualification
checker, the live probe and the helper guidance (skill version 1.1.1). Following
KiCad's commit cleanup and junction analysis, planning now refuses block wires
whose in-line ends meet with no pin there, block wires or junctions touching manual
wiring, and moving or removing a block pin with manual wiring attached (the
manifest records planned pin points for this). Symbol and sheet pins count as a
junction exit; ports and no-connect pins do not. Wire/junction geometry is checked
after apply; owned-item hashes are deterministic across processes (map fields); and
checker and probe failures no longer use `assert`.

A third review found that the schematic API handler runs connectivity cleanup after
an UpdateItems of a connectable item even inside an open commit, before the batch's
staged deletes and creates apply, and pushes it as a separate undo step. Wire and
junction keys now include geometry so wires are replaced, never updated; moving a
pin at a wire branch, removing a block wire/junction with manual wiring attached,
duplicate no-connect markers and `add()` of wires/junctions/symbols/sheets are
refused; manifests are version 2. Changing the handler is a recorded followup. The
combined offline suite passed **339 tests** with these changes. None of this is
native evidence: the in-memory editor does not model KiCad's cleanup.

The frozen receipt, its skill content hash and the native helper probe above describe
the earlier helper source and skill 1.1.0. They were **not** rerun for these fixes:
no new package was built and no native probe was requested from the runtime owner.

## Limits

One writer is required. Fingerprints cover serialized sheet items before a batch;
they cannot prevent another writer racing after that read, inspect state omitted
by GetItems, or protect multiple documents atomically. Post-read detection can
report a problem after a commit has already taken effect. Unknown outcomes require
inspection and a new read/plan, with explicit manifest recovery if needed. There
are no automatic mutation retries, implicit saves or sheet rebuilds. Symbol builder
placement is unrotated/unmirrored; routes use explicit corridors and do not avoid
manual geometry automatically. True document revisions, operation-result lookup and
skipping mid-commit cleanup in the schematic UpdateItems handler remain protocol
followups.
