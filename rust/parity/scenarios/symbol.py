# SPDX-License-Identifier: GPL-3.0-or-later
"""Parity scenarios: symbol libraries, the Symbol Editor and the library tables.

Covers the 14 live_symbol_* tools and the four footprint library-table tools that share the
table code (library_table, add_library, remove_library, reload_library with kind="footprint").
"""

from google.protobuf import empty_pb2

PIN_A = "aaaaaaaa-aaaa-1aaa-8aaa-aaaaaaaaaaaa"
PIN_B = "bbbbbbbb-bbbb-1bbb-8bbb-bbbbbbbbbbbb"
SHAPE = "cccccccc-cccc-1ccc-8ccc-cccccccccccc"
TEXT_ID = "dddddddd-dddd-1ddd-8ddd-dddddddddddd"

LDO_PINS = [{"number": "1", "name": "VIN", "type": "power_in"},
            {"number": "2", "name": "GND", "type": "power_in"},
            {"number": "3", "name": "EN", "type": "input"},
            {"number": "4", "name": "NC"},
            {"number": "5", "name": "ADJ", "type": "input"},
            {"number": " 6 ", "name": "VOUT", "type": "power_out", "shape": "SPS_INVERTED"}]


def scenarios(h):
    library, lib_types, commands, sch = (h.pyclient.library, h.pyclient.lib_types, h.pyclient.commands,
                                         h.pyclient.sch)
    editor, envelope, types, S, reply, parse, failure = (h.editor, h.envelope, h.types, h.S, h.reply,
                                                         h.parse, h.failure)
    doc = h.SYMBOL_DOC

    def empty(revision=""):
        return lambda r: reply(empty_pb2.Empty(), revision=revision)

    def lib_status(code="LCS_OK", message=None):
        value = {"code": code}
        if message is not None:
            value["error_message"] = message
        return lambda r: reply(parse(lib_types.LibraryCommandStatus, value))

    def table(*rows):
        return lambda r: reply(parse(library.LibraryTableResponse, {"rows": list(rows)}))

    rows = [{"nickname": "Device", "uri": "/usr/share/kicad/symbols/Device.kicad_sym", "type": "LT_SYMBOL",
             "scope": "LTS_GLOBAL", "description": "Stock devices"},
            {"nickname": "Lib", "uri": "C:/fixture/parity/Lib.kicad_sym", "type": "LT_SYMBOL",
             "scope": "LTS_PROJECT", "writable": True},
            {"nickname": "Old", "uri": "C:/fixture/old.kicad_sym", "scope": "LTS_GLOBAL", "disabled": True},
            {"nickname": "Odd", "uri": "x", "scope": "LTS_BOTH"}]

    open_editor = ("OpenFootprintEditor", empty())
    begin = h.BEGIN
    end_ok = h.END_OK
    drop_ok = h.DROP_OK

    def status(state, **kwargs):
        return ("GetOperationStatus", h.status(state, **kwargs))

    # ---- Library symbols ----
    def symbol(lib_id, footprint="", pins=2, units=0, extra=()):
        nickname, name = lib_id.split(":", 1)
        items = [{"item": {"@type": "type.googleapis.com/kiapi.schematic.types.SchematicPin",
                           "number": str(n + 1), "name": f"P{n + 1}"}} for n in range(pins)]
        items += list(extra)
        value = {"id": {"library_nickname": nickname, "entry_name": name}, "type": "SST_NORMAL",
                 "reference_field": {"text": {"text": "U"}}, "value_field": {"text": {"text": name}},
                 "footprint_field": {"text": {"text": footprint}},
                 "description_field": {"text": {"text": f"{name} description"}},
                 "keywords": "regulator ldo", "items": items}
        if units:
            value["unit_count"] = units
        return parse(sch.SchematicSymbol, value)

    def library_items(*symbols):
        response = editor.GetItemsResponse(status=types.IRS_OK)
        for item in symbols:
            response.items.add().Pack(item)
        return lambda r: reply(response)

    def search_reply(*ids, loading=0):
        value = {"items": [dict(zip(("library_nickname", "entry_name"), i.split(":", 1))) for i in ids]}
        if loading:
            value["libraries_loading"] = loading
        return lambda r: reply(parse(library.SearchLibrariesResponse, value))

    rich_symbol = parse(sch.SchematicSymbol, {
        "id": {"library_nickname": "Lib", "entry_name": "TLV757"}, "type": "SST_NORMAL",
        "reference_field": {"text": {"text": "U", "position": {"x_nm": "5080000", "y_nm": "-1270001"},
                                     "attributes": {"horizontal_alignment": "HA_LEFT"}}},
        "value_field": {"text": {"text": "TLV757", "position": {"x_nm": "1", "y_nm": "0"}}},
        "footprint_field": {"text": {"text": "Package_TO_SOT_SMD:SOT-23-5"}},
        "datasheet_field": {"text": {"text": "https://www.ti.com/lit/ds/symlink/tlv757p.pdf"}},
        "description_field": {"text": {"text": "1 A LDO"}},
        "keywords": "ldo regulator", "footprint_filters": ["SOT?23*", "SOT-23*"], "unit_count": 2,
        "items": [
            {"item": {"@type": "type.googleapis.com/kiapi.schematic.types.SchematicField",
                      "name": "Manufacturer", "text": {"text": "TI"}}},
            {"item": {"@type": "type.googleapis.com/kiapi.schematic.types.SchematicField",
                      "name": "", "text": {"text": "nameless"}}},
            {"item": {"@type": "type.googleapis.com/kiapi.schematic.types.SchematicField",
                      "name": "MPN", "text": {"text": "TLV75733PDBVR"}}},
            {"item": {"@type": "type.googleapis.com/kiapi.schematic.types.SchematicPin", "number": "10",
                      "name": "NC", "electrical_type": "EPT_NO_CONNECT", "visible": False,
                      "position": {"x_nm": "-7620000", "y_nm": "2540000"}, "orientation": "SPO_RIGHT"},
             "unit": {"unit": 2}},
            {"item": {"@type": "type.googleapis.com/kiapi.schematic.types.SchematicPin", "number": "2",
                      "name": "GND", "electrical_type": "EPT_POWER_INPUT", "visible": True,
                      "position": {"x_nm": "0", "y_nm": "7620000"}, "orientation": "SPO_UP"}},
            {"item": {"@type": "type.googleapis.com/kiapi.schematic.types.SchematicPin", "number": "1",
                      "name": "VIN", "electrical_type": "EPT_POWER_INPUT", "visible": True,
                      "position": {"x_nm": "-7620000", "y_nm": "-2540000"}, "orientation": "SPO_RIGHT"},
             "unit": {"unit": 1}},
            {"item": {"@type": "type.googleapis.com/kiapi.schematic.types.SchematicPin", "number": "A1",
                      "name": "VOUT", "electrical_type": "EPT_POWER_OUTPUT", "visible": True,
                      "position": {"x_nm": "7620000", "y_nm": "-2540000"}, "orientation": "SPO_LEFT"}},
            {"item": {"@type": "type.googleapis.com/kiapi.schematic.types.SchematicGraphicShape",
                      "id": {"value": SHAPE}}}]})

    odd_pin = parse(sch.SchematicSymbol, {
        "id": {"library_nickname": "Lib", "entry_name": "Odd"},
        "items": [{"item": {"@type": "type.googleapis.com/kiapi.schematic.types.SchematicPin", "number": "1",
                            "electrical_type": 99}}]})

    # ---- Symbol Editor ----
    new_doc = {"type": "DOCTYPE_SYMBOL", "lib_id": {"library_nickname": "Lib", "entry_name": "NEW1"}}
    new_reply = ("NewLibrarySymbol", lambda r: reply(parse(commands.LibrarySymbolResponse, {"document": new_doc})))
    place = ("SetLibrarySymbolProperties", empty("fx-2"))

    def created(count, codes=None, **kwargs):
        return h.created(*(codes or [editor.ISC_OK] * count), **kwargs)

    def deleted(*codes, status=None):
        def respond(request):
            delete = editor.DeleteItems()
            request.message.Unpack(delete)
            result = editor.DeleteItemsResponse(status=types.IRS_OK if status is None else status)
            for item_id, code in zip(delete.item_ids, codes):
                entry = result.deleted_items.add()
                entry.id.CopyFrom(item_id)
                entry.status = code
            return reply(result, revision="fx-2")
        return ("DeleteItems", respond)

    def editor_items(*values):
        return lambda r: reply(parse(editor.GetItemsResponse, {"header": {"document": doc}, "status": "IRS_OK",
                                                               "items": list(values)}))

    pin_item = {"@type": "type.googleapis.com/kiapi.schematic.types.SchematicPin", "id": {"value": PIN_A},
                "number": "1", "name": "VIN", "electrical_type": "EPT_POWER_INPUT", "visible": True,
                "position": {"x_nm": "-7620000", "y_nm": "-2540000"}, "orientation": "SPO_RIGHT"}
    hidden_pin = {"@type": "type.googleapis.com/kiapi.schematic.types.SchematicPin", "id": {"value": PIN_B},
                  "number": "4", "name": "NC", "electrical_type": "EPT_NO_CONNECT",
                  "position": {"x_nm": "1234567", "y_nm": "1"}, "orientation": "SPO_LEFT"}
    shape_item = {"@type": "type.googleapis.com/kiapi.schematic.types.SchematicGraphicShape",
                  "id": {"value": SHAPE}}
    shape_no_id = {"@type": "type.googleapis.com/kiapi.schematic.types.SchematicGraphicShape"}
    text_item = {"@type": "type.googleapis.com/kiapi.schematic.types.SchematicText", "id": {"value": TEXT_ID}}

    add_pins = {"symbol_document": doc, "pins": LDO_PINS}
    as_given = {"symbol_document": doc, "arrange": "as_given", "group_gap": 0, "pins": [
        {"number": "1", "name": "IN", "side": "left"}, {"gap": True, "side": "left"},
        {"number": "2", "name": 5, "side": "left", "shape": "SPS_CLOCK"},
        {"number": "3", "name": "CLK", "side": "top", "type": "input"},
        {"number": "4", "name": "OUT", "side": "right", "type": "output", "hidden": True},
        {"number": "5", "name": "NC", "hidden": False},
        {"number": "6", "name": "DNC"}, {"number": "7", "type": "no_connect"},
        {"number": "8", "name": "GND", "side": "bottom"}, {"number": "9", "name": "PAD"}]}

    props_all = {"symbol_document": doc, "value": "", "footprint": "Package_TO_SOT_SMD:SOT-23-5",
                 "datasheet": "https://example.com/ds.pdf", "description": "1 A LDO", "keywords": "ldo",
                 "reference_prefix": "U", "footprint_filters": ["SOT?23*"],
                 "user_fields": [{"name": "Distributor PN", "value": "296-1", "visible": "yes"},
                                 {"name": "Note"}, {"name": "Count", "value": 3, "visible": 0},
                                 {"name": "Data", "value": {"a": [1.5, None, True]}, "visible": [1]}],
                 "remove_user_fields": ["Old"], "show_pin_numbers": False, "show_pin_names": True,
                 "exclude_from_bom": False, "exclude_from_board": True, "exclude_from_sim": False,
                 "manufacturer": "TI", "mpn": ""}

    results = [
        # ---- Library tables ----
        S("symbol_libraries", [("live_symbol_libraries", {})], [("GetLibraryTable", table(*rows))]),
        S("symbol_libraries_project", [("live_symbol_libraries", {"scope": "project"})],
          [("GetLibraryTable", table())]),
        S("symbol_libraries_bad_scope", [("live_symbol_libraries", {"scope": "it's\\x"})], []),
        S("symbol_libraries_refused", [("live_symbol_libraries", {"scope": "global"})],
          [("GetLibraryTable", lambda r: failure(envelope.AS_UNHANDLED, "no handler"))]),
        S("symbol_libraries_scope_wrong_type", [("live_symbol_libraries", {"scope": 3})], [],
          compare="error_prefix"),
        S("symbol_footprint_libraries", [("live_footprint_libraries", {})],
          [open_editor, ("GetLibraryTable", table(rows[1]))]),
        S("symbol_footprint_libraries_bad_scope", [("live_footprint_libraries", {"scope": "all"})], [open_editor]),
        S("symbol_footprint_libraries_editor_busy", [("live_footprint_libraries", {})],
          [("OpenFootprintEditor", lambda r: failure(envelope.AS_BUSY, "modal dialog open"))]),
        S("symbol_create_library", [("live_symbol_create_library",
                                     {"nickname": "Mine", "path": "${KIPRJMOD}/Mine.kicad_sym//",
                                      "description": "Project parts"})],
          [("AddLibraryTableEntry", lib_status())]),
        S("symbol_create_library_project", [("live_symbol_create_library",
                                             {"nickname": "Mine", "path": "Mine.kicad_sym", "scope": "project"})],
          [("AddLibraryTableEntry", lib_status())]),
        S("symbol_create_library_bad_suffix", [("live_symbol_create_library",
                                                {"nickname": "Mine", "path": "Mine.lib"})], []),
        S("symbol_create_library_bad_scope", [("live_symbol_create_library",
                                               {"nickname": "Mine", "path": "Mine.lib", "scope": "both"})], []),
        S("symbol_create_library_exists", [("live_symbol_create_library",
                                            {"nickname": "Mine", "path": "Mine.kicad_sym"})],
          [("AddLibraryTableEntry", lib_status("LCS_ALREADY_EXISTS", "nickname Mine is in use"))]),
        S("symbol_create_library_error_without_message", [("live_symbol_create_library",
                                                           {"nickname": "Mine", "path": "Mine.kicad_sym"})],
          [("AddLibraryTableEntry", lib_status("LCS_ERROR"))]),
        S("symbol_create_library_unknown_code", [("live_symbol_create_library",
                                                  {"nickname": "Mine", "path": "Mine.kicad_sym"})],
          [("AddLibraryTableEntry", lib_status(9, "?"))]),
        S("symbol_register_library", [("live_symbol_register_library",
                                       {"nickname": "Old", "path": "/libs/old.lib/", "scope": "project"})],
          [("AddLibraryTableEntry", lib_status())]),
        S("symbol_register_library_lost_reply", [("live_symbol_register_library",
                                                  {"nickname": "Old", "path": "/libs/old.kicad_sym"})],
          [("AddLibraryTableEntry", lambda r: h.DROP)]),
        S("symbol_footprint_create_library", [("live_footprint_create_library",
                                               {"nickname": "Fp", "path": "C:/libs/Fp.pretty/", "scope": "project"})],
          [open_editor, ("AddLibraryTableEntry", lib_status())]),
        S("symbol_footprint_create_library_bad_suffix", [("live_footprint_create_library",
                                                          {"nickname": "Fp", "path": "C:/libs/Fp.kicad_sym"})], []),
        S("symbol_footprint_register_library", [("live_footprint_register_library",
                                                 {"nickname": "Fp", "path": "C:/libs/Fp", "description": "d"})],
          [open_editor, ("AddLibraryTableEntry", lib_status("LCS_READ_ONLY", "table is read-only"))]),
        S("symbol_footprint_register_library_bad_scope", [("live_footprint_register_library",
                                                           {"nickname": "Fp", "path": "x", "scope": "x"})], []),
        S("symbol_unregister_library", [("live_symbol_unregister_library", {"nickname": "Mine"})],
          [("DeleteLibraryTableEntry", lib_status())]),
        S("symbol_unregister_library_not_found", [("live_symbol_unregister_library",
                                                   {"nickname": "Nope", "scope": "project"})],
          [("DeleteLibraryTableEntry", lib_status("LCS_NOT_FOUND", ""))]),
        S("symbol_unregister_library_both_scopes", [("live_symbol_unregister_library",
                                                   {"nickname": "Mine", "scope": "both"})],
          [("DeleteLibraryTableEntry", lib_status())]),
        S("symbol_unregister_library_unknown_scope", [("live_symbol_unregister_library",
                                                       {"nickname": "Mine", "scope": "everywhere"})], []),
        S("symbol_footprint_unregister_library", [("live_footprint_unregister_library", {"nickname": "Fp"})],
          [open_editor, ("DeleteLibraryTableEntry", lib_status())]),
        S("symbol_footprint_unregister_library_bad_scope", [("live_footprint_unregister_library",
                                                             {"nickname": "Fp", "scope": "nope"})], [open_editor]),
        S("symbol_library_nickname_missing", [("live_symbol_unregister_library", {})], [], compare="error_prefix"),

        # ---- Search and read ----
        S("symbol_search", [("live_symbol_search", {"query": "ldo"})],
          [("SearchLibraries", search_reply("Lib:TLV757", "Device:R", "Lib:Missing", loading=2)),
           ("GetItemsFromLibrary", library_items(symbol("Device:R", footprint="Resistor_SMD:R_0603", units=1),
                                                 symbol("Lib:TLV757", pins=5, units=2),
                                                 symbol("Lib:TLV757", pins=3)))]),
        S("symbol_search_none", [("live_symbol_search", {"query": "zzz"})], [("SearchLibraries", search_reply())]),
        S("symbol_search_none_loading", [("live_symbol_search", {"query": "zzz"})],
          [("SearchLibraries", search_reply(loading=5))]),
        S("symbol_search_limits", [("live_symbol_search", {"query": "r", "limit": 0}),
                                   ("live_symbol_search", {"query": "r", "limit": "2"}),
                                   ("live_symbol_search", {"query": "r", "limit": 10 ** 30})],
          [("SearchLibraries", search_reply("Device:R", "Device:R_Small")),
           ("GetItemsFromLibrary", library_items()),
           ("SearchLibraries", search_reply(*[f"Device:R{n}" for n in range(3)])),
           ("GetItemsFromLibrary", library_items(symbol("Device:R1", extra=[
               {"item": {"@type": "type.googleapis.com/kiapi.schematic.types.SchematicText"}}]))),
           ("SearchLibraries", search_reply(*[f"Big:S{n}" for n in range(203)])),
           ("GetItemsFromLibrary", library_items())]),
        S("symbol_search_bad_id_from_kicad", [("live_symbol_search", {"query": "x"})],
          [("SearchLibraries", search_reply(":R"))]),
        S("symbol_search_lost_reply", [("live_symbol_search", {"query": "x"})],
          [("SearchLibraries", lambda r: h.DROP)]),
        S("symbol_search_limit_wrong_type", [("live_symbol_search", {"query": "x", "limit": "many"})], [],
          compare="error_prefix"),
        S("symbol_get", [("live_symbol_get", {"lib_id": "Lib:TLV757"})],
          [("GetItemsFromLibrary", library_items(rich_symbol))]),
        S("symbol_get_plain", [("live_symbol_get", {"lib_id": "Device:R"})],
          [("GetItemsFromLibrary", library_items(symbol("Device:R")))]),
        S("symbol_get_not_found", [("live_symbol_get", {"lib_id": "Lib:Nope"})],
          [("GetItemsFromLibrary", library_items())]),
        S("symbol_get_bad_lib_id", [("live_symbol_get", {"lib_id": "TLV757"})], []),
        S("symbol_get_unknown_pin_type", [("live_symbol_get", {"lib_id": "Lib:Odd"})],
          [("GetItemsFromLibrary", library_items(odd_pin))]),
        S("symbol_get_refused", [("live_symbol_get", {"lib_id": "Lib:X"})],
          [("GetItemsFromLibrary", lambda r: failure(envelope.AS_BAD_REQUEST, "library Lib not loaded"))]),

        # ---- New and edit ----
        S("symbol_new", [("live_symbol_new", {"library": "Lib", "name": "NEW1", "manufacturer": "TI"})],
          [new_reply, begin, ("SetLibrarySymbolProperties", empty("fx-2")), end_ok]),
        S("symbol_new_all_options", [("live_symbol_new", {
            "library": "Lib", "name": "NEW1", "reference_prefix": "PS", "unit_count": "3", "inherit_from": "BASE",
            "power_symbol": True, "pin_names_outside": True, "exclude_from_bom": True,
            "exclude_from_board": "yes", "replace_existing": 1, "mpn": "ABC-1"})],
          [new_reply, begin, ("SetLibrarySymbolProperties", empty("fx-2")), end_ok]),
        S("symbol_new_unit_count_out_of_range", [("live_symbol_new", {"library": "Lib", "name": "X",
                                                                      "unit_count": 2 ** 31})], []),
        S("symbol_new_unit_count_huge", [("live_symbol_new", {"library": "Lib", "name": "X",
                                                              "unit_count": -10 ** 25})], []),
        S("symbol_new_refused", [("live_symbol_new", {"library": "Device", "name": "X"})],
          [("NewLibrarySymbol", lambda r: failure(envelope.AS_BAD_REQUEST, "library Device is read-only"))]),
        S("symbol_new_properties_rolled_back", [("live_symbol_new", {"library": "Lib", "name": "NEW1"})],
          [new_reply, begin, ("SetLibrarySymbolProperties", lambda r: failure(envelope.AS_BAD_REQUEST, "no")),
           drop_ok]),
        S("symbol_new_properties_commit_lost", [("live_symbol_new", {"library": "Lib", "name": "NEW1"})],
          [new_reply, begin, place, ("EndCommit", lambda r: h.DROP),
           status("OPS_COMMITTED", revision_after="fx-5")]),
        S("symbol_new_name_missing", [("live_symbol_new", {"library": "Lib"})], [], compare="error_prefix"),
        S("symbol_edit", [("live_symbol_edit", {"lib_id": "Lib:TLV757"})],
          [("EditLibrarySymbol", lambda r: reply(parse(commands.LibrarySymbolResponse, {"document": doc})))]),
        S("symbol_edit_bad_lib_id", [("live_symbol_edit", {"lib_id": "Lib:"})], []),
        S("symbol_edit_busy", [("live_symbol_edit", {"lib_id": "Lib:R"})],
          [("EditLibrarySymbol", lambda r: failure(envelope.AS_BUSY, "Symbol Editor has unsaved changes"))]),

        # ---- Pins ----
        S("symbol_add_pins", [("live_symbol_add_pins", add_pins)], [begin, created(7), place, end_ok],
          random_ids=True),
        S("symbol_add_pins_no_body", [("live_symbol_add_pins", dict(add_pins, draw_body=False, group_gap=0))],
          [begin, created(6), end_ok], random_ids=True),
        S("symbol_add_pins_as_given", [("live_symbol_add_pins", as_given)], [begin, created(11), place, end_ok],
          random_ids=True),
        S("symbol_add_pins_replace", [("live_symbol_add_pins", dict(add_pins, replace=True))],
          [("GetItems", editor_items(pin_item, text_item, shape_item, shape_no_id)),
           begin, deleted(editor.IDS_OK, editor.IDS_OK), end_ok,
           begin, created(7), place, end_ok], random_ids=True),
        S("symbol_add_pins_replace_nothing", [("live_symbol_add_pins", dict(add_pins, replace=True))],
          [("GetItems", editor_items(text_item)), begin, created(7), place, end_ok], random_ids=True),
        S("symbol_add_pins_replace_without_ids", [("live_symbol_add_pins",
                                                   dict(add_pins, replace=True, draw_body=False))],
          [("GetItems", editor_items(shape_no_id)), begin, deleted(), end_ok, begin, created(6), end_ok],
          random_ids=True),
        S("symbol_add_pins_replace_delete_rejected", [("live_symbol_add_pins", dict(add_pins, replace=True))],
          [("GetItems", editor_items(pin_item)), begin, deleted(editor.IDS_IMMUTABLE), drop_ok]),
        S("symbol_add_pins_layout_errors", [
            ("live_symbol_add_pins", dict(add_pins, pins=[{"number": "1"}, {"number": "1"}])),
            ("live_symbol_add_pins", dict(add_pins, arrange="sorted")),
            ("live_symbol_add_pins", dict(add_pins, group_gap=7)),
            ("live_symbol_add_pins", dict(add_pins, group_gap=10 ** 20)),
            ("live_symbol_add_pins", dict(add_pins, pins=[])),
            ("live_symbol_add_pins", dict(add_pins, pins=[{"name": "X"}])),
            ("live_symbol_add_pins", dict(add_pins, pins=[{"number": "1", "type": "power"}])),
            ("live_symbol_add_pins", dict(add_pins, pins=[{"number": "1", "side": "middle"}])),
            ("live_symbol_add_pins", dict(add_pins, pins=[{"number": "1"}, {"gap": True, "side": ["l"]}])),
            ("live_symbol_add_pins", dict(add_pins, pins=[{"number": "1"}, {"gap": True, "side": "up"}]))], []),
        S("symbol_add_pins_bad_shapes", [
            ("live_symbol_add_pins", dict(add_pins, pins=[{"number": "1", "shape": "SPS_WAVY"}])),
            ("live_symbol_add_pins", dict(add_pins, pins=[{"number": "1", "shape": 3}])),
            ("live_symbol_add_pins", dict(add_pins, pins=[{"number": "1", "shape": None}])),
            ("live_symbol_add_pins", dict(add_pins, pins=[{"number": "1", "shape": ["SPS_LINE"]}])),
            ("live_symbol_add_pins", dict(add_pins, pins=[{"number": "1", "shape": {"a": 1}}]))], []),
        # A pin's own orientation and length go through SchematicPinOrientation.Value() and
        # round(length * 1_000_000), with Python's errors for every JSON type
        S("symbol_add_pins_bad_orientation_length", [
            ("live_symbol_add_pins", dict(add_pins, pins=[{"number": "1", "orientation": value}]))
            for value in (2, None, "bogus", ["SPO_LEFT"])] + [
            ("live_symbol_add_pins", dict(add_pins, pins=[{"number": "1", "length": value}]))
            for value in ("3.81", None, [1], {"a": 1}, 1e300, 10_000_000_000_000)], []),
        S("symbol_add_pins_own_orientation_length", [
            ("live_symbol_add_pins", dict(add_pins, pins=[
                {"number": "1", "orientation": "SPO_UP", "length": True},
                {"number": "2", "length": 2.5e-7}, {"number": "3", "length": 3}]))],
          [begin, created(4), place, end_ok], random_ids=True),
        S("symbol_add_pins_bad_shape_after_replace", [("live_symbol_add_pins", dict(
            add_pins, replace=True, pins=[{"number": "1", "shape": "bad"}]))],
          [("GetItems", editor_items(pin_item)), begin, deleted(editor.IDS_OK), end_ok], random_ids=True),
        S("symbol_add_pins_item_rejected", [("live_symbol_add_pins", add_pins)],
          [begin, created(7, [1, 1, editor.ISC_INVALID_DATA, 1, 1, 1, 1], message="pin outside the body"), drop_ok],
          random_ids=True),
        S("symbol_add_pins_item_rejected_without_message", [("live_symbol_add_pins", add_pins)],
          [begin, created(7, [editor.ISC_EXISTING] * 7), drop_ok], random_ids=True),
        S("symbol_add_pins_status_refused", [("live_symbol_add_pins", add_pins)],
          [begin, created(7, status=types.IRS_DOCUMENT_NOT_FOUND), drop_ok], random_ids=True),
        S("symbol_add_pins_place_refused", [("live_symbol_add_pins", add_pins)],
          [begin, created(7), ("SetLibrarySymbolProperties", lambda r: failure(envelope.AS_BAD_REQUEST, "x")),
           drop_ok], random_ids=True),
        S("symbol_add_pins_step_lost", [("live_symbol_add_pins", add_pins)],
          [begin, ("CreateItems", lambda r: h.DROP), drop_ok], random_ids=True),
        S("symbol_add_pins_rollback_unconfirmed", [("live_symbol_add_pins", add_pins)],
          [begin, created(7, status=types.IRS_DOCUMENT_NOT_FOUND),
           ("EndCommit", lambda r: failure(envelope.AS_BAD_REQUEST, "no such commit"))], random_ids=True),
        S("symbol_add_pins_commit_lost_committed", [("live_symbol_add_pins", add_pins)],
          [begin, created(7), place, ("EndCommit", lambda r: h.DROP),
           status("OPS_COMMITTED", revision_after="fx-5")], random_ids=True),
        S("symbol_add_pins_commit_lost_in_progress", [("live_symbol_add_pins", add_pins)],
          [begin, created(7), place, ("EndCommit", lambda r: h.DROP), status("OPS_IN_PROGRESS"), drop_ok,
           status("OPS_ROLLED_BACK", message="dropped by its client")], random_ids=True),
        S("symbol_add_pins_commit_lost_unknown", [("live_symbol_add_pins", add_pins)],
          [begin, created(7), place, ("EndCommit", lambda r: h.DROP), status("OPS_UNKNOWN")], random_ids=True),
        S("symbol_add_pins_conflict_at_commit", [("live_symbol_add_pins", add_pins)],
          [begin, created(7), place,
           ("EndCommit", lambda r: failure(envelope.AS_REVISION_MISMATCH, "changed while open", "fx-8"))],
          random_ids=True),
        S("symbol_add_pins_begin_lost_open", [("live_symbol_add_pins", add_pins)],
          [("BeginCommit", lambda r: h.DROP), status("OPS_IN_PROGRESS"), drop_ok]),
        S("symbol_add_pins_begin_lost_unseen", [("live_symbol_add_pins", add_pins)],
          [("BeginCommit", lambda r: h.DROP), status("OPS_UNKNOWN")]),
        S("symbol_add_pins_begin_refused", [("live_symbol_add_pins", add_pins)],
          [("BeginCommit", lambda r: failure(envelope.AS_BUSY, "editor busy"))]),
        S("symbol_add_pins_bad_document", [("live_symbol_add_pins", dict(add_pins, symbol_document=h.FOOTPRINT_DOC))],
          []),
        S("symbol_add_pins_document_without_entry", [("live_symbol_add_pins", dict(
            add_pins, symbol_document={"type": "DOCTYPE_SYMBOL", "lib_id": {"library_nickname": "Lib"}}))], []),
        S("symbol_add_pins_pins_not_a_list", [("live_symbol_add_pins", dict(add_pins, pins=3))], [],
          compare="error_prefix"),

        # ---- Properties ----
        S("symbol_set_properties", [("live_symbol_set_properties", props_all)],
          [begin, ("SetLibrarySymbolProperties", empty("fx-2")), end_ok]),
        S("symbol_set_properties_nothing", [("live_symbol_set_properties", {"symbol_document": doc})],
          [begin, ("SetLibrarySymbolProperties", empty()), end_ok]),
        S("symbol_set_properties_reference_only", [("live_symbol_set_properties", {
            "symbol_document": doc, "reference_prefix": "IC", "footprint_filters": [], "user_fields": [],
            "remove_user_fields": [], "mpn": None})],
          [begin, ("SetLibrarySymbolProperties", empty()), end_ok]),
        S("symbol_set_properties_field_without_name", [("live_symbol_set_properties", {
            "symbol_document": doc, "user_fields": [{"value": "x"}]})], []),
        S("symbol_set_properties_field_name_not_text", [("live_symbol_set_properties", {
            "symbol_document": doc, "user_fields": [{"name": 5}]})], []),
        S("symbol_set_properties_field_name_null", [("live_symbol_set_properties", {
            "symbol_document": doc, "user_fields": [{"name": None, "value": None}]})],
          [begin, ("SetLibrarySymbolProperties", empty()), end_ok]),
        S("symbol_set_properties_refused", [("live_symbol_set_properties", dict(props_all, mpn="X"))],
          [begin, ("SetLibrarySymbolProperties", lambda r: failure(envelope.AS_BAD_REQUEST, "bad footprint")),
           drop_ok]),
        S("symbol_set_properties_step_lost", [("live_symbol_set_properties", {"symbol_document": doc,
                                                                               "value": "X"})],
          [begin, ("SetLibrarySymbolProperties", lambda r: h.DROP), drop_ok]),
        S("symbol_set_properties_commit_lost_rolled_back", [("live_symbol_set_properties",
                                                             {"symbol_document": doc, "value": "X"})],
          [begin, place, ("EndCommit", lambda r: h.DROP), status("OPS_ROLLED_BACK", message="lease expired")]),
        S("symbol_set_properties_commit_lost_query_lost", [("live_symbol_set_properties",
                                                            {"symbol_document": doc, "value": "X"})],
          [begin, place, ("EndCommit", lambda r: h.DROP), ("GetOperationStatus", lambda r: h.DROP)]),
        S("symbol_set_properties_begin_lost_rejected", [("live_symbol_set_properties",
                                                         {"symbol_document": doc, "value": "X"})],
          [("BeginCommit", lambda r: h.DROP), status("OPS_REJECTED", message="stale")]),
        S("symbol_set_properties_schematic_document", [("live_symbol_set_properties",
                                                        {"symbol_document": h.SHEET_DOC, "value": "X"})], []),
        S("symbol_set_properties_bool_wrong_type", [("live_symbol_set_properties",
                                                     {"symbol_document": doc, "show_pin_names": "maybe"})], [],
          compare="error_prefix"),

        # ---- View, summary, delete, save ----
        S("symbol_view", [("live_symbol_view", {"symbol_document": doc})],
          [("RenderLibrarySymbol", lambda r: reply(commands.RenderLibrarySymbolResponse(
              image=b"\x89PNG\r\n\x1a\nfixture", mime_type="image/png")))]),
        S("symbol_view_dpi", [("live_symbol_view", {"symbol_document": doc, "dpi": "150"}),
                              ("live_symbol_view", {"symbol_document": doc, "dpi": 1200})],
          [("RenderLibrarySymbol", lambda r: reply(commands.RenderLibrarySymbolResponse())),
           ("RenderLibrarySymbol", lambda r: reply(commands.RenderLibrarySymbolResponse(image=b"x")))]),
        S("symbol_view_dpi_out_of_range", [("live_symbol_view", {"symbol_document": doc, "dpi": 149}),
                                           ("live_symbol_view", {"symbol_document": doc, "dpi": 1201}),
                                           ("live_symbol_view", {"symbol_document": doc, "dpi": 10 ** 22})], []),
        S("symbol_view_refused", [("live_symbol_view", {"symbol_document": doc})],
          [("RenderLibrarySymbol", lambda r: failure(envelope.AS_UNHANDLED, "Symbol Editor is closed"))]),
        S("symbol_view_bad_document", [("live_symbol_view", {"symbol_document": {"type": "DOCTYPE_SYMBOL"}})], []),
        S("symbol_summary", [("live_symbol_summary", {"symbol_document": doc})],
          [("GetItems", editor_items(pin_item, text_item, shape_item, hidden_pin, shape_no_id))]),
        S("symbol_summary_empty", [("live_symbol_summary", {"symbol_document": json_text(doc)})],
          [("GetItems", lambda r: reply(editor.GetItemsResponse(status=types.IRS_DOCUMENT_NOT_FOUND)))]),
        S("symbol_summary_unknown_orientation", [("live_symbol_summary", {"symbol_document": doc})],
          [("GetItems", editor_items(dict(pin_item, orientation=42)))]),
        S("symbol_delete_items", [("live_symbol_delete_items", {
            "symbol_document": doc, "ids": [PIN_A, "{" + PIN_B.upper() + "}", "urn:uuid:" + SHAPE]})],
          [begin, deleted(editor.IDS_OK, editor.IDS_OK, editor.IDS_OK), end_ok]),
        S("symbol_delete_items_none", [("live_symbol_delete_items", {"symbol_document": doc, "ids": []})],
          [begin, deleted(), end_ok]),
        S("symbol_delete_items_bad_uuid", [("live_symbol_delete_items",
                                            {"symbol_document": doc, "ids": [PIN_A, "not-a-uuid"]})], []),
        S("symbol_delete_items_bad_hex", [("live_symbol_delete_items",
                                           {"symbol_document": doc, "ids": ["g" * 32]})], []),
        S("symbol_delete_items_rejected", [("live_symbol_delete_items",
                                            {"symbol_document": doc, "ids": [PIN_A, PIN_B]})],
          [begin, deleted(editor.IDS_OK, editor.IDS_NONEXISTENT), drop_ok]),
        S("symbol_delete_items_status_refused", [("live_symbol_delete_items", {"symbol_document": doc,
                                                                                "ids": [PIN_A]})],
          [begin, deleted(editor.IDS_OK, status=types.IRS_DOCUMENT_NOT_FOUND), drop_ok]),
        S("symbol_delete_items_step_lost", [("live_symbol_delete_items", {"symbol_document": doc, "ids": [PIN_A]})],
          [begin, ("DeleteItems", lambda r: h.DROP), drop_ok]),
        S("symbol_delete_items_commit_lost_committed", [("live_symbol_delete_items",
                                                         {"symbol_document": doc, "ids": [PIN_A]})],
          [begin, deleted(editor.IDS_OK), ("EndCommit", lambda r: h.DROP), status("OPS_IN_PROGRESS"),
           ("EndCommit", lambda r: failure(envelope.AS_BAD_REQUEST, "no commit to drop")),
           status("OPS_COMMITTED", revision_after="fx-6")]),
        S("symbol_delete_items_ids_wrong_type", [("live_symbol_delete_items", {"symbol_document": doc,
                                                                                "ids": [1]})], [],
          compare="error_prefix"),
        S("symbol_save", [("live_symbol_save", {"symbol_document": doc})],
          [("SaveDocument", empty()), ("GetLibraryTable", table(*rows)), ("ReloadLibrary", lib_status())]),
        S("symbol_save_library_missing", [("live_symbol_save", {"symbol_document": doc})],
          [("SaveDocument", empty()), ("GetLibraryTable", table(rows[0]))]),
        S("symbol_save_unknown_scope", [("live_symbol_save", {"symbol_document": dict(
            doc, lib_id={"library_nickname": "Odd", "entry_name": "R"})})],
          [("SaveDocument", empty()), ("GetLibraryTable", table(*rows))]),
        S("symbol_save_refused", [("live_symbol_save", {"symbol_document": doc})],
          [("SaveDocument", lambda r: failure(envelope.AS_BAD_REQUEST, "library is read-only"))]),
        S("symbol_save_reload_failed", [("live_symbol_save", {"symbol_document": doc})],
          [("SaveDocument", empty()), ("GetLibraryTable", table(*rows)),
           ("ReloadLibrary", lib_status("LCS_ERROR", "parse error "))]),
        S("symbol_save_lost_reply", [("live_symbol_save", {"symbol_document": doc})],
          [("SaveDocument", lambda r: h.DROP)]),
    ]
    return results


def json_text(value):
    import json
    return json.dumps(value)
