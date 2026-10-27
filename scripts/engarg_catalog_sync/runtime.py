"""Admit the selected EAL runtime and retain its execution identity."""

import hashlib
import json
import os
import pathlib
import selectors
import signal
import subprocess
import time

from .json_io import ExactNumber, exact_json, read, unique


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


def validate_collector_capabilities(collector, receipt_path, timeout_seconds=10):
    """Require the bounded, read-only composition capability before model calls."""
    message = ('The selected eal-collector does not support the identifier-audit composition '
               'required by this evidence plan. Install the matching pinned EAL runtime.')
    try:
        process = subprocess.Popen([collector, 'capabilities'], stdin=subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                   start_new_session=True)
    except OSError as error:
        raise SystemExit(message) from error
    output = bytearray()
    deadline = time.monotonic() + timeout_seconds
    selector = selectors.DefaultSelector()
    reaped = False
    try:
        selector.register(process.stdout, selectors.EVENT_READ)
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise ValueError('Collector capability check timed out')
            ready = selector.select(remaining)
            if not ready:
                raise ValueError('Collector capability check timed out')
            chunk = os.read(process.stdout.fileno(), 4096)
            if not chunk:
                break
            output.extend(chunk)
            if len(output) > 65536:
                raise ValueError('Collector capability output exceeds 64 KiB')
        status = process.wait(timeout=max(0, deadline - time.monotonic()))
        reaped = True
        if status != 0:
            raise ValueError('Collector capability check failed')
        document = json.loads(output, object_pairs_hook=unique)
        if (not isinstance(document, dict)
                or document.get('schema') != 'eal.collector-capabilities/1'
                or document.get('qualified_population_quantity') is not True
                or not isinstance(document.get('coverage_proofs'), list)
                or not all(isinstance(value, str) for value in document['coverage_proofs'])
                or 'identifier_audit_v1' not in document['coverage_proofs']):
            raise ValueError('Collector capability marker is missing')
        pathlib.Path(receipt_path).write_text(json.dumps(document, indent=2) + '\n')
    except (ValueError, UnicodeDecodeError, subprocess.TimeoutExpired) as error:
        raise SystemExit(message) from error
    finally:
        selector.close()
        if not reaped:
            # Kill before reaping the leader. Its PID cannot be reused while it
            # is unreaped, even if an exited leader's descendant holds stdout.
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
        process.stdout.close()


def validate_composed_collector(analysis_path, collector, receipt_path):
    """Check collector compatibility only when a compiled graph uses a witness."""
    _, analysis = read(pathlib.Path(analysis_path), limit=16_777_216)
    planning = analysis.get('evidence_planning')
    if not isinstance(planning, dict):
        raise SystemExit('Graph analysis is missing evidence planning')
    routes = planning.get('routes')
    if not isinstance(routes, dict):
        raise SystemExit('Graph analysis is missing evidence routes')
    execution = routes.get('execution')
    if execution is None:
        # Legacy or synthetic reports may have no compiled execution plan.
        # Native graph export remains responsible for structural admission.
        return
    acquisitions = execution.get('acquisitions') if isinstance(execution, dict) else None
    if not isinstance(acquisitions, list) or not all(isinstance(item, dict) for item in acquisitions):
        raise SystemExit('Graph execution acquisitions are invalid')
    proofs = [step['coverage_proof'] for step in acquisitions if step.get('coverage_proof') is not None]
    if any(not isinstance(proof, dict) or proof.get('kind') != 'identifier_audit_v1' for proof in proofs):
        raise SystemExit('Graph execution contains an unsupported coverage proof')
    if proofs:
        validate_collector_capabilities(collector, receipt_path)


def record_settings(run_directory, executable_path, host_path, host_selection, *settings):
    """Retain executable identities and reject changed connection settings."""
    directory, executable, host = map(pathlib.Path, (run_directory, executable_path, host_path))
    def output(*args):
        return subprocess.check_output(args, text=True).strip()

    record = dict(zip(SETTING_NAMES, settings[:len(SETTING_NAMES)]))
    tool_model, tool_reasoning_effort, tool_output_tokens = settings[len(SETTING_NAMES):]
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
    if record['tool_implementation']['enabled']:
        record['tool_implementation'].update(
            model=tool_model, reasoning_effort=tool_reasoning_effort,
            max_output_tokens=int(tool_output_tokens))
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
