#!/usr/bin/env bash
# Author a dev-cluster/Grafana claim from operator inputs. Bash 3.2+ and Python 3.
# Usage: bash scripts/run-catalog-sync-authoring.sh [OPTIONS] INPUTS.json [CATALOGUE.json]
# Defaults to independent support requirements and qualified capability planning.
# Preflight accepts tracked changes and needs no Rust tools, host or API key.
# Full runs need a clean checkout and Rustup; the pinned EAL runtime is cached locally.
# Authoring, rendering, export and planning do not collect or assess evidence.
set -euo pipefail
umask 077

fail() { printf '%s\n' "$*" >&2; exit 2; }
usage() {
  cat <<'HELP'
Usage: bash scripts/run-catalog-sync-authoring.sh [OPTIONS] INPUTS.json [CATALOGUE.json]
  --check-only                 Check inputs without build, Docker, host or inference.
  --evidence-config JSON        Required graph planning configuration (collector may be "managed").
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
evidence_planning='graph'
evidence_config=${ENGARG_EVIDENCE_CONFIG:-}
evidence_config_explicit=0
implement_tools=0
tool_options=0
tool_language='rust'
toolchain=${ENGARG_TOOLCHAIN:-}
tool_store=${ENGARG_TOOL_STORE:-}
tool_max_calls=${ENGARG_TOOL_MAX_CALLS:-16}
tool_timeout=${ENGARG_TOOL_TIMEOUT_SECS:-3600}
while [[ $# -gt 0 ]]; do
  case $1 in
    --check-only) check_only=1; shift ;;
    --legacy-flat) evidence_planning='flat'; shift ;;
    --evidence-config)
      [[ $# -ge 2 && -n $2 ]] || fail 'Missing value for --evidence-config'
      evidence_config=$2; evidence_config_explicit=1; shift 2 ;;
    --implement-tools) implement_tools=1; shift ;;
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
  evidence_config=''
fi
[[ $implement_tools -eq 1 || $tool_options -eq 0 ]] || fail 'Tool implementation options require --implement-tools.'
for required in git python3; do command -v "$required" >/dev/null || fail "Required command: $required"; done
repo=$(git rev-parse --show-toplevel) || fail 'Run from an EngArg checkout.'
# Keep operator paths relative to the invocation directory, including leaf-file
# symlinks so the bounded, no-follow admission below can reject them.
absolute_path() { python3 -c 'import os,sys; print(os.path.abspath(sys.argv[1]))' "$1"; }
inputs=$(absolute_path "$1")
[[ -e $inputs ]] || fail "Operator inputs do not exist: $inputs"
if [[ $evidence_planning == graph ]]; then
  evidence_config=$(absolute_path "$evidence_config")
  [[ -e $evidence_config ]] || fail "Evidence planning configuration does not exist: $evidence_config"
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
stage='input preflight'
host_report=''
on_exit() {
  status=$?
  if [[ $status -ne 0 ]]; then
    printf 'Run stopped with status %s during %s. Retained artifacts: %s\n' "$status" "$stage" "$run_dir" >&2
    if [[ -n $host_report && -s $host_report ]]; then
      printf 'Complete EAL host response: %s\n' "$host_report" >&2
      printf 'Inspect with: python3 -m json.tool %q\n' "$host_report" >&2
    fi
  fi
}
trap on_exit EXIT

# Read each operator file once, bound its size, reject duplicate keys and retain
# exact private snapshots. Only allowlisted scope fields enter the LLM request.
python3 - "$inputs" "$catalogue" "$run_dir" "$implement_tools" "$tool_language" \
  "$toolchain" "$tool_store" "$tool_max_calls" "$tool_timeout" "$evidence_planning" "$evidence_config" <<'PY'
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
    catalogue_raw, catalogue = read(catalogue_path) if sys.argv[2] else (None, None)
    planning = {'mode': sys.argv[10]}
    if planning['mode'] == 'graph':
        config_path = pathlib.Path(sys.argv[11])
        config_raw, config = read(config_path, limit=4_194_304)
        if (config.get('schema') != 'engarg.capability-planning-config/1'
                or set(config) - {'schema', 'collector', 'connections', 'store', 'environment', 'ranking', 'scheduling', 'bindings'}
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
        'tool_implementation': implementation, 'evidence_planning': planning
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
stage='full-run prerequisites'
git diff --quiet HEAD -- || fail 'Full runs require a clean tracked checkout. --check-only accepts tracked changes; commit or stash them before inference.'
model=${ENGARG_MODEL:-gpt-6.1-sol}
service_tier=${ENGARG_SERVICE_TIER:-default}
reasoning_effort=${ENGARG_REASONING_EFFORT:-high}
repairs=${ENGARG_MAX_REPAIRS:-1}
[[ $repairs =~ ^[0-3]$ ]] || fail 'ENGARG_MAX_REPAIRS must be 0, 1, 2 or 3.'
max_calls=$((6 + repairs))
request_timeout=${ENGARG_REQUEST_TIMEOUT_SECS:-1800}
run_timeout=${ENGARG_TIMEOUT_SECS:-10800}
input_bytes=${ENGARG_MAX_INPUT_BYTES:-1048576}
response_bytes=${ENGARG_MAX_RESPONSE_BYTES:-1048576}
output_tokens=${ENGARG_MAX_OUTPUT_TOKENS:-65536}
concurrent=${ENGARG_MAX_CONCURRENT_REQUESTS:-3}
eal_timeout=${ENGARG_EAL_TIMEOUT_SECS:-30}
# Set concurrency to 1 to pace the specialists. Request deadlines include queue waits.
# Three retries permit up to four HTTP attempts per logical call, at most 28 with defaults.

for required in cargo rustc; do command -v "$required" >/dev/null || fail "Required command for a full run: $required"; done
: "${OPENAI_API_KEY:?Export OPENAI_API_KEY before running inference}"
if [[ $prepare_rust_image -eq 1 && $evidence_planning == flat ]]; then
  stage='Rust tooling prerequisites'
  command -v docker >/dev/null || fail 'Docker is required with --implement-tools. Install and start Docker before generating Rust collectors.'
  docker info >/dev/null || fail 'Docker must be running before generating Rust collectors.'
fi
if [[ -z $eal_host ]]; then
  stage='EAL runtime installation'
  eal_host=$(bash "$repo/scripts/install-eal-runtime.sh")
fi
stage='full-run prerequisites'
[[ -n $eal_host ]] || fail 'The EAL runtime installation did not return a host executable.'
eal_host=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "$eal_host")
printf 'Selected EAL host (%s): %s\n' "$eal_host_selection" "$eal_host" >&2
[[ -f $eal_host && -x $eal_host && -f $(dirname "$eal_host")/eal-mcp && -x $(dirname "$eal_host")/eal-mcp ]] ||
  fail 'eal-host and its sibling eal-mcp must both be regular executable files.'
# The server resolves its worker and supervisor beside its own physical binary.
# Follow an operator-installed eal-mcp alias, preserving the runtime's legacy
# supervisor preference when both companion names are installed.
eal_runtime_dir=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).with_name("eal-mcp").resolve().parent)' "$eal_host")
[[ -f $eal_runtime_dir/eal-method-worker && -x $eal_runtime_dir/eal-method-worker ]] ||
  fail "Missing executable EAL companion: $eal_runtime_dir/eal-method-worker. Install the pinned eal-runtime package; see README.md."
eal_supervisor="$eal_runtime_dir/eal-process-supervisor"
if [[ -f $eal_runtime_dir/eal-command-supervisor ]]; then eal_supervisor="$eal_runtime_dir/eal-command-supervisor"; fi
[[ -f $eal_supervisor && -x $eal_supervisor ]] ||
  fail "Missing executable EAL companion: $eal_supervisor. Install the pinned EAL runtime/process packages; see README.md."
if [[ $evidence_planning == graph ]]; then
  stage='evidence collector admission'
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

stage='release build'
cargo build --locked --release --bin engarg --bin engarg-process-supervisor
target_dir=$(cargo metadata --locked --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
engarg_bin="$target_dir/release/engarg"
[[ -x $engarg_bin && -x $target_dir/release/engarg-process-supervisor ]] || fail 'Release companion binaries are missing.'

python3 - "$run_dir" "$engarg_bin" "$eal_host" "$eal_host_selection" "$model" "$service_tier" "$reasoning_effort" \
  "$max_calls" "$repairs" "$request_timeout" "$run_timeout" "$input_bytes" "$response_bytes" "$output_tokens" "$concurrent" "$eal_timeout" <<'PY'
import hashlib, json, pathlib, subprocess, sys
directory, executable, host = map(pathlib.Path, sys.argv[1:4])
def output(*args): return subprocess.check_output(args, text=True).strip()
record = dict(zip(['model','service_tier','reasoning_effort','max_calls','max_repairs',
    'request_timeout_seconds','run_timeout_seconds','max_input_bytes','max_response_bytes',
    'max_output_tokens','max_concurrent_requests','eal_timeout_seconds'], sys.argv[5:]))
record.update(git_commit=output('git','rev-parse','HEAD'), rustc=output('rustc','-vV'),
    eal_host_selection=sys.argv[4], eal_host_path=str(host),
    eal_mcp_path=str(host.with_name('eal-mcp').resolve()),
    engarg_binary_sha256=hashlib.sha256(executable.read_bytes()).hexdigest(),
    eal_host_binary_sha256=hashlib.sha256(host.read_bytes()).hexdigest(),
    eal_mcp_binary_sha256=hashlib.sha256(host.with_name('eal-mcp').read_bytes()).hexdigest(),
    api='responses', http_retries=3, retry_max_delay_seconds=120)
record['tool_implementation'] = json.loads((directory/'input-identity.json').read_text())['tool_implementation']
record['evidence_planning'] = json.loads((directory/'input-identity.json').read_text())['evidence_planning']
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
stage='EAL host startup preflight'
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
PY
host_report=''

if [[ $prepare_rust_image -eq 1 && $evidence_planning == flat ]]; then
  stage='Rust tooling image preparation'
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
  if [[ $evidence_planning == graph ]]; then
    set -- "$@" --evidence-planning graph --evidence-config "$run_dir/evidence-config.json"
  fi
  if [[ $implement_tools -eq 1 && $evidence_planning == flat ]]; then
    set -- "$@" --implement-tools --tool-language "$tool_language" \
      --toolchain "$run_dir/toolchain.snapshot.json" --tool-store "$tool_store" \
      --tool-output-dir "$run_dir/tooling" --tool-max-calls "$tool_max_calls" \
      --tool-timeout-secs "$tool_timeout"
  fi
  with_catalogue analyse "$@"
}
stage='model analysis'
analysis_output="$run_dir/analysis.json"
if [[ $implement_tools -eq 1 && $evidence_planning == flat ]]; then stage='model analysis and tool implementation'; fi
if [[ $implement_tools -eq 1 && $evidence_planning == graph ]]; then analysis_output="$run_dir/analysis.original.json"; fi
run_analysis --input "$run_dir/request.json" \
  --model "$model" --service-tier "$service_tier" --reasoning-effort "$reasoning_effort" \
  --api responses --api-key-env OPENAI_API_KEY --mode hybrid --argument-depth 2 \
  --max-calls "$max_calls" --max-repairs "$repairs" \
  --http-retries 3 --retry-max-delay-secs 120 --max-concurrent-requests "$concurrent" \
  --request-timeout-secs "$request_timeout" --timeout-secs "$run_timeout" \
  --max-input-bytes "$input_bytes" --max-output-tokens "$output_tokens" --max-response-bytes "$response_bytes" \
  --format json --audit "$run_dir/analysis.audit.jsonl" --output "$analysis_output"

if [[ $implement_tools -eq 1 && $evidence_planning == graph ]]; then
  stage='tool implementation gap admission'
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
    stage='Rust tooling prerequisites'
    command -v docker >/dev/null || fail 'Docker is required to implement the identified missing operations. The original analysis has been retained.'
    docker info >/dev/null || fail 'Docker must be running to implement the identified missing operations.'
    stage='Rust tooling image preparation'
    docker build --tag engarg-rust-tools:1.99.1 "$repo/examples/toolchains/rust"
  fi
  stage='tool implementation'
  "$engarg_bin" eal implement --input "$analysis_output" \
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

python3 - "$run_dir" <<'PY'
import json, pathlib, sys
directory = pathlib.Path(sys.argv[1])
report = json.loads((directory/'analysis.json').read_text())
expected = json.loads((directory/'request.json').read_text())
if report['request'] != expected:
    raise SystemExit('Saved analysis changed the supplied request')
(directory/'case.json').write_text(json.dumps(report['argument'], indent=2)+'\n')
PY
stage='argument validation'
"$engarg_bin" validate --input "$run_dir/request.json" --case "$run_dir/case.json"
stage='argument rendering'
"$engarg_bin" render --input "$run_dir/analysis.json" --format argument --outline-depth 3 --display-depth 2 --output "$run_dir/argument.txt"
"$engarg_bin" render --input "$run_dir/analysis.json" --format prose --display-depth 2 --output "$run_dir/prose.md"
"$engarg_bin" render --input "$run_dir/analysis.json" --format html --display-depth 2 --output "$run_dir/argument.html"
stage='EAL export'
if [[ $evidence_planning == graph ]]; then
  "$engarg_bin" eal export --input "$run_dir/analysis.json" --export-policy "$run_dir/export-policy.json" \
    --output "$run_dir/argument.eal" --tools-output "$run_dir/argument.tools.toml" --manifest "$run_dir/argument.bundle.json"
else
  stage='current-case collector preparation and EAL export'
  with_catalogue eal export --input "$run_dir/analysis.json" \
    --export-policy "$run_dir/export-policy.json" \
    --operator-inputs "$run_dir/collector-inputs.snapshot.json" \
    --operator-input-base "$(dirname "$inputs")" \
    --preparation-output-dir "$run_dir/collector-inputs" \
    --output "$run_dir/argument.eal" --tools-output "$run_dir/argument.tools.toml" --manifest "$run_dir/argument.bundle.json"
fi
# Prepared stubs are intentionally not passed as an executable registry to the host.
stage='EAL validation'
host_report="$run_dir/eal-validation.json"
"$engarg_bin" eal validate --input "$run_dir/argument.bundle.json" --input-format json \
  --eal-host "$eal_host" --workspace "$run_dir" --timeout-secs "$eal_timeout" --output "$host_report"
stage='EAL evidence planning'
host_report="$run_dir/evidence-plan.json"
"$engarg_bin" eal plan --input "$run_dir/argument.bundle.json" --input-format json --claim current_claim \
  --eal-host "$eal_host" --workspace "$run_dir" --timeout-secs "$eal_timeout" --output "$host_report"
host_report=''
stage='run summary'
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
    gaps = routes['gaps']
    readiness = {
        'schema': 'engarg.graph-readiness/1',
        'case_digest': exported['case_digest'],
        'execution_plan_sha256': exported['execution_plan_sha256'],
        'requirements': len(requirements), 'logical_acquisitions': len(acquisitions),
        'selected_capabilities': sorted({route['selected'] for route in acquisitions if route.get('selected')}),
        'routes': acquisitions, 'gaps': gaps, 'pending': exported['pending'],
        'discovery_incomplete': routes['discovery']['incomplete'],
        'ready_for_collection': bool(acquisitions) and not gaps and not routes['discovery']['incomplete'] and all(route.get('selected') and not route.get('configuration_gaps') for route in acquisitions),
        'evidence_collected': False, 'evidence_truth_verified': False, 'inferential_soundness_verified': False,
        'independent_review_obligations': len(exported['independent_review_obligations']),
        'next_step': 'Resolve typed configuration, availability, qualification and clarification gaps before explicit collection; independent judgement remains separate.'
    }
    summary = {
        'completed_phase': 'argument authoring, independent support extraction, qualified route planning and deterministic export validation',
        'evidence_planning': 'graph', 'argument_status': report['argument']['status'],
        'recorded_model_calls': len(report.get('transcript', [])),
        'evidence_collected': False, 'graph_readiness': readiness,
        'next_step': readiness['next_step'],
        'tool_implementation': {'enabled': identity['tool_implementation']['enabled']},
    }
    if identity['tool_implementation']['enabled']:
        tooling = report.get('tooling')
        if not isinstance(tooling, dict) or tooling.get('schema') != 'engarg.capability-tooling/1':
            raise SystemExit('Graph implementation was enabled but the analysis has no capability tooling report')
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
    'completed_phase': 'argument authoring and deterministic export validation',
    'evidence_collected': False,
    'argument_status': report['argument']['status'],
    'recorded_model_calls': len(report.get('transcript', [])),
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
printf 'Authoring, rendering, export and EAL planning passed. Artifacts: %s\n' "$run_dir"
if [[ $evidence_planning == graph ]]; then
  python3 - "$run_dir/graph-readiness.json" <<'PY'
import json, sys
with open(sys.argv[1], encoding='utf-8') as source:
    readiness = json.load(source)
print('Graph readiness: %s requirements; %s shared logical acquisitions; %s typed gaps; ready for collection: %s. Evidence truth and inferential soundness remain unverified.' % (
    readiness['requirements'], readiness['logical_acquisitions'], len(readiness['gaps']), readiness['ready_for_collection']))
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
