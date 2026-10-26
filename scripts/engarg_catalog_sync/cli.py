"""Adapt shell arguments and standard streams to the workflow's Python functions."""

import argparse
from dataclasses import fields
import datetime
import json
import os
from pathlib import Path
import sys

from . import assessment, preflight, runtime, summary, tooling


def absolute_path(path):
    """Preserve leaf symlinks for the later no-follow input admission."""
    print(os.path.abspath(path))


def resolved_path(path):
    print(Path(path).resolve())


def runtime_directory(host):
    print(Path(host).with_name("eal-mcp").resolve().parent)


def target_directory():
    print(json.load(sys.stdin)["target_directory"])


def inspection_status():
    value = json.load(sys.stdin)
    if value.get("schema") != "engarg.authoring-inspection/1" or type(value.get("resumable")) is not bool:
        raise ValueError("unrecognised authoring inspection")
    print("resumable" if value["resumable"] else "unavailable")


def minimum_calls(identity_path, repairs):
    debate = json.loads(Path(identity_path).read_text()).get("debate")
    print((debate["participants"] * (debate["rounds"] + 1) + 3 if debate else 6) + int(repairs))


def simple_example(identity_path):
    print(1 if json.loads(Path(identity_path).read_text()).get("simple_example") else 0)


def utc_now():
    print(datetime.datetime.now(datetime.timezone.utc).isoformat())


def main(argv=None):
    """Dispatch internal commands with checked positional argument counts."""
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)

    def command(name, handler, arguments=(), list_arguments=False):
        child = commands.add_parser(name, help=handler.__doc__)
        for argument in arguments:
            child.add_argument(argument)
        child.set_defaults(handler=handler, arguments=arguments, list_arguments=list_arguments)

    command("preflight", preflight.main,
            tuple(field.name for field in fields(preflight.PreflightOptions)), list_arguments=True)
    command("admit-collector", runtime.admit_collector, ("config_path", "selected_collector"))
    command("record-settings", runtime.record_settings, (
        "run_directory", "executable_path", "host_path", "host_selection", *runtime.SETTING_NAMES))
    command("validate-host", runtime.validate_host, ("report_path",))
    command("select-tool-gaps", tooling.select_gaps, ("analysis_path", "selection_path"))
    command("merge-tool-analysis", tooling.merge_analysis, ("original_path", "enriched_path", "final_path"))
    command("extract-case", tooling.extract_case, ("run_directory",))
    command("collection-id", assessment.collection_id, ("report_path",))
    command("assessment", assessment.write_result, ("run_directory",))
    command("summary", summary.main, ("run_directory",), list_arguments=True)
    command("execution-status", summary.execution_status, ("report_path",), list_arguments=True)
    command("graph-status", summary.graph_status, ("report_path",), list_arguments=True)
    command("preparation-status", summary.preparation_status, ("report_path",), list_arguments=True)
    command("absolute-path", absolute_path, ("path",))
    command("resolved-path", resolved_path, ("path",))
    command("runtime-directory", runtime_directory, ("host",))
    command("target-directory", target_directory)
    command("inspection-status", inspection_status)
    command("minimum-calls", minimum_calls, ("identity_path", "repairs"))
    command("simple-example", simple_example, ("identity_path",))
    command("utc-now", utc_now)

    options = parser.parse_args(argv)
    arguments = [getattr(options, name) for name in options.arguments]
    if options.list_arguments:
        options.handler(arguments)
    else:
        options.handler(*arguments)
