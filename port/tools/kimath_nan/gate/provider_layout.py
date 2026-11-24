# SPDX-License-Identifier: GPL-3.0-or-later
"""Recognize function placement changes without accepting different provider code.

The original provider digest includes the function's section-relative range. A
preceding function can grow while every byte and relocation of this function
stays unchanged. Historical digest preimages allow exactly this equivalence:
their complete canonical JSON must hash to the original locked digest, and the
current proven preimage must equal it except for a same-sized range. The link
proof still checks every linked byte and independently reconstructs relocations.
"""
import argparse
import hashlib
import json
import pathlib


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def valid_range(value):
    return (type(value) is list and len(value) == 2 and all(type(n) is int for n in value)
            and 0 <= value[0] < value[1])


def bind(current, historical, locked_digest):
    """Return whether both digest preimages prove identical code at different offsets."""
    fields = {'section', 'range', 'bytes_sha256', 'relocations'}
    if type(current) is not dict or type(historical) is not dict:
        return False
    if set(current) != fields or set(historical) != fields:
        return False
    if not valid_range(current['range']) or not valid_range(historical['range']):
        return False
    if digest(historical) != locked_digest:
        return False
    if current['range'][1] - current['range'][0] != historical['range'][1] - historical['range'][0]:
        return False
    # Compare canonical bytes so equal-valued JSON floats/booleans cannot stand
    # in for the original integer relocation metadata.
    candidate = dict(current, range=historical['range'])
    return digest(candidate) == locked_digest


def apply(prover, proof, locked, references):
    """Return historical digest bindings plus an auditable list of placement changes.

    Unknown providers, changed bytes/relocations and invalid references remain
    different, so the existing full inventory comparison rejects them.
    """
    bound = dict(proof['provider_units'])
    changes = []
    for unit, current in prover.provider_canons.items():
        if len(unit) != 4:
            continue
        obj = prover.objects[unit[0]]
        names = [s['name'] for s in obj.symbols
                 if s['shndx'] == unit[1] and s['value'] == unit[2] and s['type'] == 2]
        if len(names) != 1:
            continue
        name = names[0]
        if digest(current) != bound.get(name):
            continue
        historical = references.get(name)
        if name not in locked or historical is None or bound.get(name) == locked[name]:
            continue
        if bind(current, historical, locked[name]):
            changes.append({'symbol': name, 'range': current['range'],
                            'historical_range': historical['range'],
                            'raw_digest': bound[name], 'historical_digest': locked[name]})
            bound[name] = locked[name]
    return bound, changes


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--facts', required=True, type=pathlib.Path)
    p.add_argument('--symbol', required=True)
    p.add_argument('--historical-range', required=True, help='START:END from the retained reviewed unit')
    p.add_argument('--lock', required=True, type=pathlib.Path)
    p.add_argument('--out', required=True, type=pathlib.Path)
    a = p.parse_args()
    current = json.loads(a.facts.read_bytes())
    span = [int(n) for n in a.historical_range.split(':')]
    historical = dict(current, range=span)
    locked = json.loads(a.lock.read_bytes())['link']['provider_units'][a.symbol]
    if not bind(current, historical, locked):
        p.error('the reconstructed preimage does not match the historical locked digest')
    a.out.write_text(json.dumps({a.symbol: historical}, indent=2, sort_keys=True) + '\n', encoding='utf-8')


if __name__ == '__main__':
    main()
