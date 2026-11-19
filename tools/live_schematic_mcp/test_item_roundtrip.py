# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline tests for the item round-trip probe helpers (no KiCad needed)."""

import json

import pytest

from client import sch, types
import e2e_item_roundtrip
from item_roundtrip import (api_fixture, block_differences, mask_fresh_ids, message_differences,
                            sexpr_blocks)

SAVED = """(kicad_sch
\t(version 20261106)
\t(uuid "00000000-0000-0000-0000-00000000aaaa")
\t(paper "A4")
\t(lib_symbols
\t\t(symbol "Device:R" (pin passive line (uuid "00000000-0000-0000-0000-0000000000ff")))
\t)
\t(no_connect
\t\t(at 10 20)
\t\t(uuid "11111111-1111-1111-1111-111111111111")
\t)
\t(text "has \\"(uuid \\"22222222-2222-2222-2222-222222222222\\")\\" in it"
\t\t(at 1 2 0)
\t\t(uuid "33333333-3333-3333-3333-333333333333")
\t)
\t(symbol
\t\t(lib_id "Device:R")
\t\t(uuid "44444444-4444-4444-4444-444444444444")
\t\t(pin "1"
\t\t\t(uuid "55555555-5555-5555-5555-555555555555")
\t\t)
\t)
\t(sheet_instances
\t\t(path "/" (page "1"))
\t)
)
"""


def test_blocks_are_keyed_by_their_own_uuid():
    blocks = sexpr_blocks(SAVED)
    # The document's own (uuid ...) is a leaf block, not an item
    assert set(blocks) == {"version#0", "uuid#0", "paper#0", "lib_symbols#0",
                           "11111111-1111-1111-1111-111111111111", "33333333-3333-3333-3333-333333333333",
                           "44444444-4444-4444-4444-444444444444", "sheet_instances#0"}
    assert blocks["11111111-1111-1111-1111-111111111111"].startswith("(no_connect")
    # A UUID inside a quoted string or a nested pin is not the block's own
    assert "pin" in blocks["44444444-4444-4444-4444-444444444444"]


def test_unbalanced_text_is_rejected():
    with pytest.raises(ValueError):
        sexpr_blocks("(kicad_sch (paper \"A4\")")
    with pytest.raises(ValueError):
        sexpr_blocks("(kicad_sch) (extra)")


def test_differences_name_the_block_and_field():
    changed = SAVED.replace("(at 10 20)", "(at 10 21)").replace('\t(paper "A4")\n', "")
    found = block_differences(SAVED, changed)
    assert [(d["key"], d["head"], d["kind"]) for d in found] == [
        ("11111111-1111-1111-1111-111111111111", "no_connect", "changed"),
        ("paper#0", "paper", "removed")]
    assert "-\t\t(at 10 20)" in found[0]["diff"] and "+\t\t(at 10 21)" in found[0]["diff"]
    assert block_differences(SAVED, SAVED) == []


def test_fresh_pin_ids_are_masked_only_when_asked():
    recreated = SAVED.replace("55555555-5555-5555-5555-555555555555", "66666666-6666-6666-6666-666666666666")
    assert [d["key"] for d in block_differences(SAVED, recreated)] == ["44444444-4444-4444-4444-444444444444"]
    assert block_differences(SAVED, recreated, mask=True) == []
    # The symbol's own UUID is never masked
    masked = mask_fresh_ids(sexpr_blocks(SAVED)["44444444-4444-4444-4444-444444444444"])
    assert '(uuid "44444444-4444-4444-4444-444444444444")' in masked and "<fresh>" in masked


def test_message_differences_list_changed_paths():
    sent = sch.NoConnectMarker(position=types.Vector2(x_nm=1, y_nm=2), size=types.Distance(value_nm=1270000))
    same = sch.NoConnectMarker()
    same.CopyFrom(sent)
    assert message_differences(sent, same) == []
    other = sch.NoConnectMarker(position=types.Vector2(x_nm=1, y_nm=2), size=types.Distance(value_nm=1219200))
    assert message_differences(sent, other) == ["size.value_nm"]


def test_explicit_differences_ignore_defaults_kiCad_fills_in():
    sent = sch.NoConnectMarker(position=types.Vector2(x_nm=1, y_nm=2))
    reply = sch.NoConnectMarker(position=types.Vector2(x_nm=1, y_nm=2), size=types.Distance(value_nm=1219200),
                                locked=types.LS_UNLOCKED)
    assert e2e_item_roundtrip.explicit_differences(sent, reply) == []
    sent.size.value_nm = 1270000
    assert e2e_item_roundtrip.explicit_differences(sent, reply) == ["size.value_nm"]


def test_fixture_covers_every_creatable_type_with_unique_ids():
    items = api_fixture()
    kinds = {type(item).DESCRIPTOR.name for item in items}
    assert kinds == {"SchematicLine", "Junction", "NoConnectMarker", "BusEntry", "SchematicText",
                     "SchematicTextBox", "SchematicGraphicShape", "LocalLabel", "GlobalLabel",
                     "HierarchicalLabel", "DirectiveLabel", "SchematicRuleArea", "SchematicImage",
                     "SheetSymbol", "SchematicTable"}
    ids = [item.id.value for item in items]
    assert len(ids) == len(set(ids)) and all(ids)
    shapes = {item.shape.WhichOneof("geometry") for item in items if isinstance(item, sch.SchematicGraphicShape)}
    assert shapes == {"rectangle", "circle", "arc", "bezier", "polygon"}
    lines = {item.type for item in items if isinstance(item, sch.SchematicLine)}
    assert lines == {sch.SLT_WIRE, sch.SLT_BUS, sch.SLT_GRAPHIC}
    # The junction sits where three wires end, so KiCad keeps it
    junction = next(item for item in items if isinstance(item, sch.Junction))
    ends = [(p.x_nm, p.y_nm) for item in items if isinstance(item, sch.SchematicLine) and item.type == sch.SLT_WIRE
            for p in (item.start, item.end)]
    assert ends.count((junction.position.x_nm, junction.position.y_nm)) == 3
    markers = [item for item in items if isinstance(item, sch.NoConnectMarker)]
    assert [m.HasField("size") for m in markers] == [False, True]
    # Rule areas must be polygons, or KiCad refuses them
    area = next(item for item in items if isinstance(item, sch.SchematicRuleArea))
    assert area.shape.WhichOneof("geometry") == "polygon"
    # Everything sits far right of an A4/A3 page
    assert all(item.id.value for item in items)


def test_prepare_copies_the_kitchen_sinks_and_the_sub_sheet(tmp_path):
    folder = tmp_path / "roundtrip-sink"
    e2e_item_roundtrip.prepare(folder)
    settings = json.loads((folder / "roundtrip-sink.kicad_pro").read_text(encoding="utf-8"))
    assert settings["meta"]["filename"] == "roundtrip-sink.kicad_pro"
    assert (folder / "roundtrip-sink.kicad_pcb").read_bytes() ==         e2e_item_roundtrip.BOARD_SINK.with_suffix(".kicad_pcb").read_bytes()
    assert (folder / "roundtrip-sink.kicad_dru").is_file()
    text = (folder / "roundtrip-sink.kicad_sch").read_text(encoding="utf-8")
    assert text == e2e_item_roundtrip.KITCHEN_SINK.read_text(encoding="utf-8")
    for line in text.splitlines():
        if '"Sheetfile"' in line:
            assert (folder / line.split('"')[3]).is_file()


def test_isolated_mode_refuses_without_explicit_selection(monkeypatch, tmp_path):
    monkeypatch.delenv("KICAD_ROUNDTRIP_ISOLATED", raising=False)
    monkeypatch.setattr("sys.argv", ["e2e_item_roundtrip.py", "--isolated", str(tmp_path)])
    with pytest.raises(SystemExit, match="REFUSED"):
        e2e_item_roundtrip.main()


def test_fixture_image_is_a_valid_png():
    import struct
    import zlib
    from item_roundtrip import PNG
    assert PNG[:8] == b"\x89PNG\r\n\x1a\n"
    offset, kinds = 8, []
    while offset < len(PNG):
        size = struct.unpack(">I", PNG[offset:offset + 4])[0]
        kind, data = PNG[offset + 4:offset + 8], PNG[offset + 8:offset + 8 + size]
        assert struct.unpack(">I", PNG[offset + 8 + size:offset + 12 + size])[0] == zlib.crc32(kind + data)
        kinds.append(kind)
        if kind == b"IDAT":
            assert len(zlib.decompress(data)) == 2 * (1 + 2 * 4)
        offset += 12 + size
    assert kinds == [b"IHDR", b"IDAT", b"IEND"]


def test_nested_sheet_pin_ids_are_masked():
    sheet = """(kicad_sch
\t(sheet
\t\t(at 1 2)
\t\t(uuid "77777777-7777-7777-7777-777777777777")
\t\t(pin "LINK" input
\t\t\t(at 1 3 180)
\t\t\t(uuid "88888888-8888-8888-8888-888888888888")
\t\t\t(effects (font (size 1.27 1.27)) (justify left))
\t\t)
\t)
)"""
    masked = mask_fresh_ids(sexpr_blocks(sheet)["77777777-7777-7777-7777-777777777777"])
    assert "88888888" not in masked and '(uuid "77777777-7777-7777-7777-777777777777")' in masked
    renewed = sheet.replace("88888888-8888-8888-8888-888888888888", "99999999-9999-9999-9999-999999999999")
    assert block_differences(sheet, renewed, mask=True) == []


def test_created_symbols_compare_without_pin_ids_or_their_order():
    from item_roundtrip import normalize_created

    def symbol(pins):
        message = sch.SchematicSymbolInstance()
        for number, uid in pins:
            message.definition.items.add().item.Pack(sch.SchematicPin(number=number, id=types.KIID(value=uid)))
        return message

    original = symbol([("2", "1111"), ("1", "2222")])
    created = symbol([("1", "3333"), ("2", "0000")])
    assert message_differences(original, created)
    assert message_differences(normalize_created(original), normalize_created(created)) == []
    # A real change still shows
    changed = symbol([("1", "3333"), ("3", "0000")])
    assert message_differences(normalize_created(original), normalize_created(changed))


def test_known_unpreservable_fields_are_split_out_with_a_reason():
    unexpected, known = e2e_item_roundtrip.split_known("NoConnectMarker", ["size.value_nm", "position.x_nm"])
    assert unexpected == ["position.x_nm"]
    assert "does not store a marker size" in known["size.value_nm"]
    assert e2e_item_roundtrip.split_known("Junction", ["size.value_nm"]) == (["size.value_nm"], {})


def test_field_values_report_what_was_sent():
    marker = sch.NoConnectMarker(size=types.Distance(value_nm=1270000))
    assert e2e_item_roundtrip.field_values(marker, ["size.value_nm", "locked"]) == {
        "size.value_nm": "1270000", "locked": None}


def test_reopen_ignore_list_matches_paths_without_indices():
    from item_roundtrip import NOT_SAVED, unindexed
    assert unindexed("table.cells[12].text_box.parent.value") == "table.cells.text_box.parent.value"
    assert unindexed("table.cells[12].text_box.parent.value") in NOT_SAVED
    assert unindexed("definition.items[1].pad_stack.copper_layers[0].custom_shapes[2].id.value") in NOT_SAVED
    assert unindexed("position.x_nm") not in NOT_SAVED


def test_colour_channels_compare_at_document_precision():
    from item_roundtrip import same_colour_channel
    assert same_colour_channel("shape.attributes.fill.color.r", 0.9, 230 / 255)
    assert same_colour_channel("color.g", 0.5, 128 / 255)
    assert not same_colour_channel("color.g", 0.5, 0.6)
    assert not same_colour_channel("position.x_nm", 1, 1.001)
    sent = sch.Junction(color=types.Color(r=0.5, g=0.3, b=0.1, a=1))
    reply = sch.Junction(color=types.Color(r=128 / 255, g=77 / 255, b=26 / 255, a=1))
    assert e2e_item_roundtrip.explicit_differences(sent, reply) == []


def test_type_checks_use_the_protobuf_type():
    from commands import message_class
    from item_roundtrip import is_type
    pooled = message_class("kiapi.schematic.types.NoConnectMarker")()
    assert is_type(pooled, sch.NoConnectMarker) and not is_type(pooled, sch.Junction)


def test_symbol_definition_graphics_ids_are_normalized_too():
    from item_roundtrip import normalize_created
    first, second = sch.SchematicSymbolInstance(), sch.SchematicSymbolInstance()
    for message, uid in ((first, "1111"), (second, "2222")):
        message.definition.items.add().item.Pack(sch.SchematicGraphicShape(id=types.KIID(value=uid)))
    assert message_differences(normalize_created(first), normalize_created(second)) == []
