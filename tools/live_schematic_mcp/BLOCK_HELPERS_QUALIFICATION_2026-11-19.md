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
The owner retains the full native receipt with the acceptance evidence; this
layer never concurrently used that app.

The native-tested helper source was
`dbd3075e3c9b05752c3a7ffa277ecf1331442bfe`; the helper files are unchanged in the
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

## Limits

One writer is required. Fingerprints cover serialized sheet items before a batch;
they cannot prevent another writer racing after that read, inspect state omitted
by GetItems, or protect multiple documents atomically. Post-read detection can
report a problem after a commit has already taken effect. Unknown outcomes require
inspection and a new read/plan, with explicit manifest recovery if needed. There
are no automatic mutation retries, implicit saves or sheet rebuilds. Symbol builder
placement is unrotated/unmirrored; routes use explicit corridors and do not avoid
manual geometry automatically. True document revisions and operation-result lookup
remain protocol followups.
