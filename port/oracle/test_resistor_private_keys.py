#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Replay literal serde private-marker-key envelopes through actual resistor adapters.

Object keys spelled like serde_json's internal Number/RawValue markers are
ordinary JSON keys. The pinned C++ adapter treats them as plain objects; these
complete replies were recorded from it and stay fixed. Refusals here are
adapter input controls, separate from the registered successful difftest cases.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

FIXTURES = Path(__file__).resolve().parent / 'fixtures'
REQUESTS = FIXTURES / 'resistor-private-keys.requests.jsonl'
REPLIES = FIXTURES / 'resistor-private-keys.replies.jsonl'
REQUESTS_SHA256 = '854ef879f3374c8868ddc9ae3391b90cad34f1b578a08c27eca4ffe5a59bf159'
REPLIES_SHA256 = '579853934d0bec0491d4aba2517978bf6954da74c16fea8dd5cfaf8eba3603e7'
ESCAPED_REQUESTS = FIXTURES / 'resistor-private-keys-escaped.requests.jsonl'
ESCAPED_REPLIES = FIXTURES / 'resistor-private-keys-escaped.replies.jsonl'
ESCAPED_REQUESTS_SHA256 = '044fcf237467dac57fd32a6389a9beabbab6cc61c6dd9a6369f4f5a5565a6959'
ESCAPED_REPLIES_SHA256 = 'f12a2fb72cf3dbec71e85cd3cf3382218402f5514f1537d4e9b7236cff37cbed'
CASES = 889 + 199


def strict(line):
    """Parse a reply line rejecting duplicate keys and non-integer numbers."""
    def pairs(items):
        keys = [key for key, _ in items]
        if len(keys) != len(set(keys)):
            raise ValueError('duplicate reply key: ' + line)
        return dict(items)

    def refuse(token):
        raise ValueError('unexpected non-integer reply number ' + token + ': ' + line)

    return typed(json.loads(line, object_pairs_hook=pairs, parse_float=refuse, parse_constant=refuse))


def typed(value):
    """Tag JSON types so Python's True == 1 equality cannot hide a changed reply."""
    if isinstance(value, dict):
        return ('object', tuple(sorted((key, typed(item)) for key, item in value.items())))
    if isinstance(value, list):
        return ('array', tuple(typed(item) for item in value))
    return (type(value).__name__, value)


def lines(data):
    if b'\r' in data or not data.endswith(b'\n'):
        raise ValueError('fixtures and replies must be LF-terminated JSONL without CR')
    return data[:-1].split(b'\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--byte-identical', action='store_true',
                        help='also require each reply line to equal the recorded C++ bytes')
    parser.add_argument('adapters', nargs='+', type=Path)
    args = parser.parse_args()
    request_bytes, reply_bytes = REQUESTS.read_bytes(), REPLIES.read_bytes()
    assert hashlib.sha256(request_bytes).hexdigest() == REQUESTS_SHA256, 'request fixture changed'
    assert hashlib.sha256(reply_bytes).hexdigest() == REPLIES_SHA256, 'reply fixture changed'
    assert len(lines(request_bytes)) == len(lines(reply_bytes)) == 889
    escaped_requests, escaped_replies = ESCAPED_REQUESTS.read_bytes(), ESCAPED_REPLIES.read_bytes()
    assert hashlib.sha256(escaped_requests).hexdigest() == ESCAPED_REQUESTS_SHA256, 'escaped request fixture changed'
    assert hashlib.sha256(escaped_replies).hexdigest() == ESCAPED_REPLIES_SHA256, 'escaped reply fixture changed'
    assert len(lines(escaped_requests)) == len(lines(escaped_replies)) == 199
    from generate_resistor_escaped_keys import requests as generate_escaped_requests
    assert escaped_requests == generate_escaped_requests(), 'literal escaped keys were changed'
    request_bytes += escaped_requests
    reply_bytes += escaped_replies
    requests, expected = lines(request_bytes), lines(reply_bytes)
    assert len(requests) == len(expected) == CASES, (len(requests), len(expected))
    parsed = [strict(line.decode()) for line in expected]
    tops = [{key for key, _ in reply[1]} for reply in parsed]
    results = sum('result' in keys for keys in tops)
    assert results == 65 and sum('error' in keys for keys in tops) == CASES - results
    for adapter in args.adapters:
        process = subprocess.run([str(adapter.resolve())], input=request_bytes, capture_output=True,
                                 check=True, timeout=60)
        assert not process.stderr, (adapter, process.stderr)
        actual = lines(process.stdout)
        assert len(actual) == CASES, (adapter, len(actual))
        for request, want, reply, got in zip(requests, expected, parsed, actual):
            if args.byte_identical:
                assert got == want, (adapter, request, want, got)
            assert strict(got.decode()) == reply, (adapter, request, want, got)
    print(f'{CASES} literal private-marker-key envelopes matched for {len(args.adapters)} adapter(s): '
          f'{results} results, {CASES - results} refusals')


if __name__ == '__main__':
    main()
