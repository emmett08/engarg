"""Only declared extra environment names cross the isolated EAL host boundary."""

import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
from engarg_catalog_sync.collector_environment import HOST_DEFAULT_ENVIRONMENT, selected_names


def bundle(*names):
    return {'schema_version': 'engarg.support-export/1',
            'registry': {'tools': {'audit': {'inherit_env': list(names)}}}}


class CollectorEnvironmentTests(unittest.TestCase):
    def test_only_present_declared_extras_are_forwarded(self):
        inherited = bundle('HOME', 'LANG', 'PATH', 'SSH_AUTH_SOCK', 'AUDIT_TOKEN', 'EMPTY', 'MISSING')
        inherited['registry']['tools']['other'] = {'inherit_env': ['AUDIT_TOKEN']}
        environment = {'HOME': '/home/user', 'LANG': 'C', 'PATH': '/bin',
                       'SSH_AUTH_SOCK': '/synthetic/socket', 'AUDIT_TOKEN': 'private',
                       'UNDECLARED_SECRET': 'private', 'EMPTY': ''}
        self.assertEqual(selected_names(inherited, environment), ['AUDIT_TOKEN', 'SSH_AUTH_SOCK'])

    def test_host_process_defaults_stay_synchronised(self):
        source = (ROOT / 'src/eal_host.rs').read_text().split('fn command(', 1)[1]
        defaults = source.split('for name in [', 1)[1].split('] {', 1)[0]
        self.assertEqual(HOST_DEFAULT_ENVIRONMENT, set(re.findall(r'"([A-Z_]+)"', defaults)))

    def test_cli_outputs_names_without_values_or_undeclared_names(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'bundle.json'
            path.write_text(json.dumps(bundle('SSH_AUTH_SOCK', 'EMPTY')))
            result = subprocess.run(
                [sys.executable, str(ROOT / 'scripts/catalog_sync.py'), 'collector-environment', str(path)],
                env={'SSH_AUTH_SOCK': 'synthetic-private-socket-value', 'EMPTY': '',
                     'UNDECLARED_SECRET': 'another-private-value'},
                capture_output=True, text=True, check=True,
            )
            self.assertEqual(result.stdout, 'SSH_AUTH_SOCK\n')
            self.assertEqual(result.stderr, '')

    def test_invalid_or_excessive_names_fail_before_any_output(self):
        for value in [bundle('INVALID\nNAME'), bundle('X' * 1025), bundle(1),
                      {'schema_version': 'engarg.support-export/1'},
                      {'schema_version': 'engarg.eal-export/3', 'registry': {'tools': {}}}]:
            with self.subTest(bundle=value), self.assertRaises(ValueError):
                selected_names(value, {})
        names = ['TOKEN_' + str(number) for number in range(65)]
        with self.assertRaisesRegex(ValueError, 'exceeds 64'):
            selected_names(bundle(*names), dict.fromkeys(names, 'secret'))


if __name__ == '__main__':
    unittest.main()
