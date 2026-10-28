#!/usr/bin/env bash
# Run the public EngArg/EAL workflow; Bash 3.2+ and Python 3.
set -euo pipefail
script_dir=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
command -v python3 >/dev/null 2>&1 || {
  printf 'assessment: Python 3 is required\n' >&2
  exit 2
}
exec python3 "$script_dir/assess_claim.py" "$@"
