#!/usr/bin/env bash
# Run the local catalogue evidence example, or a scoped operator claim.
# Bash 3.2+ and Python 3.7+ (standard library only).
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

# The selected checkout is resolved before the first helper call.
catalog_sync() {
  local command=$1
  shift
  # Preserve operator paths beginning with a dash. -B keeps imports from
  # creating bytecode files during the clean-checkout workflow.
  if [[ $# -gt 0 ]]; then
    python3 -B "$repo/scripts/catalog_sync.py" "$command" -- "$@"
  else
    python3 -B "$repo/scripts/catalog_sync.py" "$command"
  fi
}

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
  --freshness-clarifications JSON  Explicit scoped freshness revisions for a retained graph.
  --legacy-flat                 Use the earlier flat collector-planning workflow.
  --implement-tools            Generate and fixture-test missing collectors (default: Rust).
  --tool-language LANGUAGE      Implementation language (default: rust), matching the profile.
  --toolchain JSON              Operator profile override; image must already be installed.
  --tool-store DIR              Reusable package store (default: .engarg-local/tool-store).
  --tool-max-calls N            Separate implementation budget (1-256; default: 16).
  --tool-timeout-secs N         Separate implementation deadline (1-86400; default: 3600).
Graph tool-stage settings inherit ENGARG_MODEL, ENGARG_REASONING_EFFORT and
ENGARG_MAX_OUTPUT_TOKENS unless ENGARG_TOOL_MODEL, ENGARG_TOOL_REASONING_EFFORT
or ENGARG_TOOL_MAX_OUTPUT_TOKENS overrides the corresponding setting.
HELP
}
check_only=0
collect_and_assess=0
evidence_planning='graph'
evidence_config=${ENGARG_EVIDENCE_CONFIG:-}
evidence_config_explicit=0
replan_analysis=''
freshness_clarifications=''
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
    --freshness-clarifications)
      [[ $# -ge 2 && -n $2 ]] || fail 'Missing value for --freshness-clarifications'
      freshness_clarifications=$2; shift 2 ;;
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
[[ -z $freshness_clarifications || -n $replan_analysis ]] || fail '--freshness-clarifications requires --replan-analysis.'
[[ $implement_tools -eq 1 || $tool_options -eq 0 ]] || fail 'Tool implementation options require --implement-tools.'
for required in git python3; do command -v "$required" >/dev/null || fail "Required command: $required"; done
repo=$(git rev-parse --show-toplevel) || fail 'Run from an EngArg checkout.'
# Keep operator paths relative to the invocation directory, including leaf-file
# symlinks so the bounded, no-follow admission below can reject them.
absolute_path() { catalog_sync absolute-path "$1"; }
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
if [[ -n $freshness_clarifications ]]; then
  freshness_clarifications=$(absolute_path "$freshness_clarifications")
  [[ -e $freshness_clarifications ]] || fail "Freshness clarifications do not exist: $freshness_clarifications"
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
    if recovery_status=$(catalog_sync inspection-status <<< "$inspection" 2>/dev/null); then :; else recovery_status=unknown; fi
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
catalog_sync preflight "$inputs" "$catalogue" "$run_dir" "$implement_tools" "$tool_language" \
  "$toolchain" "$tool_store" "$tool_max_calls" "$tool_timeout" "$evidence_planning" "$evidence_config" "$replan_analysis" \
  "$topology" "$debate_participants" "$debate_rounds" "$topology_config" "$eal_coordinate" "$coordination_run" "$freshness_clarifications"

if [[ $check_only -eq 1 ]]; then
  printf 'Input preflight passed. No build, inference or evidence acquisition performed. Artifacts: %s\n' "$run_dir"
  exit 0
fi
simple_example=$(catalog_sync simple-example "$run_dir/input-identity.json")
# A full run identifies its build by commit. Input-only checks remain available
# while tracked edits are staged or unstaged.
set_stage 'full-run prerequisites'
git diff --quiet HEAD -- || fail 'Full runs require a clean tracked checkout. --check-only accepts tracked changes; commit or stash them before inference.'
model=${ENGARG_MODEL:-gpt-6.1-sol}
service_tier=${ENGARG_SERVICE_TIER:-default}
reasoning_effort=${ENGARG_REASONING_EFFORT:-high}
repairs=${ENGARG_MAX_REPAIRS:-1}
[[ $repairs =~ ^[0-3]$ ]] || fail 'ENGARG_MAX_REPAIRS must be 0, 1, 2 or 3.'
minimum_calls=$(catalog_sync minimum-calls "$run_dir/input-identity.json" "$repairs")
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
tool_model=${ENGARG_TOOL_MODEL:-$model}
tool_reasoning_effort=${ENGARG_TOOL_REASONING_EFFORT:-$reasoning_effort}
tool_output_tokens=$output_tokens
if [[ $implement_tools -eq 1 ]]; then
  if [[ -n ${ENGARG_TOOL_MAX_OUTPUT_TOKENS:-} ]]; then
    tool_output_tokens=$ENGARG_TOOL_MAX_OUTPUT_TOKENS
  elif [[ $tool_model != "$model" ]]; then
    if [[ $tool_model == gpt-6.1-sol ]]; then
      tool_output_tokens=128000
    else
      fail 'Set ENGARG_TOOL_MAX_OUTPUT_TOKENS to a supported allowance for the selected ENGARG_TOOL_MODEL. Different models require their own limit.'
    fi
  fi
  [[ $tool_output_tokens =~ ^[1-9][0-9]{0,9}$ && $tool_output_tokens -le 4294967295 ]] ||
    fail 'ENGARG_TOOL_MAX_OUTPUT_TOKENS must be a positive 32-bit integer within the selected provider model limit.'
  if [[ $tool_model == gpt-6.1-sol && $tool_output_tokens -gt 128000 ]]; then
    fail 'The tool-stage gpt-6.1-sol model supports at most 128000 output tokens per request.'
  fi
  if [[ $evidence_planning == flat && ( $tool_model != "$model" || $tool_reasoning_effort != "$reasoning_effort" || $tool_output_tokens != "$output_tokens" ) ]]; then
    fail 'Separate tool-stage model settings require graph planning. The legacy flat workflow shares its authoring provider.'
  fi
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
if [[ $prepare_rust_image -eq 1 ]]; then
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
eal_host=$(catalog_sync resolved-path "$eal_host")
printf 'Selected EAL host (%s): %s\n' "$eal_host_selection" "$eal_host" >&2
[[ -f $eal_host && -x $eal_host && -f $(dirname "$eal_host")/eal-mcp && -x $(dirname "$eal_host")/eal-mcp ]] ||
  fail 'eal-host and its sibling eal-mcp must both be regular executable files.'
# The server resolves its worker and supervisor beside its own physical binary.
# Follow an operator-installed eal-mcp alias, preserving the runtime's legacy
# supervisor preference when both companion names are installed.
eal_runtime_dir=$(catalog_sync runtime-directory "$eal_host")
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
  catalog_sync admit-collector "$run_dir/evidence-config.json" "$eal_runtime_dir/eal-collector"
  if [[ $simple_example -eq 1 ]]; then
    set_stage 'catalogue collector compatibility preflight'
    catalog_sync validate-collector-capabilities "$eal_runtime_dir/eal-collector" "$run_dir/collector-capabilities.json"
  fi
fi

set_stage 'release build'
cargo build --locked --release --bin engarg --bin engarg-process-supervisor
target_dir=$(cargo metadata --locked --no-deps --format-version 1 | catalog_sync target-directory)
engarg_bin="$target_dir/release/engarg"
[[ -x $engarg_bin && -x $target_dir/release/engarg-process-supervisor ]] || fail 'Release companion binaries are missing.'

catalog_sync record-settings "$run_dir" "$engarg_bin" "$eal_host" "$eal_host_selection" "$model" "$service_tier" "$reasoning_effort" \
  "$max_calls" "$repairs" "$request_timeout" "$run_timeout" "$input_bytes" "$response_bytes" "$output_tokens" "$concurrent" "$eal_timeout" "$argument_depth" "$eal_collection_timeout" \
  "$tool_model" "$tool_reasoning_effort" "$tool_output_tokens"

# Initialise the actual server and inspect its required authoring interface
# before any paid inference.
set_stage 'EAL host startup preflight'
host_report="$run_dir/eal-preflight.json"
"$engarg_bin" eal describe --eal-host "$eal_host" --workspace "$run_dir" \
  --timeout-secs "$eal_timeout" --output "$host_report"
catalog_sync validate-host "$host_report"
host_report=''

if [[ $prepare_rust_image -eq 1 ]]; then
  set_stage 'Rust tooling image preparation'
  docker build --tag engarg-rust-tools:1.99.1 "$repo/examples/toolchains/rust"
fi
if [[ $implement_tools -eq 1 ]]; then
  set_stage 'isolated toolchain preflight'
  "$engarg_bin" eal toolchain-check --toolchain "$run_dir/toolchain.snapshot.json" \
    --output "$run_dir/toolchain-preflight.json"
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
    if [[ -n $freshness_clarifications ]]; then
      set -- --freshness-clarifications "$run_dir/freshness-clarifications.snapshot.json"
    else
      set --
    fi
    "$engarg_bin" eal replan --input "$run_dir/retained-analysis.snapshot.json" \
      --evidence-config "$run_dir/evidence-config.json" --output "$analysis_output" "$@"
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

if [[ $evidence_planning == graph ]]; then
  if [[ $simple_example -eq 0 ]]; then
    set_stage 'composed collector compatibility preflight'
    catalog_sync validate-composed-collector "$analysis_output" "$eal_runtime_dir/eal-collector" "$run_dir/collector-capabilities.json"
  fi
  set_stage 'graph freshness preflight'
  "$engarg_bin" eal freshness-check --input "$analysis_output"
fi

if [[ $implement_tools -eq 1 && $evidence_planning == graph ]]; then
  if [[ $simple_example -eq 1 ]]; then
    set_stage 'bounded catalogue operation admission'
    python3 "$repo/scripts/catalogue_evidence_example.py" admit-operation "$run_dir" "$analysis_output"
  fi
  set_stage 'tool implementation gap admission'
  catalog_sync select-tool-gaps "$analysis_output" "$run_dir/tooling-selection.json"
  set_stage 'tool implementation'
  "$engarg_bin" eal implement --progress --input "$analysis_output" \
    --model "$tool_model" --service-tier "$service_tier" --reasoning-effort "$tool_reasoning_effort" \
    --api responses --api-key-env OPENAI_API_KEY \
    --http-retries 3 --retry-max-delay-secs 120 --request-timeout-secs "$request_timeout" \
    --max-input-bytes "$input_bytes" --max-output-tokens "$tool_output_tokens" --max-response-bytes "$response_bytes" \
    --tool-language "$tool_language" --toolchain "$run_dir/toolchain.snapshot.json" \
    --tool-store "$tool_store" --tool-output-dir "$run_dir/tooling" \
    --tool-max-calls "$tool_max_calls" --tool-timeout-secs "$tool_timeout"
  catalog_sync merge-tool-analysis "$analysis_output" "$run_dir/tooling/analysis.with-tools.json" "$run_dir/analysis.json"
fi

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

catalog_sync extract-case "$run_dir"
set_stage 'argument validation'
"$engarg_bin" validate --input "$run_dir/request.json" --case "$run_dir/case.json"
set_stage 'argument rendering'
"$engarg_bin" render --input "$run_dir/analysis.json" --format markdown --no-audit-appendix --output "$run_dir/argument.md"
"$engarg_bin" render --input "$run_dir/analysis.json" --format argument --outline-depth 3 --display-depth 2 --output "$run_dir/argument.txt"
"$engarg_bin" render --input "$run_dir/analysis.json" --format prose --display-depth 2 --output "$run_dir/prose.md"
"$engarg_bin" render --input "$run_dir/analysis.json" --format html --display-depth 2 --output "$run_dir/argument.html"
set_stage 'EAL export'
if [[ $evidence_planning == graph ]]; then
  if [[ $simple_example -eq 0 ]]; then
    set_stage 'composed collector compatibility preflight'
    catalog_sync validate-composed-collector "$run_dir/analysis.json" "$eal_runtime_dir/eal-collector" "$run_dir/collector-capabilities.json"
    set_stage 'EAL export'
  fi
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
  # The graph registry is generated from the admitted operator configuration.
  # The host separately validates every forwarded name against the actual TOML.
  # Read names from the JSON copy rather than parsing TOML in Bash/Python 3.7.
  set --
  if [[ $evidence_planning == graph ]]; then
    catalog_sync collector-environment "$run_dir/argument.bundle.json" > "$run_dir/collector-environment.names"
    while IFS= read -r collector_variable; do
      set -- "$@" --collector-env "$collector_variable"
    done < "$run_dir/collector-environment.names"
  fi
  set_stage 'EAL evidence collection'
  host_report="$run_dir/evidence-collection.json"
  "$engarg_bin" eal collect --input "$run_dir/argument.bundle.json" --input-format json --claim current_claim \
    --eal-host "$eal_host" --workspace "$run_dir" --database "$run_dir/eal.sqlite" \
    --tools "$run_dir/argument.tools.toml" "$@" \
    --timeout-secs "$eal_collection_timeout" --output "$host_report"
  collection_id=$(catalog_sync collection-id "$host_report")
  set_stage 'EAL argument assessment'
  host_report="$run_dir/eal-assessment.json"
  assessed_at=$(catalog_sync utc-now)
  "$engarg_bin" eal assess --input "$run_dir/argument.bundle.json" --input-format json \
    --eal-host "$eal_host" --workspace "$run_dir" --database "$run_dir/eal.sqlite" \
    --tools "$run_dir/argument.tools.toml" "$@" \
    --timeout-secs "$eal_collection_timeout" --now "$assessed_at" \
    --collection-id "$collection_id" --output "$host_report"
  catalog_sync assessment "$run_dir"
  host_report=''
fi
set_stage 'run summary'
catalog_sync summary "$run_dir"
if [[ -n $replan_analysis ]]; then
  printf 'Route replanning, rendering, export and EAL planning passed. Artifacts: %s\n' "$run_dir"
else
  printf 'Authoring, rendering, export and EAL planning passed. Artifacts: %s\n' "$run_dir"
fi
if [[ $collect_and_assess -eq 1 ]]; then
  catalog_sync execution-status "$run_dir/execution-result.json"
fi
if [[ $evidence_planning == graph ]]; then
  catalog_sync graph-status "$run_dir/graph-readiness.json"
else
  printf 'Review per-need bindings and independent review obligations before collection and assessment.\n'
  catalog_sync preparation-status "$run_dir/collector-inputs/collector-preparation.json"
fi
