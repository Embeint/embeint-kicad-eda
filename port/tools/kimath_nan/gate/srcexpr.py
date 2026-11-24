"""Derives source expressions from the exact pinned matrix3x3.h bytes (private prototype tool).

A fail-closed interpreter for the C++ subset of the reviewed MATRIX3x3<double> definitions:
local declarations of `result`, `for( int i = 0; i < 3; i++ )` loops, assignments to
`result.m_data[i][j]` / `result.x|y|z`, `return`, member reads `X.m_data[i][j]`, `m_data[i][j]`,
`aVector.x|y|z`, `aScalar`, `Determinant()`, parentheses, binary `* / + -` (C++ precedence and
left associativity) and decimal floating literals (converted to binary64 with correct rounding,
as C++ does for these literals). Operator overloads dispatch to the parsed header definitions.
Any other token or statement raises SourceError.

Also checks the data-member layout of MATRIX3x3, VECTOR2 and VECTOR3 that the input/output maps
rely on (row-major `T m_data[3][3]`, `T x, y` and `T x{}; T y{}; T z{};`).
"""
import hashlib
import re
import struct


class SourceError(Exception):
    pass


SIGNATURES = {
    'determinant': 'T MATRIX3x3<T>::Determinant() const',
    'inverse': 'MATRIX3x3<T> MATRIX3x3<T>::Inverse() const',
    'mul_matrix': 'MATRIX3x3<T> const operator*( MATRIX3x3<T> const& aA, MATRIX3x3<T> const& aB )',
    'mul_vector2': 'VECTOR2<T> const operator*( MATRIX3x3<T> const& aMatrix, VECTOR2<T> const& aVector )',
    'mul_vector3': 'VECTOR3<T> const operator*( MATRIX3x3<T> const& aMatrix, VECTOR3<T> const& aVector )',
    'mul_scalar': 'MATRIX3x3<T> const operator*( MATRIX3x3<T> const& aMatrix, S aScalar )',
    'scalar_mul': 'MATRIX3x3<T> const operator*( S aScalar, MATRIX3x3<T> const& aMatrix )',
}
PARAMS = {
    'determinant': [], 'inverse': [], 'mul_matrix': ['aA', 'aB'], 'mul_vector2': ['aMatrix', 'aVector'],
    'mul_vector3': ['aMatrix', 'aVector'], 'mul_scalar': ['aMatrix', 'aScalar'], 'scalar_mul': ['aScalar', 'aMatrix'],
}


def definition(text, signature):
    """Exact bytes of the unique definition (template line, signature, braced body)."""
    hits = [m.start() for m in re.finditer(re.escape(signature) + r'\s*\n\{', text)]
    if len(hits) != 1:
        raise SourceError(f'{len(hits)} definitions of {signature!r}')
    start = text.rfind('\ntemplate', 0, hits[0]) + 1
    if text[start:hits[0]].strip() not in ('template <class T>', 'template <class T, class S>'):
        raise SourceError(f'unexpected template header for {signature!r}')
    i = text.index('{', hits[0])
    depth = 0
    for k in range(i, len(text)):
        depth += text[k] == '{'
        depth -= text[k] == '}'
        if depth == 0:
            return text[start:k + 1], text[i + 1:k]
    raise SourceError('unbalanced braces')


TOKEN = re.compile(r'\s*(?:(\d+\.\d+|\d+)|([A-Za-z_]\w*)|(\+\+|<=|>=|==|[{}()\[\];,.*/+\-=<>&]))')


def tokens(body):
    body = re.sub(r'//[^\n]*', '', body)
    out, pos = [], 0
    while pos < len(body):
        if body[pos:].strip() == '':
            break
        m = TOKEN.match(body, pos)
        if not m:
            raise SourceError(f'untokenizable source near {body[pos:pos + 30]!r}')
        out.append(m.group(1) or m.group(2) or m.group(3))
        pos = m.end()
    return out


def dbits(text):
    return ('const', '0x' + struct.pack('>d', float(text)).hex())


class Value:
    def __init__(self, kind, v):
        self.kind, self.v = kind, v   # kind: scalar | int | matrix | vector2 | vector3


class Interp:
    def __init__(self, header):
        self.header = header
        self.defs = {k: definition(header, s) for k, s in SIGNATURES.items()}
        self.check_layouts()

    def check_layouts(self):
        def members(cls):
            m = re.search(r'\nclass ' + cls + r'\n\{', self.header_for[cls])
            if not m:
                raise SourceError(f'class {cls} not found')
            text, i, depth = self.header_for[cls], m.end() - 1, 0
            for k in range(i, len(text)):
                depth += text[k] == '{'
                depth -= text[k] == '}'
                if depth == 0:
                    body = text[i + 1:k]
                    break
            lines, depth = [], 0
            for line in body.split('\n'):
                s = line.strip()
                if depth == 0 and re.fullmatch(r'T [\w\[\]{}, ]+;', s):
                    lines.append(s)
                depth += line.count('{') - line.count('}')
            return lines
        self.layouts = {c: members(c) for c in ('MATRIX3x3', 'VECTOR2', 'VECTOR3')}
        expect = {'MATRIX3x3': ['T m_data[3][3];'], 'VECTOR2': ['T x, y;'],
                  'VECTOR3': ['T x{};', 'T y{};', 'T z{};']}
        if self.layouts != expect:
            raise SourceError(f'data-member layout differs: {self.layouts}')

    header_for = {}

    def call(self, name, this=None, args=()):
        _, body = self.defs[name]
        env = dict(zip(PARAMS[name], args))
        self.toks, self.pos, self.this, self.env = tokens(body), 0, this, env
        result = self.block_until_end()
        return result

    # --- statements ---------------------------------------------------------------------------
    def peek(self, k=0):
        return self.toks[self.pos + k] if self.pos + k < len(self.toks) else None

    def take(self, expect=None):
        t = self.peek()
        if t is None or (expect is not None and t != expect):
            raise SourceError(f'expected {expect!r}, got {t!r}')
        self.pos += 1
        return t

    def block_until_end(self):
        while self.peek() is not None:
            r = self.statement()
            if r is not None:
                if self.peek() is not None:
                    raise SourceError('statements after return')
                return r
        raise SourceError('no return statement')

    def statement(self):
        t = self.peek()
        if t == '{':
            self.take('{')
            while self.peek() != '}':
                r = self.statement()
                if r is not None:
                    raise SourceError('return inside a nested block')
            self.take('}')
            return None
        if t == 'return':
            self.take('return')
            v = self.expr()
            self.take(';')
            return v
        if t in ('MATRIX3x3', 'VECTOR2', 'VECTOR3'):
            self.take()
            self.take('<'), self.take('T'), self.take('>')
            name = self.take()
            if name != 'result':
                raise SourceError('only a local named result is reviewed')
            zero = dbits('0.0')
            if t == 'MATRIX3x3':
                self.take(';')
                self.env['result'] = Value('matrix', [[zero] * 3 for _ in range(3)])
            else:
                n = 2 if t == 'VECTOR2' else 3
                self.take('(')
                for k in range(n):
                    if self.take() != '0':
                        raise SourceError('vector initializer other than 0')
                    if k < n - 1:
                        self.take(',')
                self.take(')'), self.take(';')
                self.env['result'] = Value('vector%d' % n, [zero] * n)
            return None
        if t == 'for':
            self.take('for'), self.take('('), self.take('int')
            var = self.take()
            self.take('='), self.take('0'), self.take(';')
            if self.take() != var:
                raise SourceError('loop condition variable')
            self.take('<'), self.take('3'), self.take(';')
            if self.take() != var:
                raise SourceError('loop increment variable')
            self.take('++'), self.take(')')
            body_start = self.pos
            for k in range(3):
                self.pos = body_start
                self.env[var] = Value('int', k)
                self.statement()
            del self.env[var]
            return None
        if t == 'result':
            self.take('result'), self.take('.')
            r = self.env['result']
            field = self.take()
            if field == 'm_data':
                i, j = self.index(), self.index()
                self.take('=')
                r.v[i][j] = self.scalar(self.expr())
            elif field in ('x', 'y', 'z') and r.kind.startswith('vector'):
                self.take('=')
                r.v['xyz'.index(field)] = self.scalar(self.expr())
            else:
                raise SourceError(f'assignment to result.{field}')
            self.take(';')
            return None
        raise SourceError(f'unreviewed statement starting {t!r}')

    def index(self):
        self.take('[')
        v = self.expr()
        self.take(']')
        if v.kind != 'int':
            raise SourceError('non-integer index')
        return v.v

    # --- expressions --------------------------------------------------------------------------
    def scalar(self, v):
        if v.kind != 'scalar':
            raise SourceError(f'expected a scalar, got {v.kind}')
        return v.v

    def expr(self):
        v = self.term()
        while self.peek() in ('+', '-'):
            op = {'+': 'add', '-': 'sub'}[self.take()]
            v = self.binary(op, v, self.term())
        return v

    def term(self):
        v = self.primary()
        while self.peek() in ('*', '/'):
            op = {'*': 'mul', '/': 'div'}[self.take()]
            v = self.binary(op, v, self.primary())
        return v

    def binary(self, op, a, b):
        if a.kind == 'scalar' and b.kind == 'scalar':
            return Value('scalar', ('op', op, a.v, b.v))
        if op == 'mul' and a.kind == 'matrix' and b.kind == 'scalar':
            return self.sub_call('mul_scalar', None, (a, b))
        if op == 'mul' and a.kind == 'scalar' and b.kind == 'matrix':
            return self.sub_call('scalar_mul', None, (a, b))
        if op == 'mul' and a.kind == 'matrix' and b.kind == 'matrix':
            return self.sub_call('mul_matrix', None, (a, b))
        raise SourceError(f'unreviewed operator {op} on {a.kind}, {b.kind}')

    def sub_call(self, name, this, args):
        saved = (self.toks, self.pos, self.this, self.env)
        r = self.call(name, this, args)
        self.toks, self.pos, self.this, self.env = saved
        return r

    def primary(self):
        t = self.take()
        if t == '(':
            v = self.expr()
            self.take(')')
            return v
        if re.fullmatch(r'\d+\.\d+', t):
            return Value('scalar', dbits(t))
        if re.fullmatch(r'\d+', t):
            return Value('int', int(t))
        if t == 'm_data':
            if self.this is None:
                raise SourceError('m_data without an object')
            i, j = self.index(), self.index()
            return Value('scalar', self.this.v[i][j])
        if t == 'Determinant':
            self.take('('), self.take(')')
            return self.sub_call('determinant', self.this, ())
        if t in self.env:
            v = self.env[t]
            if self.peek() == '.':
                self.take('.')
                f = self.take()
                if f == 'm_data' and v.kind == 'matrix':
                    i, j = self.index(), self.index()
                    return Value('scalar', v.v[i][j])
                if f in ('x', 'y', 'z') and v.kind.startswith('vector'):
                    return Value('scalar', v.v['xyz'.index(f)])
                raise SourceError(f'unreviewed member {t}.{f}')
            return v
        raise SourceError(f'unreviewed primary {t!r}')


def matrix_leaves(prefix):
    return Value('matrix', [[('leaf', f'{prefix}{i}{j}') for j in range(3)] for i in range(3)])


def derive(header_text, vector2_text, vector3_text):
    """Source expressions (and the exact definition bytes) for every reviewed site."""
    Interp.header_for = {'MATRIX3x3': header_text, 'VECTOR2': vector2_text, 'VECTOR3': vector3_text}
    it = Interp(header_text)
    m, a, b = matrix_leaves('m'), matrix_leaves('a'), matrix_leaves('b')
    s = Value('scalar', ('leaf', 's'))
    v2 = Value('vector2', [('leaf', 'vx'), ('leaf', 'vy')])
    v3 = Value('vector3', [('leaf', 'vx'), ('leaf', 'vy'), ('leaf', 'vz')])

    def outs(v):
        if v.kind == 'matrix':
            return {f'r{i}{j}': v.v[i][j] for i in range(3) for j in range(3)}
        if v.kind.startswith('vector'):
            return {'xyz'[k]: e for k, e in enumerate(v.v)}
        return {'det': v.v}
    exprs = {
        'determinant': outs(it.call('determinant', m)),
        'inverse': outs(it.call('inverse', m)),
        'mul_matrix': outs(it.call('mul_matrix', None, (a, b))),
        'mul_vector2': outs(it.call('mul_vector2', None, (m, v2))),
        'mul_vector3': outs(it.call('mul_vector3', None, (m, v3))),
        'mul_scalar': outs(it.call('mul_scalar', None, (m, s))),
        'scalar_mul': outs(it.call('scalar_mul', None, (s, m))),
    }
    spans = {k: hashlib.sha256(d[0].encode()).hexdigest() for k, d in it.defs.items()}
    return exprs, spans, it.layouts
