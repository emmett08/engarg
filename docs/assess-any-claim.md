# Start a claim assessment from one command

[Documentation](index.md) · [Detailed workflow](USAGE.md) · [Library selection](library-selection.md)

`scripts/assess-claim.sh` runs the public EngArg and EAL commands from a bounded
claim or request JSON. It retains authoring exchanges, generated-tool verification,
concrete bindings, an executable package, observations, an assessment and HTML.
The script requires Bash 3.2+, Python 3.8+, installed EngArg and a matching EAL
runtime. Prepare the selected toolchain image before starting.

## Supply the claim and execution authority

Describe the actual subject, revision, source, scope, units, acceptance criteria
and original observation time. State the operator-authorised freshness bound.
For example, a request file can ask whether the dependencies in an identified
installed Python environment satisfy their declared version requirements:

```json
{
  "claim": "Every applicable declared runtime dependency of the identified installed Python distributions is present at a compatible version.",
  "context": "Identify the exact environment image and installed-distribution scope before acquisition. Evaluate dependency declarations with their version specifiers and environment markers. Retain raw package identities, versions, declarations, violations and the original observation time. The operator authorises an observation age of at most 3600 seconds. A feasible missing operation needs a complete reusable operation_spec. Missing environment access, source evidence and independent judgement remain explicit obligations.",
  "evidence": []
}
```

Replace the subject and source details with the actual investigation. Bind
existing operations in an operator-owned `tools.toml`. An omitted manifest
creates an empty one, leaving acquisition gaps for EngArg to author and, where
eligible, implement. This does not grant access to an unspecified source or
create a reviewer record.

From the repository root, with `ENGARG_MODEL` and `OPENAI_API_KEY` configured:

```sh
bash scripts/assess-claim.sh \
  --input request.json --tools operator-tools.toml \
  --model "$ENGARG_MODEL" --api-key-env OPENAI_API_KEY \
  --eal-host "$EAL_HOST" \
  --toolchain examples/toolchains/python-evidence.json \
  --output-dir work/my-assessment-001
```

The [Python evidence profile](../examples/toolchains/python-evidence.json)
selects a separately prepared scientific image. Use another language profile or
operator image when its dependencies and interface fit the required operation.
Both legacy runtime profiles and toolchain `/2` library preferences are accepted
through EngArg's public CLI.

The [real Python dependency example](../examples/assess-claim/python-dependencies/README.md)
includes a fully scoped request, an operator-selected Packaging library, a native
generated package, frozen verification cases and actual positive/negative EAL
collections. It distinguishes successful empirical checks from unresolved
argument qualification.

Its [completed qualification revision](../examples/assess-claim/python-dependencies/qualified/README.md)
connects actual native provenance and verification artifacts, exact scoped AI
assessments and a bound native query. The current metadata claim is supported;
missing-binding and wrong-scope controls remain unsupported under the same source
and criteria. The records preserve their original one-hour freshness limits.

For a short text input, replace `--input` with `--claim "..." --context "..."`.
Select an installed EngArg binary with `--engarg PATH` or `ENGARG_BIN`.
`--eal-host PATH` defaults to `EAL_HOST`, then `eal-host` on `PATH`; its companion
directory is placed first on the execution path. Generation requires matching
`engarg-process-supervisor`, `eal-tool-run` and `eal-process-supervisor` companions.
Install binaries before invocation and keep them unchanged while verification
runs.

## Inspect the result and continue

Every output directory is new and private. Existing directories and symlinks
are rejected. The retained files include:

| File or directory | Purpose |
|---|---|
| `summary.json` | Workflow status, claim status, EAL validity/completeness, stage exit codes and distinct verified/new/reused operation counts |
| `commands.txt`, `logs/` | Exact argument vectors rendered as quoted commands, plus each stage's stdout and stderr |
| `runtime.json`, `toolchain-check.json` | Runtime description and native isolated toolchain preflight |
| `analysis.json`, `analysis.audit.jsonl`, `analysis.checkpoint.json` | Native authoring result and retained exchanges |
| `authoring.html`, `authoring.md` | Argument and obligations available before tool implementation |
| `implemented/`, `tool-store/` | Native generated source, frozen cases, verification receipts and exported concrete bindings |
| `argument.html`, `argument.md` | Final report presentation after available implementations are attached |
| `investigation/` | Executable EAL, concrete TOML and pinned implementation resources |
| `execution/`, `execution.json`, `assessment.json` | EAL run store, full execution response and assessment |

Exit zero means the workflow returned an assessment. Read `claim_status`, `valid`
and `complete` in `summary.json`; a claim may remain unsupported. Operations left
unbound or deferred by implementation stop before package execution with exit 3
and `status: "blocked"`. With `--no-implement-tools`, native build admits the
current manifest, so newly supplied bindings can close an earlier report's gaps;
missing bindings stop at that stage with its native exit code. Native failures
retain their logs. Interrupted
commands retain their partial records and use exit 130 or 143.

The script never supplies accepted reviews, changes criteria or invents missing
measurements. Pending review obligations remain in the package and can leave
the claim unsupported after successful acquisition. A failed authoring call can
retain a checkpoint; inspect it with `engarg authoring inspect` and use the
[documented recovery commands](USAGE.md#if-authoring-times-out).

After inspecting an admitted report and supplying additional authorised
bindings, continue in a fresh output directory without reauthoring:

```sh
bash scripts/assess-claim.sh \
  --analysis work/my-assessment-001/analysis.json \
  --tools reviewed-tools.toml --model "$ENGARG_MODEL" \
  --eal-host "$EAL_HOST" --toolchain operator-python.json \
  --tool-store work/shared-verified-tools \
  --output-dir work/my-assessment-002
```

`--no-implement-tools` uses existing bindings only and skips the toolchain
preflight. Native build decides whether the current manifest supplies every
acquisition; authoring metadata is retained as its original snapshot.
With `--analysis`, this mode needs no model or provider credentials:

```sh
bash scripts/assess-claim.sh \
  --analysis work/my-assessment-001/analysis.json \
  --tools concrete-tools.toml --no-implement-tools \
  --eal-host "$EAL_HOST" --output-dir work/offline-assessment-001
```

The top-level `analysis.json` is retained before implementation and can be paired
with exported bindings without carrying an earlier implementation's registration
metadata. Native report admission still applies to every selected report.

With implementation enabled, the native stage always rechecks the supplied
manifest. A report's earlier bound/unbound list cannot override the current
operator authority. Already bound operations spend zero implementation calls.
The summary records `verified_operations`, `newly_generated_operations`,
`reused_operations` and `implementation_model_calls` separately. A reused record
does not count as newly generated, and operation counts do not imply paid calls.
`--tool-store DIR` optionally
reuses an existing verified cache; EngArg rechecks its retained identities and
library policy before reuse. Input request, report and manifest paths are passed
directly to the public CLI so its file admission and resource rules remain in
force.

## Set explicit budgets

Authoring and implementation have separate model-call and overall time budgets.
The defaults follow the full workflow: hybrid authoring with seven calls and one
repair, a 2400-second authoring deadline, eight tool calls and a 900-second
implementation deadline. Each provider request has a 600-second deadline. The
authoring output allowance is 24000 tokens and the implementation allowance is
16384 tokens, including reasoning tokens where applicable. Actual model calls
may incur provider charges within those bounds.

For a smaller one-call authoring baseline with structural repair allowance:

```sh
bash scripts/assess-claim.sh --input request.json \
  --model "$ENGARG_MODEL" --eal-host "$EAL_HOST" \
  --toolchain operator-python.json --output-dir work/bounded-001 \
  --author-mode single --author-max-calls 3 --author-max-repairs 2 \
  --request-timeout-secs 180 --author-timeout-secs 900 \
  --author-max-output-tokens 32768 --tool-max-output-tokens 24000 \
  --max-response-bytes 1048576 --tool-max-calls 8 \
  --tool-timeout-secs 900 --execution-timeout-secs 300
```

For another strict-schema provider, select `--api responses|chat`, `--endpoint`
and the credential variable name. URLs with embedded credentials are rejected.
Credential values are inherited by the native provider client and are never
placed in command arguments, summaries or progress messages. Run
`bash scripts/assess-claim.sh --help` for the complete option list.
