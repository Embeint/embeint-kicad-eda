# SPDX-License-Identifier: GPL-3.0-or-later
"""Serial native checks for the helper's reviewed cleanup restrictions.

The acceptance owner supplies its disposable document and client. Refused plans
never reach apply; successful commits are inspected before exact owned cleanup.
An uncertain apply stops immediately, leaving the runtime owner to preserve it.
"""

import uuid

from block_helpers import Block, Manifest, apply, fingerprint, plan
from client import kiid, point, sch


def require(condition, message):
    if not condition:
        raise RuntimeError(f"helper restriction check failed: {message}")


def probe(client, doc):
    """Exercise guards against real native readbacks without sending unsafe edits."""
    original = fingerprint(client.items(doc).items)
    definitions = client.library_symbols(["Device:R"])
    require(len(definitions) == 1, "Device:R definition required")
    definition = definitions[0]
    checks = []

    def current():
        return client.items(doc).items

    def refused(name, desired, state, reason):
        before = fingerprint(current())
        try:
            plan(desired, state, doc, current())
        except ValueError as error:
            require(reason in str(error), f"unexpected refusal for {name}: {error}")
            checks.append({"check": name, "result": "refused before apply", "reason": str(error),
                           "native_items_sha256": before})
        else:
            raise RuntimeError(f"unsafe plan accepted: {name}")
        require(fingerprint(current()) == before, f"refused {name} changed native items")

    def empty(block):
        return Manifest.empty(block.namespace, doc)

    def commit(block, state):
        return apply(client, doc, plan(block, state, doc, current()))

    def cleanup(block, state):
        commit(Block(block.namespace), state)

    def wire(start, end):
        return sch.SchematicLine(id=kiid(), type=sch.SLT_WIRE,
                                 start=point(*start), end=point(*end))

    # Native cleanup must not merge adjacent owned in-line segments. Labels do
    # not provide the third exit; a real numbered symbol pin does.
    for label in (False, True):
        block = Block(str(uuid.uuid4()))
        block.wire("a", "SIGNAL", [(40, 50), (50, 50), (60, 50)])
        if label:
            block.port("port", "SIGNAL", (50, 50))
        refused("in-line wire ends" + (" with label" if label else ""), block, empty(block), "merge")

    namespace = str(uuid.uuid4())

    def branch(at=(50, 50), end=(40, 50)):
        block = Block(namespace)
        pins = block.symbol("part", definition, "R911", "1", at)
        block.wire("a", "SIGNAL", [pins["1"], end])
        block.wire("b", "SIGNAL", [pins["1"], (at[0], at[1] - 10)])
        return block

    first = branch()
    baseline = fingerprint(current())
    state = commit(first, empty(first))
    placed = fingerprint(current())
    junctions = [item for item in current() if item.Is(sch.Junction.DESCRIPTOR)]
    require(len(junctions) == 1, "native symbol-pin branch needs one junction")
    pins = next(s for s in client.symbol_pins(doc) if s["reference"] == "R911")["pins"]
    require(next(p for p in pins if p["number"] == "1")["position_mm"] == [50.0, 50.0],
            "real branch pin endpoint")
    refused("move symbol pin at owned wire branch", branch((50, 70), (40, 70)), state, "wire branch")
    revision = branch(end=(35, 50))
    pending = plan(revision, state, doc, current())
    require(not pending.updates, "reroute must replace wires, never update them")
    state = apply(client, doc, pending)
    revised = fingerprint(current())
    require(revised != placed, "route revision did not change items")
    require(client.undo(doc).get("performed"), "revision undo")
    require(fingerprint(current()) == placed, "one undo did not restore exact branch")
    require(client.undo(doc, redo=True).get("performed"), "revision redo")
    require(fingerprint(current()) == revised, "one redo did not restore exact revision")
    cleanup(revision, state)
    require(fingerprint(current()) == baseline, "branch cleanup changed other items")
    checks.append({"check": "symbol pin branch and route-only revision", "result": "passed",
                   "wire_updates": list(pending.updates), "undo_redo": "one step; exact native items"})

    namespace = str(uuid.uuid4())

    def sheet_branch(at=(100, 50), filename="restriction-child.kicad_sch"):
        block = Block(namespace)
        pin = block.sheet("child", "Guard probe", filename, at, (30, 25), "2",
                          [("SIGNAL", "left", 10, "passive")])["SIGNAL"]
        block.wire("a", "SIGNAL", [pin, (pin[0] - 10, pin[1])])
        block.wire("b", "SIGNAL", [pin, (pin[0], pin[1] - 10)])
        return block

    block = sheet_branch()
    state = commit(block, empty(block))
    refused("move sheet pin at owned wire branch", sheet_branch((100, 80)), state, "wire branch")
    refused("delete owned sheet", Block(namespace), state, "Deleting a sheet")
    refused("retarget owned sheet", sheet_branch(filename="other-child.kicad_sch"), state, "Retargeting")
    # The owner created this empty child for the probe. Explicit deletion of its
    # known UUIDs is separate from reconciliation, whose sheet guard stays intact.
    client.delete(doc, [block.id(record.key) for record in state.owned])
    require(fingerprint(current()) == original, "sheet probe cleanup")

    # Native manual geometry is read back after its own commit, rather than
    # fabricated as a client-only snapshot. All cleanup uses known UUIDs.
    manual = wire((40, 100), (80, 100))
    client.mutate(doc, [manual])
    manual_baseline = fingerprint(current())
    block = Block(str(uuid.uuid4()))
    block.wire("touch", "SIGNAL", [(60, 90), (60, 100)])
    refused("new block wire touches manual wire", block, empty(block), "manual")
    crossing = Block(str(uuid.uuid4()))
    crossing.wire("crossing", "SIGNAL", [(60, 90), (60, 110)])
    state = commit(crossing, empty(crossing))
    cleanup(crossing, state)
    require(fingerprint(current()) == manual_baseline, "crossing changed manual wire")
    client.delete(doc, [manual.id.value])
    require(fingerprint(current()) == original, "crossing/manual cleanup")
    checks.append({"check": "plain wire interior crossing preserves manual wire", "result": "passed"})

    block = Block(str(uuid.uuid4()))
    block.symbol("part", definition, "R912", "1", (50, 50))
    state = commit(block, empty(block))
    attached = wire((40, 50), (50, 50))
    client.mutate(doc, [attached])
    moved = Block(block.namespace)
    moved.symbol("part", definition, "R912", "1", (50, 70))
    refused("move symbol pin with manual wire", moved, state, "manual wiring attached")
    refused("remove symbol pin with manual wire", Block(block.namespace), state, "manual wiring attached")
    client.delete(doc, [attached.id.value])
    cleanup(block, state)

    block = Block(str(uuid.uuid4()))
    block.wire("owned", "SIGNAL", [(40, 50), (50, 50)])
    state = commit(block, empty(block))
    attached = wire((50, 50), (50, 60))
    client.mutate(doc, [attached])
    refused("remove owned wire attached to manual wire", Block(block.namespace), state, "manual wiring attached")
    client.delete(doc, [attached.id.value])
    cleanup(block, state)

    block = Block(str(uuid.uuid4()))
    block.port("port", "SIGNAL", (50, 50))
    state = commit(block, empty(block))
    attached = wire((40, 50), (60, 50))
    client.mutate(doc, [attached])
    moved = Block(block.namespace)
    moved.port("port", "SIGNAL", (70, 70))
    refused("move label on manual wire", moved, state, "pin or label")
    refused("remove label on manual wire", Block(block.namespace), state, "pin or label")
    client.delete(doc, [attached.id.value])
    cleanup(block, state)

    marker = sch.NoConnectMarker(id=kiid(), position=point(50, 50))
    marker.size.value_nm = 1270000
    client.mutate(doc, [marker])
    block = Block(str(uuid.uuid4()))
    block.add("nc", sch.NoConnectMarker(position=point(50, 50)))
    refused("duplicate native no-connect marker", block, empty(block), "would share")
    client.delete(doc, [marker.id.value])

    block = Block(str(uuid.uuid4()))
    block.add("nc", sch.NoConnectMarker(position=point(50, 50), size=marker.size))
    state = commit(block, empty(block))
    renamed = Block(block.namespace)
    renamed.add("renamed-nc", sch.NoConnectMarker(position=point(50, 50), size=marker.size))
    state = commit(renamed, state)
    cleanup(renamed, state)
    checks.append({"check": "replace owned no-connect key at same point", "result": "passed"})

    # add() must not bypass the route/pin model; these fail before native edits.
    for item in (sch.SchematicLine(type=sch.SLT_WIRE), sch.Junction(),
                 sch.SchematicSymbolInstance(), sch.SheetSymbol()):
        try:
            Block(str(uuid.uuid4())).add("bypass", item)
        except ValueError:
            checks.append({"check": f"add bypass {item.DESCRIPTOR.name}", "result": "refused before apply"})
        else:
            raise RuntimeError("add bypass accepted")
    require(fingerprint(current()) == original, "exact original items after all probes")
    return {"result": "passed", "checks": checks, "explicit_save": False,
            "cleanup": "exact original native items", "independent_agent_evidence": False,
            "limits": "Guard refusals use native readbacks; unsafe mutations are never sent."}
