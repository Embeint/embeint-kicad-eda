# SPDX-License-Identifier: GPL-3.0-or-later
"""End-to-end item round-trip check: unchanged API edits must not change the saved document.

    python e2e_item_roundtrip.py [output_dir]              live-demo schematic; never saves
    python e2e_item_roundtrip.py --isolated [output_dir]   disposable project; saves and reopens
    python e2e_item_roundtrip.py prepare <dir>             write a disposable project into <dir>

For every item it sends the item back exactly as GetItems returned it (UpdateItems) and compares
the document saved to a string (SaveDocumentToString, KiCad's own file writer) before and after,
block by block.  It then deletes the item, creates it again from the returned message, compares
again and undoes both steps.  Items whose created reply differs from the request in a field the
request set are reported too ("explicit values").

The default run creates a fixture of every schematic item type with non-default values
(item_roundtrip.api_fixture plus library and power symbols) far off the page of the live-demo
schematic, and deletes it afterwards (also when a check fails).  It also sends the library symbol
without its definition and with a lib_id naming another symbol: the update must keep the symbol's
library link, so the saved document must not change.

--isolated requires KICAD_ROUNDTRIP_ISOLATED=1 and an explicit KICAD_API_SOCKET for a separately
launched KiCad whose open project (KICAD_LIVE_TEST_PROJECT) is disposable, such as one written by
`prepare` (qa's api_kitchen_sink schematic, sub-sheet and board).  It adds the fixture, checks every
item on the root sheet and the board (file-loaded items carry values the API cannot create),
saves both documents to disk, reverts them to the saved files and compares every item's message
with the one before saving.

The report (JSON, with a unified diff per changed block) is written to the output directory.
"""

import argparse
import json
import os
from pathlib import Path
import shutil

from google.protobuf import empty_pb2, json_format

from board_client import ITEM_TYPES
from client import LiveSchematic, editor, project, sch
from e2e_guard import test_board, test_document
from item_roundtrip import (NOT_SAVED, NO_CONNECT_SIZE_NM, UNPRESERVABLE, api_fixture, block_differences,
                            is_type, item_id, message_differences, normalize_created, same_colour_channel,
                            save_string, unindexed, unpack)

REPO = Path(__file__).resolve().parents[2]
QA = REPO / "qa" / "data"
KITCHEN_SINK = QA / "eeschema" / "api_kitchen_sink.kicad_sch"
BOARD_SINK = QA / "pcbnew" / "api_kitchen_sink"

#: Board item kinds checked in isolated mode (pads are footprint children; groups only hold ids)
BOARD_KINDS = ("footprints", "tracks", "arcs", "vias", "zones", "shapes", "texts", "textboxes",
               "dimensions", "tables", "images")


def prepare(folder):
    """A disposable project: the api_kitchen_sink schematic, its sub-sheet, board and rules."""
    folder.mkdir(parents=True, exist_ok=True)
    name = folder.name
    text = KITCHEN_SINK.read_text(encoding="utf-8")
    (folder / f"{name}.kicad_sch").write_text(text, encoding="utf-8", newline="\n")
    for line in text.splitlines():
        if '"Sheetfile"' in line:
            sub = line.split('"')[3]
            if (KITCHEN_SINK.parent / sub).is_file():
                shutil.copyfile(KITCHEN_SINK.parent / sub, folder / sub)
    for suffix in (".kicad_pcb", ".kicad_dru"):
        shutil.copyfile(BOARD_SINK.with_suffix(suffix), folder / f"{name}{suffix}")
    settings = json.loads(BOARD_SINK.with_suffix(".kicad_pro").read_text(encoding="utf-8"))
    settings.setdefault("meta", {})["filename"] = f"{name}.kicad_pro"
    (folder / f"{name}.kicad_pro").write_text(json.dumps(settings, indent=2), encoding="utf-8")
    print(folder / f"{name}.kicad_pro")


def item_map(client, doc, kinds=()):
    """{uuid: message} for the document's items (of the given board kinds, for a board)."""
    request = editor.GetItems()
    request.header.document.CopyFrom(doc)
    request.types.extend(ITEM_TYPES[kind] for kind in kinds)
    response = client.send(request, editor.GetItemsResponse)
    found = {}
    for packed in response.items:
        message, uuid = unpack(packed)
        found[uuid] = message
    return found


def explicit_differences(sent, returned):
    """Fields the request set whose value the reply does not keep (defaults KiCad adds are fine).

    Colour channels are compared at the 8 bits documents store, to which KiCad rounds them."""
    sent_fields = set(message_differences(type(sent)(), sent))
    paths = [path for path in message_differences(sent, returned) if path in sent_fields]
    old, new = field_values(sent, paths), field_values(returned, paths)
    return [path for path in paths if not same_colour_channel(path, old[path], new[path])]


def split_known(kind, paths):
    """(unexpected, known) field paths: known ones are listed in UNPRESERVABLE with a reason."""
    known = {path: UNPRESERVABLE[(kind, unindexed(path))] for path in paths
             if (kind, unindexed(path)) in UNPRESERVABLE}
    return [path for path in paths if path not in known], known


def field_values(message, paths):
    flat = {}

    def walk(value, prefix):
        if isinstance(value, dict):
            for key, child in value.items():
                walk(child, f"{prefix}.{key}" if prefix else key)
        elif isinstance(value, list):
            for index, child in enumerate(value):
                walk(child, f"{prefix}[{index}]")
        else:
            flat[prefix] = value

    walk(json_format.MessageToDict(message, preserving_proto_field_name=True), "")
    return {path: flat.get(path) for path in paths}


def _replies(reply, response_type, field):
    response = json_format.ParseDict(reply, response_type(), ignore_unknown_fields=True)
    return [unpack(result.item)[0] for result in getattr(response, field)]


def create(client, doc, items):
    return _replies(client.mutate(doc, items), editor.CreateItemsResponse, "created_items")


def check_item(client, doc, item, recreate=True):
    """Unchanged update and delete + create-from-returned for one item; restores the document."""
    uuid = item_id(item)
    entry = {"id": uuid, "type": type(item).DESCRIPTOR.name}
    before = save_string(client, doc)

    def update(message):
        """(reply fields that differ, file differences) for one unchanged update, undone."""
        returned = _replies(client.mutate(doc, [message], update=True), editor.UpdateItemsResponse,
                            "updated_items")[0]
        # A generated table's cells are rebuilt from the board, so its reply is not the request
        fields = message_differences(item, returned) if "id" in item.DESCRIPTOR.fields_by_name else []
        changed = block_differences(before, save_string(client, doc))
        # An update that changed nothing may leave no undo step, so only a change is undone
        if changed:
            client.undo(doc)
        return fields, changed

    target = item
    if is_type(item, sch.SchematicSymbolInstance):
        # The instance round trip: without a definition the symbol keeps its library link.  A
        # full definition replaces the cached library symbol; that is reported separately
        target = type(item)()
        target.CopyFrom(item)
        target.ClearField("definition")
    try:
        entry["update_reply_fields"], entry["update_file"] = update(target)
        if target is not item:
            entry["definition_update_file"] = update(item)[1]
            entry["update_reply_fields"] = [f for f in entry["update_reply_fields"]
                                            if not f.startswith("definition.")]
    except RuntimeError as error:
        entry["update_error"] = str(error)

    if recreate:
        client.delete(doc, [uuid])
        try:
            created = create(client, doc, [item])[0]
        except RuntimeError as error:
            entry["create_error"] = str(error)
        else:
            entry["create_reply_fields"] = split_known(
                entry["type"], explicit_differences(normalize_created(item), normalize_created(created)))[0]
            # Only the item's own block and the symbol cache: deleting a wire can make KiCad drop
            # a junction it no longer needs, which re-creating the wire does not restore
            created_file = block_differences(before, save_string(client, doc), mask=True,
                                             keys={uuid, "lib_symbols#0"})
            # Creating a symbol needs its full definition, which replaces the cached library
            # symbol; that is reported separately from the instance round trip
            entry["definition_create_file" if is_type(item, sch.SchematicSymbolInstance)
                  else "create_file"] = created_file
            client.undo(doc)
        client.undo(doc)
    restored = save_string(client, doc)
    if restored != before:
        raise SystemExit(f"FAILED: undo did not restore the document after {entry['type']} {uuid}: "
                         + json.dumps(block_differences(before, restored), indent=1))
    return entry


def failures(entry):
    keys = ("update_error", "update_reply_fields", "update_file", "create_error", "create_reply_fields",
            "create_file")
    return {k: entry[k] for k in keys if entry.get(k)}


def check_definitionless_lib_id(client, doc, symbol):
    """File differences after a definition-less update whose lib_id names a symbol the schematic
    does not embed (undone).  The update keeps the embedded symbol, so it must keep the library
    link to it too."""
    before = save_string(client, doc)
    message = type(symbol)()
    message.CopyFrom(symbol)
    message.ClearField("definition")
    message.lib_id.library_nickname = "Device"
    message.lib_id.entry_name = "R_roundtrip_missing"
    client.mutate(doc, [message], update=True)
    changed = block_differences(before, save_string(client, doc))
    if changed:
        client.undo(doc)
    return changed


def build_fixture(client, doc, created_ids):
    """Create the API fixture and symbols; returns {id: message}, explicit-value losses and the
    known (documented) ones.  Appends the id of every item it creates to created_ids as it goes,
    so the caller can remove them even if this fails part way."""
    sent = api_fixture()
    created = create(client, doc, sent)
    created_ids.extend(item.id.value for item in created)
    losses, known = {}, {}
    for request, reply in zip(sent, created):
        kind = type(request).DESCRIPTOR.name
        lost, expected = split_known(kind, explicit_differences(normalize_created(request),
                                                                normalize_created(reply)))
        if is_type(reply, sch.NoConnectMarker) and reply.size.value_nm != NO_CONNECT_SIZE_NM:
            lost.append("size.value_nm")
        if lost:
            losses[f"{kind} {request.id.value}"] = {"sent": field_values(request, lost),
                                                    "returned": field_values(reply, lost)}
        if expected:
            known[f"{kind} {request.id.value}"] = expected
    symbols = {}
    for lib_id, x in (("Device:R", 520), ("power:GND", 540), ("power:+3V3", 560)):
        symbols[lib_id] = client.place_symbol(doc, lib_id, x, 150)["id"]
        created_ids.append(symbols[lib_id])
    current = item_map(client, doc)
    for request in sent:
        if request.id.value not in current:
            losses[f"{type(request).DESCRIPTOR.name} {request.id.value}"] = "not returned by GetItems"
    fixture = {uuid: current[uuid] for uuid in created_ids if uuid in current}
    return fixture, losses, known, symbols["Device:R"]


def remove_fixture(client, doc, created_ids):
    """Delete whichever of the created items are still in the document."""
    present = set(item_map(client, doc))
    remaining = [uuid for uuid in created_ids if uuid in present]
    for first in range(0, len(remaining), 200):
        client.delete(doc, remaining[first:first + 200])


def reopen_check(client, doc, kinds=()):
    """Save to disk, revert to the saved file and compare every item with its pre-save message."""
    before = item_map(client, doc, kinds)
    client.send(project.SaveDocument(document=doc), empty_pb2.Empty)
    saved = save_string(client, doc)
    client.send(editor.RevertDocument(document=doc), empty_pb2.Empty)
    after = item_map(client, doc, kinds)
    changes = {}
    for uuid, message in before.items():
        key = f"{type(message).DESCRIPTOR.name} {uuid}"
        if uuid not in after:
            changes[key] = "missing after reopen"
            continue
        old, new = message, after[uuid]
        if is_type(message, sch.BusEntry) and message.type == sch.BET_BUS_TO_BUS:
            if is_type(new, sch.SchematicLine) and new.type == sch.SLT_BUS:
                continue  # saved as the bus segment it draws (same UUID): KiCad has no such token
        if is_type(message, sch.SchematicSymbolInstance):
            # Pins of units that are not placed carry the library pin's UUID, which is not saved
            old, new = normalize_created(old), normalize_created(new)
        kind = type(message).DESCRIPTOR.name
        fields = [f for f in message_differences(old, new)
                  if unindexed(f) not in NOT_SAVED and (kind, unindexed(f)) not in UNPRESERVABLE
                  and not (f == "fields_autoplaced" and not getattr(message, "fields", None)
                           and "fields" in message.DESCRIPTOR.fields_by_name)]
        if fields:
            changes[key] = {"before": field_values(old, fields), "after": field_values(new, fields)}
    return {"items": changes, "file": block_differences(saved, save_string(client, doc))}


def check_document(client, doc, targets, recreate):
    report = [check_item(client, doc, item, recreate(uuid, item)) for uuid, item in targets.items()]
    informational = ("definition_update_file", "definition_create_file")
    return {"items": {f"{e['type']} {e['id']}": failures(e) for e in report if failures(e)},
            # Full symbol definitions replace the cached library symbol (tracked separately)
            "symbol_definition": {f"{e['type']} {e['id']}": {k: e[k] for k in informational if e.get(k)}
                                  for e in report if any(e.get(k) for k in informational)},
            "checked": len(report), "types": sorted({e["type"] for e in report})}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--isolated", action="store_true")
    parser.add_argument("out", nargs="?", default="live-roundtrip-output")
    parser.add_argument("folder", nargs="?", help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.out == "prepare":
        if not args.folder:
            parser.error("prepare needs a folder")
        prepare(Path(args.folder).resolve())
        return
    out = Path(args.out)
    if args.isolated and (os.environ.get("KICAD_ROUNDTRIP_ISOLATED") != "1"
                          or not os.environ.get("KICAD_API_SOCKET")):
        raise SystemExit("REFUSED: --isolated needs KICAD_ROUNDTRIP_ISOLATED=1 and an explicit "
                         "KICAD_API_SOCKET for a disposable project")
    out.mkdir(parents=True, exist_ok=True)

    client = LiveSchematic(timeout_ms=30000)
    doc = test_document(client)
    start = save_string(client, doc)
    created_ids = []
    try:
        fixture, losses, known, resistor = build_fixture(client, doc, created_ids)
        targets = item_map(client, doc) if args.isolated else fixture

        # Recreating a loaded sheet would give it an empty screen in place of its sub-sheet
        result = {"explicit_value_losses": losses, "known_unpreservable": known,
                  "schematic": check_document(client, doc, targets,
                                              lambda i, item: not is_type(item, sch.SheetSymbol) or i in fixture),
                  "definitionless_lib_id": check_definitionless_lib_id(client, doc, fixture[resistor])}
        if args.isolated:
            board = test_board(client)
            # A created footprint gets new UUIDs for all its children by design, and generated tables
            # (wrapping a table) are rebuilt from the board rather than created from a message
            result["board"] = check_document(client, board, item_map(client, board, BOARD_KINDS),
                                             lambda i, item: type(item).DESCRIPTOR.name != "FootprintInstance"
                                             and "id" in item.DESCRIPTOR.fields_by_name)
            result["reopen"] = {"schematic": reopen_check(client, doc),
                                "board": reopen_check(client, board, BOARD_KINDS)}
    finally:
        # The default run edits the open live-demo schematic: never leave the fixture behind
        if not args.isolated:
            remove_fixture(client, doc, created_ids)
    if not args.isolated:
        leftover = block_differences(start, save_string(client, doc))
        result["cleanup"] = [d for d in leftover if d["head"] != "lib_symbols"]

    report = out / ("item_roundtrip_isolated.json" if args.isolated else "item_roundtrip.json")
    report.write_text(json.dumps(result, indent=1))
    problems = []
    for name in ("schematic", "board"):
        for key, value in result.get(name, {}).get("items", {}).items():
            problems.append(f"CHANGED {name} {key} " + json.dumps(
                {k: (v if not k.endswith("_file") else [d["key"] for d in v]) for k, v in value.items()}))
    problems += [f"EXPLICIT {key} {value}" for key, value in losses.items()]
    problems += [f"LIB_ID a definition-less update with another lib_id changed {d['kind']} {d['head']} {d['key']}"
                 for d in result["definitionless_lib_id"]]
    for name, check in result.get("reopen", {}).items():
        problems += [f"REOPEN {name} {key} {value}" for key, value in check["items"].items()]
        problems += [f"REOPEN {name} file block {d['key']}" for d in check["file"]]
    problems += [f"CLEANUP left {d['kind']} {d['head']} {d['key']}" for d in result.get("cleanup", [])]
    for name in ("schematic", "board"):
        if name in result:
            print(f"{name}: checked {result[name]['checked']} items of {len(result[name]['types'])} types")
            if result[name]["symbol_definition"]:
                print(f"{name}: {len(result[name]['symbol_definition'])} symbols change the library cache when "
                      "sent with a full definition (informational; see report)")
    print(f"report: {report}")
    if problems:
        print("\n".join(problems))
        raise SystemExit("FAILED: API round trip changed items (see report)")
    print("PASSED item round trip: unchanged updates and re-creations leave the saved file unchanged")


if __name__ == "__main__":
    main()
