#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check the deferral accounting of the two partial comparisons of include.font.application.ui.p001.

--deferred is the kicad-difftest report of the production shim: every case is compared or
deferred, and the only deferrals are the cursor-taking FONT::Draw (owned by font.cpp's unit)
on draw cases and wxOnAssert (the wx assertion handler, which no unit ports).

--returning is the report of the same oracle replies against the shim with
--assert-host=returning, a handler that records wxOnAssert's arguments and returns as wx's
default handler does: there the only deferrals are the same Draw cases, so every case that
deferred at wxOnAssert compared exactly, its recorded assertions and continuation included.
Both reports must come from the same corpus and the same oracle output.
"""
import argparse
import json
from collections import Counter
from pathlib import Path

UNIT = 'include.font.application.ui.p001'
DRAW = ('common.font.application.ui.p001', 'draw-')
ASSERT = (UNIT, 'wxOnAssert')


def load(path):
    report = json.loads(Path(path).read_text(encoding='utf-8'))
    assert report['unit'] == UNIT and report['mode'] == 'partial', path
    assert report['failures'] == 0 and report['unprocessed'] == 0, (path, report.get('error'))
    assert not report['run_faults'] and not report['extra_replies'], path
    return report


def deferrals(report):
    return {o['case']: (o['unit'], o['upstream']) for o in report['outcomes']
            if o['outcome'] == 'deferred'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--deferred', required=True)
    parser.add_argument('--returning', required=True)
    args = parser.parse_args()
    production, returning = load(args.deferred), load(args.returning)
    for key in ('cases', 'cases_sha256', 'oracle_output_sha256'):
        assert production[key] == returning[key], key
    draw = {}
    asserted = {}
    for case, (unit, upstream) in deferrals(production).items():
        if unit == DRAW[0] and case.startswith(DRAW[1]):
            draw[case] = upstream
        elif (unit, upstream) == ASSERT:
            asserted[case] = upstream
        else:
            raise AssertionError(f'unexpected deferral {case}: {unit} {upstream}')
    remaining = deferrals(returning)
    assert remaining == {case: (DRAW[0], upstream) for case, upstream in draw.items()}, \
        sorted(set(remaining) ^ set(draw))
    kinds = Counter(case.split('-')[0] for case in asserted)
    print(f'{production["cases"]} cases: {production["compared"]} compared, {len(draw)} Draw and '
          f'{len(asserted)} wxOnAssert deferrals ({dict(sorted(kinds.items()))}); with a returning '
          f'assertion host {returning["compared"]} compared and only the {len(draw)} Draw '
          'deferrals remain')


if __name__ == '__main__':
    main()
