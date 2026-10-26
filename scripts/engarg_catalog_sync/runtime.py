"""Admit the selected EAL runtime and retain its execution identity."""

import hashlib
import json
import pathlib
import subprocess

from .json_io import ExactNumber, exact_json


SETTING_NAMES = (
    'model', 'service_tier', 'reasoning_effort', 'max_calls', 'max_repairs',
    'request_timeout_seconds', 'run_timeout_seconds', 'max_input_bytes',
    'max_response_bytes', 'max_output_tokens', 'max_concurrent_requests',
    'eal_timeout_seconds', 'argument_depth', 'eal_collection_timeout_seconds',
)


def admit_collector(config_path, selected_collector):
    """Bind managed planning to the selected runtime without rounding numbers."""
    path, selected = map(pathlib.Path, (config_path, selected_collector))
    config = json.loads(path.read_text(), parse_float=ExactNumber)
    if config['collector'] != 'managed':
        configured = pathlib.Path(config['collector'])
        try:
            matches = configured.resolve() == selected.resolve() or hashlib.sha256(configured.read_bytes()).digest() == hashlib.sha256(selected.read_bytes()).digest()
        except OSError:
            matches = False
        if not matches:
            raise SystemExit('Configured collector differs from the selected EAL runtime. Select its sibling eal-collector or use collector: "managed" in the planning configuration.')
    config['collector'] = str(selected.resolve())
    path.write_text(exact_json(config) + '\n')


def record_settings(run_directory, executable_path, host_path, host_selection, *settings):
    """Retain executable identities and reject changed connection settings."""
    directory, executable, host = map(pathlib.Path, (run_directory, executable_path, host_path))
    def output(*args):
        return subprocess.check_output(args, text=True).strip()

    record = dict(zip(SETTING_NAMES, settings))
    record['argument_depth'] = int(record['argument_depth']) if record['argument_depth'] else None
    record.update(git_commit=output('git','rev-parse','HEAD'), rustc=output('rustc','-vV'),
        eal_host_selection=host_selection, eal_host_path=str(host),
        eal_mcp_path=str(host.with_name('eal-mcp').resolve()),
        engarg_binary_sha256=hashlib.sha256(executable.read_bytes()).hexdigest(),
        eal_host_binary_sha256=hashlib.sha256(host.read_bytes()).hexdigest(),
        eal_mcp_binary_sha256=hashlib.sha256(host.with_name('eal-mcp').read_bytes()).hexdigest(),
        api='responses', http_retries=3, retry_max_delay_seconds=120)
    identity = json.loads((directory/'input-identity.json').read_text())
    record['tool_implementation'] = identity['tool_implementation']
    record['evidence_planning'] = identity['evidence_planning']
    record['analysis_reuse'] = identity['analysis_reuse']
    record['debate'] = identity.get('debate')
    if record['debate']:
        coordinator = pathlib.Path(record['debate']['coordinator']) if record['debate']['coordinator'] else host.with_name('eal-coordinate')
        record['debate'].update(coordinator=str(coordinator.resolve()),
            journal=record['debate']['journal'] or str(directory/'coordination'),
            coordinator_binary_sha256=hashlib.sha256(coordinator.read_bytes()).hexdigest())
    if record['evidence_planning']['mode'] == 'graph':
        configuration = json.loads((directory/'evidence-config.json').read_text())
        collector = pathlib.Path(configuration['collector'])
        if hashlib.sha256(pathlib.Path(configuration['connections']).read_bytes()).hexdigest() != record['evidence_planning']['connections_sha256']:
            raise SystemExit('Evidence connection configuration changed after input preflight; rerun with a stable operator configuration.')
        record.update(eal_collector_path=str(collector), eal_collector_binary_sha256=hashlib.sha256(collector.read_bytes()).hexdigest(),
                      evidence_config_sha256=hashlib.sha256((directory/'evidence-config.json').read_bytes()).hexdigest())
    metadata = json.loads(output('cargo','metadata','--locked','--no-deps','--format-version','1'))
    record['eal_dependency_sources'] = [dependency['source'] for package in metadata['packages']
        if package['name']=='engarg' for dependency in package['dependencies']
        if dependency['name'] in ['eal-collector-contracts','eal-process','eal-adapters']]
    (directory/'run-settings.json').write_text(json.dumps(record, indent=2)+'\n')


def validate_host(report_path):
    """Require EAL/3, structured reasoning and a usable collection budget."""
    with open(report_path, encoding='utf-8') as source:
        envelope = json.load(source)
    result = envelope.get('result')
    methods = result.get('methods') if isinstance(result, dict) else None
    if (envelope.get('is_error') is not False or not isinstance(result, dict)
            or result.get('source_syntax') != 'EAL/3'
            or not isinstance(methods, dict) or 'structured/1' not in methods):
        raise SystemExit('EAL host preflight requires EAL/3 syntax and the structured/1 method. Install the pinned compatible runtime.')
    for field in ['execution_limits', 'budgets']:
        advertised = result.get(field)
        if isinstance(advertised, dict) and 'collection_evidence' in advertised:
            if type(advertised['collection_evidence']) is not int or advertised['collection_evidence'] < 1:
                raise SystemExit('EAL host collection_evidence budget must be a positive integer')
            break
