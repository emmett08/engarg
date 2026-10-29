"""Public CLI orchestration contracts; these stubs are not provider evidence."""
import json
import os
from pathlib import Path
import signal
import shutil
import subprocess
import sys
import tempfile
import time
import unittest


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/assess-claim.sh"
FAKE = r'''#!PYTHON_VALUE
import json,os,pathlib,sys,time
role=ROLE_VALUE
args=sys.argv[1:]
def option(name): return args[args.index(name)+1]
stage='execute' if role=='host' else (args[1] if args[0]=='eal' else args[0])
with open(os.environ['FAKE_TRACE'],'a') as stream:
    stream.write(json.dumps({'stage':stage,'argv':sys.argv[1:],'pid':os.getpid()})+'\n')
if os.environ.get('SLEEP_STAGE')==stage:
    pathlib.Path(os.environ['SLEEP_READY']).write_text(str(os.getpid()))
    time.sleep(30)
if stage=='describe':
    pathlib.Path(option('--output')).write_text(json.dumps({'is_error':False,'result':{'source_syntax':'EAL/4'}}))
elif stage=='toolchain-check':
    pathlib.Path(option('--output')).write_text(json.dumps({'passed':os.environ.get('PREFLIGHT_FALSE')!='1'}))
elif stage=='analyse':
    payload=json.loads(pathlib.Path(option('--input')).read_text())
    report={'request':payload,'required_operations':['audit'],'direct_bindings':{'unbound_operations':[] if os.environ.get('BOUND')=='1' else ['audit']}}
    pathlib.Path(option('--output')).write_text(json.dumps(report))
    pathlib.Path(option('--audit')).write_text('retained authoring exchange\n')
    pathlib.Path(option('--checkpoint')).write_text('{}\n')
elif stage=='render':
    pathlib.Path(option('--output')).write_text('<html>authored argument</html>' if option('--format')=='html' else '# Authored argument\n')
elif stage=='implement':
    root=pathlib.Path(option('--tool-output-dir'));root.mkdir()
    (root/'bindings').mkdir();(root/'bindings/tools.toml').write_text('[tools.audit]\nkind="command"\n')
    report=json.loads(pathlib.Path(option('--input')).read_text())
    needed=any('[tools.'+operation+']' not in pathlib.Path(option('--tools')).read_text() for operation in report.get('required_operations',[]))
    deferred=needed and os.environ.get('DEFER')=='1'
    reused=needed and not deferred and os.environ.get('REUSE')=='1'
    report['direct_bindings']['unbound_operations']=['audit'] if deferred else []
    report['tooling']={'operations':[{'acquisition_id':'audit','implementation':{'reused':reused}}] if needed and not deferred else [],'model_calls':2 if needed and not deferred and not reused else 0}
    (root/'implementation.json').write_text(json.dumps(report['tooling']))
    (root/'analysis.with-tools.json').write_text(json.dumps(report))
elif stage=='build':
    report=json.loads(pathlib.Path(option('--input')).read_text())
    if any('[tools.'+operation+']' not in pathlib.Path(option('--tools')).read_text() for operation in report.get('required_operations',[])):
        print('Missing direct tool binding for audit',file=sys.stderr);raise SystemExit(1)
    root=pathlib.Path(option('--output-dir'));root.mkdir()
    (root/'case.eal').write_text('language "EAL/4"\n')
    (root/'tools.toml').write_text('[tools.audit]\nkind="command"\n')
    (root/'build.json').write_text(json.dumps({'claims':{'current_claim':'bounded_claim'}}))
elif stage=='execute':
    pathlib.Path(option('--workspace')).mkdir()
    if os.environ.get('RUNTIME_ERROR')=='1': print(json.dumps({'is_error':True,'result':{}}))
    else: print(json.dumps({'is_error':False,'result':{'assessment':{'valid':True,'complete':True,'claims':{'bounded_claim':{'status':'unsupported'}}},'collection':{'records':{'source':{'collected_at':'2026-10-28T08:00:00Z'}}}}}))
if os.environ.get('FAIL_STAGE')==stage: raise SystemExit(int(os.environ.get('FAIL_STATUS','17')))
'''


class ClaimAssessmentTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="claim assessment ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.trace = self.root / "calls.jsonl"
        self.engarg = self.root / "engarg binary"
        self.host = self.root / "eal host"
        for path, role in ((self.engarg, "engarg"), (self.host, "host")):
            path.write_text(FAKE.replace("PYTHON_VALUE", sys.executable).replace("ROLE_VALUE", repr(role)))
            path.chmod(0o700)
        for name in ("engarg-process-supervisor", "eal-tool-run", "eal-process-supervisor"):
            path = self.root / name
            path.write_text("#!/bin/sh\nexit 0\n")
            path.chmod(0o700)
        self.toolchain = self.root / "operator toolchain.json"
        self.toolchain.write_text('{"schema":"engarg.toolchain/2","language":"python"}')
        self.tools = self.root / "operator tools.toml"
        self.tools.write_text("[tools]\n")
        self.output = self.root / "retained assessment"
        self.environment = dict(os.environ, FAKE_TRACE=str(self.trace),
                                WORKFLOW_TEST_API_KEY="private-token-not-for-command-logs")
        self.base = ["--engarg", str(self.engarg), "--eal-host", str(self.host),
                     "--model", "configured-model", "--api-key-env", "WORKFLOW_TEST_API_KEY",
                     "--output-dir", str(self.output)]

    def run_script(self, *args, **environment):
        return subprocess.run(["bash", str(SCRIPT), *self.base, *map(str, args)], cwd=self.root,
                              env=dict(self.environment, **environment), text=True, capture_output=True)

    def calls(self):
        return [json.loads(line) for line in self.trace.read_text().splitlines()] if self.trace.exists() else []

    def call(self, stage):
        return next(call["argv"] for call in self.calls() if call["stage"] == stage)

    def summary(self):
        return json.loads((self.output / "summary.json").read_text())

    def source_args(self):
        return ["--claim", "A bounded real claim", "--toolchain", self.toolchain, "--tools", self.tools]

    def test_full_workflow_uses_native_implementation_and_concrete_exported_bindings(self):
        marker = self.root / "must not execute"
        claim = 'Quoted claim; $(touch "' + str(marker) + '")'
        result = self.run_script("--claim", claim, "--context", "A bounded\nsource scope",
                                 "--toolchain", self.toolchain, "--tools", self.tools,
                                 "--author-mode", "single", "--author-max-calls", "3",
                                 "--author-max-repairs", "2", "--execution-timeout-secs", "456")
        self.assertEqual(result.returncode, 0, result.stderr)
        stages = [call["stage"] for call in self.calls()]
        self.assertEqual(stages[:3], ["describe", "toolchain-check", "analyse"])
        self.assertLess(stages.index("implement"), stages.index("build"))
        self.assertLess(stages.index("build"), stages.index("execute"))
        request = json.loads((self.output / "request.json").read_text())
        self.assertEqual(request["claim"], claim)
        self.assertEqual(request["context"], "A bounded\nsource scope")
        self.assertFalse(marker.exists())
        build = self.call("build")
        self.assertEqual(build[build.index("--tools") + 1], str(self.output / "implemented/bindings/tools.toml"))
        self.assertEqual(build[build.index("--input") + 1], str(self.output / "implemented/analysis.with-tools.json"))
        run = self.call("execute")
        self.assertEqual(run[:2], ["run", str(self.output / "investigation/case.eal")])
        self.assertEqual(run[-2:], ["--timeout", "456"])
        self.assertEqual(self.summary()["claim_status"], "unsupported")
        self.assertEqual(self.summary()["status"], "complete")
        self.assertEqual(self.summary()["verified_operations"], 1)
        self.assertEqual(self.summary()["newly_generated_operations"], 1)
        self.assertEqual(self.summary()["reused_operations"], 0)
        self.assertEqual(self.summary()["implementation_model_calls"], 2)
        self.assertTrue((self.output / "argument.html").is_file())
        record = json.loads((self.output / "execution.json").read_text())["result"]["collection"]["records"]["source"]
        self.assertEqual(record["collected_at"], "2026-10-28T08:00:00Z")
        logs = (self.output / "commands.txt").read_text() + (self.output / "summary.json").read_text()
        self.assertIn("WORKFLOW_TEST_API_KEY", logs)
        self.assertNotIn(self.environment["WORKFLOW_TEST_API_KEY"], logs + result.stdout + result.stderr)
        self.assertEqual(self.output.stat().st_mode & 0o777, 0o700)
        self.assertEqual((self.output / "summary.json").stat().st_mode & 0o777, 0o600)

    def test_request_path_is_passed_directly_and_bound_tools_need_no_generation(self):
        request = self.root / "original request.json"
        request.write_text('{"claim":"source request","context":"bounded","evidence":[]}')
        self.tools.write_text('[tools.audit]\nkind="command"\n')
        result = self.run_script("--input", request, "--toolchain", self.toolchain, "--tools", self.tools, BOUND="1")
        self.assertEqual(result.returncode, 0, result.stderr)
        author = self.call("analyse")
        self.assertEqual(author[author.index("--input") + 1], str(request))
        self.assertIn("implement", [call["stage"] for call in self.calls()])
        self.assertEqual(self.summary()["newly_generated_operations"], 0)
        self.assertEqual(self.summary()["verified_operations"], 0)
        self.assertEqual(self.summary()["implementation_model_calls"], 0)

    def test_assistant_backends_reach_authoring_and_implementation_without_http_credentials(self):
        configuration = self.root / "assistant command.toml"
        configuration.write_text('argv = ["operator-assistant"]\ninherit_env = ["PATH"]\n')
        for provider in ("cursor", "augment", "command"):
            with self.subTest(provider=provider):
                environment = {key: value for key, value in self.environment.items()
                               if key not in ("OPENAI_API_KEY", "ENGARG_API_KEY", "WORKFLOW_TEST_API_KEY")}
                result = subprocess.run([
                    "bash", str(SCRIPT), *self.base, *map(str, self.source_args()),
                    "--provider", provider, "--assistant-config", str(configuration),
                ], cwd=self.root, env=environment, text=True, capture_output=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                for stage in ("analyse", "implement"):
                    arguments = self.call(stage)
                    self.assertEqual(arguments[arguments.index("--provider") + 1], provider)
                    self.assertEqual(arguments[arguments.index("--assistant-config") + 1], str(configuration))
                    for flag in ("--api-key-env", "--api", "--endpoint"):
                        self.assertNotIn(flag, arguments)
                self.assertEqual(self.summary()["provider"], provider)
                self.assertIsNone(self.summary()["api_key_env"])
                shutil.rmtree(self.output)
                self.trace.unlink()

    def test_native_assistant_preset_needs_no_command_configuration(self):
        result = self.run_script(*self.source_args(), "--provider", "cursor")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("--assistant-config", self.call("analyse"))
        self.assertNotIn("--endpoint", self.call("analyse"))

    def test_default_http_selection_is_explicit_in_each_model_stage(self):
        result = self.run_script(*self.source_args(), ENGARG_PROVIDER="command")
        self.assertEqual(result.returncode, 0, result.stderr)
        for stage in ("analyse", "implement"):
            arguments = self.call(stage)
            self.assertEqual(arguments[arguments.index("--provider") + 1], "http")

    def test_invalid_assistant_options_precede_runtime_calls_and_output_creation(self):
        for extra in (["--provider", "command"],
                      ["--assistant-config", str(self.tools)],
                      ["--provider", "augment", "--endpoint", "https://example.test/v1"],
                      ["--provider", "augment", "--api", "chat"],
                      ["--provider", "cursor", "--service-tier", "priority"],
                      ["--provider", "cursor", "--assistant-config", str(self.root / "missing.toml")]):
            with self.subTest(extra=extra):
                result = self.run_script(*self.source_args(), *extra)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertEqual(self.calls(), [])
                self.assertFalse(self.output.exists())

    def test_assistant_authoring_admits_its_supervisor_before_creating_output(self):
        (self.root / "engarg-process-supervisor").unlink()
        result = self.run_script("--claim", "bounded claim", "--provider", "augment",
                                 "--no-implement-tools", "--tools", self.tools)
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("engarg-process-supervisor", result.stderr)
        self.assertEqual(self.calls(), [])
        self.assertFalse(self.output.exists())

    def test_native_implementation_rechecks_missing_tools_despite_a_bound_authoring_snapshot(self):
        result = self.run_script(*self.source_args(), BOUND="1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("implement", [call["stage"] for call in self.calls()])
        self.assertEqual(self.summary()["newly_generated_operations"], 1)
        self.assertEqual(self.summary()["implementation_model_calls"], 2)

    def test_existing_analysis_continues_without_reauthoring_or_toolchain(self):
        report = self.root / "admitted analysis.json"
        report.write_text('{"direct_bindings":{"unbound_operations":[]}}')
        result = self.run_script("--analysis", report, "--no-implement-tools", "--tools", self.tools)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("analyse", [call["stage"] for call in self.calls()])
        self.assertNotIn("toolchain-check", [call["stage"] for call in self.calls()])
        self.assertEqual(self.call("build")[2], str(report))

    def test_cached_operation_records_are_not_counted_as_new_generation(self):
        result = self.run_script(*self.source_args(), REUSE="1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.summary()["verified_operations"], 1)
        self.assertEqual(self.summary()["newly_generated_operations"], 0)
        self.assertEqual(self.summary()["reused_operations"], 1)
        self.assertEqual(self.summary()["implementation_model_calls"], 0)

    def test_native_failure_preserves_exit_status_and_partial_authoring_records(self):
        result = self.run_script(*self.source_args(), FAIL_STAGE="analyse", FAIL_STATUS="17")
        self.assertEqual(result.returncode, 17)
        self.assertEqual(self.summary()["stage"], "author")
        self.assertEqual(self.summary()["status"], "failed")
        self.assertTrue((self.output / "analysis.checkpoint.json").is_file())
        self.assertFalse(any(call["stage"] == "implement" for call in self.calls()))

    def test_failed_or_false_preflight_prevents_paid_authoring(self):
        for environment in ({"FAIL_STAGE": "toolchain-check", "FAIL_STATUS": "19"}, {"PREFLIGHT_FALSE": "1"}):
            with self.subTest(environment=environment):
                result = self.run_script(*self.source_args(), **environment)
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn("analyse", [call["stage"] for call in self.calls()])
                self.assertEqual(self.summary()["status"], "failed")
                shutil.rmtree(self.output)
                self.trace.unlink()

    def test_deferred_operations_are_blocked_without_build_or_execution(self):
        result = self.run_script(*self.source_args(), DEFER="1")
        self.assertEqual(result.returncode, 3)
        self.assertEqual(self.summary()["status"], "blocked")
        self.assertEqual(self.summary()["unbound_operations"], ["audit"])
        self.assertTrue((self.output / "implemented/implementation.json").is_file())
        self.assertTrue((self.output / "authoring.html").is_file())
        self.assertNotIn("build", [call["stage"] for call in self.calls()])

    def test_no_implementation_does_not_bypass_missing_bindings(self):
        result = self.run_script("--claim", "bounded claim", "--no-implement-tools", "--tools", self.tools)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(self.summary()["status"], "blocked")
        self.assertEqual(self.summary()["stage"], "build")
        self.assertNotIn("implement", [call["stage"] for call in self.calls()])
        self.assertNotIn("execute", [call["stage"] for call in self.calls()])

    def test_native_build_can_admit_new_bindings_for_a_previously_unbound_report(self):
        report = self.root / "admitted unbound analysis.json"
        report.write_text('{"required_operations":["audit"],"direct_bindings":{"unbound_operations":["audit"]}}')
        self.tools.write_text('[tools.audit]\nkind="command"\n')
        result = self.run_script("--analysis", report, "--no-implement-tools", "--tools", self.tools)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.summary()["status"], "complete")
        self.assertNotIn("analyse", [call["stage"] for call in self.calls()])
        self.assertNotIn("implement", [call["stage"] for call in self.calls()])
        self.assertEqual(self.call("build")[2], str(report))
        self.assertEqual(json.loads(report.read_text())["direct_bindings"]["unbound_operations"], ["audit"])

    def test_missing_current_bindings_block_even_a_previously_bound_report(self):
        report = self.root / "admitted formerly bound analysis.json"
        report.write_text('{"required_operations":["audit"],"direct_bindings":{"unbound_operations":[]}}')
        result = self.run_script("--analysis", report, "--no-implement-tools", "--tools", self.tools)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(self.summary()["status"], "blocked")
        self.assertNotIn("execute", [call["stage"] for call in self.calls()])

    def test_offline_analysis_and_existing_bindings_need_no_model_or_credentials(self):
        report = self.root / "admitted offline analysis.json"
        report.write_text('{"required_operations":["audit"],"direct_bindings":{"unbound_operations":["audit"]}}')
        self.tools.write_text('[tools.audit]\nkind="command"\n')
        arguments = ["--engarg", str(self.engarg), "--eal-host", str(self.host),
                     "--output-dir", str(self.output), "--analysis", str(report),
                     "--tools", str(self.tools), "--no-implement-tools"]
        environment = {key: value for key, value in self.environment.items()
                       if key not in ("ENGARG_MODEL", "OPENAI_API_KEY", "ENGARG_API_KEY", "WORKFLOW_TEST_API_KEY")}
        result = subprocess.run(["bash", str(SCRIPT), *arguments], cwd=self.root,
                                env=environment, text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIsNone(self.summary()["model"])
        self.assertNotIn("analyse", [call["stage"] for call in self.calls()])
        self.assertNotIn("implement", [call["stage"] for call in self.calls()])

    def test_runtime_error_envelope_is_not_reported_as_a_completed_assessment(self):
        result = self.run_script(*self.source_args(), RUNTIME_ERROR="1")
        self.assertEqual(result.returncode, 3)
        self.assertEqual(self.summary()["status"], "failed")
        self.assertTrue((self.output / "logs/execute.stdout.log").is_file())

    def test_existing_output_is_not_reused_or_modified(self):
        self.output.mkdir()
        (self.output / "retained").write_text("original")
        result = self.run_script(*self.source_args())
        self.assertEqual(result.returncode, 2)
        self.assertEqual((self.output / "retained").read_text(), "original")
        self.assertEqual(self.calls(), [])

    def test_unstartable_executable_has_a_failed_stage_receipt(self):
        self.engarg.write_text("not an executable format")
        result = self.run_script(*self.source_args())
        self.assertEqual(result.returncode, 126)
        self.assertEqual(self.summary()["status"], "failed")
        self.assertEqual(self.summary()["stages"][0]["exit_code"], 126)
        self.assertNotIn("Traceback", result.stderr)

    def test_invalid_options_and_credential_urls_have_no_execution_side_effects(self):
        for args in (["--claim", "x"], ["--claim", "x", "--no-implement-tools", "--context"],
                     self.source_args() + ["--endpoint", "https://user:password@example.test/v1/responses"],
                     self.source_args() + ["--endpoint", "https://example.test/v1/responses?api_key=credential"]):
            with self.subTest(args=args):
                self.assertEqual(self.run_script(*args).returncode, 2)
        self.assertEqual(self.calls(), [])

    def test_explicit_language_mismatch_fails_before_preflights_or_paid_calls(self):
        result = self.run_script(*self.source_args(), "--tool-language", "rust")
        self.assertEqual(result.returncode, 2)
        self.assertEqual(self.summary()["status"], "failed")
        self.assertEqual(self.summary()["stage"], "configuration")
        self.assertIn("differs from", result.stderr)
        self.assertEqual(self.calls(), [])

    @unittest.skipUnless(os.name == "posix", "requires process signals")
    def test_termination_is_forwarded_and_a_partial_summary_is_retained(self):
        ready = self.root / "child ready"
        process = subprocess.Popen(["bash", str(SCRIPT), *self.base, *map(str, self.source_args())],
                                   cwd=self.root, env=dict(self.environment, SLEEP_STAGE="analyse", SLEEP_READY=str(ready)),
                                   text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.addCleanup(lambda: process.kill() if process.poll() is None else None)
        deadline = time.monotonic() + 10
        while not ready.exists() and process.poll() is None and time.monotonic() < deadline:
            time.sleep(0.02)
        self.assertTrue(ready.exists())
        child = int(ready.read_text())
        process.send_signal(signal.SIGTERM)
        stdout, stderr = process.communicate(timeout=10)
        self.assertEqual(process.returncode, 143, (stdout, stderr))
        self.assertEqual(self.summary()["exit_code"], 143)
        self.assertEqual(self.summary()["status"], "failed")
        with self.assertRaises(ProcessLookupError):
            os.kill(child, 0)


if __name__ == "__main__":
    unittest.main()
