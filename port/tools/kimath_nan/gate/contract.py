"""Reviewed site scope and canonical generation contract (private prototype; code-reviewed).

REQUIRED_SITES is the reviewed coverage inventory. It is declared here, independently of any
lock: deleting, adding or renaming a site in a lock cannot change it, so the gate fails instead
of silently verifying fewer sites. Changing coverage requires a reviewed change to this file.

For every operation the canonical contract fixes the request-field roles, the output consumer,
the output order, the source definition and the typed Rust mapping (function name, parameters,
return type, prelude, leaf -> Rust expression and outputs -> return expression). Locks must
state exactly these values (they are checked, not documentary) and the emitter uses only the
canonical mapping derived here.
"""

REQUIRED_SITES = {
    'determinant-doubleops': 'determinant',
    'inverse': 'inverse',
    'mul_matrix-doubleops': 'mul_matrix',
    'mul_vector2-doubleops': 'mul_vector2',
    'mul_vector3-doubleops': 'mul_vector3',
    'mul_scalar-doubleops': 'mul_scalar',
    'scalar_mul-doubleops': 'scalar_mul',
}
# Emission order of the accepted generated.rs (the reviewed file's order).
EMIT_ORDER = ['determinant-doubleops', 'inverse', 'scalar_mul-doubleops', 'mul_scalar-doubleops',
              'mul_vector2-doubleops', 'mul_vector3-doubleops', 'mul_matrix-doubleops']
assert sorted(EMIT_ORDER) == sorted(REQUIRED_SITES)

MATRIX_OUT = [f'r{i}{j}' for i in range(3) for j in range(3)]
MATRIX_RET = 'Matrix3x3D::from_elements(' + ', '.join('{r%d%d}' % (i, j) for i in range(3) for j in range(3)) + ')'

# role prefix -> (kind, Rust variable)
ROLE_RUST = {'m': ('matrix', 'm'), 'a': ('matrix', 'a'), 'b': ('matrix', 'b'), 'v': ('vector', 'v'), 's': ('scalar', 'scalar')}

OPS = {
    'determinant': {'roles': {'matrix': 'm'}, 'consumer': {'effect': 'bits_value', 'name': 'det'}, 'outputs': ['det'],
                    'name': 'determinant_doubleops_generated', 'params': 'matrix: &Matrix3x3D', 'ret': 'f64',
                    'prelude': 'let m = &matrix.data;', 'return': '{det}', 'vector_len': 0},
    'inverse': {'roles': {'matrix': 'm'}, 'consumer': {'effect': 'matrix_reply'}, 'outputs': MATRIX_OUT,
                'name': 'inverse_member', 'params': 'matrix: &Matrix3x3D', 'ret': 'Matrix3x3D',
                'prelude': 'let m = &matrix.data;', 'return': MATRIX_RET, 'vector_len': 0},
    'mul_matrix': {'roles': {'a': 'a', 'b': 'b'}, 'consumer': {'effect': 'matrix_reply'}, 'outputs': MATRIX_OUT,
                   'name': 'mul_matrix_doubleops', 'params': 'a: &Matrix3x3D, b: &Matrix3x3D', 'ret': 'Matrix3x3D',
                   'prelude': 'let (a, b) = (&a.data, &b.data);', 'return': MATRIX_RET, 'vector_len': 0},
    'mul_vector2': {'roles': {'matrix': 'm', 'vector': 'v'}, 'consumer': {'effect': 'vector2_json'}, 'outputs': ['x', 'y'],
                    'name': 'mul_vector2_doubleops', 'params': 'matrix: &Matrix3x3D, v: &Vector2<f64>',
                    'ret': 'Vector2<f64>', 'prelude': 'let m = &matrix.data;', 'return': 'Vector2::new({x}, {y})',
                    'vector_len': 2},
    'mul_vector3': {'roles': {'matrix': 'm', 'vector': 'v'},
                    'consumer': {'effect': 'json_array_init', 'count': 3, 'names': ['x', 'y', 'z']},
                    'outputs': ['x', 'y', 'z'], 'name': 'mul_vector3_doubleops',
                    'params': 'matrix: &Matrix3x3D, v: &Vector3<f64>', 'ret': 'Vector3<f64>',
                    'prelude': 'let m = &matrix.data;', 'return': 'Vector3::new({x}, {y}, {z})', 'vector_len': 3},
    'mul_scalar': {'roles': {'matrix': 'm', 'scalar': 's'}, 'consumer': {'effect': 'matrix_reply'}, 'outputs': MATRIX_OUT,
                   'name': 'mul_scalar_doubleops', 'params': 'matrix: &Matrix3x3D, scalar: f64', 'ret': 'Matrix3x3D',
                   'prelude': 'let m = &matrix.data;', 'return': MATRIX_RET, 'vector_len': 0},
    'scalar_mul': {'roles': {'matrix': 'm', 'scalar': 's'}, 'consumer': {'effect': 'matrix_reply'}, 'outputs': MATRIX_OUT,
                   'name': 'scalar_mul_doubleops', 'params': 'scalar: f64, matrix: &Matrix3x3D', 'ret': 'Matrix3x3D',
                   'prelude': 'let m = &matrix.data;', 'return': MATRIX_RET, 'vector_len': 0},
}


def canonical_leaves(op):
    c = OPS[op]
    leaves = {}
    for role in c['roles'].values():
        kind, var = ROLE_RUST[role]
        if kind == 'matrix':
            for i in range(3):
                for j in range(3):
                    leaves[f'{role}{i}{j}'] = f'{var}[{i}][{j}]'
        elif kind == 'vector':
            for k in range(c['vector_len']):
                leaves[f'{role}{"xyz"[k]}'] = f'{var}.{"xyz"[k]}'
        else:
            leaves[role] = var
    return leaves


def canonical_rust(op):
    c = OPS[op]
    return {'name': c['name'], 'params': c['params'], 'ret': c['ret'], 'prelude': c['prelude'],
            'leaves': canonical_leaves(op), 'return': c['return']}


def typed_equal(actual, expected):
    """Compare canonical JSON values recursively, including each numeric type."""
    if type(actual) is not type(expected):
        return False
    if isinstance(expected, dict):
        return actual.keys() == expected.keys() and all(typed_equal(actual[k], v) for k, v in expected.items())
    if isinstance(expected, list):
        return len(actual) == len(expected) and all(typed_equal(a, b) for a, b in zip(actual, expected))
    return actual == expected


def site_contract_errors(site_id, s):
    """Every safety-relevant site field must equal the canonical contract (or fail)."""
    if site_id not in REQUIRED_SITES:
        return [f'{site_id}: not a reviewed required site']
    op = REQUIRED_SITES[site_id]
    c = OPS[op]
    errs = []
    for field, want in [('op', op), ('source', op), ('roles', c['roles']), ('consumer', c['consumer']),
                        ('output_order', c['outputs']), ('rust', canonical_rust(op))]:
        if not typed_equal(s.get(field), want):
            errs.append(f'{site_id}: {field} differs from the canonical {op} contract')
    if s.get('dispatch', {}).get('literal') != op:
        errs.append(f'{site_id}: dispatch literal differs from the operation name')
    return errs
