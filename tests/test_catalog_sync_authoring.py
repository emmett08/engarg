"""Executable-package script contract; provider authoring is a separate command."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/run-catalog-sync-authoring.sh"


class ExecutableWorkflowTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="executable workflow ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.trace = self.root / "calls.jsonl"
        self.host = self.root / "eal host"
        self.engarg = self.root / "engarg binary"
        self.input = self.root / "authored report.json"
        self.tools = self.root / "operator tools.toml"
        self.input.write_text('{}', encoding='utf-8')
        self.tools.write_text('[tools]', encoding='utf-8')
        code = '''#!{python}
import json,os,pathlib,sys
kind={kind!r}
with open(os.environ['TRACE'],'a',encoding='utf-8') as stream:
    stream.write(json.dumps([kind,*sys.argv[1:]])+'\\n')
status=int(os.environ.get('BUILD_STATUS' if kind=='build' else 'RUN_STATUS','0'))
if status: raise SystemExit(status)
if kind=='build':
    out=pathlib.Path(sys.argv[sys.argv.index('--output-dir')+1]);out.mkdir(parents=True)
    (out/'case.eal').write_text('language "EAL/3"\\n')
    (out/'tools.toml').write_text('[tools]\\n')
    (out/'build.json').write_text(json.dumps({{'claims':{{'current_claim':'claim_63757272656e745f636c61696d'}}}}))
else: print(json.dumps({{'status':'supported'}}))
'''
        for path, kind in [(self.engarg, 'build'), (self.host, 'run')]:
            path.write_text(code.format(python=sys.executable, kind=kind), encoding='utf-8')
            path.chmod(0o700)
        self.environment = dict(os.environ, ENGARG_BIN=str(self.engarg), EAL_HOST=str(self.host), TRACE=str(self.trace))
        self.output = self.root / "portable package"

    def run_script(self, *args, **environment):
        return subprocess.run(['bash', str(SCRIPT), *map(str,args)], cwd=self.root,
                              env=dict(self.environment, **environment), text=True, capture_output=True)

    def calls(self):
        return [json.loads(line) for line in self.trace.read_text().splitlines()] if self.trace.exists() else []

    def source_args(self):
        return ['--input', self.input, '--tools', self.tools, '--output-dir', self.output]

    def test_builds_then_runs_exact_direct_package_paths_and_current_claim(self):
        result = self.run_script(*self.source_args())
        self.assertEqual(result.returncode, 0, result.stderr)
        build, run = self.calls()
        self.assertEqual(build, ['build','build','--input',str(self.input),'--tools',str(self.tools),'--output-dir',str(self.output)])
        self.assertEqual(run, ['run','run',str(self.output/'case.eal'),'--tools',str(self.output/'tools.toml'),
                               '--workspace',str(self.output/'run'),'--claim','claim_63757272656e745f636c61696d'])

    def test_explicit_claim_workspace_and_host_remain_operator_selected(self):
        workspace = self.root / 'observations'
        result = self.run_script(*self.source_args(), '--claim', 'bounded_claim', '--workspace', workspace, '--eal-host', self.host)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls()[1][-4:], ['--workspace',str(workspace),'--claim','bounded_claim'])

    def test_build_failure_prevents_collection_and_preserves_exit_status(self):
        result = self.run_script(*self.source_args(), BUILD_STATUS='17')
        self.assertEqual(result.returncode, 17)
        self.assertEqual(len(self.calls()), 1)

    def test_unresolved_assessment_preserves_runtime_exit_status(self):
        result = self.run_script(*self.source_args(), RUN_STATUS='3')
        self.assertEqual(result.returncode, 3)
        self.assertTrue((self.output/'case.eal').is_file())

    def test_synthetic_example_copies_portable_tools_without_engarg_or_model_calls(self):
        result = self.run_script('--simple-example','--output-dir',self.output, ENGARG_BIN='/nonexistent')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(self.calls()), 1)
        self.assertEqual(self.calls()[0][-2:], ['--claim','inventory_verified'])
        self.assertTrue((self.output/'tools/snapshot.py').is_file())
        self.assertTrue((self.output/'tools/audit.py').is_file())
        self.assertIn('kind = "mcp_stdio"', (self.output/'tools.toml').read_text())
        self.assertIn('profile "execution/1"', (self.output/'case.eal').read_text())
        self.assertFalse((self.output/'registered-planning.json').exists())

    def test_existing_output_and_mixed_modes_are_rejected_without_execution(self):
        self.output.mkdir()
        (self.output/'retained').write_text('original')
        for args in [self.source_args(), ['--simple-example','--input',self.input]]:
            result = self.run_script(*args)
            self.assertEqual(result.returncode, 2)
        self.assertEqual(self.calls(), [])
        self.assertEqual((self.output/'retained').read_text(), 'original')

    def test_removed_flags_and_missing_values_are_errors(self):
        for args in [['--legacy-flat'], ['--replan-analysis',self.input], ['--input'], ['--input',self.input]]:
            with self.subTest(args=args):
                self.assertEqual(self.run_script(*args).returncode, 2)
        self.assertEqual(self.calls(), [])

    def test_help_has_no_execution_side_effect(self):
        result = self.run_script('--help')
        self.assertEqual(result.returncode, 0)
        self.assertIn('never calls an', result.stdout)
        self.assertEqual(self.calls(), [])


if __name__ == '__main__':
    unittest.main()
