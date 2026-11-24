"""Targeted synthetic validator regressions; no oracle, full gate or emitter qualification."""
import collections
import ast
import copy
import json
from pathlib import Path
import tempfile
import types
import unittest
from unittest.mock import patch

import contract
import gate_inventory
import handoff
import lockschema
import apply_parent_fixes


class TypedSchemaTests(unittest.TestCase):
    def errors(self, spec, value):
        errors = []
        lockschema.validate(spec, value, 'fixture', errors)
        return errors

    def test_argument_fields_are_recursively_typed(self):
        genuine = dict(symbol='Inverse', sha256='0' * 64, effect='analyse', contract_sha256='0' * 64,
                       rationale='synthetic declared regions', padding=0, args=copy.deepcopy(lockschema.ANALYSE_ARGS))
        self.assertEqual(self.errors(lockschema.CALLEE, genuine), [])
        for value in (72.0, True, '72', None):
            for register in ('rdi', 'rsi'):
                changed = copy.deepcopy(genuine)
                changed['args'][register]['size'] = value
                with self.subTest(register=register, value=value):
                    self.assertTrue(self.errors(lockschema.CALLEE, changed))
        for value in (0.0, False, -1):
            changed = copy.deepcopy(genuine)
            changed['padding'] = value
            with self.subTest(padding=value):
                self.assertTrue(self.errors(lockschema.CALLEE, changed))
        changed = copy.deepcopy(genuine)
        changed['args']['rsi']['unreviewed'] = 1
        self.assertTrue(self.errors(lockschema.CALLEE, changed))

    def test_consumer_count_is_typed_and_contract_is_exact(self):
        sid = 'mul_vector3-doubleops'
        op = contract.REQUIRED_SITES[sid]
        expected = copy.deepcopy(contract.OPS[op]['consumer'])
        spec = lockschema.site_spec(sid)[1]['consumer']
        self.assertEqual(self.errors(spec, expected), [])
        for value in (3.0, True, '3', None):
            changed = copy.deepcopy(expected)
            changed['count'] = value
            with self.subTest(count=value):
                self.assertTrue(self.errors(spec, changed))
        changed = copy.deepcopy(expected)
        changed['unreviewed'] = 1
        self.assertTrue(self.errors(spec, changed))
        changed = copy.deepcopy(expected)
        del changed['count']
        self.assertTrue(self.errors(spec, changed))

    def test_canonical_equality_rejects_nested_float_and_bool(self):
        expected = {'regions': [{'size': 72}], 'count': 3}
        self.assertTrue(contract.typed_equal(copy.deepcopy(expected), expected))
        for value in (72.0, True):
            changed = copy.deepcopy(expected)
            changed['regions'][0]['size'] = value
            self.assertFalse(contract.typed_equal(changed, expected))
        self.assertFalse(contract.typed_equal({'size': 1}, {'size': True}))
        self.assertFalse(contract.typed_equal([1], [1.0]))
        self.assertFalse(contract.typed_equal((1,), [1]))

    def test_site_semantics_rejects_equal_float_consumer(self):
        sid = 'mul_vector3-doubleops'
        op = contract.REQUIRED_SITES[sid]
        spec = contract.OPS[op]
        site = dict(op=op, source=op, roles=spec['roles'], consumer=copy.deepcopy(spec['consumer']),
                    output_order=spec['outputs'], rust=contract.canonical_rust(op), dispatch={'literal': op})
        self.assertEqual(contract.site_contract_errors(sid, site), [])
        site['consumer']['count'] = 3.0
        self.assertEqual(contract.site_contract_errors(sid, site),
                         [sid + ': consumer differs from the canonical mul_vector3 contract'])


class InventoryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        root = Path(self.temp.name)
        self.locks = root / 'locks'
        self.locks.mkdir()
        self.elock = dict(compile_count=76, source={'blobs': {k: {'blob': '0' * 40, 'git_path': k}
                                                          for k in ('header', 'vector2', 'vector3', 'adapter')}},
                          callees={k: {'effect': 'fixture'} for k in lockschema.REVIEWED_CALLEES},
                          frame={'setup_text': 'sub $0x708,%rsp', 'size': 0x708},
                          sites={k: {} for k in contract.REQUIRED_SITES},
                          contexts={k: {} for k in contract.REQUIRED_SITES})
        self.glock = {'sites': {k: {'fingerprint': '0' * 64} for k in contract.REQUIRED_SITES}}
        oracle = root / 'oracle'
        oracle.write_bytes(b'synthetic validator input, not an executable oracle')
        self.elock['oracle'] = {'sha256': handoff.sha(oracle.read_bytes())}
        inputs = {}
        for key in ('header', 'vector2', 'vector3', 'adapter', 'compile_commands', 'provenance', 'source_manifest'):
            p = root / key
            p.write_text(key)
            inputs[key] = {'path': str(p), 'sha256': handoff.sha(p.read_bytes())}
        for name, value in (('eligibility.lock.json', self.elock), ('graphs.lock.json', self.glock)):
            (self.locks / name).write_text(json.dumps(value))
        inventory = gate_inventory.check_inventory(self.elock, self.glock, 'full', with_categories=True)
        self.report = dict(mode='verify', phase='full', bypass_eligibility=False, listing_patches=[],
                           listing={'first': {'decoder': 'synthetic fixture'}, 'second': {'decoder': 'synthetic fixture'}},
                           decision='PASS', decisions=dict.fromkeys(handoff.STAGES, 'pass'), failures=[],
                           checks=[dict(category=category, check=name, status='pass', detail=None)
                                   for category, name in inventory],
                           sites={k: {'fingerprint': '0' * 64, 'finite_proof': {'ok': True}, 'trees': {}}
                                  for k in contract.REQUIRED_SITES},
                           bindings=dict(eligibility_lock={'sha256': handoff.sha((self.locks / 'eligibility.lock.json').read_bytes())},
                                         graphs_lock={'sha256': handoff.sha((self.locks / 'graphs.lock.json').read_bytes())},
                                         inputs=inputs, oracle={'path': str(oracle), 'sha256': handoff.sha(oracle.read_bytes()),
                                                                'in_memory_fixture': False},
                                         generation_contract_sha256=handoff.generation_contract_sha256(),
                                         tool_sources_sha256={'fixture': 'source hash fixture'}))
        class Checks:
            def failures(self):
                return []
        # A handoff-unit fixture; the real lock/ELF gate is deliberately not simulated as qualified.
        self.gate = types.SimpleNamespace(Checks=Checks, schema=lambda *_args: None,
                                         check_inventory=gate_inventory.check_inventory)
        self.recount(self.report)

    def recount(self, report):
        report['counts'] = {c: {s: sum(r['category'] == c and r['status'] == s for r in report['checks'])
                                for s in ('pass', 'fail', 'not-run')} for c in handoff.STAGES}

    def reasons(self, report):
        with patch.object(handoff, 'tool_sources', return_value={'fixture': 'source hash fixture'}):
            return handoff.validate_full_pass(report, self.locks, self.gate)

    def test_valid_and_reordered_fixture(self):
        self.assertEqual(self.reasons(self.report), [])
        self.report['checks'].reverse()
        self.assertEqual(self.reasons(self.report), [])

    def test_inventory_preserves_names_and_binds_categories(self):
        pairs = gate_inventory.check_inventory(self.elock, self.glock, 'full', with_categories=True)
        names = gate_inventory.check_inventory(self.elock, self.glock, 'full')
        self.assertEqual(names, [name for _category, name in pairs])
        original_source = (Path(__file__).parent / 'gate_inventory.original.py').read_text()
        namespace = dict(vars(gate_inventory))
        exec(compile(original_source, '<reviewed original inventory>', 'exec'), namespace)
        self.assertEqual(names, namespace['check_inventory'](self.elock, self.glock, 'full'))
        self.assertEqual(collections.Counter(c for c, _name in pairs),
                         {'schema': 30, 'eligibility': 46, 'context': 41, 'scope': 5, 'arithmetic': 44})
        changed = copy.deepcopy(self.report)
        row = next(r for r in changed['checks'] if r['category'] == 'eligibility')
        row['category'] = 'arithmetic'
        self.recount(changed)  # Prior gap: names and all row statuses still match.
        self.assertEqual(collections.Counter(r['check'] for r in changed['checks']), collections.Counter(names))
        self.assertTrue(any('categories/names differ' in reason for reason in self.reasons(changed)))

    def test_duplicate_or_missing_check_rejects(self):
        for action in ('duplicate', 'remove'):
            changed = copy.deepcopy(self.report)
            if action == 'duplicate':
                changed['checks'].append(copy.deepcopy(changed['checks'][0]))
            else:
                changed['checks'].pop()
            self.recount(changed)
            with self.subTest(action=action):
                self.assertTrue(any('categories/names differ' in reason for reason in self.reasons(changed)))

    def test_old_inventory_provider_fails_closed(self):
        self.gate.check_inventory = lambda elock, glock, phase: []
        self.assertTrue(any('do not define a check inventory' in reason for reason in self.reasons(self.report)))


class PatchTests(unittest.TestCase):
    def test_gate_overlay_binds_both_inventory_sides(self):
        root = Path(__file__).parent
        old_function = (root / 'gate_inventory.original.py').read_text()
        new_module = (root / 'gate_inventory.py').read_text()
        node = next(x for x in ast.parse(new_module).body if isinstance(x, ast.FunctionDef))
        new_function = '\n'.join(new_module.splitlines()[node.lineno - 1:node.end_lineno])
        # A minimal syntax fixture, not the unavailable full source tree or runtime gate.
        fixture = old_function + '''\ndef verify_inventory(ck, elock, glock, phase):
    want = collections.Counter(check_inventory(elock, glock, phase) )
    have = collections.Counter([r['check'] for r in ck.rows] + [INVENTORY_CHECK])
    return want == have
'''
        changed = apply_parent_fixes.patch_gate(fixture, new_function)
        self.assertIn('with_categories=True', changed)
        self.assertIn("(r['category'], r['check'])", changed)
        self.assertIn("('scope', INVENTORY_CHECK)", changed)
        with self.assertRaises(ValueError):
            apply_parent_fixes.patch_gate(changed, new_function)
        with self.assertRaises(ValueError):
            apply_parent_fixes.patch_gate(fixture.replace('have = collections.Counter', 'other = collections.Counter'),
                                          new_function)


if __name__ == '__main__':
    unittest.main()
