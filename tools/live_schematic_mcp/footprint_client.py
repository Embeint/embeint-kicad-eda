# SPDX-License-Identifier: GPL-3.0-or-later
"""Footprint Editor, footprint library and embedded-file operations, mixed into LiveSchematic."""

from pathlib import Path
import re
import sys
import uuid

from google.protobuf import empty_pb2, json_format

sys.path.insert(0, str(Path(__file__).resolve().parent / "_generated"))
from board import board_commands_pb2 as bc            # noqa: E402
from board import board_types_pb2 as bt               # noqa: E402
from common.commands import base_commands_pb2 as base  # noqa: E402
from common.commands import editor_commands_pb2 as editor  # noqa: E402
from common.commands import library_commands_pb2 as library  # noqa: E402
from common.commands import project_commands_pb2 as project  # noqa: E402
from common.types import base_types_pb2 as types      # noqa: E402
from common.types import enums_pb2 as enums           # noqa: E402
from common.types import library_types_pb2 as lib_types  # noqa: E402
from schematic import schematic_commands_pb2 as sch_commands  # noqa: E402

LAYERS = {"F.Cu": bt.BL_F_Cu, "B.Cu": bt.BL_B_Cu, "F.SilkS": bt.BL_F_SilkS, "F.Fab": bt.BL_F_Fab,
          "F.CrtYd": bt.BL_F_CrtYd, "F.Mask": bt.BL_F_Mask, "B.Mask": bt.BL_B_Mask, "F.Paste": bt.BL_F_Paste,
          "B.SilkS": bt.BL_B_SilkS, "B.Fab": bt.BL_B_Fab, "B.CrtYd": bt.BL_B_CrtYd}
ALL_COPPER = [bt.BoardLayer.Value(name) for name in bt.BoardLayer.keys()
              if name.endswith("_Cu") and name.startswith("BL_")]
PAD_SHAPES = {"rect": bt.PSS_RECTANGLE, "roundrect": bt.PSS_ROUNDRECT, "circle": bt.PSS_CIRCLE, "oval": bt.PSS_OVAL}
MOUNTING = {"smd": bt.FMS_SMD, "tht": bt.FMS_THROUGH_HOLE, "unspecified": bt.FMS_UNSPECIFIED}


def _nm(value):
    return round(value * 1_000_000)


def _vec(x, y):
    return types.Vector2(x_nm=_nm(x), y_nm=_nm(y))


def _mm(vector):
    return [vector.x_nm / 1e6, vector.y_nm / 1e6]


def _id():
    return types.KIID(value=str(uuid.uuid4()))


def footprint_document(value):
    """A DOCTYPE_FOOTPRINT document as returned by new_footprint/edit_footprint."""
    result = json_format.ParseDict(value, types.DocumentSpecifier())
    if result.type != types.DOCTYPE_FOOTPRINT or not result.lib_id.entry_name:
        raise ValueError("Use the footprint document returned by live_footprint_new or live_footprint_edit")
    return result


def library_kind(kind):
    kinds = {"symbol": lib_types.LT_SYMBOL, "footprint": lib_types.LT_FOOTPRINT}
    if kind not in kinds:
        raise ValueError("kind must be symbol or footprint")
    return kinds[kind]


def make_pad(pad):
    """A Pad message from footprint_gen's pad dict."""
    message = bt.Pad(id=_id(), number=pad["number"], position=_vec(*pad["position"]))
    stack = message.pad_stack
    stack.type = bt.PST_NORMAL
    width, height = pad["size"]
    if pad["type"] == "smd":
        message.type = bt.PT_SMD
        stack.layers.extend([bt.BL_F_Cu, bt.BL_F_Paste, bt.BL_F_Mask])
    else:
        message.type = bt.PT_PTH if pad["type"] == "pth" else bt.PT_NPTH
        stack.layers.extend(ALL_COPPER + [bt.BL_F_Mask, bt.BL_B_Mask])
        stack.drill.diameter.CopyFrom(_vec(pad["drill"], pad["drill"]))
        stack.drill.shape = bt.DS_CIRCLE
        stack.drill.start_layer = bt.BL_F_Cu
        stack.drill.end_layer = bt.BL_B_Cu
    copper = stack.copper_layers.add(layer=bt.BL_F_Cu, shape=PAD_SHAPES[pad["shape"]], size=_vec(width, height))
    if pad["shape"] == "roundrect":
        # KLC: 25 % corner ratio, radius at most 0.25 mm
        copper.corner_rounding_ratio = min(0.25, 0.25 / min(width, height))
    return message


def make_line(start, end, layer, width):
    shape = bt.BoardGraphicShape(id=_id(), layer=LAYERS[layer])
    shape.shape.segment.start.CopyFrom(_vec(*start))
    shape.shape.segment.end.CopyFrom(_vec(*end))
    shape.shape.attributes.stroke.width.value_nm = _nm(width)
    shape.shape.attributes.stroke.style = enums.SLS_SOLID
    return shape


def make_rect(box, layer, width):
    shape = bt.BoardGraphicShape(id=_id(), layer=LAYERS[layer])
    shape.shape.rectangle.top_left.CopyFrom(_vec(box[0], box[1]))
    shape.shape.rectangle.bottom_right.CopyFrom(_vec(box[2], box[3]))
    shape.shape.attributes.stroke.width.value_nm = _nm(width)
    shape.shape.attributes.stroke.style = enums.SLS_SOLID
    shape.shape.attributes.fill.fill_type = types.GFT_UNFILLED
    return shape


def make_polygon(points, layer, width):
    shape = bt.BoardGraphicShape(id=_id(), layer=LAYERS[layer])
    outline = shape.shape.polygon.polygons.add().outline
    outline.closed = True
    for point in points:
        outline.nodes.add().point.CopyFrom(_vec(*point))
    shape.shape.attributes.stroke.width.value_nm = _nm(width)
    shape.shape.attributes.stroke.style = enums.SLS_SOLID
    shape.shape.attributes.fill.fill_type = types.GFT_FILLED
    return shape


def set_text_style(text, size):
    """KLC text: square glyphs, stroke 15 % of the height, centred."""
    text.attributes.size.CopyFrom(_vec(size, size))
    text.attributes.stroke_width.value_nm = _nm(round(size * 0.15, 3))
    text.attributes.horizontal_alignment = enums.HA_CENTER
    text.attributes.vertical_alignment = enums.VA_CENTER
    text.attributes.visible = True


def make_text(content, position, layer, size):
    message = bt.BoardText(id=_id(), layer=LAYERS[layer])
    message.text.text = content
    message.text.position.CopyFrom(_vec(*position))
    set_text_style(message.text, size)
    return message


def generated_items(shape, fab_reference=True):
    """Every board item for a footprint_gen result (fields are positioned separately)."""
    from footprint_gen import COURTYARD_WIDTH, FAB_WIDTH, SILK_WIDTH
    items = [make_pad(pad) for pad in shape["pads"]]
    items += [make_line(a, b, "F.Fab", FAB_WIDTH) for a, b in shape["fab_lines"]]
    items += [make_line(a, b, "F.SilkS", SILK_WIDTH) for a, b in shape["silk_lines"]]
    if shape.get("pin1_marker"):
        items.append(make_polygon(shape["pin1_marker"], "F.SilkS", SILK_WIDTH))
    items.append(make_rect(shape["courtyard"], "F.CrtYd", COURTYARD_WIDTH))
    if fab_reference:
        fab_ref = shape["fab_reference"]
        items.append(make_text("${REFERENCE}", fab_ref["position"], "F.Fab", fab_ref["size"]))
    return items


class FootprintTools:
    """Methods that need LiveSchematic.send/transaction; LiveSchematic inherits these."""

    # ---- Libraries (symbol or footprint) -----------------------------------------------------

    def open_footprint_editor(self):
        """The Footprint Editor lives in pcbnew; ask the schematic editor to open it if needed."""
        self.send(sch_commands.OpenFootprintEditor(), empty_pb2.Empty)

    def footprint_search(self, query, limit=25):
        self.open_footprint_editor()
        response = self.send(library.SearchLibraries(type=lib_types.LT_FOOTPRINT, query=query),
                             library.SearchLibrariesResponse)
        ids = [f"{i.library_nickname}:{i.entry_name}" for i in response.items]
        result = {"total": len(ids), "results": [{"lib_id": i} for i in ids[:max(1, min(limit, 200))]]}
        if response.libraries_loading:
            result["libraries_still_loading"] = response.libraries_loading
        return result

    def library_footprint(self, lib_id):
        self.open_footprint_editor()
        nickname, _, name = lib_id.partition(":")
        request = library.GetItemsFromLibrary(type=lib_types.LT_FOOTPRINT)
        request.item_ids.add(library_nickname=nickname, entry_name=name)
        response = self.send(request, editor.GetItemsResponse)
        footprint = bt.Footprint()
        if not response.items or not response.items[0].Unpack(footprint):
            raise ValueError(f"{lib_id} not found")
        return summarize_footprint(footprint)

    # ---- Footprint Editor (live, unsaved until footprint_save) -------------------------------

    def new_footprint(self, library_nickname, name, mounting="smd", replace_existing=False):
        self.open_footprint_editor()
        request = bc.NewLibraryFootprint(library_nickname=library_nickname, name=name,
                                         mounting_style=MOUNTING[mounting], replace_existing=replace_existing)
        return json_format.MessageToDict(self.send(request, bc.LibraryFootprintResponse).document,
                                         preserving_proto_field_name=True)

    def edit_footprint(self, lib_id):
        self.open_footprint_editor()
        nickname, _, name = lib_id.partition(":")
        request = bc.EditLibraryFootprint(id=types.LibraryIdentifier(library_nickname=nickname, entry_name=name))
        return json_format.MessageToDict(self.send(request, bc.LibraryFootprintResponse).document,
                                         preserving_proto_field_name=True)

    def footprint_items(self, doc):
        return self.send(editor.GetItems(header=types.ItemHeader(document=doc)), editor.GetItemsResponse).items

    def footprint_generate(self, doc, package, params, replace=False):
        """Build a KLC-style footprint from package dimensions, as one undo step."""
        from footprint_gen import generate
        shape = generate(package, **params)
        with self.transaction(doc, f"MCP: generate {shape['name']}"):
            if replace:
                existing = [item for item in self.footprint_items(doc)
                            if not item.TypeName().endswith(".Field")]
                if existing:
                    delete = editor.DeleteItems(header=types.ItemHeader(document=doc))
                    for packed in existing:
                        identifier = _item_id(packed)
                        if identifier:
                            delete.item_ids.add(value=identifier)
                    self.send(delete, editor.DeleteItemsResponse)
            # New footprints already carry KiCad's default ${REFERENCE} fab text: move it rather than
            # adding a second one
            reused = self._place_fab_reference(doc, shape)
            create = editor.CreateItems(header=types.ItemHeader(document=doc))
            for item in generated_items(shape, fab_reference=not reused):
                create.items.add().Pack(item)
            response = self.send(create, editor.CreateItemsResponse)
            rejected = [r.status.error_message or editor.ItemStatusCode.Name(r.status.code)
                        for r in response.created_items if r.status.code != editor.ISC_OK]
            if rejected:
                raise RuntimeError("; ".join(rejected))
            self._place_fields(doc, shape)
            self.send(bc.SetLibraryFootprintProperties(document=doc, mounting_style=MOUNTING[shape["mounting"]]),
                      empty_pb2.Empty)
        return {"name": shape["name"], "pads": len(shape["pads"]), "courtyard_mm": list(shape["courtyard"]),
                "body_mm": list(shape["body"])}

    def _place_fab_reference(self, doc, shape):
        """Move existing ${REFERENCE} texts to the body centre; True if there was one."""
        update = editor.UpdateItems(header=types.ItemHeader(document=doc))
        spec = shape["fab_reference"]
        for packed in self.footprint_items(doc):
            text = bt.BoardText()
            if packed.Unpack(text) and text.text.text == "${REFERENCE}":
                text.text.position.CopyFrom(_vec(*spec["position"]))
                text.layer = bt.BL_F_Fab
                set_text_style(text.text, spec["size"])
                update.items.add().Pack(text)
        if not update.items:
            return False
        self.send(update, editor.UpdateItemsResponse)
        return True

    def _place_fields(self, doc, shape):
        """Reference on F.SilkS above the courtyard, value on F.Fab below it (KLC)."""
        update = editor.UpdateItems(header=types.ItemHeader(document=doc))
        for packed in self.footprint_items(doc):
            field = bt.Field()
            if not packed.Unpack(field) or field.name not in ("Reference", "Value"):
                continue
            spec = shape["reference"] if field.name == "Reference" else shape["value"]
            field.text.text.position.CopyFrom(_vec(*spec["position"]))
            field.text.layer = LAYERS[spec["layer"]]
            set_text_style(field.text.text, spec["size"])
            field.visible = True
            update.items.add().Pack(field)
        if update.items:
            self.send(update, editor.UpdateItemsResponse)

    def footprint_set_properties(self, doc, description=None, keywords=None, mounting=None, value=None,
                                 datasheet=None, exclude_from_bom=None, exclude_from_position_files=None,
                                 models=None):
        request = bc.SetLibraryFootprintProperties(document=doc)
        for name, setting in (("description", description), ("keywords", keywords), ("value", value),
                              ("datasheet", datasheet), ("exclude_from_bill_of_materials", exclude_from_bom),
                              ("exclude_from_position_files", exclude_from_position_files)):
            if setting is not None:
                setattr(request, name, setting)
        if mounting is not None:
            request.mounting_style = MOUNTING[mounting]
        if models is not None:
            request.set_models = True
            for model in models:
                entry = request.models.add(filename=model["filename"], visible=model.get("visible", True),
                                           opacity=model.get("opacity", 1.0))
                for key, default in (("offset", (0, 0, 0)), ("rotation", (0, 0, 0)), ("scale", (1, 1, 1))):
                    x, y, z = model.get(key, default)
                    # Vector3D carries raw values here: mm for offset, degrees, plain factors
                    getattr(entry, key).CopyFrom(types.Vector3D(x_nm=x, y_nm=y, z_nm=z))
        with self.transaction(doc, "MCP: footprint properties"):
            self.send(request, empty_pb2.Empty)
        return {"success": True}

    def footprint_render(self, doc, dpi=1500, layers=None):
        request = bc.RenderLibraryFootprint(document=doc, dpi=dpi)
        request.layers.extend(LAYERS[name] for name in (layers or []))
        return self.send(request, bc.RenderLibraryFootprintResponse).image

    def footprint_summary(self, doc):
        pads, graphics, texts = [], {}, []
        for packed in self.footprint_items(doc):
            pad, shape, text = bt.Pad(), bt.BoardGraphicShape(), bt.BoardText()
            if packed.Unpack(pad):
                copper = pad.pad_stack.copper_layers[0] if pad.pad_stack.copper_layers else None
                pads.append({"number": pad.number, "type": bt.PadType.Name(pad.type), "position_mm": _mm(pad.position),
                             "size_mm": _mm(copper.size) if copper else None,
                             "shape": bt.PadStackShape.Name(copper.shape) if copper else None})
            elif packed.Unpack(shape):
                layer = bt.BoardLayer.Name(shape.layer).removeprefix("BL_")
                graphics[layer] = graphics.get(layer, 0) + 1
            elif packed.Unpack(text):
                texts.append({"text": text.text.text, "layer": bt.BoardLayer.Name(text.layer).removeprefix("BL_")})
        return {"lib_id": f"{doc.lib_id.library_nickname}:{doc.lib_id.entry_name}", "pads": pads,
                "graphics_per_layer": graphics, "texts": texts}

    def footprint_save(self, doc):
        self.send(project.SaveDocument(document=doc), empty_pb2.Empty)
        self.reload_library(doc.lib_id.library_nickname, kind="footprint")
        return {"saved": f"{doc.lib_id.library_nickname}:{doc.lib_id.entry_name}"}

    # ---- Embedded files (footprints and library symbols) -------------------------------------

    def embed_file(self, doc, path, overwrite=False):
        request = editor.EmbedFileFromPath(document=doc, path=str(path), overwrite=overwrite)
        with self.transaction(doc, "MCP: embed file"):
            response = self.send(request, editor.EmbedFileFromPathResponse)
        return json_format.MessageToDict(response.file, preserving_proto_field_name=True)

    def embedded_files(self, doc):
        response = self.send(editor.GetEmbeddedFileList(document=doc), editor.EmbeddedFileListResponse)
        return [json_format.MessageToDict(f, preserving_proto_field_name=True) for f in response.files]

    def remove_embedded_file(self, doc, name):
        with self.transaction(doc, "MCP: remove embedded file"):
            self.send(editor.RemoveEmbeddedFile(document=doc, name=name), empty_pb2.Empty)
        return {"removed": name}

    def footprint_attach_step(self, doc, step_path, offset=(0, 0, 0), rotation=(0, 0, 0), scale=(1, 1, 1)):
        """Embed a STEP/WRL model in the footprint and make it the footprint's 3D model.

        Paths are translated to the form KiCad reads: Windows paths (as SolidCraft reports them)
        to the WSL path under WSL, and WSL /mnt/x paths back to X:\ for native Windows KiCad.
        """
        from model_handoff import kicad_path
        step_path = kicad_path(step_path)
        if Path(step_path).suffix.lower() not in (".step", ".stp", ".stpz", ".wrl", ".wrz"):
            raise ValueError("3D models must be .step, .stp, .stpz, .wrl or .wrz")
        embedded = self.embed_file(doc, step_path, overwrite=True)
        self.footprint_set_properties(doc, models=[{"filename": embedded["uri"], "offset": offset,
                                                    "rotation": rotation, "scale": scale}])
        return {"embedded": embedded["name"], "model": embedded["uri"]}


    # ---- 3D models: SolidCraft handoff and checks in KiCad ------------------------------------

    def footprint_model_plan(self, package, params, body_height, standoff=0.1, lead_thickness=0.2,
                             lead_width=None, lead_span=None, foot_length=None, export_path=None):
        """The SolidCraft batch and export call that model this package, and how to come back."""
        from footprint_gen import generate
        from model_handoff import default_model_dir, kicad_path, model_plan
        if export_path is None:
            export_path = str(Path(default_model_dir()) / f"{generate(package, **params)['name']}.step")
        Path(kicad_path(export_path)).parent.mkdir(parents=True, exist_ok=True)
        return model_plan(package, params, body_height, standoff, lead_thickness, lead_width, lead_span,
                          foot_length, export_path)

    def kicad_cli(self):
        path = self.send(base.GetKiCadBinaryPath(binary_name="kicad-cli"), base.PathResponse).path
        if not path or not Path(path).exists():
            raise RuntimeError("KiCad did not report a kicad-cli binary")
        return path

    def saved_footprint_text(self, lib_id):
        nickname, _, name = lib_id.partition(":")
        for row in self.library_table(kind="footprint"):
            if row["nickname"] == nickname:
                path = Path(row["path"]) / f"{name}.kicad_mod"
                if not path.exists():
                    raise ValueError(f"{lib_id} is not saved yet (no {path})")
                return path.read_text()
        raise ValueError(f"footprint library {nickname} is not in the tables")

    def footprint_view_3d(self, lib_id, rotate="-45,0,45", zoom=1.0):
        """KiCad's own 3D render of a SAVED footprint with its models, on a scratch board."""
        from model_handoff import render_footprint_3d
        return render_footprint_3d(self.kicad_cli(), self.saved_footprint_text(lib_id), lib_id, rotate, zoom=zoom)

    def footprint_check_model(self, lib_id, source_path=None):
        """Check a saved footprint's embedded models: stored bytes, links, and KiCad loading them."""
        import hashlib
        from model_handoff import embedded_file_bytes, export_board_step, kicad_path
        text = self.saved_footprint_text(lib_id)
        links = re.findall(r'\(model\s+"([^"]+)"', text)
        report = {"lib_id": lib_id, "models": links, "embedded": []}
        for name in re.findall(r'\(file\s+\(name\s+"([^"]+)"\)', text):
            data = embedded_file_bytes(text, name)
            entry = {"name": name, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest(),
                     "is_step": data.lstrip()[:13] == b"ISO-10303-21;", "linked": f"kicad-embed://{name}" in links}
            if source_path:
                source = Path(kicad_path(source_path)).read_bytes()
                entry["matches_source"] = hashlib.sha256(source).hexdigest() == entry["sha256"]
            report["embedded"].append(entry)
        step = export_board_step(self.kicad_cli(), text, lib_id)
        report["kicad_export_step_bytes"] = len(step)
        report["kicad_export_solids"] = step.count("MANIFOLD_SOLID_BREP")
        return report


def _item_id(packed):
    for message in (bt.Pad(), bt.BoardGraphicShape(), bt.BoardText()):
        if packed.Unpack(message):
            return message.id.value or None
    return None


def summarize_footprint(footprint):
    pads, models = [], []
    for packed in footprint.items:
        pad, model = bt.Pad(), bt.Footprint3DModel()
        if packed.Unpack(pad):
            copper = pad.pad_stack.copper_layers[0] if pad.pad_stack.copper_layers else None
            pads.append({"number": pad.number, "type": bt.PadType.Name(pad.type), "position_mm": _mm(pad.position),
                         "size_mm": _mm(copper.size) if copper else None})
        elif packed.Unpack(model):
            models.append(model.filename)
    attributes = footprint.attributes
    return {"lib_id": f"{footprint.id.library_nickname}:{footprint.id.entry_name}",
            "description": attributes.description, "keywords": attributes.keywords,
            "mounting": bt.FootprintMountingStyle.Name(attributes.mounting_style),
            "value": footprint.value_field.text.text.text, "datasheet": footprint.datasheet_field.text.text.text,
            "models": models, "pads": sorted(pads, key=lambda p: (len(p["number"]), p["number"]))}
