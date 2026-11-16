# SPDX-License-Identifier: GPL-3.0-or-later
"""Every schematic IPC command the editor registers, for the generic execute/batch tools.

Kinds: "read" never changes anything; "edit" changes the design (one undo step each, or part of a
batch's single step); "view" changes only the editor's selection, view or highlighting; "file"
touches files on disk or discards unsaved state; "job" writes an export.  Mirrors the
registerHandler calls in eeschema/api/api_handler_sch.cpp and common/api/api_handler_editor.cpp.
"""

from google.protobuf import descriptor_pool, empty_pb2, json_format, symbol_database  # noqa: F401

C = "kiapi.common.commands."
S = "kiapi.schematic.commands."
J = "kiapi.schematic.jobs."
T = "kiapi.common.types."
EMPTY = "google.protobuf.Empty"

COMMANDS = {
    # Documents and files
    C + "GetOpenDocuments": (C + "GetOpenDocumentsResponse", "read"),
    C + "GetDocumentModifiedState": (C + "GetDocumentModifiedStateResponse", "read"),
    C + "SaveDocument": (EMPTY, "file"),
    C + "SaveDocumentAs": (EMPTY, "file"),
    C + "SaveCopyOfDocument": (EMPTY, "file"),
    C + "RevertDocument": (EMPTY, "file"),
    C + "SaveDocumentToString": (C + "SavedDocumentResponse", "read"),
    C + "SaveSelectionToString": (C + "SavedSelectionResponse", "read"),
    # Items
    C + "GetItems": (C + "GetItemsResponse", "read"),
    C + "GetItemsById": (C + "GetItemsResponse", "read"),
    C + "HitTest": (C + "HitTestResponse", "read"),
    C + "CreateItems": (C + "CreateItemsResponse", "edit"),
    C + "UpdateItems": (C + "UpdateItemsResponse", "edit"),
    C + "DeleteItems": (C + "DeleteItemsResponse", "edit"),
    S + "PlaceSymbolFromLibrary": (C + "PlaceFromLibraryResponse", "edit"),
    S + "AnnotateSchematic": (S + "AnnotateSchematicResponse", "edit"),
    S + "UpdateSymbolsFromLibrary": (S + "UpdateSymbolsFromLibraryResponse", "edit"),
    S + "AutoplaceFields": (EMPTY, "edit"),
    S + "TransformItems": (S + "TransformItemsResponse", "edit"),
    S + "UndoLastChange": (S + "UndoRedoResponse", "edit"),
    S + "RedoLastChange": (S + "UndoRedoResponse", "edit"),
    # Structure and connectivity
    S + "GetSchematicHierarchy": (S + "SchematicHierarchyResponse", "read"),
    S + "GetSchematicNetlist": (S + "SchematicNetlistResponse", "read"),
    S + "GetSymbolPins": (S + "SymbolPinsResponse", "read"),
    "kiapi.schematic.tasks.StartErcTask": ("kiapi.common.tasks.StartTaskResponse", "read"),
    "kiapi.schematic.tasks.GetErcTaskResult": ("kiapi.schematic.tasks.ErcTaskResult", "read"),
    "kiapi.common.tasks.GetTaskStatus": ("kiapi.common.tasks.TaskStatusResponse", "read"),
    # Selection and view
    C + "GetSelection": (C + "SelectionResponse", "read"),
    C + "ClearSelection": (EMPTY, "view"),
    C + "AddToSelection": (C + "SelectionResponse", "view"),
    C + "RemoveFromSelection": (C + "SelectionResponse", "view"),
    C + "FocusOnItems": (EMPTY, "view"),
    C + "HighlightNets": (C + "HighlightNetsResponse", "view"),
    C + "SyncSelection": (C + "SyncSelectionResponse", "view"),
    C + "CrossProbeAnnounce": (C + "CrossProbeAnnounceResponse", "view"),
    # Page, title block, variables, net classes
    C + "GetPageSettings": (T + "PageSettings", "read"),
    C + "SetPageSettings": (T + "PageSettings", "edit"),
    C + "GetTitleBlockInfo": (T + "TitleBlockInfo", "read"),
    C + "SetTitleBlockInfo": (EMPTY, "edit"),
    C + "ExpandTextVariables": (C + "ExpandTextVariablesResponse", "read"),
    C + "GetTextVariables": ("kiapi.common.project.TextVariables", "read"),
    C + "SetTextVariables": (EMPTY, "edit"),
    C + "GetNetClasses": (C + "NetClassesResponse", "read"),
    C + "GetNetClassAssignments": (C + "NetClassAssignmentsResponse", "read"),
    # Design variants
    C + "GetVariants": (C + "VariantsResponse", "read"),
    C + "GetCurrentVariant": (C + "CurrentVariantResponse", "read"),
    C + "AddVariant": (EMPTY, "edit"),
    C + "DeleteVariant": (EMPTY, "edit"),
    C + "RenameVariant": (EMPTY, "edit"),
    C + "CopyVariant": (EMPTY, "edit"),
    C + "SetVariantDescription": (EMPTY, "edit"),
    C + "SetCurrentVariant": (EMPTY, "edit"),
    # Exports (from the unsaved model)
    J + "RunSchematicJobExportSvg": (T + "RunJobResponse", "job"),
    J + "RunSchematicJobExportPdf": (T + "RunJobResponse", "job"),
    J + "RunSchematicJobExportDxf": (T + "RunJobResponse", "job"),
    J + "RunSchematicJobExportPs": (T + "RunJobResponse", "job"),
    J + "RunSchematicJobExportPng": (T + "RunJobResponse", "job"),
    J + "RunSchematicJobExportNetlist": (T + "RunJobResponse", "job"),
    J + "RunSchematicJobExportBOM": (T + "RunJobResponse", "job"),
}

# Commands that may run inside a batch's commit.  Undo/redo refuse an open commit, and file
# operations must not be rolled back half-way.
BATCHABLE_KINDS = {"read", "edit", "view"}
NOT_BATCHABLE = {S + "UndoLastChange", S + "RedoLastChange"}


def resolve(name):
    """Accept a full name or a unique short name (e.g. 'TransformItems')."""
    if name in COMMANDS:
        return name
    matches = [full for full in COMMANDS if full.rsplit(".", 1)[-1] == name]
    if len(matches) != 1:
        raise ValueError(f"Unknown command {name!r}; use live_schematic_list_commands")
    return matches[0]


def message_class(full_name):
    descriptor = descriptor_pool.Default().FindMessageTypeByName(full_name)
    try:
        from google.protobuf.message_factory import GetMessageClass
    except ImportError:  # protobuf < 4.22
        return symbol_database.Default().GetPrototype(descriptor)
    return GetMessageClass(descriptor)


def build_request(name, payload):
    full = resolve(name)
    request = json_format.ParseDict(payload or {}, message_class(full)())
    response_name, kind = COMMANDS[full]
    return full, request, message_class(response_name), kind


def describe(name):
    full = resolve(name)
    response_name, kind = COMMANDS[full]
    descriptor = descriptor_pool.Default().FindMessageTypeByName(full)
    return {
        "command": full,
        "kind": kind,
        "returns": response_name,
        "fields": [{"name": f.name, "repeated": f.is_repeated,
                    "type": f.message_type.full_name if f.message_type
                    else ([v.name for v in f.enum_type.values] if f.enum_type else f.type)}
                   for f in descriptor.fields],
    }
