"""Admit operator inputs and retain the exact sources of an authoring request."""

import datetime
from dataclasses import dataclass
import hashlib
import json
import os
import pathlib
import re
import urllib.parse

from .json_io import exact_json, read


@dataclass(frozen=True)
class PreflightOptions:
    """The positional shell-to-Python preflight contract, named for internal use."""

    inputs: str
    catalogue: str
    directory: str
    implement_tools: str
    tool_language: str
    toolchain: str
    tool_store: str
    tool_max_calls: str
    tool_timeout: str
    planning_mode: str
    evidence_config: str
    replan_analysis: str
    topology: str
    participants: str
    rounds: str
    topology_config: str
    coordinator: str
    coordination_run: str
    freshness_clarifications: str


def text(config, key):
    """Read a configured scope field without accepting placeholders."""
    value = config.get(key)
    if type(value) is not str or not value.strip() or 'REPLACE' in value:
        raise ValueError('Unconfigured scope field: ' + key)
    return value


def write(directory, name, value):
    """Retain generated JSON with the workflow's exact numeric representation."""
    (directory / name).write_text(exact_json(value) + '\n', encoding='utf-8')


def admit_topology(options, directory):
    """Validate the selected debate topology and snapshot its supplied configuration."""
    selected_topology, participant_text, round_text = options.topology, options.participants, options.rounds
    topology_path, coordinator, journal = options.topology_config, options.coordinator, options.coordination_run
    debate = None
    topology_raw = None
    if selected_topology:
        if selected_topology not in {'fully-connected', 'fully_connected', 'grouped', 'chain', 'ring', 'star', 'tree', 'layered', 'sparse', 'bipartite'}:
            raise ValueError('Unknown topology preset')
        participants = int(participant_text or '6')
        rounds = int(round_text or '1')
        debate = {'preset': selected_topology, 'participants': participants, 'rounds': rounds}
    elif topology_path:
        topology_raw, graph = read(pathlib.Path(topology_path), limit=65536)
        if len(topology_raw) > 65536 or not isinstance(graph, dict):
            raise ValueError('Topology configuration exceeds its byte bound')
        participants = graph.get('participants', 6)
        rounds = graph.get('rounds', 1)
        debate = {'configuration_sha256': hashlib.sha256(topology_raw).hexdigest(), 'participants': participants, 'rounds': rounds}
    if debate:
        if type(participants) is not int or type(rounds) is not int or not 3 <= participants <= 16 or not 1 <= rounds <= 8:
            raise ValueError('Invalid topology participant or round count')
        debate.update(coordinator=coordinator or None, journal=journal or None)
        if topology_raw is not None:
            (directory / 'topology.snapshot.json').write_bytes(topology_raw)
    return debate


def admit_planning(mode, configuration_path, directory):
    """Validate graph routes and retain their private configuration sources."""
    planning = {'mode': mode}
    if planning['mode'] == 'graph':
        config_path = pathlib.Path(configuration_path)
        config_raw, config = read(config_path, limit=4_194_304)
        if (config.get('schema') != 'engarg.capability-planning-config/1'
                or set(config) - {'schema', 'collector', 'connections', 'store', 'environment', 'ranking', 'scheduling', 'bindings', 'prefer_configured', 'operation_specifications'}
                or 'prefer_configured' in config and type(config['prefer_configured']) is not bool
                or not isinstance(config.get('ranking'), dict)
                or not isinstance(config.get('scheduling'), dict)
                or not isinstance(config.get('bindings'), list) or len(config['bindings']) > 4096):
            raise ValueError('A versioned capability planning configuration is required')
        operations = config.get('operation_specifications', {})
        if not isinstance(operations, dict) or len(operations) > 64:
            raise ValueError('Operation specifications require a bounded method-to-specification object')
        def statement(value):
            return (isinstance(value, str) and value.strip() and len(value.encode('utf-8')) <= 8192
                    and all(character in '\n\t' or not (ord(character) < 32 or 127 <= ord(character) <= 159)
                            for character in value))
        for method, specification in operations.items():
            fields = {'schema', 'definition', 'input_conditions', 'output_guarantees', 'input_schema', 'output_schema'}
            if (not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_.-]*/[A-Za-z0-9_.-]+', method)
                    or len(method.encode('utf-8')) > 256
                    or not isinstance(specification, dict) or set(specification) != fields
                    or specification['schema'] != 'engarg.operation-specification/1'
                    or not statement(specification['definition'])):
                raise ValueError('Invalid operator operation specification')
            for field in ['input_conditions', 'output_guarantees']:
                statements = specification[field]
                if (not isinstance(statements, list) or len(statements) > 128
                        or field == 'output_guarantees' and not statements
                        or any(not statement(item) for item in statements)):
                    raise ValueError('Invalid operator operation conditions or guarantees')
            for field in ['input_schema', 'output_schema']:
                if not isinstance(specification[field], dict) or len(exact_json(specification[field]).encode('utf-8')) > 262144:
                    raise ValueError('Invalid operator operation interface schema')
        ranking = config['ranking']
        dimensions = ranking.get('dimensions')
        known_dimensions = {'configuration_effort_seconds', 'implementation_effort_seconds', 'execution_requests',
                            'execution_latency_ms', 'monetary_microunits', 'transfer_bytes'}
        if (set(ranking) != {'dimensions'} or not isinstance(dimensions, list) or not 1 <= len(dimensions) <= 6
                or any(not isinstance(dimension, str) or dimension not in known_dimensions for dimension in dimensions)
                or len(set(dimensions)) != len(dimensions)):
            raise ValueError('Ranking requires distinct explicit cost dimensions')
        scheduling = config['scheduling']
        limit, per_provider = scheduling.get('maximum_concurrent_sources'), scheduling.get('per_provider')
        if (set(scheduling) != {'maximum_concurrent_sources', 'per_provider'} or type(limit) is not int or not 1 <= limit <= 64
                or not isinstance(per_provider, dict)
                or any(not name or type(count) is not int or not 1 <= count <= 64 for name, count in per_provider.items())):
            raise ValueError('Graph scheduling requires finite explicit concurrency limits')
        for key in ['collector', 'connections', 'store']:
            path = text(config, key)
            if key == 'collector' and path == 'managed':
                continue
            config[key] = os.path.abspath(config_path.parent / path)
        environment = config.get('environment', [])
        if (not isinstance(environment, list)
                or any(not isinstance(name, str) or not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*', name) for name in environment)):
            raise ValueError('Planning environment entries must name operator environment variables')
        # Keep the connection file's original base directory. Its private values
        # and capability descriptions never enter the claim-authoring request.
        connection_raw, connection_config = read(pathlib.Path(config['connections']), limit=4_194_304)
        if connection_config.get('schema') != 'eal.evidence-config/1':
            raise ValueError('Planning connections require an EAL evidence configuration')
        (directory / 'evidence-config.snapshot.json').write_bytes(config_raw)
        (directory / 'evidence-connections.snapshot.json').write_bytes(connection_raw)
        write(directory, 'evidence-config.json', config)
        planning.update(operator_config=str(config_path), config_sha256=hashlib.sha256(config_raw).hexdigest(),
                        connections_sha256=hashlib.sha256(connection_raw).hexdigest(),
                        collector_selection='managed' if config['collector'] == 'managed' else 'operator')
    return planning


def admit_implementation(options):
    """Validate the implementation profile and its separate resource budget."""
    implementation = {'enabled': options.implement_tools == '1'}
    profile_raw = None
    if implementation['enabled']:
        language, profile_path, store = options.tool_language, options.toolchain, options.tool_store
        calls, timeout = options.tool_max_calls, options.tool_timeout
        profile_raw, profile = read(pathlib.Path(profile_path), limit=65_536)
        if profile.get('schema') != 'engarg.toolchain/1' or profile.get('language') != language:
            raise ValueError('Tool language must match the operator toolchain profile')
        if (not re.fullmatch(r'[0-9]+', calls) or not 1 <= int(calls) <= 256
                or not re.fullmatch(r'[0-9]+', timeout) or not 1 <= int(timeout) <= 86_400):
            raise ValueError('Invalid tool implementation budget')
        implementation.update(language=language, operator_toolchain=profile_path,
                              toolchain_sha256=hashlib.sha256(profile_raw).hexdigest(),
                              store=store, max_calls=int(calls), timeout_seconds=int(timeout))
    return implementation, profile_raw


def operator_request(inputs, catalogue, planning_mode):
    """Build a claim from allowlisted Kubernetes and Grafana scope fields."""
    kube, grafana = inputs['kubernetes'], inputs['grafana']
    if not isinstance(kube, dict) or not isinstance(grafana, dict):
        raise ValueError('Kubernetes and Grafana scope require JSON objects')
    if kube.get('context') != 'dev':
        raise ValueError('This run is restricted to kubectl context dev')
    selectors = {key: text(kube, key) for key in ['resource', 'kind', 'namespace', 'name']}
    origin, uid, query = [text(grafana, key) for key in ['origin', 'datasource_uid', 'query']]
    url = urllib.parse.urlsplit(origin)
    _ = url.port
    if (url.scheme != 'https' or not url.hostname or url.username or url.password
            or url.path not in ['', '/'] or url.query or url.fragment
            or any(character.isspace() for character in origin)):
        raise ValueError('Grafana requires an HTTPS origin without credentials, path, query or fragment')
    if len(query.encode()) > 65_536:
        raise ValueError('TraceQL query exceeds 64 KiB')
    start, end = grafana.get('start_unix'), grafana.get('end_unix')
    if type(start) is not int or type(end) is not int or not 0 < start < end:
        raise ValueError('Configure ordered integer grafana.start_unix and grafana.end_unix')
    window = [datetime.datetime.fromtimestamp(value, datetime.timezone.utc).isoformat() for value in [start, end]]
    for key, expected in [('rollout_kind', selectors['kind']), ('rollout_name', selectors['name'])]:
        if grafana.get(key) is not None and grafana[key] != expected:
            raise ValueError('Grafana and Kubernetes selectors disagree: ' + key)
    if catalogue is not None and (catalogue.get('schema') != 'eal.collector-catalogue/1' or not catalogue.get('collectors')):
        raise ValueError('Select a nonempty versioned collector catalogue')
    quote = lambda value: json.dumps(value, ensure_ascii=False)
    claim = (
        f'The Kubernetes resource of kind {quote(selectors["kind"])} named {quote(selectors["name"])} '
        f'(resource {quote(selectors["resource"])}) exists in namespace {quote(selectors["namespace"])} '
        'in the single development cluster selected by kubectl context "dev". '
        f'The exact TraceQL query below returns at least one trace in the search window {window[0]} to {window[1]} '
        f'through Grafana origin {quote(origin)} and Tempo datasource UID {quote(uid)}, '
        'and at least one retrieved trace can be attributed to that same resource in that same cluster.\n'
        + query
    )
    context = {
        'kubernetes_scope': dict(context='dev', **selectors),
        'grafana_scope': dict(origin=origin, datasource_uid=uid, start_unix=start, end_unix=end),
        'observations_supplied': False,
        'binding_state': 'Operator settings describe scope. Each evidence need still requires an explicit compatible binding.',
        'query_contract': 'The literal query is retained in the claim. Native Tempo generates a different query contract. Exact-query execution requires a compatible Grafana/scoped/custom HTTP route.',
        'correspondence_obligation': 'Search matches require full trace and Kubernetes identity review for cluster, namespace, UID, resource identity and time correspondence. Missing attributes remain unresolved.'
    }
    artifacts = inputs.get('artifacts', {})
    if (not isinstance(artifacts, dict) or any(
            not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]{0,127}', reference)
            for reference in artifacts)):
        raise ValueError('Retained artifacts require named references')
    context['retained_artifact_references'] = sorted(artifacts)
    context['collector_preparation'] = (
        'Current-case input files are prepared after argument generation. '
        'For scoped_json declare input_preparation.kind scoped and its explicit operation '
        'kubernetes, grafana_search or grafana_trace. Full traces require actual supplied trace IDs. '
        'For artifact_json reference only a supplied retained_artifact_reference. '
        'Unavailable independent reviews remain evidence gaps. Unsupported producing operations select custom.'
    ) if planning_mode == 'flat' else (
        'Derive tool-independent support contracts and shared logical acquisition operations from this claim. '
        'The operator connection configuration is separate from these requirements. '
        'Discover identifiers before detailed reads; preserve incomplete historical access and independent judgements as gaps.'
    )
    request = dict(claim=claim, context=json.dumps(context, ensure_ascii=False), evidence=[])
    return request, window


def example_request(inputs, inputs_path, directory, planning, implementation):
    """Retain the catalogue demonstration request and its independent probes."""
    if planning['mode'] != 'graph' or not implementation['enabled']:
        raise ValueError('The catalogue example requires graph planning and custom tool implementation')
    from catalogue_evidence_example import request_for

    request = request_for(inputs)
    probe_raw, _ = read(inputs_path.parent / 'qualification-probes.json')
    (directory / 'catalogue-probes.snapshot.json').write_bytes(probe_raw)
    return request, [], hashlib.sha256(probe_raw).hexdigest()


def admit_reuse(source, request, directory):
    """Admit retained analysis only for the exact current operator request."""
    reuse = None
    if source:
        retained_path = pathlib.Path(source)
        retained_raw, retained = read(retained_path, limit=16_777_216)
        support = retained.get('evidence_planning')
        if (retained.get('request') != request or not isinstance(retained.get('argument'), dict)
                or not isinstance(support, dict) or support.get('schema') != 'engarg.support-planning/1'
                or not isinstance(support.get('elaboration'), dict)
                or not isinstance(support.get('routes'), dict)):
            raise ValueError('Saved graph analysis must retain this exact operator request')
        (directory / 'retained-analysis.snapshot.json').write_bytes(retained_raw)
        reuse = {'source': str(retained_path), 'sha256': hashlib.sha256(retained_raw).hexdigest()}
    return reuse


def freshness_ages():
    """Validate the two explicit evidence freshness bounds."""
    ages = {name: int(os.environ.get(variable, '86400')) for name, variable in [
        ('review_max_age_seconds', 'ENGARG_REVIEW_MAX_AGE_SECONDS'),
        ('acquisition_max_age_seconds', 'ENGARG_ACQUISITION_MAX_AGE_SECONDS')]}
    if any(not 1 <= age <= 315_360_000 for age in ages.values()):
        raise ValueError('Freshness ages must be positive seconds through 315360000')
    return ages


def prepare(options):
    """Validate the inputs in workflow order and write their retained identities."""
    inputs_path, catalogue_path, directory = map(
        pathlib.Path, (options.inputs, options.catalogue, options.directory)
    )
    original, inputs = read(inputs_path)
    debate = admit_topology(options, directory)
    catalogue_raw, catalogue = read(catalogue_path) if options.catalogue else (None, None)
    planning = admit_planning(options.planning_mode, options.evidence_config, directory)
    implementation, profile_raw = admit_implementation(options)
    simple_example = inputs.get('schema') == 'engarg.catalogue-evidence-example/1'
    example_probes_sha256 = None
    if simple_example:
        request, window, example_probes_sha256 = example_request(
            inputs, inputs_path, directory, planning, implementation
        )
    else:
        request, window = operator_request(inputs, catalogue, planning['mode'])
    reuse = admit_reuse(options.replan_analysis, request, directory)
    if options.freshness_clarifications:
        clarification_raw, clarification = read(pathlib.Path(options.freshness_clarifications), limit=1_048_576)
        if not isinstance(clarification, dict):
            raise ValueError('Freshness clarifications must be a JSON object')
        (directory / 'freshness-clarifications.snapshot.json').write_bytes(clarification_raw)
    ages = freshness_ages()
    (directory / 'collector-inputs.snapshot.json').write_bytes(original)
    if catalogue_raw is not None:
        (directory / 'collectors.snapshot.json').write_bytes(catalogue_raw)
    if profile_raw is not None:
        (directory / 'toolchain.snapshot.json').write_bytes(profile_raw)
    write(directory, 'request.json', request)
    write(directory, 'export-policy.json', dict(schema='engarg.export-policy/1', **ages, acquisition_per_need={}))
    write(directory, 'input-identity.json', {
        'inputs_sha256': hashlib.sha256(original).hexdigest(),
        'catalogue_sha256': hashlib.sha256(catalogue_raw).hexdigest() if catalogue_raw is not None else None,
        'catalogue_selection': 'external' if catalogue_raw is not None else 'engarg built-in',
        'search_window_utc': window,
        'operator_inputs': str(inputs_path), 'operator_catalogue': str(catalogue_path) if catalogue_raw is not None else None,
        'tool_implementation': implementation, 'evidence_planning': planning, 'analysis_reuse': reuse,
        'simple_example': simple_example, 'debate': debate,
        'example_qualification_probes_sha256': example_probes_sha256
    })


def main(argv):
    """Run preflight with the existing shell argument order and safe diagnostic."""
    try:
        prepare(PreflightOptions(*argv))
    except (KeyError, TypeError, ValueError, OSError, OverflowError):
        raise SystemExit(
            'Input preflight failed. Check scope fields, exact query, time window, '
            'evidence planning configuration and connections, catalogue, toolchain, '
            'implementation budgets and freshness ages.'
        ) from None
