# SPDX-License-Identifier: GPL-3.0-or-later
"""Small schematic block builders and single-writer, UUID-scoped reconciliation.

No editor/file access occurs during construction or planning. Apply uses the existing
client batch, never retries, never saves, and consumes each plan once. Fingerprints
are client-side preconditions, not atomic editor revision checks.
"""

from dataclasses import dataclass, field
from hashlib import sha256
import json
import uuid

from google.protobuf import any_pb2

from client import as_dict, document, enums, kiid, point, sch, types


def _bytes(message):
    return message.SerializeToString(deterministic=True)


def _hash(message):
    return sha256(_bytes(message)).hexdigest()


def _copy(message):
    return type(message).FromString(_bytes(message))


def _pack(message):
    packed = any_pb2.Any()
    packed.Pack(message)
    return packed


def _message(packed):
    name = packed.TypeName().removeprefix("kiapi.schematic.types.")
    descriptor = sch.DESCRIPTOR.message_types_by_name.get(name)
    if descriptor is None or descriptor.full_name != packed.TypeName():
        raise ValueError("Expected a schematic item")
    message = getattr(sch, name)()
    if not packed.Unpack(message):
        raise ValueError("Invalid schematic item")
    return message


def _unpack(packed):
    message = _message(packed)
    if not hasattr(message, "id") or not message.id.value:
        raise ValueError("Expected a schematic item with a UUID")
    uuid.UUID(message.id.value)
    return message


def _index(items):
    result = {}
    for packed in items:
        item = _unpack(packed)
        if item.id.value in result:
            raise ValueError("Duplicate item UUID in snapshot")
        result[item.id.value] = _pack(item)
    return result


def fingerprint(items):
    """Order-independent hash of all serialized sheet items, including manual items."""
    indexed = _index(items)
    chunks = [_bytes(indexed[key]) for key in sorted(indexed)]
    return sha256(b"".join(len(chunk).to_bytes(8, "big") + chunk for chunk in chunks)).hexdigest()


def _doc_bytes(doc):
    # Enforce the same explicit project/type/sheet identity as the client entrypoints.
    return _bytes(document(as_dict(doc)))


def _xy(vector):
    return vector.x_nm, vector.y_nm


def _mm(position):
    return tuple(value / 1e6 for value in position)


def _text(value, at, size=1.27):
    text = types.Text(text=value, position=point(*at))
    text.attributes.size.CopyFrom(point(size, size))
    text.attributes.horizontal_alignment = enums.HA_LEFT
    return text


class Block:
    """One persistent UUID namespace; logical keys must survive block revisions.

    Supply complete library definitions from library_symbols(), not the pin-free
    definitions in GetItems. Symbol placement intentionally supports unrotated,
    unmirrored units only. Use GetSymbolPins after apply to verify real endpoints.
    """

    def __init__(self, namespace):
        self.namespace = str(uuid.UUID(namespace))
        self._items = {}
        self._pins = {}
        self._routes = {}

    def id(self, key):
        if not key or not isinstance(key, str):
            raise ValueError("A nonempty logical key is required")
        return str(uuid.uuid5(uuid.UUID(self.namespace), key))

    def add(self, key, item):
        """Add a complete supported top-level item, assigning its generated UUID."""
        supported = (sch.SchematicLine, sch.SchematicText, sch.SchematicGraphicShape,
                     sch.SchematicSymbolInstance, sch.Junction, sch.NoConnectMarker,
                     sch.LocalLabel, sch.HierarchicalLabel, sch.SheetSymbol)
        if not isinstance(item, supported):
            raise ValueError("Unsupported block item type")
        if key in self._items:
            raise ValueError(f"Duplicate logical key: {key}")
        item = _copy(item)
        item.id.CopyFrom(kiid(self.id(key)))
        if isinstance(item, sch.SheetSymbol):
            names = [pin.text.text for pin in item.pins]
            if any(not name for name in names) or len(set(names)) != len(names):
                raise ValueError("Sheet pins need unique nonempty names")
            for pin in item.pins:
                pin.id.CopyFrom(kiid(self.id(f"{key}/pin/{pin.text.text}")))
        if isinstance(item, sch.SchematicSymbolInstance):
            # Definition child identities must not be shared by placed units,
            # including callers using add() rather than the symbol convenience.
            for index, child in enumerate(item.definition.items):
                content = _message(child.item)
                if hasattr(content, "id"):
                    content.id.CopyFrom(kiid(self.id(f"{key}/child/{index}")))
                    child.item.Pack(content)
        self._items[key] = item
        return item.id.value

    def frame(self, key, title, bounds):
        x1, y1, x2, y2 = bounds
        if not title or x2 <= x1 or y2 <= y1:
            raise ValueError("A titled positive-size block is required")
        shape = sch.SchematicGraphicShape()
        shape.shape.rectangle.top_left.CopyFrom(point(x1, y1))
        shape.shape.rectangle.bottom_right.CopyFrom(point(x2, y2))
        shape.shape.attributes.stroke.style = enums.SLS_DASH
        shape.shape.attributes.stroke.width.value_nm = 254000
        shape.shape.attributes.fill.fill_type = types.GFT_UNFILLED
        self.add(key + "/box", shape)
        self.add(key + "/title", sch.SchematicText(text=_text(title, (x1 + 2.54, y1 + 2.54))))

    def symbol(self, key, definition, reference, anchor_pin, at, unit=1):
        """Place a full library unit by its actual numbered pin connection point."""
        if not definition.id.library_nickname or not definition.id.entry_name:
            raise ValueError("A full Library:Symbol definition is required")
        if not reference or not 1 <= unit <= (definition.unit_count or 1):
            raise ValueError("An explicit reference and valid unit are required")
        pins = []
        for child in definition.items:
            pin = sch.SchematicPin()
            if (child.item.Unpack(pin) and not child.is_private
                    and child.unit.unit in (0, unit) and child.body_style.style in (0, 1)):
                pins.append(pin)
        matches = [p for p in pins if p.number == str(anchor_pin)]
        if len(matches) != 1:
            raise ValueError("Anchor pin must identify exactly one real pin in this unit")
        target = point(*at)
        origin = (target.x_nm - matches[0].position.x_nm,
                  target.y_nm - matches[0].position.y_nm)
        instance = sch.SchematicSymbolInstance(position=types.Vector2(x_nm=origin[0], y_nm=origin[1]),
                                                definition=definition, lib_id=definition.id)
        instance.transform.orientation = sch.SSO_0
        instance.unit.unit = unit
        instance.attributes.CopyFrom(definition.attributes)
        for name in ("reference", "value", "footprint", "datasheet", "description"):
            source = getattr(definition, name + "_field")
            dest = getattr(instance, name + "_field")
            dest.CopyFrom(source)
            dest.text.position.x_nm += origin[0]
            dest.text.position.y_nm += origin[1]
        for child in definition.items:
            user_field = sch.SchematicField()
            if (child.item.Unpack(user_field) and not child.is_private
                    and child.unit.unit in (0, unit) and child.body_style.style in (0, 1)):
                user_field.text.position.x_nm += origin[0]
                user_field.text.position.y_nm += origin[1]
                instance.user_fields.add().CopyFrom(user_field)
        instance.reference_field.text.text = reference
        self.add(key, instance)
        self._pins[key] = [(p.number, (p.position.x_nm + origin[0], p.position.y_nm + origin[1]))
                           for p in pins]
        numbers = [p.number for p in pins]
        return {number: self.pin(key, number) for number in numbers if numbers.count(number) == 1}

    def pin(self, key, number):
        candidates = [pos for num, pos in self._pins[key] if num == str(number)]
        if len(candidates) != 1:
            raise ValueError("Pin number is absent or ambiguous in the selected unit")
        return _mm(candidates[0])

    def port(self, key, name, at, shape="input", spin="left"):
        shapes = {"input": sch.SLSH_INPUT, "output": sch.SLSH_OUTPUT, "bidi": sch.SLSH_BIDI,
                  "passive": sch.SLSH_PASSIVE}
        spins = {"left": sch.SLSS_LEFT, "right": sch.SLSS_RIGHT,
                 "up": sch.SLSS_UP, "bottom": sch.SLSS_BOTTOM}
        if not name:
            raise ValueError("A port name is required")
        self.add(key, sch.HierarchicalLabel(position=point(*at), text=_text(name, at),
                                            shape=shapes[shape], spin_style=spins[spin]))
        return tuple(at)

    def sheet(self, key, name, filename, at, size, page, ports):
        """Create an owned sheet with pins: (name, side, offset_mm, shape).

        Child labels are separate blocks/documents; apply is never cross-document.
        Existing manual sheets cannot be adopted. Sheet deletion is refused.
        """
        if not name or not filename.endswith(".kicad_sch") or not str(page):
            raise ValueError("Explicit sheet name, .kicad_sch filename and page are required")
        if min(size) <= 0:
            raise ValueError("Positive sheet size is required")
        sheet = sch.SheetSymbol(position=point(*at), size=point(*size), page_number=str(page),
                                fields_autoplaced=True)
        sheet.name_field.text.text, sheet.name_field.visible = name, True
        sheet.filename_field.text.text, sheet.filename_field.visible = filename, True
        endpoints = {}
        sides = {"left": sch.SHS_LEFT, "right": sch.SHS_RIGHT,
                 "top": sch.SHS_TOP, "bottom": sch.SHS_BOTTOM}
        shapes = {"input": sch.SLSH_INPUT, "output": sch.SLSH_OUTPUT,
                  "bidi": sch.SLSH_BIDI, "passive": sch.SLSH_PASSIVE}
        for port, side, offset, shape in ports:
            if not port or port in endpoints or side not in sides:
                raise ValueError("Unique named sheet ports and a valid side are required")
            extent = size[1] if side in ("left", "right") else size[0]
            if not 0 < offset < extent:
                raise ValueError("Sheet pin must lie inside its selected edge")
            x = at[0] + (size[0] if side == "right" else offset if side in ("top", "bottom") else 0)
            y = at[1] + (size[1] if side == "bottom" else offset if side in ("left", "right") else 0)
            endpoints[port] = (x, y)
            sheet.pins.add(id=kiid(self.id(f"{key}/pin/{port}")), position=point(x, y),
                           text=_text(port, (x, y)), side=sides[side], shape=shapes[shape])
        self.add(key, sheet)
        return endpoints

    def wire(self, key, net, path):
        """Declare an explicit Manhattan path; split same-net joins and add junctions.

        Net is a local planning name, not a label or electrical net assignment.
        Collinear overlaps and intersections between different nets are refused.
        This is not an obstacle router; manual geometry still needs visual review.
        """
        if key in self._routes or not net or len(path) < 2:
            raise ValueError("Unique route key, net name and at least two points are required")
        coords = [_xy(point(*pos)) for pos in path]
        for a, b in zip(coords, coords[1:]):
            if a == b or (a[0] != b[0] and a[1] != b[1]):
                raise ValueError("Wire legs must be nonzero Manhattan segments")
        self._routes[key] = (net, coords)

    def items(self):
        result = {key: _copy(item) for key, item in self._items.items()}
        legs = [(key, index, net, a, b) for key, (net, coords) in self._routes.items()
                for index, (a, b) in enumerate(zip(coords, coords[1:]))]
        cuts = [{a, b} for _, _, _, a, b in legs]
        for i, (_, _, net, a, b) in enumerate(legs):
            for j in range(i):
                _, _, other_net, c, d = legs[j]
                hits, overlap = _intersection(a, b, c, d)
                if overlap or (hits and net != other_net):
                    raise ValueError("Wire overlap or different-net intersection; choose another path")
                cuts[i].update(hits)
                cuts[j].update(hits)
        degree = {}
        for (key, index, net, a, b), points in zip(legs, cuts):
            ordered = sorted(points, reverse=a > b)
            for segment, (start, end) in enumerate(zip(ordered, ordered[1:])):
                logical = f"route/{key}/{index}/{segment}"
                if logical in result:
                    raise ValueError("Logical key collides with a generated route")
                result[logical] = sch.SchematicLine(id=kiid(self.id(logical)), type=sch.SLT_WIRE,
                                                    start=point(*_mm(start)), end=point(*_mm(end)))
                for pos in (start, end):
                    degree[(net, pos)] = degree.get((net, pos), 0) + 1
        for (net, pos), count in degree.items():
            if count >= 3:
                logical = f"junction/{net}/{pos[0]}/{pos[1]}"
                if logical in result:
                    raise ValueError("Logical key collides with a generated junction")
                result[logical] = sch.Junction(id=kiid(self.id(logical)), position=point(*_mm(pos)))
        return result

    def report(self):
        """Construction report only; not ERC, connectivity intent, page-fit or visual sign-off."""
        try:
            items = self.items()
        except ValueError as error:
            return {"valid": False, "errors": [str(error)]}
        return {"valid": True, "errors": [], "items": len(items),
                "wires": sum(isinstance(i, sch.SchematicLine) and i.type == sch.SLT_WIRE
                             for i in items.values()),
                "junctions": sum(isinstance(i, sch.Junction) for i in items.values()),
                "requires": ["GetSymbolPins", "connectivity intent", "ERC", "page fit", "framed view"]}


def _intersection(a, b, c, d):
    horizontal = a[1] == b[1]
    other_horizontal = c[1] == d[1]
    if horizontal == other_horizontal:
        axis, fixed = (0, 1) if horizontal else (1, 0)
        if a[fixed] != c[fixed]:
            return set(), False
        low = max(min(a[axis], b[axis]), min(c[axis], d[axis]))
        high = min(max(a[axis], b[axis]), max(c[axis], d[axis]))
        pos = (low, a[1]) if horizontal else (a[0], low)
        return ({pos} if low == high else set()), low < high
    if not horizontal:
        a, b, c, d = c, d, a, b
    pos = (c[0], a[1])
    return ({pos} if min(a[0], b[0]) <= pos[0] <= max(a[0], b[0])
            and min(c[1], d[1]) <= pos[1] <= max(c[1], d[1]) else set()), False


class StaleState(ValueError):
    """No edits sent: reread and resolve the state before replanning."""


class ApplyUncertain(RuntimeError):
    """Outcome unknown/unverified; inspect commit state, reread and replan. Never replay."""


@dataclass(frozen=True)
class OwnedItem:
    key: str
    observed_sha256: str
    desired_sha256: str


@dataclass(frozen=True)
class Manifest:
    namespace: str
    document_bytes: bytes
    owned: tuple[OwnedItem, ...] = ()

    @classmethod
    def empty(cls, namespace, doc):
        return cls(str(uuid.UUID(namespace)), _doc_bytes(doc))

    def to_json(self):
        return json.dumps({"version": 1, "namespace": self.namespace,
                           "document": as_dict(types.DocumentSpecifier.FromString(self.document_bytes)),
                           "owned": [vars(item) for item in self.owned]}, indent=2, sort_keys=True)

    @classmethod
    def from_json(cls, text):
        value = json.loads(text)
        if value["version"] != 1:
            raise ValueError("Unsupported block manifest version")
        return cls(str(uuid.UUID(value["namespace"])), _doc_bytes(document(value["document"])),
                   tuple(OwnedItem(**item) for item in value["owned"]))


@dataclass(frozen=True)
class Plan:
    manifest: Manifest
    expected_fingerprint: str
    desired: tuple[tuple[str, bytes], ...]
    creates: tuple[str, ...]
    updates: tuple[str, ...]
    deletes: tuple[str, ...]
    _attempted: bool = field(default=False, init=False, repr=False)

    def report(self):
        return {"create": list(self.creates), "update": list(self.updates), "delete": list(self.deletes),
                "expected_fingerprint": self.expected_fingerprint,
                "atomic_revision_check": False, "single_writer_required": True}


def plan(block, manifest, doc, current_items):
    """Reconcile desired/current state; refuse missing/changed owned items and UUID collisions.

    A manifest is a trusted local ownership receipt, not proof against a hostile
    caller forging one. Never construct/adopt one from arbitrary manual UUIDs.
    """
    if manifest.namespace != block.namespace or manifest.document_bytes != _doc_bytes(doc):
        raise ValueError("Manifest must match the block namespace and exact document")
    current = _index(current_items)
    previous = {item.key: item for item in manifest.owned}
    if len(previous) != len(manifest.owned):
        raise ValueError("Duplicate manifest keys")
    desired = {}
    identities = set()
    for key, item in block.items().items():
        if item.id.value != block.id(key):
            raise ValueError("Desired UUID is not owned by this block key")
        if hasattr(item, "path"):
            if item.path.path and item.path != doc.sheet_path:
                raise ValueError("Item belongs to another sheet instance")
            item.path.CopyFrom(doc.sheet_path)
        nested = [item.id.value]
        if isinstance(item, sch.SheetSymbol):
            nested.extend(pin.id.value for pin in item.pins)
        if isinstance(item, sch.SchematicSymbolInstance):
            nested.extend(content.id.value for child in item.definition.items
                          if hasattr(content := _message(child.item), "id"))
        if len(set(nested)) != len(nested) or identities.intersection(nested):
            raise ValueError("Logical keys collide with generated child identities")
        identities.update(nested)
        desired[key] = _pack(item)
    for key, record in previous.items():
        live = current.get(block.id(key))
        if live is None or _hash(live) != record.observed_sha256:
            raise StaleState(f"Owned item changed or missing: {key}")
    creates, updates, deletes = [], [], []
    for key, packed in desired.items():
        if key not in previous:
            if block.id(key) in current:
                raise StaleState(f"Unowned UUID collision: {key}")
            creates.append(key)
        elif _hash(packed) != previous[key].desired_sha256:
            if current[block.id(key)].type_url != packed.type_url:
                raise ValueError("Changing an owned item's type requires a new logical key")
            before, after = _unpack(current[block.id(key)]), _unpack(packed)
            if (isinstance(before, sch.SheetSymbol)
                    and before.filename_field.text.text != after.filename_field.text.text):
                raise ValueError("Retargeting a child sheet is outside block reconciliation")
            updates.append(key)
    for key in previous.keys() - desired.keys():
        if current[block.id(key)].Is(sch.SheetSymbol.DESCRIPTOR):
            raise ValueError("Deleting a sheet could discard manual child content; refused")
        deletes.append(key)
    if len(creates) + len(updates) + len(deletes) > 200:
        raise ValueError("A block apply is limited to 200 changed items")
    return Plan(manifest, fingerprint(current.values()),
                tuple((key, _bytes(value)) for key, value in sorted(desired.items())),
                tuple(sorted(creates)), tuple(sorted(updates)), tuple(sorted(deletes)))


def apply(client, doc, pending):
    """Apply a single-use plan via existing client.batch under its local writer lock.

    Rechecks the full sheet before BeginCommit; another client/user can race after
    that read. No save, retries, full-sheet deletion, implicit adoption or automatic
    recovery. Any exception after attempting a batch leaves its outcome unverified.
    """
    with client.lock:
        if pending._attempted:
            raise StaleState("Plan already attempted; reread and replan, never replay")
        if pending.manifest.document_bytes != _doc_bytes(doc):
            raise ValueError("Apply must target the plan's exact document")
        current = _index(client.items(doc).items)
        if fingerprint(current.values()) != pending.expected_fingerprint:
            raise StaleState("Sheet changed since planning; no edits sent")
        object.__setattr__(pending, "_attempted", True)
        desired = {key: any_pb2.Any.FromString(value) for key, value in pending.desired}
        block = Block(pending.manifest.namespace)
        steps = []
        if pending.deletes:
            steps.append({"command": "DeleteItems", "payload": {
                "item_ids": [{"value": block.id(key)} for key in pending.deletes]}})
        for command, keys in (("UpdateItems", pending.updates), ("CreateItems", pending.creates)):
            if keys:
                steps.append({"command": command, "payload": {"items": [as_dict(desired[k]) for k in keys]}})
        if not steps:
            return pending.manifest
        try:
            client.batch(doc, steps, "MCP: reconcile owned schematic block")
            observed = _index(client.items(doc).items)
            owned_before = {block.id(r.key) for r in pending.manifest.owned}
            owned_after = {block.id(key) for key in desired}
            if set(observed) != (set(current) - owned_before) | owned_after:
                raise RuntimeError("Unexpected item membership after apply")
            for value in set(current) - owned_before:
                if current[value] != observed[value]:
                    raise RuntimeError("An unrelated item changed during apply")
            for record in pending.manifest.owned:
                if record.key not in pending.updates and record.key not in pending.deletes:
                    value = block.id(record.key)
                    if current[value] != observed[value]:
                        raise RuntimeError("An unchanged owned item changed during apply")
            # KiCad normalizes defaults and omits definition pins on reads. Record
            # both requested content and the observed baseline rather than replaying
            # requests just because their protobuf representation differs.
            return Manifest(pending.manifest.namespace, pending.manifest.document_bytes,
                            tuple(OwnedItem(key, _hash(observed[block.id(key)]), _hash(packed))
                                  for key, packed in sorted(desired.items())))
        except Exception as error:
            raise ApplyUncertain("Block apply outcome unknown/unverified. Do not replay. Inspect the "
                                 "editor/commit state, reread owned UUIDs and replan; do not assume "
                                 f"rollback or save. Cause: {error}") from error
