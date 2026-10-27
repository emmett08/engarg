#!/usr/bin/env bash
# Build and execute portable EAL packages. No authoring-model calls occur here.
# Bash 3.2+ and Python 3.
set -euo pipefail
umask 077

usage() {
  cat <<'HELP'
Usage: bash scripts/run-catalog-sync-authoring.sh --input REPORT.json --tools tools.toml [OPTIONS]
       bash scripts/run-catalog-sync-authoring.sh --simple-example [OPTIONS]

  --input FILE          Authored EngArg report with support elaboration.
  --tools FILE          Direct operation bindings authorised by the operator.
  --output-dir DIR      New portable package directory (default: engarg-runs/executable-TIME-PID).
  --workspace DIR       EAL run directory (default: PACKAGE/run).
  --claim ID            EAL claim identifier to collect and assess.
  --eal-host PATH       Installed EAL host (default: EAL_HOST or eal-host).
  --simple-example      Run the bundled synthetic finite-inventory package.
  --help                Show this help.

Set ENGARG_BIN to select an installed EngArg binary. This script never calls an
LLM, registers capabilities or reads old execution plans. Generate or explicitly
revise an argument separately before building it.
HELP
}
fail() { printf 'execution: %s\n' "$*" >&2; exit 2; }
require_value() { [[ $# -ge 2 && -n $2 ]] || fail "$1 requires a value"; }
absolute() { case $1 in /*) printf '%s\n' "$1" ;; *) printf '%s/%s\n' "$PWD" "$1" ;; esac; }

repo=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
input=''
tools=''
output=''
workspace=''
claim=''
simple=0
host=${EAL_HOST:-eal-host}
engarg=${ENGARG_BIN:-engarg}
if [[ $# -eq 0 ]]; then simple=1; fi
while [[ $# -gt 0 ]]; do
  case $1 in
    --input) require_value "$@"; input=$2; shift 2 ;;
    --tools) require_value "$@"; tools=$2; shift 2 ;;
    --output-dir) require_value "$@"; output=$2; shift 2 ;;
    --workspace) require_value "$@"; workspace=$2; shift 2 ;;
    --claim) require_value "$@"; claim=$2; shift 2 ;;
    --eal-host) require_value "$@"; host=$2; shift 2 ;;
    --simple-example) simple=1; shift ;;
    --help|-h) usage; exit 0 ;;
    *) fail "unknown option: $1; use --help for the executable-package workflow" ;;
  esac
done
if [[ $simple -eq 1 ]]; then
  [[ -z $input && -z $tools ]] || fail '--simple-example cannot be combined with --input or --tools'
else
  [[ -n $input && -n $tools ]] || fail '--input and --tools are required'
  [[ -f $input && -f $tools ]] || fail 'input report and tool manifest must be regular files'
  command -v "$engarg" >/dev/null 2>&1 || fail "EngArg executable not found: $engarg"
fi
command -v "$host" >/dev/null 2>&1 || fail "EAL host executable not found: $host"
command -v python3 >/dev/null 2>&1 || fail "Python 3 is required to read package metadata"
if [[ -z $output ]]; then output="engarg-runs/executable-$(date -u +%Y%m%dT%H%M%SZ)-$$"; fi
output=$(absolute "$output")
[[ ! -e $output && ! -L $output ]] || fail "output directory already exists: $output"
if [[ -z $workspace ]]; then workspace="$output/run"; else workspace=$(absolute "$workspace"); fi

if [[ $simple -eq 1 ]]; then
  printf '[execution] preparing synthetic finite-inventory example\n' >&2
  mkdir -p -- "$(dirname -- "$output")"
  cp -R -- "$repo/examples/executable/finite-inventory" "$output"
  if [[ -z $claim ]]; then claim=inventory_verified; fi
else
  printf '[execution] compiling authored argument and direct tool bindings\n' >&2
  "$engarg" build --input "$(absolute "$input")" --tools "$(absolute "$tools")" --output-dir "$output"
  if [[ -z $claim ]]; then
    claim=$(python3 - "$output/build.json" <<'PYCLAIM'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as stream:
    claim = json.load(stream)["claims"]["current_claim"]
if not isinstance(claim, str) or not claim:
    raise SystemExit("build receipt has no current claim identifier")
print(claim)
PYCLAIM
)
  fi
fi
args=(run "$output/case.eal" --tools "$output/tools.toml" --workspace "$workspace")
if [[ -n $claim ]]; then args+=(--claim "$claim"); fi
printf '[execution] collecting evidence and assessing EAL; package: %s\n' "$output" >&2
"$host" "${args[@]}"
