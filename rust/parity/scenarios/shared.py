# SPDX-License-Identifier: GPL-3.0-or-later
"""Parity scenarios for the shared area: the bundled skills (live_skill_reference and the
kicad-skill:// resources) and live_editor_grid in every editor.

The skill scenarios contact no KiCad (their scripts are empty): the servers must list, read and
refuse exactly alike, with the same text and hashes.  Resource steps ("resources/...") send the
URI as given, so both servers' own URI handling (pydantic's AnyUrl in Python) is compared.
"""

import skills

DEFAULT = skills.DEFAULT_SKILL


def skill_files():
    """Every file the Python server lists, so a new skill file is covered without editing this."""
    return [(entry["skill"], name) for entry in skills.skill_index() for name in entry["files"]]


# Requested names that are refused, or that only a case-insensitive file system accepts
SKILL_NAMES = ["Bad", "KICAD-house-style", "kicad_house_style", "-kicad", "kicad-", "kicad--house", "nope",
               "../kicad-house-style", "kicad-house-style/", " kicad-house-style", "kicad-house-style\n",
               '"kicad-house-style"', "\uff4bicad-house-style", "kicad-house-style/../kicad-house-style", "."]
FILE_NAMES = ["missing.md", "../server.py", "..\\server.py", "/SKILL.md", "SKILL.MD", "skill.md", "PCB.md", "pcb",
              "pcb.md/", ".md", "_x.md", "-x.md", "a b.md", "SKILL.md\n", "C:\\SKILL.md", "%2e%2e.md",
              "pcb.md.md", "x" * 300 + ".md", "", "..", "SKILL.md:stream", "pcb.md ", "\u00e9.md"]

RESOURCE_REFUSALS = [
    "kicad-skill://nope", "kicad-skill://Bad/SKILL.md", "kicad-skill://kicad-house-style/x.txt",
    "kicad-skill://kicad-house-style/missing.md", "kicad-skill://kicad-house-style/a/b.md",
    "kicad-skill://kicad-house-style/pcb.md?x=1", "kicad-skill://kicad-house-style/pcb.md#part",
    "kicad-skill://kicad-house-style/p cb.md", "kicad-skill://kicad-house-style/%41.md",
    "kicad-skill:kicad-house-style/pcb.md", "kicad-skill:///kicad-house-style/pcb.md",
    "kicad-skill://kicad-house-style/", "kicad-skill://index/", "kicad-skill://user@kicad-house-style/pcb.md",
    "kicad-skill://kicad-house-style:80/pcb.md", "kicad-skill://[::1]/x.md", "kicad-skill://Index",
    "kicad-skill://kicad-house-style/PCB.md", "http://x", "embeint://MCP/capabilities/",
    # Not URLs: the SDK refuses the request's parameters
    "not a url", "", "kicad-skill://a b/x.md", "kicad-skill://kicad-house-style\\pcb.md",
]
RESOURCE_NORMALISED = [
    "KICAD-SKILL://index", "kicad-skill://manifest", "kicad-skill://kicad-house-style/./pcb.md",
    "kicad-skill://kicad-house-style/x/../pcb.md", "kicad-skill://kicad-house-style/%2e%2e/SKILL.md",
    "kicad-skill://kicad-house-style/../../SKILL.md", "kicad-skill://kicad-house-style/SKILL.md\n",
    "\tkicad-skill://kicad-house-style/pcb.md ",
]


def grid(h, size=(1270000, 1270000), index=3, presets=((2540000, 2540000), (1270000, 1270000), (635000, 635000),
                                                        (1270000, 1270000)),
         visible=True, style=1, snapping=1, origin=(0, 0), revision=""):
    """A GridSettings reply (style and snapping as numbers, so undefined values can be sent)."""
    settings = h.editor.GridSettings(preset_index=index, visible=visible, style=style, snapping=snapping)
    settings.size.x_nm, settings.size.y_nm = size
    settings.origin.x_nm, settings.origin.y_nm = origin
    for x, y in presets:
        preset = settings.presets.add(name=f"{x / 1e6:g} mm")
        preset.size.x_nm, preset.size.y_nm = x, y
    return h.reply(settings, revision=revision)


BOARD_GRID = dict(size=(500000, 500000), index=1,
                  presets=((1000000, 1000000), (500000, 500000), (250000, 250000), (100000, 100000), (25400, 25400),
                           (1, 1), (3333333, 1234567), (123456789012, 7)),
                  visible=False, style=3, snapping=2, origin=(-12700000, 101600000))


def scenarios(h):
    S, docs = h.S, {"schematic": h.SHEET_DOC, "board": h.BOARD_DOC, "symbol": h.SYMBOL_DOC,
                    "footprint": h.FOOTPRINT_DOC}
    tool = "live_skill_reference"
    result = [
        # ---- Skills ----
        S("shared_skill_listing", [(tool, {}), (tool, {"skill": ""}), (tool, {"skill": "", "file": "nope.txt"})], []),
        S("shared_skill_every_file", [(tool, {"skill": DEFAULT})]
          + [(tool, {"skill": skill, "file": name}) for skill, name in skill_files()], []),
        S("shared_skill_refused_names", [(tool, {"skill": name}) for name in SKILL_NAMES]
          + [(tool, {"skill": DEFAULT, "file": name}) for name in FILE_NAMES]
          + [(tool, {"skill": name, "file": "../SKILL.md"}) for name in ("nope", "Bad")], []),
        # A plain str parameter is never decoded from JSON text, and other types are refused
        S("shared_skill_validation", [(tool, {"skill": 5}), (tool, {"file": None}),
                                      (tool, {"skill": [DEFAULT]}), (tool, {"file": {"name": "pcb.md"}})], [],
          compare="error_prefix"),
        # ---- Resources ----
        S("shared_resources_listed", [("resources/list", {}), ("resources/templates/list", {})], []),
        S("shared_resource_reads", [("resources/read", {"uri": skills.INDEX_URI}),
                                    ("resources/read", {"uri": skills.MANIFEST_URI})]
          + [("resources/read", {"uri": f"kicad-skill://{skill}/{name}"}) for skill, name in skill_files()], []),
        S("shared_resource_uris_normalised", [("resources/read", {"uri": uri}) for uri in RESOURCE_NORMALISED], []),
        S("shared_resource_refusals", [("resources/read", {"uri": uri}) for uri in RESOURCE_REFUSALS], []),
        # ---- live_editor_grid: reads ----
        S("shared_grid_read_each_document",
          [("live_editor_grid", {"document_spec": doc}) for doc in docs.values()],
          [("GetGridSettings", lambda r: grid(h, revision="fx-1")),
           ("GetGridSettings", lambda r: grid(h, **BOARD_GRID)),
           ("GetGridSettings", lambda r: grid(h, size=(0, 0), index=0, presets=(), visible=False, style=0,
                                              snapping=0)),
           ("GetGridSettings", lambda r: grid(h, size=(100000, 100000), index=-1, presets=((100000, 100000),),
                                              style=2, snapping=3))]),
        S("shared_grid_read_spec_as_json_text",
          [("live_editor_grid", {"document_spec": h.json.dumps(h.BOARD_DOC), "size_mm": None, "style": "null"})],
          [("GetGridSettings", lambda r: grid(h, **BOARD_GRID))]),
        S("shared_grid_read_undefined_enums",
          [("live_editor_grid", {"document_spec": h.SHEET_DOC})] * 3,
          [("GetGridSettings", lambda r: grid(h, style=9)), ("GetGridSettings", lambda r: grid(h, snapping=7)),
           ("GetGridSettings", lambda r: grid(h, style=-2, snapping=8))]),
        # ---- live_editor_grid: changes ----
        S("shared_grid_set_each_setting", [
            ("live_editor_grid", {"document_spec": docs["board"], "size_mm": 0.5}),
            ("live_editor_grid", {"document_spec": docs["schematic"], "size_mm": [1.27, 2.54]}),
            ("live_editor_grid", {"document_spec": docs["symbol"], "preset_index": 2}),
            ("live_editor_grid", {"document_spec": docs["footprint"], "visible": False}),
            ("live_editor_grid", {"document_spec": docs["board"], "visible": True}),
            ("live_editor_grid", {"document_spec": docs["board"], "preset_index": 0}),
            ("live_editor_grid", {"document_spec": docs["board"], "preset_index": -1}),
            *[("live_editor_grid", {"document_spec": docs["schematic"], "style": style})
              for style in ("dots", "lines", "small_cross")],
            *[("live_editor_grid", {"document_spec": docs["footprint"], "snapping": snapping})
              for snapping in ("always", "with_grid", "never")],
            ("live_editor_grid", {"document_spec": docs["board"], "size_mm": [0.1, 0.2], "preset_index": 4,
                                  "visible": True, "style": "lines", "snapping": "never"}),
        ], [("SetGridSettings", lambda r: grid(h, **BOARD_GRID))] * 14),
        # pydantic's conversions: numeric text, JSON text, lax booleans and integers
        S("shared_grid_set_converted_arguments", [
            ("live_editor_grid", {"document_spec": h.SHEET_DOC, "size_mm": "1.27"}),
            ("live_editor_grid", {"document_spec": h.SHEET_DOC, "size_mm": "[0.635, 1]"}),
            ("live_editor_grid", {"document_spec": h.SHEET_DOC, "size_mm": [True, "2.54"]}),
            ("live_editor_grid", {"document_spec": h.SHEET_DOC, "size_mm": 1}),
            ("live_editor_grid", {"document_spec": h.SHEET_DOC, "visible": "yes", "preset_index": "3"}),
            ("live_editor_grid", {"document_spec": h.SHEET_DOC, "visible": 0, "preset_index": 2.0}),
            ("live_editor_grid", {"document_spec": h.SHEET_DOC, "size_mm": 0.0000004}),
            ("live_editor_grid", {"document_spec": h.SHEET_DOC, "size_mm": 10000}),
            ("live_editor_grid", {"document_spec": h.SHEET_DOC, "size_mm": 2.5e-6}),
        ], [("SetGridSettings", lambda r: grid(h))] * 9),
        S("shared_grid_refusals", [
            ("live_editor_grid", {"document_spec": h.SHEET_DOC, **arguments}) for arguments in (
                {"size_mm": 0}, {"size_mm": -1}, {"size_mm": [1, 0]}, {"size_mm": [-0.5, 1]}, {"size_mm": []},
                {"size_mm": [1]}, {"size_mm": [1, 2, 3]}, {"size_mm": 10000.001}, {"size_mm": [0.5, 20000]},
                {"preset_index": 2 ** 31}, {"preset_index": -2 ** 31 - 1}, {"preset_index": 2 ** 63},
                {"preset_index": 2 ** 31 - 1, "style": "grid"}, {"style": "Dots"}, {"style": ""},
                {"snapping": "sometimes"},
                # The first invalid setting in Python's order is reported
                {"size_mm": 0, "style": "grid"}, {"style": "grid", "snapping": "sometimes"},
                {"size_mm": [1, 2, 3], "preset_index": 2 ** 40})]
          + [("live_editor_grid", {"document_spec": doc, "style": "grid"}) for doc in (
                dict(h.BOARD_DOC, board_filename=""), {"type": "DOCTYPE_SCHEMATIC"}, {"type": "DOCTYPE_SYMBOL"},
                dict(h.FOOTPRINT_DOC, lib_id={}), {"type": "DOCTYPE_NOPE"}, {}, dict(h.SHEET_DOC, nope=1),
                {"type": 3, "board_filename": "x.kicad_pcb"})], []),
        S("shared_grid_validation", [
            ("live_editor_grid", {"document_spec": h.SHEET_DOC, **arguments}) for arguments in (
                {"size_mm": "abc"}, {"size_mm": {"x": 1}}, {"size_mm": [1, "x"]}, {"preset_index": 1.5},
                {"visible": "maybe"}, {"style": 5}, {"snapping": ["never"]})]
            + [("live_editor_grid", {}), ("live_editor_grid", {"document_spec": [h.SHEET_DOC]})], [],
            compare="error_prefix"),
        # ---- live_editor_grid: KiCad's errors ----
        S("shared_grid_kicad_errors", [("live_editor_grid", {"document_spec": doc, **arguments})
                                       for doc, arguments in ((h.SHEET_DOC, {}), (h.BOARD_DOC, {"size_mm": 0.5}),
                                                              (h.SYMBOL_DOC, {}), (h.FOOTPRINT_DOC, {"style": "dots"}),
                                                              (h.SHEET_DOC, {}), (h.SHEET_DOC, {"visible": True}))],
          [("GetGridSettings", lambda r: h.failure(h.envelope.AS_UNHANDLED, "no schematic editor")),
           ("SetGridSettings", lambda r: h.failure(h.envelope.AS_BAD_REQUEST, "no such preset")),
           ("GetGridSettings", lambda r: h.failure(h.envelope.AS_UNIMPLEMENTED, "old KiCad")),
           ("SetGridSettings", lambda r: h.failure(h.envelope.AS_BUSY, "a dialog is open")),
           ("GetGridSettings", lambda r: h.reply(h.editor.EndCommitResponse())),
           ("SetGridSettings", lambda r: h.reply())]),
        # A lost reply to a setting is reported as it is, never resent: a view setting is not
        # reconciled through GetOperationStatus (Python sends it outside the revision contract)
        S("shared_grid_set_reply_lost", [("live_editor_grid", {"document_spec": h.BOARD_DOC, "snapping": "never"}),
                                         ("live_editor_grid", {"document_spec": h.BOARD_DOC})],
          [("SetGridSettings", lambda r: h.DROP), ("GetGridSettings", lambda r: grid(h, **BOARD_GRID))]),
        S("shared_grid_read_reply_lost", [("live_editor_grid", {"document_spec": h.SHEET_DOC})],
          [("GetGridSettings", lambda r: h.DROP)]),
        S("shared_grid_connection_closed", [("live_editor_grid", {"document_spec": h.SHEET_DOC, "visible": False})],
          [("SetGridSettings", lambda r: h.CLOSE)]),
    ]
    return result
