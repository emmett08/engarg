#!/usr/bin/env bash
# Run the local catalogue evidence example, or a scoped operator claim.
# Bash 3.2+ and Python 3.
# Usage: bash scripts/run-catalog-sync-authoring.sh [OPTIONS] INPUTS.json [CATALOGUE.json]
# Defaults to independent support requirements and qualified capability planning.
# Preflight accepts tracked changes and needs no Rust tools, host or API key.
# Full runs need a clean checkout and Rustup; the pinned EAL runtime is cached locally.
# The no-input example collects and assesses. Positional runs do so only with
# --collect-and-assess; authoring, rendering, export and planning remain separate.
set -euo pipefail
umask 077

# With no operator input, run the bounded local MCP catalogue example. The
# positional-input workflow below remains available for existing deployments.
if [[ $# -eq 0 || $1 == --simple-example ]]; then
  if [[ $# -gt 0 ]]; then shift; fi
  exec python3 "${BASH_SOURCE[0]%/*}/catalogue_evidence_example.py" "$@"
fi

progress_started=$SECONDS
set_stage() {
  stage=$1
  printf '[authoring +%ss] %s\n' "$((SECONDS - progress_started))" "$stage" >&2
}

fail() { printf '%s\n' "$*" >&2; exit 2; }
usage() {
  cat <<'HELP'
Usage: bash scripts/run-catalog-sync-authoring.sh [OPTIONS] INPUTS.json [CATALOGUE.json]
       bash scripts/run-catalog-sync-authoring.sh --simple-example [--check-only]
  --check-only                 Check inputs without build, Docker, host or inference.
  --collect-and-assess         Collect the selected claim and run EAL/3 assessment.
  --evidence-config JSON        Required graph planning configuration (collector may be "managed").
  --topology NAME              Specialist graph passed to EngArg and its native coordinator.
  --debate-participants N       Preset participants (3-16; default: 6).
  --debate-rounds N             Peer rounds (1-8; default: 1).
  --topology-config JSON       Complete graph configuration, retained as an exact snapshot.
  --eal-coordinate PATH        Native coordinator executable, otherwise sibling of eal-host.
  --coordination-run DIR       New coordinator journal directory, otherwise under this run.
  --replan-analysis JSON        Reuse a saved graph analysis and refresh routes without authoring calls.
  --legacy-flat                 Use the earlier flat collector-planning workflow.
  --implement-tools            Generate and fixture-test missing collectors (default: Rust).
  --tool-language LANGUAGE      Implementation language (default: rust), matching the profile.
  --toolchain JSON              Operator profile override; image must already be installed.
  --tool-store DIR              Reusable package store (default: .engarg-local/tool-store).
  --tool-max-calls N            Separate implementation budget (1-256; default: 16).
  --tool-timeout-secs N         Separate implementation deadline (1-86400; default: 3600).
HELP
}
check_only=0
collect_and_assess=0
evidence_planning='graph'
evidence_config=${ENGARG_EVIDENCE_CONFIG:-}
evidence_config_explicit=0
replan_analysis=''
implement_tools=0
tool_options=0
tool_language='rust'
toolchain=${ENGARG_TOOLCHAIN:-}
tool_store=${ENGARG_TOOL_STORE:-}
tool_max_calls=${ENGARG_TOOL_MAX_CALLS:-16}
tool_timeout=${ENGARG_TOOL_TIMEOUT_SECS:-3600}
topology=''
debate_participants=''
debate_rounds=''
topology_config=''
eal_coordinate=${ENGARG_EAL_COORDINATE:-}
coordinate_explicit=0
coordination_run=''
while [[ $# -gt 0 ]]; do
  case $1 in
    --check-only) check_only=1; shift ;;
    --collect-and-assess) collect_and_assess=1; shift ;;
    --legacy-flat) evidence_planning='flat'; shift ;;
    --evidence-config)
      [[ $# -ge 2 && -n $2 ]] || fail 'Missing value for --evidence-config'
      evidence_config=$2; evidence_config_explicit=1; shift 2 ;;
    --replan-analysis)
      [[ $# -ge 2 && -n $2 ]] || fail 'Missing value for --replan-analysis'
      replan_analysis=$2; shift 2 ;;
    --implement-tools) implement_tools=1; shift ;;
    --topology|--debate-participants|--debate-rounds|--topology-config|--eal-coordinate|--coordination-run)
      [[ $# -ge 2 && -n $2 ]] || fail "Missing value for $1"
      case $1 in
        --topology) topology=$2 ;;
        --debate-participants) debate_participants=$2 ;;
        --debate-rounds) debate_rounds=$2 ;;
        --topology-config) topology_config=$2 ;;
        --eal-coordinate) eal_coordinate=$2; coordinate_explicit=1 ;;
        --coordination-run) coordination_run=$2 ;;
      esac
      shift 2 ;;
    --tool-language|--toolchain|--tool-store|--tool-max-calls|--tool-timeout-secs)
      [[ $# -ge 2 && -n $2 ]] || fail "Missing value for $1"
      tool_options=1
      case $1 in
        --tool-language) tool_language=$2 ;;
        --toolchain) toolchain=$2 ;;
        --tool-store) tool_store=$2 ;;
        --tool-max-calls) tool_max_calls=$2 ;;
        --tool-timeout-secs) tool_timeout=$2 ;;
      esac
      shift 2 ;;
    --help|-h) usage; exit 0 ;;
    --) shift; break ;;
    -*) fail "Unknown option: $1" ;;
    *) break ;;
  esac
done
[[ $# -ge 1 && $# -le 2 ]] || { usage >&2; exit 2; }
if [[ $evidence_planning == graph ]]; then
  [[ -n $evidence_config ]] || fail 'Graph planning requires --evidence-config JSON. Use --legacy-flat for the earlier collector workflow.'
  [[ $# -eq 1 ]] || fail 'A positional collector catalogue requires --legacy-flat; graph planning uses qualified capabilities from --evidence-config.'
else
  [[ $evidence_config_explicit -eq 0 ]] || fail '--evidence-config cannot be combined with --legacy-flat.'
  [[ -z $replan_analysis ]] || fail '--replan-analysis requires graph planning.'
  evidence_config=''
fi
[[ -z $topology_config || ( -z $topology && -z $debate_participants && -z $debate_rounds ) ]] || fail '--topology-config cannot be combined with preset topology options.'
[[ -n $topology || ( -z $debate_participants && -z $debate_rounds ) ]] || fail 'Participant and round options require --topology.'
[[ -n $topology || -n $topology_config || ( $coordinate_explicit -eq 0 && -z $coordination_run ) ]] || fail 'Coordinator options require a selected topology.'
[[ -z $replan_analysis || ( -z $topology && -z $topology_config ) ]] || fail 'A retained analysis already fixes its topology; select topology during authoring.'
[[ $implement_tools -eq 1 || $tool_options -eq 0 ]] || fail 'Tool implementation options require --implement-tools.'
for required in git python3; do command -v "$required" >/dev/null || fail "Required command: $required"; done
repo=$(git rev-parse --show-toplevel) || fail 'Run from an EngArg checkout.'
# Keep operator paths relative to the invocation directory, including leaf-file
# symlinks so the bounded, no-follow admission below can reject them.
absolute_path() { python3 -c 'import os,sys; print(os.path.abspath(sys.argv[1]))' "$1"; }
if [[ -n $topology_config ]]; then topology_config=$(absolute_path "$topology_config"); fi
if [[ -n $eal_coordinate ]]; then eal_coordinate=$(absolute_path "$eal_coordinate"); fi
if [[ -n $coordination_run ]]; then coordination_run=$(absolute_path "$coordination_run"); fi
inputs=$(absolute_path "$1")
[[ -e $inputs ]] || fail "Operator inputs do not exist: $inputs"
if [[ $evidence_planning == graph ]]; then
  evidence_config=$(absolute_path "$evidence_config")
  [[ -e $evidence_config ]] || fail "Evidence planning configuration does not exist: $evidence_config"
fi
if [[ -n $replan_analysis ]]; then
  replan_analysis=$(absolute_path "$replan_analysis")
  [[ -e $replan_analysis ]] || fail "Saved analysis does not exist: $replan_analysis"
fi
catalogue=''
if [[ $# -eq 2 ]]; then
  catalogue=$(absolute_path "$2")
  [[ -e $catalogue ]] || fail "Collector catalogue does not exist: $catalogue. Omit CATALOGUE.json to use the built-in planning tools."
fi
prepare_rust_image=0
if [[ $implement_tools -eq 1 ]]; then
  if [[ -z $toolchain ]]; then
    [[ $tool_language == rust ]] || fail 'A non-Rust language requires --toolchain with a matching operator profile.'
    toolchain="$repo/examples/toolchains/rust.json"
    prepare_rust_image=1
  fi
  toolchain=$(absolute_path "$toolchain")
  [[ -e $toolchain ]] || fail "Toolchain profile does not exist: $toolchain"
  if [[ -n $tool_store ]]; then
    tool_store=$(absolute_path "$tool_store")
  else
    tool_store="$repo/.engarg-local/tool-store"
  fi
fi
# Resolve an explicitly selected host before changing directory; only full runs
# require it. Do not probe tools or credentials during input-only preflight.
eal_host=''
eal_host_selection='managed'
if [[ $check_only -eq 0 && -n ${ENGARG_EAL_HOST:-} ]]; then
  eal_host=$(absolute_path "$ENGARG_EAL_HOST")
  eal_host_selection='operator'
fi
cd "$repo"

mkdir -p "$repo/engarg-runs"
run_dir=$(mktemp -d "$repo/engarg-runs/catalog-sync-operator-$(date -u +%Y%m%dT%H%M%SZ)-XXXXXX")
set_stage 'input preflight'
host_report=''
authoring_checkpoint=''
authoring_pending=0
print_authoring_recovery() {
  printf 'Authoring checkpoint retained. Inspect without model calls:\n  %q authoring inspect --checkpoint %q\n' "$engarg_bin" "$authoring_checkpoint" >&2
  local inspection recovery_status=unknown
  if inspection=$("$engarg_bin" authoring inspect --checkpoint "$authoring_checkpoint" 2>/dev/null); then
    if recovery_status=$(python3 -c '
import json, sys
value = json.load(sys.stdin)
if value.get("schema") != "engarg.authoring-inspection/1" or type(value.get("resumable")) is not bool:
    raise ValueError("unrecognised authoring inspection")
print("resumable" if value["resumable"] else "unavailable")
' <<< "$inspection" 2>/dev/null); then :; else recovery_status=unknown; fi
  fi
  case "$recovery_status" in
    resumable)
      printf 'Resume final validation with one separately budgeted repair:\n  ' >&2
      printf '%q ' "$engarg_bin" authoring resume --checkpoint "$authoring_checkpoint" \
        --evidence-config "$run_dir/evidence-config.json" --output "$run_dir/analysis.resumed.json" \
        --audit "$run_dir/authoring-resume.audit.jsonl" --next-checkpoint "$run_dir/authoring-resumed.checkpoint.json" \
        --max-repairs 1 --model "$model" --service-tier "$service_tier" --reasoning-effort "$reasoning_effort" \
        --api responses --api-key-env OPENAI_API_KEY --max-input-bytes "$input_bytes" \
        --max-response-bytes "$response_bytes" --max-output-tokens "$output_tokens" \
        --request-timeout-secs "$request_timeout" --timeout-secs "$run_timeout" --progress >&2
      printf '\n' >&2
      ;;
    unavailable)
      printf 'Recovery requires a completed final authoring stage with all response content retained. This checkpoint requires a fresh authoring run after the failure is resolved.\n' >&2
      ;;
    *)
      printf 'Resumability could not be determined. Run the inspection command above to check the retained checkpoint before recovery.\n' >&2
      ;;
  esac
}
on_exit() {
  status=$?
  if [[ $status -ne 0 ]]; then
    printf 'Run stopped with status %s during %s. Retained artifacts: %s\n' "$status" "$stage" "$run_dir" >&2
    if [[ -n $host_report && -s $host_report ]]; then
      printf 'Complete EAL host response: %s\n' "$host_report" >&2
      printf 'Inspect with: python3 -m json.tool %q\n' "$host_report" >&2
    fi
    if [[ $authoring_pending -eq 1 && -n $authoring_checkpoint && -s $authoring_checkpoint ]]; then
      print_authoring_recovery
    fi
  fi
}
trap on_exit EXIT

# Read each operator file once, bound its size, reject duplicate keys and retain
# exact private snapshots. Only allowlisted scope fields enter the LLM request.
python3 - "$inputs" "$catalogue" "$run_dir" "$implement_tools" "$tool_language" \
  "$toolchain" "$tool_store" "$tool_max_calls" "$tool_timeout" "$evidence_planning" "$evidence_config" "$replan_analysis" \
  "$topology" "$debate_participants" "$debate_rounds" "$topology_config" "$eal_coordinate" "$coordination_run" <<'PY'
import datetime, hashlib, json, os, pathlib, re, stat, sys, urllib.parse

inputs_path, catalogue_path, directory = map(pathlib.Path, sys.argv[1:4])
class ExactNumber(str):
    pass
def exact_json(value):
    if isinstance(value, ExactNumber):
        return str(value)
    if isinstance(value, dict):
        return '{' + ','.join(json.dumps(key, ensure_ascii=False) + ':' + exact_json(item) for key, item in value.items()) + '}'
    if isinstance(value, list):
        return '[' + ','.join(map(exact_json, value)) + ']'
    return json.dumps(value, ensure_ascii=False, allow_nan=False)
def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError('Duplicate JSON key')
        result[key] = value
    return result
def read(path, limit=1_048_576):
    flags = os.O_RDONLY | os.O_NONBLOCK | getattr(os, 'O_NOFOLLOW', 0)
    with os.fdopen(os.open(path, flags), 'rb') as source:
        if not stat.S_ISREG(os.fstat(source.fileno()).st_mode):
            raise ValueError('Operator inputs require regular files')
        raw = source.read(limit + 1)
    if len(raw) > limit:
        raise ValueError('Operator input exceeds its byte limit')
    value = json.loads(raw, object_pairs_hook=unique, parse_float=ExactNumber,
                       parse_constant=lambda _: (_ for _ in ()).throw(ValueError('Non-finite JSON number')))
    if not isinstance(value, dict):
        raise ValueError('Operator inputs require JSON objects')
    return raw, value
def text(config, key):
    value = config.get(key)
    if type(value) is not str or not value.strip() or 'REPLACE' in value:
        raise ValueError('Unconfigured scope field: ' + key)
    return value
def write(name, value):
    (directory / name).write_text(exact_json(value) + '\n', encoding='utf-8')
try:
    original, inputs = read(inputs_path)
    selected_topology, participant_text, round_text, topology_path, coordinator, journal = sys.argv[13:19]
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
    catalogue_raw, catalogue = read(catalogue_path) if sys.argv[2] else (None, None)
    planning = {'mode': sys.argv[10]}
    if planning['mode'] == 'graph':
        config_path = pathlib.Path(sys.argv[11])
        config_raw, config = read(config_path, limit=4_194_304)
        if (config.get('schema') != 'engarg.capability-planning-config/1'
                or set(config) - {'schema', 'collector', 'connections', 'store', 'environment', 'ranking', 'scheduling', 'bindings', 'prefer_configured'}
                or 'prefer_configured' in config and type(config['prefer_configured']) is not bool
                or not isinstance(config.get('ranking'), dict)
                or not isinstance(config.get('scheduling'), dict)
                or not isinstance(config.get('bindings'), list) or len(config['bindings']) > 4096):
            raise ValueError('A versioned capability planning configuration is required')
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
        write('evidence-config.json', config)
        planning.update(operator_config=str(config_path), config_sha256=hashlib.sha256(config_raw).hexdigest(),
                        connections_sha256=hashlib.sha256(connection_raw).hexdigest(),
                        collector_selection='managed' if config['collector'] == 'managed' else 'operator')
    implementation = {'enabled': sys.argv[4] == '1'}
    profile_raw = None
    if implementation['enabled']:
        language, profile_path, store, calls, timeout = sys.argv[5:10]
        profile_raw, profile = read(pathlib.Path(profile_path), limit=65_536)
        if profile.get('schema') != 'engarg.toolchain/1' or profile.get('language') != language:
            raise ValueError('Tool language must match the operator toolchain profile')
        if (not re.fullmatch(r'[0-9]+', calls) or not 1 <= int(calls) <= 256
                or not re.fullmatch(r'[0-9]+', timeout) or not 1 <= int(timeout) <= 86_400):
            raise ValueError('Invalid tool implementation budget')
        implementation.update(language=language, operator_toolchain=profile_path,
                              toolchain_sha256=hashlib.sha256(profile_raw).hexdigest(),
                              store=store, max_calls=int(calls), timeout_seconds=int(timeout))
    simple_example = inputs.get('schema') == 'engarg.catalogue-evidence-example/1'
    example_probes_sha256 = None
    if simple_example:
        if planning['mode'] != 'graph' or not implementation['enabled']:
            raise ValueError('The catalogue example requires graph planning and custom tool implementation')
        sys.path.insert(0, str(directory.parent.parent / 'scripts'))
        from catalogue_evidence_example import request_for
        request = request_for(inputs)
        probe_raw, _ = read(inputs_path.parent/'qualification-probes.json')
        (directory/'catalogue-probes.snapshot.json').write_bytes(probe_raw)
        example_probes_sha256 = hashlib.sha256(probe_raw).hexdigest()
        window = []
    else:
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
        ) if planning['mode'] == 'flat' else (
            'Derive tool-independent support contracts and shared logical acquisition operations from this claim. '
            'The operator connection configuration is separate from these requirements. '
            'Discover identifiers before detailed reads; preserve incomplete historical access and independent judgements as gaps.'
        )
        request = dict(claim=claim, context=json.dumps(context, ensure_ascii=False), evidence=[])
    reuse = None
    if sys.argv[12]:
        retained_path = pathlib.Path(sys.argv[12])
        retained_raw, retained = read(retained_path, limit=16_777_216)
        support = retained.get('evidence_planning')
        if (retained.get('request') != request or not isinstance(retained.get('argument'), dict)
                or not isinstance(support, dict) or support.get('schema') != 'engarg.support-planning/1'
                or not isinstance(support.get('elaboration'), dict)
                or not isinstance(support.get('routes'), dict)):
            raise ValueError('Saved graph analysis must retain this exact operator request')
        (directory / 'retained-analysis.snapshot.json').write_bytes(retained_raw)
        reuse = {'source': str(retained_path), 'sha256': hashlib.sha256(retained_raw).hexdigest()}
    ages = {name: int(os.environ.get(variable, '86400')) for name, variable in [
        ('review_max_age_seconds', 'ENGARG_REVIEW_MAX_AGE_SECONDS'),
        ('acquisition_max_age_seconds', 'ENGARG_ACQUISITION_MAX_AGE_SECONDS')]}
    if any(not 1 <= age <= 315_360_000 for age in ages.values()):
        raise ValueError('Freshness ages must be positive seconds through 315360000')
    (directory / 'collector-inputs.snapshot.json').write_bytes(original)
    if catalogue_raw is not None:
        (directory / 'collectors.snapshot.json').write_bytes(catalogue_raw)
    if profile_raw is not None:
        (directory / 'toolchain.snapshot.json').write_bytes(profile_raw)
    write('request.json', request)
    write('export-policy.json', dict(schema='engarg.export-policy/1', **ages, acquisition_per_need={}))
    write('input-identity.json', {
        'inputs_sha256': hashlib.sha256(original).hexdigest(),
        'catalogue_sha256': hashlib.sha256(catalogue_raw).hexdigest() if catalogue_raw is not None else None,
        'catalogue_selection': 'external' if catalogue_raw is not None else 'engarg built-in',
        'search_window_utc': window,
        'operator_inputs': str(inputs_path), 'operator_catalogue': str(catalogue_path) if catalogue_raw is not None else None,
        'tool_implementation': implementation, 'evidence_planning': planning, 'analysis_reuse': reuse,
        'simple_example': simple_example, 'debate': debate,
        'example_qualification_probes_sha256': example_probes_sha256
    })
except (KeyError, TypeError, ValueError, OSError, OverflowError):
    raise SystemExit('Input preflight failed. Check scope fields, exact query, time window, evidence planning configuration and connections, catalogue, toolchain, implementation budgets and freshness ages.')
PY

if [[ $check_only -eq 1 ]]; then
  printf 'Input preflight passed. No build, inference or evidence acquisition performed. Artifacts: %s\n' "$run_dir"
  exit 0
fi
# A full run identifies its build by commit. Input-only checks remain available
# while tracked edits are staged or unstaged.
set_stage 'full-run prerequisites'
git diff --quiet HEAD -- || fail 'Full runs require a clean tracked checkout. --check-only accepts tracked changes; commit or stash them before inference.'
model=${ENGARG_MODEL:-gpt-6.1-sol}
service_tier=${ENGARG_SERVICE_TIER:-default}
reasoning_effort=${ENGARG_REASONING_EFFORT:-high}
repairs=${ENGARG_MAX_REPAIRS:-1}
[[ $repairs =~ ^[0-3]$ ]] || fail 'ENGARG_MAX_REPAIRS must be 0, 1, 2 or 3.'
minimum_calls=$(python3 - "$run_dir/input-identity.json" "$repairs" <<'PY'
import json, pathlib, sys
debate = json.loads(pathlib.Path(sys.argv[1]).read_text()).get('debate')
print((debate['participants'] * (debate['rounds'] + 1) + 3 if debate else 6) + int(sys.argv[2]))
PY
)
max_calls=${ENGARG_MAX_CALLS:-$minimum_calls}
[[ $max_calls =~ ^[1-9][0-9]{0,9}$ && $max_calls -le 4294967295 && $max_calls -ge $minimum_calls ]] ||
  fail "ENGARG_MAX_CALLS must reserve at least $minimum_calls calls for the selected topology and repairs."
request_timeout=${ENGARG_REQUEST_TIMEOUT_SECS:-1800}
run_timeout=${ENGARG_TIMEOUT_SECS:-10800}
input_bytes=${ENGARG_MAX_INPUT_BYTES:-1048576}
response_bytes=${ENGARG_MAX_RESPONSE_BYTES:-16777216}
if [[ -n ${ENGARG_MAX_OUTPUT_TOKENS:-} ]]; then
  output_tokens=$ENGARG_MAX_OUTPUT_TOKENS
elif [[ $model == gpt-6.1-sol ]]; then
  # Published model limit, checked 2026-10-25. Deployment aliases and other
  # models need their own explicit limit rather than a guessed family match.
  output_tokens=128000
else
  fail 'Set ENGARG_MAX_OUTPUT_TOKENS to the supported maximum for the selected ENGARG_MODEL. The built-in maximum applies only to gpt-6.1-sol.'
fi
[[ $output_tokens =~ ^[1-9][0-9]{0,9}$ && $output_tokens -le 4294967295 ]] ||
  fail 'ENGARG_MAX_OUTPUT_TOKENS must be a positive 32-bit integer within the selected provider model limit.'
if [[ $model == gpt-6.1-sol && $output_tokens -gt 128000 ]]; then
  fail 'gpt-6.1-sol supports at most 128000 output tokens per request.'
fi
argument_depth=${ENGARG_ARGUMENT_DEPTH:-}
if [[ -n $argument_depth ]]; then
  [[ $argument_depth =~ ^[1-9][0-9]?$ && $argument_depth -le 16 ]] ||
    fail 'ENGARG_ARGUMENT_DEPTH must be an integer from 1 through 16, or unset for automatic depth.'
fi
concurrent=${ENGARG_MAX_CONCURRENT_REQUESTS:-3}
eal_timeout=${ENGARG_EAL_TIMEOUT_SECS:-30}
eal_collection_timeout=${ENGARG_EAL_COLLECTION_TIMEOUT_SECS:-600}
[[ $eal_collection_timeout =~ ^[1-9][0-9]{0,3}$ && $eal_collection_timeout -le 3600 ]] ||
  fail 'ENGARG_EAL_COLLECTION_TIMEOUT_SECS must be an integer from 1 through 3600.'
# Set concurrency to 1 to pace the specialists. Request deadlines include queue waits.
# Three retries permit up to four HTTP attempts per logical call, at most 28 with defaults.

for required in cargo rustc; do command -v "$required" >/dev/null || fail "Required command for a full run: $required"; done
if [[ -z $replan_analysis || $implement_tools -eq 1 ]]; then
  : "${OPENAI_API_KEY:?Export OPENAI_API_KEY before running inference}"
fi
if [[ $prepare_rust_image -eq 1 && $evidence_planning == flat ]]; then
  set_stage 'Rust tooling prerequisites'
  command -v docker >/dev/null || fail 'Docker is required with --implement-tools. Install and start Docker before generating Rust collectors.'
  docker info >/dev/null || fail 'Docker must be running before generating Rust collectors.'
fi
if [[ -z $eal_host ]]; then
  set_stage 'EAL runtime installation'
  eal_host=$(bash "$repo/scripts/install-eal-runtime.sh")
fi
set_stage 'full-run prerequisites'
[[ -n $eal_host ]] || fail 'The EAL runtime installation did not return a host executable.'
eal_host=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "$eal_host")
printf 'Selected EAL host (%s): %s\n' "$eal_host_selection" "$eal_host" >&2
[[ -f $eal_host && -x $eal_host && -f $(dirname "$eal_host")/eal-mcp && -x $(dirname "$eal_host")/eal-mcp ]] ||
  fail 'eal-host and its sibling eal-mcp must both be regular executable files.'
# The server resolves its worker and supervisor beside its own physical binary.
# Follow an operator-installed eal-mcp alias, preserving the runtime's legacy
# supervisor preference when both companion names are installed.
eal_runtime_dir=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).with_name("eal-mcp").resolve().parent)' "$eal_host")
if [[ -n $topology || -n $topology_config ]]; then
  eal_coordinate=${eal_coordinate:-$eal_runtime_dir/eal-coordinate}
  coordination_run=${coordination_run:-$run_dir/coordination}
  [[ -f $eal_coordinate && -x $eal_coordinate ]] || fail "Missing executable EAL coordinator: $eal_coordinate"
fi
[[ -f $eal_runtime_dir/eal-method-worker && -x $eal_runtime_dir/eal-method-worker ]] ||
  fail "Missing executable EAL companion: $eal_runtime_dir/eal-method-worker. Install the pinned eal-runtime package; see README.md."
eal_supervisor="$eal_runtime_dir/eal-process-supervisor"
if [[ -f $eal_runtime_dir/eal-command-supervisor ]]; then eal_supervisor="$eal_runtime_dir/eal-command-supervisor"; fi
[[ -f $eal_supervisor && -x $eal_supervisor ]] ||
  fail "Missing executable EAL companion: $eal_supervisor. Install the pinned EAL runtime/process packages; see README.md."
if [[ $evidence_planning == graph ]]; then
  set_stage 'evidence collector admission'
  [[ -f $eal_runtime_dir/eal-collector && -x $eal_runtime_dir/eal-collector ]] ||
    fail "Missing executable EAL companion: $eal_runtime_dir/eal-collector. Install the coherent pinned runtime including eal-collector."
  python3 - "$run_dir/evidence-config.json" "$eal_runtime_dir/eal-collector" <<'PY'
import hashlib, json, pathlib, sys
class ExactNumber(str):
    pass
def exact_json(value):
    if isinstance(value, ExactNumber):
        return str(value)
    if isinstance(value, dict):
        return '{' + ','.join(json.dumps(key, ensure_ascii=False) + ':' + exact_json(item) for key, item in value.items()) + '}'
    if isinstance(value, list):
        return '[' + ','.join(map(exact_json, value)) + ']'
    return json.dumps(value, ensure_ascii=False, allow_nan=False)
path, selected = map(pathlib.Path, sys.argv[1:])
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
PY
fi

set_stage 'release build'
cargo build --locked --release --bin engarg --bin engarg-process-supervisor
target_dir=$(cargo metadata --locked --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
engarg_bin="$target_dir/release/engarg"
[[ -x $engarg_bin && -x $target_dir/release/engarg-process-supervisor ]] || fail 'Release companion binaries are missing.'

python3 - "$run_dir" "$engarg_bin" "$eal_host" "$eal_host_selection" "$model" "$service_tier" "$reasoning_effort" \
  "$max_calls" "$repairs" "$request_timeout" "$run_timeout" "$input_bytes" "$response_bytes" "$output_tokens" "$concurrent" "$eal_timeout" "$argument_depth" "$eal_collection_timeout" <<'PY'
import hashlib, json, pathlib, subprocess, sys
directory, executable, host = map(pathlib.Path, sys.argv[1:4])
def output(*args): return subprocess.check_output(args, text=True).strip()
record = dict(zip(['model','service_tier','reasoning_effort','max_calls','max_repairs',
    'request_timeout_seconds','run_timeout_seconds','max_input_bytes','max_response_bytes',
    'max_output_tokens','max_concurrent_requests','eal_timeout_seconds','argument_depth','eal_collection_timeout_seconds'], sys.argv[5:]))
record['argument_depth'] = int(record['argument_depth']) if record['argument_depth'] else None
record.update(git_commit=output('git','rev-parse','HEAD'), rustc=output('rustc','-vV'),
    eal_host_selection=sys.argv[4], eal_host_path=str(host),
    eal_mcp_path=str(host.with_name('eal-mcp').resolve()),
    engarg_binary_sha256=hashlib.sha256(executable.read_bytes()).hexdigest(),
    eal_host_binary_sha256=hashlib.sha256(host.read_bytes()).hexdigest(),
    eal_mcp_binary_sha256=hashlib.sha256(host.with_name('eal-mcp').read_bytes()).hexdigest(),
    api='responses', http_retries=3, retry_max_delay_seconds=120)
record['tool_implementation'] = json.loads((directory/'input-identity.json').read_text())['tool_implementation']
record['evidence_planning'] = json.loads((directory/'input-identity.json').read_text())['evidence_planning']
record['analysis_reuse'] = json.loads((directory/'input-identity.json').read_text())['analysis_reuse']
record['debate'] = json.loads((directory/'input-identity.json').read_text()).get('debate')
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
PY

# Initialise the actual server and inspect its required authoring interface
# before any paid inference.
set_stage 'EAL host startup preflight'
host_report="$run_dir/eal-preflight.json"
"$engarg_bin" eal describe --eal-host "$eal_host" --workspace "$run_dir" \
  --timeout-secs "$eal_timeout" --output "$host_report"
python3 - "$host_report" <<'PY'
import json, sys
with open(sys.argv[1], encoding='utf-8') as source:
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
PY
host_report=''

if [[ $prepare_rust_image -eq 1 && $evidence_planning == flat ]]; then
  set_stage 'Rust tooling image preparation'
  docker build --tag engarg-rust-tools:1.99.1 "$repo/examples/toolchains/rust"
fi

# Append the optional catalogue only when selected. Omitting it preserves the
# executable's built-in contracts rather than manufacturing an approved file.
with_catalogue() {
  if [[ -n $catalogue ]]; then
    "$engarg_bin" "$@" --collector-catalogue "$run_dir/collectors.snapshot.json"
  else
    "$engarg_bin" "$@"
  fi
}
run_analysis() {
  if [[ -n $replan_analysis ]]; then
    set_stage 'retained analysis route planning'
    "$engarg_bin" eal replan --input "$run_dir/retained-analysis.snapshot.json" \
      --evidence-config "$run_dir/evidence-config.json" --output "$analysis_output"
    return
  fi
  if [[ $evidence_planning == graph ]]; then
    authoring_checkpoint="$run_dir/authoring.checkpoint.json"
    set -- "$@" --evidence-planning graph --evidence-config "$run_dir/evidence-config.json" \
      --checkpoint "$authoring_checkpoint"
  fi
  if [[ -n $topology || -n $topology_config ]]; then
    if [[ -n $topology_config ]]; then
      set -- "$@" --topology-config "$run_dir/topology.snapshot.json"
    else
      set -- "$@" --topology "$topology"
      if [[ -n $debate_participants ]]; then set -- "$@" --debate-participants "$debate_participants"; fi
      if [[ -n $debate_rounds ]]; then set -- "$@" --debate-rounds "$debate_rounds"; fi
    fi
    set -- "$@" --eal-coordinate "$eal_coordinate" --coordination-run "$coordination_run"
  fi
  if [[ -n $argument_depth ]]; then
    set -- "$@" --argument-depth "$argument_depth"
  fi
  if [[ $implement_tools -eq 1 && $evidence_planning == flat ]]; then
    set -- "$@" --implement-tools --tool-language "$tool_language" \
      --toolchain "$run_dir/toolchain.snapshot.json" --tool-store "$tool_store" \
      --tool-output-dir "$run_dir/tooling" --tool-max-calls "$tool_max_calls" \
      --tool-timeout-secs "$tool_timeout"
  fi
  with_catalogue analyse "$@"
}
set_stage 'model analysis'
analysis_output="$run_dir/analysis.json"
if [[ $implement_tools -eq 1 && $evidence_planning == flat ]]; then set_stage 'model analysis and tool implementation'; fi
if [[ $implement_tools -eq 1 && $evidence_planning == graph ]]; then analysis_output="$run_dir/analysis.original.json"; fi
authoring_pending=1
run_analysis --progress --input "$run_dir/request.json" \
  --model "$model" --service-tier "$service_tier" --reasoning-effort "$reasoning_effort" \
  --api responses --api-key-env OPENAI_API_KEY --mode hybrid \
  --max-calls "$max_calls" --max-repairs "$repairs" \
  --http-retries 3 --retry-max-delay-secs 120 --max-concurrent-requests "$concurrent" \
  --request-timeout-secs "$request_timeout" --timeout-secs "$run_timeout" \
  --max-input-bytes "$input_bytes" --max-output-tokens "$output_tokens" --max-response-bytes "$response_bytes" \
  --format json --audit "$run_dir/analysis.audit.jsonl" --output "$analysis_output"
authoring_pending=0

if [[ $implement_tools -eq 1 && $evidence_planning == graph ]]; then
  set_stage 'tool implementation gap admission'
  eligible_tool_gaps=$(python3 - "$analysis_output" "$run_dir/tooling-selection.json" <<'PY'
import json, pathlib, sys
report = json.loads(pathlib.Path(sys.argv[1]).read_text())
planning = report.get('evidence_planning')
routes = planning.get('routes') if isinstance(planning, dict) else None
if (not isinstance(planning, dict) or planning.get('schema') != 'engarg.support-planning/1'
        or not isinstance(routes, dict) or routes.get('schema') != 'engarg.evidence-routes/1'
        or not isinstance(routes.get('gaps'), list)
        or not isinstance(routes.get('discovery'), dict)
        or not isinstance(routes['discovery'].get('incomplete'), bool)):
    raise SystemExit('Tool implementation requires the typed graph route plan from analysis.')
kinds = {'operation_implementation', 'transformation_implementation', 'configuration',
         'source_unavailable', 'historical_unavailable', 'qualification', 'capability_mismatch',
         'clarification', 'judgement', 'discovery_incomplete'}
for gap in routes['gaps']:
    if (not isinstance(gap, dict) or not isinstance(gap.get('kind'), str) or gap['kind'] not in kinds
            or gap.get('implementation') is not None and not isinstance(gap['implementation'], dict)):
        raise SystemExit('Tool implementation route gaps have an invalid type or request.')
incomplete = routes['discovery']['incomplete'] or any(gap['kind'] == 'discovery_incomplete' for gap in routes['gaps'])
eligible = [] if incomplete else [
    {key: gap.get(key) for key in ['acquisition_id', 'requirement_id', 'kind']}
    for gap in routes['gaps'] if gap['kind'] in {'operation_implementation', 'transformation_implementation'}
    and isinstance(gap.get('implementation'), dict)
]
pathlib.Path(sys.argv[2]).write_text(json.dumps({
    'schema': 'engarg.tooling-selection/1', 'discovery_incomplete': incomplete,
    'eligible_operation_gaps': eligible, 'eligible_count': len(eligible),
    'note': 'Configuration, credentials, historical availability, qualification and judgement do not admit implementation.'
}, indent=2) + '\n')
print(len(eligible))
PY
)
  if [[ $eligible_tool_gaps -gt 0 && $prepare_rust_image -eq 1 ]]; then
    set_stage 'Rust tooling prerequisites'
    command -v docker >/dev/null || fail 'Docker is required to implement the identified missing operations. The original analysis has been retained.'
    docker info >/dev/null || fail 'Docker must be running to implement the identified missing operations.'
    set_stage 'Rust tooling image preparation'
    docker build --tag engarg-rust-tools:1.99.1 "$repo/examples/toolchains/rust"
  fi
  set_stage 'tool implementation'
  "$engarg_bin" eal implement --progress --input "$analysis_output" \
    --model "$model" --service-tier "$service_tier" --reasoning-effort "$reasoning_effort" \
    --api responses --api-key-env OPENAI_API_KEY \
    --http-retries 3 --retry-max-delay-secs 120 --request-timeout-secs "$request_timeout" \
    --max-input-bytes "$input_bytes" --max-output-tokens "$output_tokens" --max-response-bytes "$response_bytes" \
    --tool-language "$tool_language" --toolchain "$run_dir/toolchain.snapshot.json" \
    --tool-store "$tool_store" --tool-output-dir "$run_dir/tooling" \
    --tool-max-calls "$tool_max_calls" --tool-timeout-secs "$tool_timeout"
  python3 - "$analysis_output" "$run_dir/tooling/analysis.with-tools.json" "$run_dir/analysis.json" <<'PY'
import json, pathlib, sys
original, enriched, final = map(pathlib.Path, sys.argv[1:])
def exact(path):
    # A tuple distinguishes number tokens from all JSON arrays/objects/strings.
    return json.loads(path.read_text(), parse_int=lambda value: ('number', value),
                      parse_float=lambda value: ('number', value))
expected, actual = exact(original), exact(enriched)
tooling = actual.pop('tooling', None)
if (actual != expected or not isinstance(tooling, dict)
        or tooling.get('schema') != 'engarg.capability-tooling/1'):
    raise SystemExit('Tool implementation changed the original analysis or omitted capability metadata.')
with final.open('xb') as output:
    output.write(enriched.read_bytes())
PY
fi

simple_example=$(python3 - "$run_dir/input-identity.json" <<'PY'
import json, pathlib, sys
print(1 if json.loads(pathlib.Path(sys.argv[1]).read_text()).get('simple_example') else 0)
PY
)
if [[ $simple_example -eq 1 ]]; then
  set_stage 'bounded catalogue operation qualification'
  qualifications=$(python3 "$repo/scripts/catalogue_evidence_example.py" qualify "$run_dir" "$engarg_bin")
  set_stage 'generated catalogue tool registration'
  "$engarg_bin" eal register-tools --input "$run_dir/analysis.json" \
    --qualifications "$qualifications" --evidence-config "$run_dir/evidence-config.json" \
    --output "$run_dir/registered-planning.json"
  set_stage 'qualified catalogue route replanning'
  "$engarg_bin" eal replan --input "$run_dir/analysis.json" \
    --evidence-config "$run_dir/registered-planning.json" --output "$run_dir/analysis.qualified.json"
  mv "$run_dir/analysis.json" "$run_dir/analysis.with-tools.json"
  mv "$run_dir/analysis.qualified.json" "$run_dir/analysis.json"
fi

python3 - "$run_dir" <<'PY'
import json, pathlib, sys
directory = pathlib.Path(sys.argv[1])
report = json.loads((directory/'analysis.json').read_text())
expected = json.loads((directory/'request.json').read_text())
if report['request'] != expected:
    raise SystemExit('Saved analysis changed the supplied request')
(directory/'case.json').write_text(json.dumps(report['argument'], indent=2)+'\n')
PY
set_stage 'argument validation'
"$engarg_bin" validate --input "$run_dir/request.json" --case "$run_dir/case.json"
set_stage 'argument rendering'
"$engarg_bin" render --input "$run_dir/analysis.json" --format markdown --no-audit-appendix --output "$run_dir/argument.md"
"$engarg_bin" render --input "$run_dir/analysis.json" --format argument --outline-depth 3 --display-depth 2 --output "$run_dir/argument.txt"
"$engarg_bin" render --input "$run_dir/analysis.json" --format prose --display-depth 2 --output "$run_dir/prose.md"
"$engarg_bin" render --input "$run_dir/analysis.json" --format html --display-depth 2 --output "$run_dir/argument.html"
set_stage 'EAL export'
if [[ $evidence_planning == graph ]]; then
  "$engarg_bin" eal export --input "$run_dir/analysis.json" --export-policy "$run_dir/export-policy.json" \
    --output "$run_dir/argument.eal" --tools-output "$run_dir/argument.tools.toml" --manifest "$run_dir/argument.bundle.json"
else
  set_stage 'current-case collector preparation and EAL export'
  with_catalogue eal export --input "$run_dir/analysis.json" \
    --export-policy "$run_dir/export-policy.json" \
    --operator-inputs "$run_dir/collector-inputs.snapshot.json" \
    --operator-input-base "$(dirname "$inputs")" \
    --preparation-output-dir "$run_dir/collector-inputs" \
    --output "$run_dir/argument.eal" --tools-output "$run_dir/argument.tools.toml" --manifest "$run_dir/argument.bundle.json"
fi
# Prepared stubs are intentionally not passed as an executable registry to the host.
set_stage 'EAL validation'
host_report="$run_dir/eal-validation.json"
"$engarg_bin" eal validate --input "$run_dir/argument.bundle.json" --input-format json \
  --eal-host "$eal_host" --workspace "$run_dir" --timeout-secs "$eal_timeout" --output "$host_report"
set_stage 'EAL evidence planning'
host_report="$run_dir/evidence-plan.json"
"$engarg_bin" eal plan --input "$run_dir/argument.bundle.json" --input-format json --claim current_claim \
  --eal-host "$eal_host" --workspace "$run_dir" --timeout-secs "$eal_timeout" --output "$host_report"
host_report=''
if [[ $collect_and_assess -eq 1 ]]; then
  set_stage 'EAL evidence collection'
  host_report="$run_dir/evidence-collection.json"
  "$engarg_bin" eal collect --input "$run_dir/argument.bundle.json" --input-format json --claim current_claim \
    --eal-host "$eal_host" --workspace "$run_dir" --database "$run_dir/eal.sqlite" \
    --tools "$run_dir/argument.tools.toml" --timeout-secs "$eal_collection_timeout" --output "$host_report"
  collection_id=$(python3 - "$host_report" <<'PY'
import json, pathlib, sys
result = json.loads(pathlib.Path(sys.argv[1]).read_text())
identifier = result.get('result', {}).get('collection_id')
if result.get('is_error') is not False or not isinstance(identifier, str) or not identifier:
    raise SystemExit('EAL collection did not return a collection identifier. Inspect evidence-collection.json.')
print(identifier)
PY
)
  set_stage 'EAL argument assessment'
  host_report="$run_dir/eal-assessment.json"
  assessed_at=$(python3 -c 'import datetime; print(datetime.datetime.now(datetime.timezone.utc).isoformat())')
  "$engarg_bin" eal assess --input "$run_dir/argument.bundle.json" --input-format json \
    --eal-host "$eal_host" --workspace "$run_dir" --database "$run_dir/eal.sqlite" \
    --tools "$run_dir/argument.tools.toml" --timeout-secs "$eal_collection_timeout" --now "$assessed_at" \
    --collection-id "$collection_id" --output "$host_report"
  python3 - "$run_dir" <<'PY'
import json, pathlib, sys
directory = pathlib.Path(sys.argv[1])
bundle = json.loads((directory/'argument.bundle.json').read_text())
collection = json.loads((directory/'evidence-collection.json').read_text())
envelope = json.loads((directory/'eal-assessment.json').read_text())
assessment = envelope.get('result', {})
claim_name = bundle['claim_names']['current_claim']
claim = assessment.get('claims', {}).get(claim_name)
if envelope.get('is_error') is not False or not isinstance(claim, dict):
    raise SystemExit('EAL assessment did not return the current claim. Inspect eal-assessment.json.')
valid = assessment.get('valid') is True
complete = assessment.get('complete') is True
supported = valid and complete and claim.get('status') == 'supported'
reference_check = None
if json.loads((directory/'input-identity.json').read_text()).get('simple_example'):
    sys.path.insert(0, str(directory.parent.parent/'scripts'))
    from catalogue_evidence_example import verify_observations
    try:
        reference_check = verify_observations(directory)
    except (OSError, KeyError, ValueError, TypeError) as error:
        reference_check = {'passed': False, 'reason': str(error)}
        supported = False
execution = {
    'schema': 'engarg.catalog-sync-execution/1',
    'status': 'supported' if supported else 'unresolved',
    'valid': valid, 'complete': complete, 'claim_status': claim.get('status'),
    'claim_name': claim_name, 'claim': claim,
    'collection_id': collection['result']['collection_id'],
    'assessment_id': assessment.get('assessment_id'),
    'independent_reference_check': reference_check,
    'evidence_collected': any(record.get('status') == 'ok' for record in collection['result'].get('records', {}).values()),
    'measured_negative_evidence': [name for name, record in collection['result'].get('records', {}).items()
                                   if (record.get('value') or {}).get('criterion_satisfied') is False],
    'unavailable_evidence': [name for name, record in collection['result'].get('records', {}).items()
                             if record.get('status') != 'ok'],
    'scope': 'The declared formal method, evidence bindings, assumptions and selected observation set. This outcome does not establish arbitrary prose or external facts beyond those contracts.',
    'artifacts': {'collection': str(directory/'evidence-collection.json'), 'assessment': str(directory/'eal-assessment.json')},
}
(directory/'execution-result.json').write_text(json.dumps(execution, indent=2)+'\n')
PY
  host_report=''
fi
set_stage 'run summary'
python3 - "$run_dir" <<'PY'
import json, pathlib, sys
directory = pathlib.Path(sys.argv[1])
for filename in ['eal-preflight.json','eal-validation.json','evidence-plan.json']:
    result = json.loads((directory/filename).read_text())
    if result.get('is_error') is not False:
        raise SystemExit('EAL host reported a failed operation: '+filename)
    if filename == 'eal-validation.json':
        validation = result.get('result')
        if not isinstance(validation, dict) or validation.get('valid') is not True:
            raise SystemExit('Exported EAL failed validation or returned an unsupported validation envelope: '+filename)
report = json.loads((directory/'analysis.json').read_text())
bundle = json.loads((directory/'argument.bundle.json').read_text())
identity = json.loads((directory/'input-identity.json').read_text())
execution_path = directory/'execution-result.json'
completed_execution = json.loads(execution_path.read_text()) if execution_path.exists() else None
def retained_tooling(value):
    for _ in range(17):
        tooling = value.get('tooling')
        if isinstance(tooling, dict):
            return tooling
        provenance = value.get('replanning')
        if not isinstance(provenance, dict):
            return None
        value = provenance.get('previous_extensions', {})
    raise SystemExit('Tooling provenance exceeds its retained history bound')
summary_common = {
    'evidence_collected': completed_execution['evidence_collected'] if completed_execution else False,
    'eal_execution': completed_execution,
    'argument_status': report['argument']['status'],
    'recorded_model_calls': len(report.get('transcript', [])),
    'model_calls_this_run': 0 if identity['analysis_reuse'] else len(report.get('transcript', [])),
    'analysis_reuse': identity['analysis_reuse'],
    'presentations': {name: str(directory / filename) for name, filename in {
        'json': 'analysis.json', 'markdown': 'argument.md', 'text': 'argument.txt',
        'html': 'argument.html', 'prose': 'prose.md'
    }.items()},
}
if identity['evidence_planning']['mode'] == 'graph':
    planning = report.get('evidence_planning')
    try:
        exported = json.loads((directory/'argument.evidence-plan.json').read_text())
    except (OSError, ValueError):
        raise SystemExit('Graph evidence plan is missing or invalid: argument.evidence-plan.json')
    if (not isinstance(planning, dict) or planning.get('schema') != 'engarg.support-planning/1'
            or bundle.get('schema_version') != 'engarg.support-export/1'
            or exported.get('schema') != 'engarg.support-evidence-plan/1'
            or exported.get('case_digest') != bundle.get('authored', {}).get('revision')
            or exported.get('execution_plan_sha256') != bundle.get('execution_plan_sha256')
            or exported.get('support_elaboration') != planning.get('elaboration')
            or exported.get('routes') != planning.get('routes')):
        raise SystemExit('Graph evidence planning must retain the exact argument, requirements and execution-plan identity')
    routes = planning['routes']
    requirements = planning['elaboration']['requirements']
    acquisitions = routes['routes']
    authored_acquisitions = planning['elaboration'].get('acquisitions', [])
    gaps = routes['gaps']
    design_review = planning.get('design_review')
    if not isinstance(design_review, dict):
        design_review = None
    pending_designs = design_review.get('pending_designs', []) if design_review else []
    empirical_gap_kinds = {'source_unavailable', 'historical_unavailable', 'clarification',
                          'operation_implementation', 'transformation_implementation'}
    if design_review and design_review.get('status') == 'pending_repair':
        design_status = 'pending_repair'
    elif authored_acquisitions:
        design_status = 'represented'
    elif any(gap.get('kind') in empirical_gap_kinds for gap in gaps):
        design_status = 'pending_design'
    elif requirements:
        design_status = 'judgement_only'
    else:
        design_status = 'no_acquisition_design'
    # Route entries describe compiled choices. Historical logical_acquisitions
    # counted these entries, so retain it as an explicitly documented alias.
    selected_routes = [route for route in acquisitions if route.get('selected')]
    ready_routes = [route for route in selected_routes if not route.get('configuration_gaps')]
    execution = routes.get('execution', {})
    steps = {step['id']: step for step in execution.get('acquisitions', [])}
    derivations = {step['id']: step for step in execution.get('derivations', [])}
    ready_ids = {route['acquisition_id'] for route in ready_routes}
    dependencies = {}
    for step_id, step in steps.items():
        references = [binding.get('output', {}) for binding in step.get('bindings', [])]
        fanout = step.get('for_each')
        if fanout:
            references.append(fanout.get('output', {}))
            witness = fanout.get('stop_on_witness') or {}
            if witness.get('equals_output'):
                references.append(witness['equals_output'])
        dependencies[step_id] = {reference.get('step_id') for reference in references}
    for step_id, step in derivations.items():
        dependencies[step_id] = {reference.get('step_id') for reference in step.get('inputs', [])}
    # Resolve source and derivation availability without recursive traversal.
    # An unknown reference or a cycle remains blocked. A downstream source needs
    # every source behind data, discovery and witness-control dependencies.
    available = set()
    nodes = set(dependencies) | ready_ids
    while True:
        before = len(available)
        for step_id in nodes - available:
            if step_id not in derivations and step_id not in ready_ids:
                continue
            if dependencies.get(step_id, set()) <= available:
                available.add(step_id)
        if len(available) == before:
            break
    ready_ids &= available
    if routes['discovery']['incomplete']:
        ready_ids.clear()
    selection_status = ('not_requested' if not acquisitions else
                        'complete' if len(selected_routes) == len(acquisitions) else
                        'partial' if selected_routes else 'unavailable')
    source_status = ('ready' if acquisitions and len(ready_ids) == len(acquisitions) else
                     'partial' if ready_ids else 'blocked' if acquisitions else 'no_acquisitions')
    budget = exported.get('collection_budget')
    if (not isinstance(budget, dict) or budget.get('schema') != 'engarg.collection-budget/1'
            or type(budget.get('assumed_default_limit')) is not int or budget['assumed_default_limit'] < 1
            or type(budget.get('total_emitted_evidence')) is not int or budget['total_emitted_evidence'] < 0
            or not isinstance(budget.get('claim_evidence_counts'), dict)
            or any(type(count) is not int or count < 0 for count in budget['claim_evidence_counts'].values())):
        raise SystemExit('Graph evidence plan requires valid collection-budget counts and an explicit default assumption')
    selected_limit = budget['assumed_default_limit']
    limit_source = 'exported.collection_budget.assumed_default_limit'
    limit_is_assumed = True
    description = json.loads((directory/'eal-preflight.json').read_text())['result']
    # The current EAL service publishes execution_limits. Accept budgets as a
    # compatibility alias, while retaining the exact source used.
    for field in ['execution_limits', 'budgets']:
        advertised = description.get(field)
        if isinstance(advertised, dict) and 'collection_evidence' in advertised:
            selected_limit = advertised['collection_evidence']
            if type(selected_limit) is not int or selected_limit < 1:
                raise SystemExit('EAL host collection_evidence budget must be a positive integer')
            limit_source = 'eal-preflight.result.' + field + '.collection_evidence'
            limit_is_assumed = False
            break
    collection_admission = {
        'selection': 'all_emitted_evidence',
        'evidence_count': budget['total_emitted_evidence'],
        'limit': selected_limit, 'limit_source': limit_source,
        'limit_is_assumed': limit_is_assumed,
        'within_limit': budget['total_emitted_evidence'] <= selected_limit,
    }
    readiness = {
        'schema': 'engarg.graph-readiness/1',
        'case_digest': exported['case_digest'],
        'execution_plan_sha256': exported['execution_plan_sha256'],
        'requirements': len(requirements),
        'logical_acquisitions': len(acquisitions),
        'logical_acquisitions_meaning': 'compatibility alias for compiled_acquisition_routes',
        'authored_logical_acquisitions': len(authored_acquisitions),
        'compiled_acquisition_routes': len(acquisitions),
        'acquisition_design': {'status': design_status, 'review': design_review,
                               'pending_designs': len(pending_designs)},
        'capability_selection': {'status': selection_status,
                                 'selected_routes': len(selected_routes)},
        'source_collection': {'status': source_status,
                              'ready_acquisition_ids': sorted(ready_ids),
                              'blocked_acquisition_ids': sorted({route['acquisition_id'] for route in acquisitions} - ready_ids),
                              'scope': 'configured compiled source routes and their dependencies; independent judgement remains pending'},
        'selected_capabilities': sorted({route['selected'] for route in acquisitions if route.get('selected')}),
        'routes': acquisitions, 'gaps': gaps, 'pending': exported['pending'],
        'collection_budget': budget, 'collection_admission': collection_admission,
        'discovery_incomplete': routes['discovery']['incomplete'],
        'ready_for_collection': (collection_admission['within_limit'] and source_status == 'ready'
                                 and design_status not in {'pending_repair', 'pending_design'}
                                 and not any(gap.get('kind') != 'judgement' for gap in gaps)),
        'evidence_collected': False, 'evidence_truth_verified': False, 'inferential_soundness_verified': False,
        'independent_review_obligations': len(exported['independent_review_obligations']),
        'next_step': 'Resolve typed configuration, availability, qualification and clarification gaps before explicit collection; independent judgement remains separate.'
    }
    if design_status in {'pending_repair', 'pending_design'}:
        readiness['next_step'] = ('Repair the retained acquisition design before route replanning. '
            'Review pending designs and the saved transcript; completed authoring calls remain retained.')
    if not collection_admission['within_limit']:
        budget_next = ('Default collect-all selects %s emitted evidence declarations against limit %s from %s. '
            'Review the selected host budget or explicitly select a bounded claim before collection; the complete graph and independent reviews remain retained.' % (
                budget['total_emitted_evidence'], selected_limit, limit_source))
        readiness['next_step'] = ((readiness['next_step'] + ' ' + budget_next)
            if design_status in {'pending_repair', 'pending_design'} else budget_next)
    summary = {
        **summary_common,
        'completed_phase': ('retained argument route replanning and deterministic export validation' if identity['analysis_reuse']
                            else 'argument authoring, independent support extraction, qualified route planning and deterministic export validation'),
        'evidence_planning': 'graph', 'graph_readiness': readiness,
        'stage_status': {'argument_validation': 'passed', 'artifact_generation': 'passed',
                         'acquisition_design': design_status, 'capability_selection': selection_status,
                         'source_collection': source_status,
                         'evidence_assessment': completed_execution['status'] if completed_execution else 'not_performed'},
        'next_step': readiness['next_step'],
        'tool_implementation': {'enabled': identity['tool_implementation']['enabled']},
    }
    summary['stage_status']['tool_implementation'] = 'not_requested'
    if identity['tool_implementation']['enabled']:
        tooling = retained_tooling(report)
        if not isinstance(tooling, dict) or tooling.get('schema') != 'engarg.capability-tooling/1':
            raise SystemExit('Graph implementation was enabled but the analysis has no capability tooling report')
        summary['stage_status']['tool_implementation'] = (
            'function_contracts_verified' if tooling['capabilities'] else
            'deferred' if tooling['deferred_implementation_gaps'] else 'no_eligible_implementation')
        summary['tool_implementation'].update(
            language=tooling['language'], model_calls=tooling['model_calls'],
            verified_packages=len(tooling['capabilities']),
            reused_packages=sum(package['reused'] for package in tooling['capabilities']),
            skipped_non_implementation_gaps=tooling['skipped_non_implementation_gaps'],
            deferred_implementation_gaps=tooling['deferred_implementation_gaps'],
            audit_path=tooling['audit_path'], note=tooling['note'])
    (directory/'graph-readiness.json').write_text(json.dumps(readiness, indent=2)+'\n')
    (directory/'run-summary.json').write_text(json.dumps(summary, indent=2)+'\n')
    raise SystemExit(0)
plan = bundle.get('acquisition_plan') or {}
entries = plan.get('entries', [])
summary = {
    **summary_common,
    'completed_phase': 'argument authoring and deterministic export validation',
    'selected_collectors': sorted({entry['tool_id'] for entry in entries}),
    'unresolved_arguments': [{'need_id': entry['need_id'], 'arguments': entry['unresolved_arguments']}
        for entry in entries if entry['unresolved_arguments']],
    'pending_review_obligations': len(bundle.get('obligations', [])),
    'next_step': 'Review exact-query compatibility and operator bindings before explicit collection and assessment.'
}
preparation_path = directory/'collector-inputs/collector-preparation.json'
try:
    preparation = json.loads(preparation_path.read_text())
except (OSError, ValueError):
    raise SystemExit('Current-case collector preparation report is missing or invalid: '+str(preparation_path))
if (not isinstance(preparation, dict)
        or preparation.get('schema') != 'engarg.collector-preparation/1'
        or not isinstance(bundle.get('revision'), str)
        or len(bundle['revision']) != 64
        or any(character not in '0123456789abcdef' for character in bundle['revision'])
        or preparation.get('case_digest') != bundle.get('revision')
        or preparation.get('evidence_collected') is not False):
    raise SystemExit('Collector preparation must describe this exact export without claiming collection')
summary['collector_preparation'] = preparation
summary['next_step'] = (
    'Resolve each reported configuration or evidence gap before explicit collection and independent assessment.'
)
implementation = json.loads((directory/'input-identity.json').read_text())['tool_implementation']
summary['tool_implementation'] = {'enabled': implementation['enabled']}
if implementation['enabled']:
    tooling = report.get('tooling')
    if not isinstance(tooling, dict) or tooling.get('schema') != 'engarg.tooling/1':
        raise SystemExit('Implementation was enabled but the analysis has no tooling report')
    summary['tool_implementation'].update(
        language=tooling['language'], model_calls=tooling['model_calls'],
        verified_packages=len(tooling['packages']),
        reused_packages=sum(package['reused'] for package in tooling['packages']),
        audit_path=tooling['audit_path'], catalogue_path=str(directory/'tooling/collectors.json'))
    if tooling.get('coverage') is not None:
        summary['tool_implementation']['coverage'] = tooling['coverage']
(directory/'run-summary.json').write_text(json.dumps(summary, indent=2)+'\n')
PY
if [[ -n $replan_analysis ]]; then
  printf 'Route replanning, rendering, export and EAL planning passed. Artifacts: %s\n' "$run_dir"
else
  printf 'Authoring, rendering, export and EAL planning passed. Artifacts: %s\n' "$run_dir"
fi
if [[ $collect_and_assess -eq 1 ]]; then
  python3 - "$run_dir/execution-result.json" <<'PY'
import json, pathlib, sys
result = json.loads(pathlib.Path(sys.argv[1]).read_text())
print('EAL execution: %s; valid: %s; complete: %s; current claim: %s. Full collection and assessment are retained.' % (
    result['status'], result['valid'], result['complete'], result['claim_status']))
if result['status'] != 'supported':
    raise SystemExit(3)
PY
fi
if [[ $evidence_planning == graph ]]; then
  python3 - "$run_dir/graph-readiness.json" <<'PY'
import json, sys
with open(sys.argv[1], encoding='utf-8') as source:
    readiness = json.load(source)
print('Graph readiness: %s requirements; %s authored logical acquisitions; %s compiled acquisition routes; %s typed gaps; ready for collection: %s. Evidence truth and inferential soundness remain unverified.' % (
    readiness['requirements'], readiness['authored_logical_acquisitions'], readiness['compiled_acquisition_routes'], len(readiness['gaps']), readiness['ready_for_collection']))
print('Acquisition design: %s; capability selection: %s; source collection: %s; independent reviews pending: %s.' % (
    readiness['acquisition_design']['status'], readiness['capability_selection']['status'],
    readiness['source_collection']['status'], readiness['independent_review_obligations']))
if readiness['acquisition_design']['status'] in {'pending_repair', 'pending_design'}:
    print('engarg: warning: acquisition planning is incomplete. Retained designs and completed model calls are available for targeted repair; inspect analysis.json and graph-readiness.json.', file=sys.stderr)

budget = readiness['collection_admission']
print('Collection budget for all emitted evidence: %s/%s; source: %s; assumed limit: %s.' % (
    budget['evidence_count'], budget['limit'], budget['limit_source'], budget['limit_is_assumed']))
PY
else
  printf 'Review per-need bindings and independent review obligations before collection and assessment.\n'
  python3 - "$run_dir/collector-inputs/collector-preparation.json" <<'PY'
import json, sys
with open(sys.argv[1], encoding='utf-8') as source:
    preparation = json.load(source)
summary = preparation['summary']
print('Collector preparation: %s bound; %s unresolved; %s retained-artifact inputs verified. Evidence acquisition remains a separate step.' % (
    summary['bound'], summary['unresolved'], summary['retained_artifacts_verified']))
PY
fi
