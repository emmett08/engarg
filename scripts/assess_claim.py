"""Orchestrate public CLI authoring, tool implementation and EAL assessment.

This module supplies no evidence collectors or reviewer verdicts. It records
bounded operator-selected commands and preserves every native stage's output.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import signal
import subprocess
import sys
from datetime import datetime, timezone
from urllib.parse import parse_qs, urlsplit


class WorkflowError(Exception):
    def __init__(self, message, status=2, blocked=False):
        super().__init__(message)
        self.status = status
        self.blocked = blocked


class Interrupted(Exception):
    def __init__(self, signum):
        self.signum = signum


def positive(value):
    try:
        result = int(value)
    except ValueError as error:
        raise argparse.ArgumentTypeError("expected a positive integer") from error
    if result < 1:
        raise argparse.ArgumentTypeError("expected a positive integer")
    return result


def nonnegative(value):
    result = int(value)
    if result < 0:
        raise argparse.ArgumentTypeError("expected a nonnegative integer")
    return result


def arguments(argv):
    parser = argparse.ArgumentParser(
        description="Assess a bounded claim through the public EngArg and EAL CLIs.",
        epilog=("Provider calls can incur charges within the explicit budgets. "
                "A successful workflow can conclude unsupported. Missing sources, "
                "bindings and independent reviews are never supplied by this script."),
    )
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--claim", help="claim text; use --context for its declared scope")
    source.add_argument("--input", type=Path, help="request JSON containing claim/context/evidence")
    source.add_argument("--analysis", type=Path, help="admitted report; continue without reauthoring")
    parser.add_argument("--context", help="context for --claim only")
    parser.add_argument("--tools", type=Path, help="operator TOML; defaults to an empty manifest")
    parser.add_argument("--toolchain", type=Path, help="operator toolchain JSON, including library preferences")
    parser.add_argument("--tool-language", help="defaults to the selected toolchain's language")
    parser.add_argument("--tool-store", type=Path, help="verified package cache; defaults to OUTPUT/tool-store")
    parser.add_argument("--no-implement-tools", action="store_true", help="use existing bindings only")
    parser.add_argument("--output-dir", type=Path, help="new private directory; existing paths are rejected")
    parser.add_argument("--eal-host", default=os.environ.get("EAL_HOST", "eal-host"))
    parser.add_argument("--engarg", default=os.environ.get("ENGARG_BIN", "engarg"))
    parser.add_argument("--model", default=os.environ.get("ENGARG_MODEL"), help="explicit model or ENGARG_MODEL")
    parser.add_argument("--provider", choices=("http", "cursor", "augment", "command"), default="http",
                        help="model API or local AI assistant backend")
    parser.add_argument("--assistant-config", type=Path, help="operator TOML for the selected assistant command")
    parser.add_argument("--api-key-env", default="OPENAI_API_KEY", help="credential variable NAME, never its value")
    parser.add_argument("--api", choices=("responses", "chat"), default="responses")
    parser.add_argument("--endpoint", help="provider endpoint; defaults to the selected OpenAI protocol")
    parser.add_argument("--reasoning-effort", help="optional model-specific effort")
    parser.add_argument("--service-tier", help="optional provider service tier")
    parser.add_argument("--author-mode", choices=("single", "hybrid"), default="hybrid")
    parser.add_argument("--author-max-calls", type=positive, default=7)
    parser.add_argument("--author-max-repairs", type=nonnegative, default=1)
    parser.add_argument("--author-timeout-secs", type=positive, default=2400)
    parser.add_argument("--author-max-output-tokens", type=positive, default=24000)
    parser.add_argument("--tool-max-calls", type=positive, default=8)
    parser.add_argument("--tool-timeout-secs", type=positive, default=900)
    parser.add_argument("--tool-max-output-tokens", type=positive, default=16384)
    parser.add_argument("--request-timeout-secs", type=positive, default=600)
    parser.add_argument("--execution-timeout-secs", type=positive, default=300)
    parser.add_argument("--max-response-bytes", type=positive, default=1048576)
    parser.add_argument("--max-input-bytes", type=positive, default=1048576)
    parser.add_argument("--claim-id", help="compiled EAL claim ID; defaults to the current claim")
    options = parser.parse_args(argv)
    if options.context is not None and options.claim is None:
        parser.error("--context requires --claim")
    if not options.model and (options.analysis is None or not options.no_implement_tools):
        parser.error("--model or ENGARG_MODEL is required")
    if options.provider == "command" and options.assistant_config is None:
        parser.error("--provider command requires --assistant-config")
    if options.provider == "http" and options.assistant_config is not None:
        parser.error("--assistant-config requires an assistant provider")
    if options.provider != "http" and (options.endpoint or options.reasoning_effort
                                       or options.service_tier or options.api != "responses"):
        parser.error("--endpoint, --api chat, --reasoning-effort and --service-tier require --provider http")
    if options.provider == "http" and not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", options.api_key_env):
        parser.error("--api-key-env must name an environment variable")
    if options.author_max_repairs >= options.author_max_calls:
        parser.error("--author-max-calls must reserve at least one non-repair call")
    if not options.no_implement_tools and options.toolchain is None:
        parser.error("--toolchain is required unless --no-implement-tools is selected")
    if options.no_implement_tools and options.tool_language:
        parser.error("--tool-language requires tool implementation")
    if options.endpoint:
        endpoint = urlsplit(options.endpoint)
        if endpoint.scheme not in ("http", "https") or not endpoint.netloc or endpoint.username or endpoint.password:
            parser.error("--endpoint must be HTTP(S) without embedded credentials")
        if any(re.search(r"key|token|secret|password|authorization", name, re.I)
               for name in parse_qs(endpoint.query)):
            parser.error("put endpoint credentials in the named environment variable, not the URL")
    return options


def executable(value, label):
    found = shutil.which(value)
    if found is None:
        raise WorkflowError(f"{label} executable not found: {value}")
    return str(Path(found).resolve())


def read_json(path):
    try:
        with path.open(encoding="utf-8") as stream:
            value = json.load(stream)
        if not isinstance(value, dict):
            raise ValueError("expected an object")
        return value
    except (OSError, ValueError) as error:
        raise WorkflowError(f"cannot read JSON: {path}") from error


def write_json(path, value):
    with path.open("w", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2, ensure_ascii=False)
        stream.write("\n")


class Workflow:
    def __init__(self, options):
        self.options = options
        self.root = (options.output_dir or Path("engarg-runs") / (
            "assessment-" + datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
            + "-" + str(os.getpid())
        )).absolute()
        if self.root.exists() or self.root.is_symlink():
            raise WorkflowError(f"output directory already exists: {self.root}")
        self.engarg = executable(options.engarg, "EngArg")
        self.host = executable(options.eal_host, "EAL host")
        self.environment = dict(os.environ)
        self.environment["PATH"] = str(Path(self.host).parent) + os.pathsep + self.environment.get("PATH", "")
        for path, label in ((options.input, "request"), (options.analysis, "analysis"),
                            (options.tools, "tool manifest"), (options.toolchain, "toolchain"),
                            (options.assistant_config, "assistant configuration")):
            if path is not None and not path.is_file():
                raise WorkflowError(f"{label} is not a regular file: {path}")
        if options.tool_store is not None and options.tool_store.exists() and not options.tool_store.is_dir():
            raise WorkflowError("--tool-store must be a directory")
        if not options.no_implement_tools:
            for parent, name in ((Path(self.engarg).parent, "engarg-process-supervisor"),
                                 (Path(self.host).parent, "eal-tool-run"),
                                 (Path(self.host).parent, "eal-process-supervisor")):
                if not os.access(parent / name, os.X_OK):
                    raise WorkflowError(f"missing matching runtime companion: {parent / name}")
        elif options.provider != "http" and options.analysis is None:
            supervisor = Path(self.engarg).parent / "engarg-process-supervisor"
            if not os.access(supervisor, os.X_OK):
                raise WorkflowError(f"missing matching runtime companion: {supervisor}")
        self.root.parent.mkdir(parents=True, exist_ok=True)
        self.root.mkdir(mode=0o700)
        (self.root / "logs").mkdir()
        self.stage = "configuration"
        self.report = options.analysis.absolute() if options.analysis else self.root / "analysis.json"
        self.tools = options.tools.absolute() if options.tools else self.root / "operator-tools.toml"
        if options.tools is None:
            self.tools.write_text("[tools]\n", encoding="utf-8")
        self.summary = {
            "schema": "engarg.claim-assessment-run/1", "status": "running",
            "stage": "configuration",
            "output_dir": str(self.root), "model": options.model, "provider": options.provider,
            "api_key_env": options.api_key_env if options.provider == "http" else None,
            "stages": [],
        }
        self.record()

    def record(self):
        write_json(self.root / "summary.json", self.summary)

    def run(self, stage, argv):
        self.stage = stage
        stdout = self.root / "logs" / (stage + ".stdout.log")
        stderr = self.root / "logs" / (stage + ".stderr.log")
        event = {"stage": stage, "argv": argv, "status": "running",
                 "stdout": str(stdout), "stderr": str(stderr)}
        self.summary["stages"].append(event)
        self.summary["stage"] = stage
        self.record()
        with (self.root / "commands.txt").open("a", encoding="utf-8") as stream:
            stream.write(shlex.join(argv) + "\n")
        print(f"[assessment] {stage}; progress/log: {stderr}", file=sys.stderr, flush=True)
        with stdout.open("wb") as out, stderr.open("wb") as err:
            try:
                process = subprocess.Popen(argv, stdout=out, stderr=err, env=self.environment)
            except OSError as error:
                status = 127 if isinstance(error, FileNotFoundError) else 126
                event.update({"exit_code": status, "status": "failed"})
                self.record()
                raise WorkflowError(f"{stage} executable could not start; inspect the configured executable", status) from error
            try:
                result = process.wait()
            except (KeyboardInterrupt, Interrupted) as interruption:
                signum = interruption.signum if isinstance(interruption, Interrupted) else signal.SIGINT
                process.send_signal(signum)
                try:
                    process.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
                result = 128 + signum
            event["exit_code"] = result if result >= 0 else 128 - result
            event["status"] = "complete" if result == 0 else "failed"
            self.record()
        if result:
            raise WorkflowError(f"{stage} failed; inspect {stderr}", event["exit_code"])
        return stdout

    def provider(self):
        value = self.options
        result = ["--model", value.model, "--provider", value.provider,
                  "--request-timeout-secs", str(value.request_timeout_secs),
                  "--max-input-bytes", str(value.max_input_bytes),
                  "--max-response-bytes", str(value.max_response_bytes)]
        if value.provider != "http":
            if value.assistant_config is not None:
                result += ["--assistant-config", str(value.assistant_config.absolute())]
            return result
        endpoint = value.endpoint or ("https://api.openai.com/v1/responses" if value.api == "responses"
                                      else "https://api.openai.com/v1/chat/completions")
        result += ["--api-key-env", value.api_key_env, "--api", value.api, "--endpoint", endpoint]
        for flag, selected in (("--reasoning-effort", value.reasoning_effort),
                               ("--service-tier", value.service_tier)):
            if selected:
                result += [flag, selected]
        return result

    def render(self, stem):
        for format_, suffix in (("html", "html"), ("markdown", "md")):
            destination = self.root / (stem + "." + suffix)
            self.run("render_" + stem + "_" + format_, [self.engarg, "render", "--input", str(self.report),
                     "--format", format_, "--output", str(destination)])
        self.summary["presentation"] = str(self.root / (stem + ".html"))

    def assess(self):
        value = self.options
        if not value.no_implement_tools:
            language = read_json(value.toolchain).get("language")
            if not isinstance(language, str) or not language:
                raise WorkflowError("toolchain has no language")
            if value.tool_language is not None and value.tool_language != language:
                raise WorkflowError("--tool-language differs from the operator toolchain language")
        self.run("runtime_preflight", [self.engarg, "eal", "describe", "--eal-host", self.host,
                 "--workspace", str(self.root), "--output", str(self.root / "runtime.json")])
        runtime = read_json(self.root / "runtime.json")
        if runtime.get("is_error") or runtime.get("result", {}).get("source_syntax") != "EAL/4":
            raise WorkflowError("selected runtime did not describe EAL/4; inspect runtime.json")
        if not value.no_implement_tools:
            self.run("toolchain_preflight", [self.engarg, "eal", "toolchain-check", "--toolchain",
                     str(value.toolchain.absolute()), "--output", str(self.root / "toolchain-check.json")])
            if read_json(self.root / "toolchain-check.json").get("passed") is not True:
                raise WorkflowError("isolated toolchain is not ready; inspect toolchain-check.json")
        if value.analysis is None:
            request = value.input.absolute() if value.input else self.root / "request.json"
            if value.claim is not None:
                write_json(request, {"claim": value.claim, "context": value.context or "", "evidence": []})
            self.run("author", [self.engarg, "analyse", "--input", str(request), "--tools", str(self.tools),
                     "--eal-host", self.host, "--mode", value.author_mode,
                     "--max-calls", str(value.author_max_calls), "--max-repairs", str(value.author_max_repairs),
                     "--timeout-secs", str(value.author_timeout_secs),
                     "--max-output-tokens", str(value.author_max_output_tokens),
                     "--audit", str(self.root / "analysis.audit.jsonl"),
                     "--checkpoint", str(self.root / "analysis.checkpoint.json"),
                     "--progress", "--format", "json", "--output", str(self.report)] + self.provider())
        self.render("authoring")
        report = read_json(self.report)
        unbound = (report.get("direct_bindings") or {}).get("unbound_operations", [])
        if not value.no_implement_tools:
            self.run("implement", [self.engarg, "eal", "implement", "--input", str(self.report),
                     "--tools", str(self.tools), "--eal-host", self.host,
                     "--tool-language", language, "--toolchain", str(value.toolchain.absolute()),
                     "--tool-store", str((value.tool_store or self.root / "tool-store").absolute()),
                     "--tool-output-dir", str(self.root / "implemented"),
                     "--tool-max-calls", str(value.tool_max_calls),
                     "--tool-timeout-secs", str(value.tool_timeout_secs),
                     "--max-output-tokens", str(value.tool_max_output_tokens), "--progress"] + self.provider())
            self.report = self.root / "implemented/analysis.with-tools.json"
            self.tools = self.root / "implemented/bindings/tools.toml"
            report = read_json(self.report)
            tooling = report.get("tooling") or {}
            operations = tooling.get("operations", [])
            self.summary["verified_operations"] = len(operations)
            self.summary["newly_generated_operations"] = sum(
                operation["implementation"]["reused"] is False for operation in operations
            )
            self.summary["reused_operations"] = sum(
                operation["implementation"]["reused"] is True for operation in operations
            )
            self.summary["implementation_model_calls"] = tooling.get("model_calls")
            unbound = (report.get("direct_bindings") or {}).get("unbound_operations", [])
            if unbound:
                self.summary["unbound_operations"] = unbound
                raise WorkflowError("implementation left unbound/deferred operations; inspect implemented/implementation.json", 3, True)
        else:
            self.summary.update({"verified_operations": 0, "newly_generated_operations": 0,
                                 "reused_operations": 0})
            self.summary["implementation_model_calls"] = 0
        self.render("argument")
        package = self.root / "investigation"
        try:
            self.run("build", [self.engarg, "build", "--input", str(self.report), "--tools", str(self.tools),
                     "--output-dir", str(package)])
        except WorkflowError as error:
            # Authoring metadata describes its original manifest. Native build
            # admits the current operator bindings, including newly supplied ones.
            if value.no_implement_tools:
                self.summary["authoring_unbound_operations"] = unbound
                error.blocked = True
            raise
        receipt = read_json(package / "build.json")
        claim = value.claim_id or receipt.get("claims", {}).get("current_claim")
        if not isinstance(claim, str) or not claim:
            raise WorkflowError("package has no current claim ID; inspect investigation/build.json")
        result = self.run("execute", [self.host, "run", str(package / "case.eal"), "--tools",
                          str(package / "tools.toml"), "--workspace", str(self.root / "execution"),
                          "--claim", claim, "--timeout", str(value.execution_timeout_secs)])
        execution = read_json(result)
        assessment = execution.get("result", {}).get("assessment", {})
        if (execution.get("is_error") or not isinstance(assessment.get("claims"), dict)
                or claim not in assessment["claims"]):
            raise WorkflowError("runtime did not return an assessment; inspect logs/execute.stdout.log", 3)
        write_json(self.root / "execution.json", execution)
        write_json(self.root / "assessment.json", assessment)
        self.summary.update({"status": "complete", "report": str(self.report), "package": str(package),
                             "claim_id": claim, "claim_status": assessment["claims"].get(claim, {}).get("status"),
                             "valid": assessment.get("valid"), "complete": assessment.get("complete")})

    def finish(self, status, error=None):
        self.summary["exit_code"] = status
        if error is not None:
            self.summary.update({"status": "blocked" if error.blocked else "failed", "error": str(error)})
        self.record()
        print(json.dumps({key: self.summary.get(key) for key in (
            "status", "claim_status", "valid", "complete", "output_dir", "presentation", "error"
        )}, ensure_ascii=False))


def interrupt(signum, _frame):
    raise Interrupted(signum)


def main(argv=None):
    os.umask(0o077)
    options = arguments(argv)
    workflow = None
    try:
        workflow = Workflow(options)
        signal.signal(signal.SIGTERM, interrupt)
        workflow.assess()
        workflow.finish(0)
        return 0
    except WorkflowError as error:
        if workflow is not None:
            workflow.finish(error.status, error)
        print("assessment: " + str(error), file=sys.stderr)
        return error.status
    except (KeyboardInterrupt, Interrupted) as interruption:
        signum = interruption.signum if isinstance(interruption, Interrupted) else signal.SIGINT
        error = WorkflowError("assessment interrupted; retained stages remain available", 128 + signum)
        if workflow is not None:
            workflow.finish(error.status, error)
        return error.status


if __name__ == "__main__":
    sys.exit(main())
