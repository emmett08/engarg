# Catalog sync authoring smoke test

[`scripts/run-catalog-sync-authoring.sh`](../scripts/run-catalog-sync-authoring.sh)
authors a claim about one Kubernetes resource in context `dev` and one literal
Grafana TraceQL query. It renders three views, exports an EAL bundle, validates
the domain case and EAL source, and obtains an evidence plan. Collection and
assessment remain separate operator actions.

## Check operator inputs

From any directory within the EngArg checkout:

```sh
bash scripts/run-catalog-sync-authoring.sh --check-only \
  .engarg-local/catalog-sync-operator-20261024T120522Z/collector-inputs.json
```

Use the appropriate relative path to the script when invoking it from a
subdirectory. Operator input and optional catalogue paths resolve from the
invocation directory. `--check-only` needs Bash 3.2 or later, Python 3 and Git.
It accepts staged and unstaged tracked changes and requires no Rust tools,
`eal-host`, `eal-collector` or API key. It writes private input snapshots and a
request under the ignored `engarg-runs/` directory. Existing files are not edited.

The inputs must be a JSON object with this scope, using real selectors and an
explicit positive ordered Unix-second window:

```json
{
  "kubernetes": {
    "context": "dev",
    "resource": "applications.example.invalid",
    "kind": "Application",
    "namespace": "example",
    "name": "catalog-sync-operator"
  },
  "grafana": {
    "origin": "https://grafana.example.invalid",
    "datasource_uid": "example-tempo",
    "query": "{ span.dpe.rollout_kind = \"Application\" && span.dpe.rollout_name = \"catalog-sync-operator\" }",
    "start_unix": 1792832400,
    "end_unix": 1792836000
  }
}
```

The query is preserved literally, including its trailing whitespace. A supplied
`rollout_kind` or `rollout_name` must match the Kubernetes selector. Inputs are
bounded regular files; duplicate keys, missing selectors, credential-bearing
origins and invalid windows fail preflight. Scope fields are allowlisted into
the model request; the private snapshot retains the complete original input.
Keep credentials in environment variables rather than the input file.

## Select a catalogue when needed

No `approved.collectors.json` file is needed by default. Omit the optional second
argument to use EngArg's implemented `artifact_json`, `scoped_json`, `kubectl_get`,
`gh_api_get` and `http_json_get` adapters alongside unresolved `custom` bindings.
These describe prospective acquisition. Reading settings into a claim does not
create executable bindings. The direct Kubernetes adapter can use the supplied
context and resource selectors. Scoped Grafana operations still need a matching
operator-owned input file and trace IDs for full-trace reads. Review each generated
binding and unresolved parameter before collection.

To admit additional collector capabilities, pass an existing operator-selected
catalogue as the second argument:

```sh
bash scripts/run-catalog-sync-authoring.sh --check-only \
  .engarg-local/collector-inputs.json \
  .engarg-local/infrastructure.collectors.json
```

An explicitly supplied missing path fails with an explanation; the script never
silently substitutes capabilities. Input preflight checks the external
catalogue's JSON envelope; the EngArg executable validates its complete contract
before inference. For native infrastructure capabilities, generate a version 2
catalogue using an installed collector's absolute path if it is absent from
`PATH`:

```sh
mkdir -p .engarg-local
/absolute/path/to/eal-collector describe --catalogue-version 2 \
  > .engarg-local/infrastructure.collectors.json
```

Native Tempo generates its own filtered query. Executing the exact Grafana query
in this claim requires a compatible HTTP/custom route or an explicitly retained
[scoped adapter binding](PRIVATE_COLLECTORS.md#existing-scoped-adapter-profile).
Neither selecting a catalogue nor finding a nonempty search establishes resource
attribution. Full traces, Kubernetes identity, cluster, namespace, resource UID
and time correspondence require independent review.

## Run authoring and deterministic validation

Full runs require a clean tracked checkout and Rustup with the repository's pinned
Rust toolchain. The script installs the EAL revision selected by locked Cargo
metadata into `.engarg-local/eal-runtime/REVISION` on its first full run. Later
runs verify and reuse that installation. This keeps the Rust host, server, method
worker and both supervisor names in one prefix. Existing Python EARL tools on
`PATH` remain available to their callers.

The standard full run selects the managed installation automatically:

```sh
# Export OPENAI_API_KEY through your normal secret-management mechanism.
bash scripts/run-catalog-sync-authoring.sh \
  .engarg-local/catalog-sync-operator-20261024T120522Z/collector-inputs.json
```

An explicit `ENGARG_EAL_HOST` selects an operator-owned installation and bypasses
the managed installer. That host needs its sibling `eal-mcp`; the server needs
`eal-method-worker` and its selected supervisor beside its physical executable.
The server prefers an existing `eal-command-supervisor` file over
`eal-process-supervisor`, so an explicit prefix must contain compatible
companions under both names when both are present. The script prints the resolved
host path and retains the selection and binary paths in `run-settings.json`.

Run `bash scripts/install-eal-runtime.sh` independently to prepare the runtime
without authoring a claim. Its stdout contains the absolute host path. Use
`--root DIR` for an empty dedicated prefix or an existing installation managed by
this helper. The installer preserves unrelated nonempty prefixes. Installation
records bind the revision and executable hashes, so partial or changed bundles
are rebuilt as a complete package set.

The script admits the required companions, builds the EngArg binary and process
supervisor, then checks actual EAL server startup through `eal describe` before
any paid inference. The retained `eal-preflight.json` must advertise EAL/3 source
syntax and the `structured/1` method. Executable presence and startup compatibility
are separate checks. The default model is `gpt-6.1-sol`, with high reasoning effort,
the Responses API and the default service tier. Six planned model calls plus
one repair permit seven logical calls. Three HTTP retries allow up to four HTTP
attempts per logical call. No inference occurs during `--check-only`.

Environment overrides retain the downloaded script's `ENGARG_MODEL`,
`ENGARG_SERVICE_TIER`, `ENGARG_REASONING_EFFORT`, `ENGARG_MAX_REPAIRS` (0–3),
`ENGARG_REQUEST_TIMEOUT_SECS`, `ENGARG_TIMEOUT_SECS`, `ENGARG_MAX_INPUT_BYTES`,
`ENGARG_MAX_RESPONSE_BYTES`, `ENGARG_MAX_OUTPUT_TOKENS` and
`ENGARG_MAX_CONCURRENT_REQUESTS`. Freshness policy overrides are
`ENGARG_REVIEW_MAX_AGE_SECONDS` and `ENGARG_ACQUISITION_MAX_AGE_SECONDS`.
`ENGARG_EAL_TIMEOUT_SECS` sets the separate host deadline for startup, validation
and planning, from 1 to 3600 seconds with a default of 30 seconds. The longer model
and overall analysis deadlines do not change this host deadline.

Each run retains request and input identity, build and binary provenance, the
analysis audit, all renders, the EAL bundle, validation result, evidence plan and
`run-summary.json` in a unique private directory. Failures preserve existing run
artifacts. A passing run means authoring and deterministic planning succeeded;
`evidence_collected` remains `false`.

## Inspect and retry a host failure

The line `Structural and reporting checks passed` confirms EngArg's domain
validation. A subsequent EAL host failure belongs to a separate deterministic
operation. With an output destination, EngArg saves the complete JSON response
to that file before returning a failure status. The script now names the stage
and retained response path when it stops.

Inspect the applicable file in the failed run directory:

| File | Operation |
| --- | --- |
| `eal-preflight.json` | Server startup and supported syntax/method discovery |
| `eal-validation.json` | Validation of the exported EAL source |
| `evidence-plan.json` | Evidence planning for the exported current claim |

For example:

```sh
run="/absolute/path/to/retained/run"
python3 -m json.tool "$run/eal-validation.json"
```

The generic `EAL host reported failure` message alone does not identify the cause.
Missing runtime companions can close the MCP connection during initialisation.
Other responses can identify source diagnostics, compatibility problems or a host
deadline. Inspect the saved response before changing source or resource limits.

The response `methods must have the form package.module:function` comes from the
Python EARL method loader. EngArg passes `--methods builtin`, which Rust EAL
accepts. Both projects install the same host and server executable names, so PATH
selection can choose the Python implementation. Replacing only the server cannot
correct a selected Python host, which starts its own Python module.

Select the complete pinned Rust installation and probe it without model calls:

```sh
export ENGARG_EAL_HOST="$(bash scripts/install-eal-runtime.sh)"
engarg eal describe --eal-host "$ENGARG_EAL_HOST" --workspace .
```

A startup-preflight failure occurs before argument generation. Rerun the original
authoring command after this probe succeeds. For raw `.eal` input, use the
exported claim identifier; canonical `current_claim` mapping is available with
the matching JSON bundle.

For the generated HTML, open the downloaded file directly in a browser. Native
disclosures need no JavaScript. Some document previews remove styling or native
interaction. The renderer keeps claim qualifications and critical conditions
visible while placing complete registers behind labelled disclosure headings.

A custom binding marks an implementation gap. The optional
[collector implementation stage](TOOL_IMPLEMENTATION.md) can implement it from
the evidence-need contract. Frozen fixtures and observed test results gate
registration. Reuse an implemented adapter when it fits, and retain missing
settings as unresolved inputs. Applicability and sufficiency remain independent
assessment obligations.

When a run reached export, reuse its saved `argument.bundle.json`. Validation
and planning require no model call or API key. Use a new output directory because
the CLI preserves existing files:

```sh
retry=$(mktemp -d "$run/eal-retry-XXXXXX")
engarg eal validate --input "$run/argument.bundle.json" --input-format json \
  --eal-host "$ENGARG_EAL_HOST" --workspace "$retry" --timeout-secs 30 \
  --output "$retry/eal-validation.json"
engarg eal plan --input "$run/argument.bundle.json" --input-format json \
  --claim current_claim --eal-host "$ENGARG_EAL_HOST" --workspace "$retry" \
  --timeout-secs 30 --output "$retry/evidence-plan.json"
```

Use the EngArg binary recorded in the run or a compatible build. Preserve the
original analysis and audit when retrying the deterministic operations.
