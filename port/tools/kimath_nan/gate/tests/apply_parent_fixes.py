"""Apply the parent's reviewed P3 overlay to a NEW private scripts directory.

The input files must have the exact independently reviewed hashes. This never edits the
input tree or any lock, arithmetic function, corpus, golden or production repository.
"""
import argparse
import ast
import hashlib
import json
from pathlib import Path
import re


def sha(data):
    return hashlib.sha256(data).hexdigest()


def patch_gate(text, replacement):
    functions = [node for node in ast.parse(text).body if isinstance(node, ast.FunctionDef)
                 and node.name == 'check_inventory']
    if len(functions) != 1:
        raise ValueError('expected exactly one check_inventory definition')
    node = functions[0]
    lines = text.splitlines(keepends=True)
    text = ''.join(lines[:node.lineno - 1]) + replacement.rstrip() + '\n' + ''.join(lines[node.end_lineno:])
    text, changed = re.subn(r'want = collections\.Counter\(check_inventory\(elock, glock, phase\)\s*\)',
                           "want = collections.Counter(check_inventory(elock, glock, phase, with_categories=True))",
                           text)
    if changed != 1:
        raise ValueError('expected exactly one gate self-inventory expectation')
    before = "have = collections.Counter([r['check'] for r in ck.rows] + [INVENTORY_CHECK])"
    after = "have = collections.Counter([(r['category'], r['check']) for r in ck.rows] + [('scope', INVENTORY_CHECK)])"
    if text.count(before) != 1:
        raise ValueError('expected exactly one gate self-inventory observation')
    text = text.replace(before, after)
    ast.parse(text)
    return text


def apply(source, output, package):
    payload = json.loads((package / 'parent_payload.json').read_text())
    inputs = {}
    for name, expected in payload['expected_input_sha256'].items():
        data = (source / 'scripts' / name).read_bytes()
        if sha(data) != expected:
            raise ValueError(f'{name}: source differs from the independently reviewed revision')
        inputs[name] = data
    fixed = dict(payload['replacement_sources'])
    fixed['gate.py'] = patch_gate(inputs['gate.py'].decode(), payload['check_inventory_function'])
    for text in fixed.values():
        ast.parse(text)
    if output.exists() or output.is_symlink():
        raise ValueError('output must be a new private scripts directory')
    output.mkdir(parents=True)
    written = {}
    for name, text in fixed.items():
        data = text.encode()
        (output / name).write_bytes(data)
        written[name] = sha(data)
    (output / 'parent-overlay.json').write_text(json.dumps({
        'input_sha256': payload['expected_input_sha256'], 'output_sha256': written,
        'scope': 'Private P3 corrections only; not a full gate or runtime qualification',
    }, indent=2) + '\n')
    return written


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path, help='Unmodified private enforcement folder')
    parser.add_argument('output_scripts', type=Path, help='NEW private overlay scripts directory')
    args = parser.parse_args()
    print(json.dumps(apply(args.source, args.output_scripts, Path(__file__).resolve().parent), indent=2))
