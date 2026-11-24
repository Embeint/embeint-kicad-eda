# SPDX-License-Identifier: GPL-3.0-or-later
"""Typed shape of the new object lock; reference measurements still require review."""
import copy

import contract
import lockschema as s


def errors(lock, generated_hash):
    external = {'__dso_handle': {'kind': 'address-only', 'symbol_type': 'object'}}
    fields = {
        'note': s.DOC, 'mode': s.CONST('object-v1'),
        'status': s.ENUM('DERIVED-PENDING-REVIEW', 'REVIEWED-EXPERIMENTAL-UNQUALIFIED'),
        'home_object': s.obj({'file': s.CONST('kimath_math.cpp.o'), 'sha256': s.HEX(64),
                              'comment': s.lst(s.STR, 1)}),
        'compile': s.obj({'command': s.STR, 'compiler_path': s.STR, 'header_include_dir': s.STR}),
        'toolchain': s.obj({'identity_sha256': s.HEX(64), 'input_lock_sha256': s.HEX(64),
                            'gxx': s.obj({'path': s.STR, 'version': s.STR}),
                            'glibc': s.STR, 'libstdcxx_runtime': s.STR}),
        'linked': s.obj({'comment': s.lst(s.STR, 1), 'needed': s.lst(s.STR, 1)}),
        'link': s.obj({'home_sections': s.lst(s.STR, 1), 'provider_units': s.mp(None, s.HEX(64)),
                       'imports': s.lst(s.STR, 1), 'externals': s.CONST(external),
                       'externals_spec': s.CONST(external)}),
        'reference_layout': s.lst(s.obj({'start': s.INT(0), 'end': s.INT(1),
                                         'key': s.lst(s.STR, 2, 3)}), 1),
        'context': s.mp(set(contract.REQUIRED_SITES), s.obj({
            'path_instructions': s.INT(1), 'path_callees_sha256': s.HEX(64),
            'path_object_bytes_sha256': s.HEX(64)})),
        'context_review': s.obj({'basis': s.DOC, 'link_digest': s.HEX(64),
                                 'object_sha256': s.HEX(64), 'reviewer': s.STR,
                                 'status': s.ENUM('PENDING', 'REVIEWED')}),
        'eq_operator': s.ADDR,
        'emission': s.obj({'generated_rs_sha256': s.CONST(generated_hash),
                            'required_sites': s.CONST(sorted(contract.REQUIRED_SITES))}),
    }
    view = copy.deepcopy(lock)
    review = view.get('context_review') if isinstance(view, dict) else None
    out = []
    if isinstance(review, dict):
        if review.get('status') == 'PENDING' and review.get('reviewer') is None:
            review['reviewer'] = 'pending-null'  # Only the pending state permits JSON null.
            if view.get('status') != 'DERIVED-PENDING-REVIEW':
                out.append('object.status: pending review requires derived-pending status')
        elif review.get('status') == 'REVIEWED':
            if view.get('status') != 'REVIEWED-EXPERIMENTAL-UNQUALIFIED':
                out.append('object.status: reviewed context requires reviewed-unqualified status')
    s.validate(s.obj(fields), view, 'object', out)
    if not out:
        for i, row in enumerate(view['reference_layout']):
            if row['start'] >= row['end']:
                out.append(f'object.reference_layout[{i}]: empty/reversed range')
            key = row['key']
            if key[0] == 'home' and len(key) == 2:
                continue
            if key[0] == 'provider' and len(key) == 3:
                s.validate(s.HEX(64), key[2], f'object.reference_layout[{i}].key[2]', out)
            else:
                out.append(f'object.reference_layout[{i}]: unknown unit key shape')
    return out
