"""Normalized arithmetic graph fingerprint and the separate finite proof (private prototype tool).

Fingerprint (NaN-semantic, ORDERED): nodes are numbered by first visit in a depth-first walk
of the outputs in reviewed order, visiting the instruction destination before the source.
Each node records only semantic content:
  input  ['input', name]                         (role-based input name, e.g. m12, vx, s)
  const  ['const', 'binary64', bits]              (bits read from the binary)
  op     ['op', kind, 'binary64', 'rne', dst, src] (dst = SSE2 NaN-priority operand)
plus the ordered output map [name, node]. No addresses, register names, frame offsets,
symbols, build IDs or hashes enter it. Children are never sorted, so a priority change
changes the fingerprint. Conversions: none at these sites (recorded explicitly).

Finite proof (value-semantic, UNORDERED at + and * only): each output equals its derived source
expression when the operands of add/mul are compared as unordered pairs; sub/div keep order.
"""
import hashlib
import json

FORMAT = 'pr50-nan-arith-graph-v1'
NAN_RULE = 'x86-64 SSE2 scalar/packed binary64: dst NaN quieted, else src NaN quieted, else invalid -> 0xfff8000000000000'


def tup(e):
    return tuple(tup(x) if isinstance(x, list) else x for x in e)


def fingerprint(outputs, order):
    nodes, ids = [], {}

    def visit(e):
        e = tup(e)
        if e in ids:
            return ids[e]
        if e[0] == 'leaf':
            rec = ['input', e[1]]
        elif e[0] == 'const':
            rec = ['const', 'binary64', e[1]]
        elif e[0] == 'op':
            d, s = visit(e[2]), visit(e[3])
            rec = ['op', e[1], 'binary64', 'rne', d, s]
        else:
            raise ValueError(f'non-arithmetic node {e!r}')
        ids[e] = len(nodes)
        nodes.append(rec)
        return ids[e]
    out = [[name, visit(outputs[name])] for name in order]
    doc = {'format': FORMAT, 'nan_rule': NAN_RULE, 'conversions': [], 'nodes': nodes, 'outputs': out}
    blob = json.dumps(doc, sort_keys=True, separators=(',', ':')).encode()
    return hashlib.sha256(blob).hexdigest(), doc


def canon(e):
    e = tup(e)
    if e[0] != 'op':
        return e
    a, b = canon(e[2]), canon(e[3])
    if e[1] in ('add', 'mul'):
        a, b = sorted([a, b], key=repr)
    return ('op', e[1], a, b)


def finite_proof(outputs, source):
    if set(outputs) != set(source):
        return {'ok': False, 'reason': f'output names {sorted(outputs)} vs source {sorted(source)}'}
    bad = [n for n in source if canon(outputs[n]) != canon(source[n])]
    return {'ok': not bad, 'mismatched_outputs': bad}


def to_json(e):
    e = tup(e)
    if e[0] == 'op':
        return ['op', e[1], to_json(e[2]), to_json(e[3])]
    return list(e)
