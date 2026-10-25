# Catalog sync authoring smoke test

[`scripts/run-catalog-sync-authoring.sh`](../scripts/run-catalog-sync-authoring.sh)
authors a claim about one Kubernetes resource in context `dev` and one literal
Grafana TraceQL query. It renders the argument presentations, exports an EAL bundle, validates
the domain case and EAL source, and obtains an evidence plan. Collection and
assessment remain separate operator actions.

The script now defaults to graph planning and requires an operator evidence
configuration. Start with the [support-planning guide](support-planning.md):

```sh
bash scripts/run-catalog-sync-authoring.sh --check-only \
  --evidence-config operator-planning.json .engarg-local/collector-inputs.json

bash scripts/run-catalog-sync-authoring.sh --implement-tools \
  --evidence-config operator-planning.json .engarg-local/collector-inputs.json
```

The script reports phase transitions on stderr and enables terminal progress for
authoring and tool implementation. Model calls report their role, response receipt
and elapsed time. A waiting update is scheduled every ten seconds during an active
operation, including long provider waits. Redirect stderr to retain the terminal
status log. The private `analysis.audit.jsonl` retains completed provider exchanges
for diagnosis, including responses rejected by later validation.

Every specialist and final-critic assessment list has a 32-entry bound in the
provider schema and local validator. The bound applies separately to `findings`,
`objections` and `evidence_gaps`. A rejected list reports its field and received
count. A failed assessment stops the run with its audit retained. A subsequent
run uses the corrected schema but does not automatically resume the failed audit.

Graph implementation first retains `analysis.original.json` and inspects its
typed gaps. It prepares the default Rust image only for a genuine unsupported
operation or transformation. Configuration-only runs need no Docker or coding
calls. Graph evidence plans, native/MCP configuration, scoped reviews and the
function-test qualification boundary are documented in that guide.

Graph readiness retains the exporter's complete collection-budget metadata,
including its assumed default of 128 evidence declarations and per-claim counts.
Default collection selects all emitted declarations. The readiness check uses
the selected host's advertised `execution_limits.collection_evidence` when
available, accepts `budgets.collection_evidence` as a compatibility alias, and
otherwise labels the exporter default as an assumption. A larger actual host
limit can admit a graph whose conservative default-limit pending record remains
visible; a smaller actual limit can block collection below 128 declarations.
Independent review obligations remain visible and do not themselves block
acquiring the source evidence.

After correcting connection bindings or approving a catalogue, reuse the saved
argument and support requirements rather than paying for authoring again:

```sh
bash scripts/run-catalog-sync-authoring.sh \
  --replan-analysis engarg-runs/PREVIOUS-RUN/analysis.json \
  --evidence-config operator-planning.json .engarg-local/collector-inputs.json
```

The operator inputs must produce the same request as the retained report. A
changed claim or scope requires fresh authoring. Replanning reads a private,
bounded snapshot and invokes `engarg eal replan` with the current configuration.
It preserves the argument, support requirements and prior transcript, then
renders and exports the refreshed plan into a new run directory. It makes no
authoring model calls and needs no API key unless `--implement-tools` is also
requested. `run-summary.json` distinguishes retained `recorded_model_calls` from
`model_calls_this_run` and records the reused report's source and SHA-256.
Replanning refreshes discovery and route selection; it does not collect evidence
or turn missing independent judgements into observations.

The catalogue and per-need preparation workflow below remains available through
the explicit `--legacy-flat` switch. Its examples select that compatibility mode.

## Check operator inputs

From any directory within the EngArg checkout:

```sh
bash scripts/run-catalog-sync-authoring.sh --legacy-flat --check-only \
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
These describe prospective acquisition. The preparation stage uses explicit per-need
operations to create current-case input bindings. The direct Kubernetes adapter
can use the supplied context and resource selectors. Full-trace reads also need
actual trace IDs. The preparation report identifies remaining configuration and
evidence gaps before collection.

To admit additional collector capabilities, pass an existing operator-selected
catalogue as the second argument:

```sh
bash scripts/run-catalog-sync-authoring.sh --legacy-flat --check-only \
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

## Prepare inputs for the current argument

Every legacy flat-mode script run prepares collector inputs during the normal EAL export.
The output remains `argument.eal`, `argument.tools.toml` and
`argument.bundle.json` in the run directory. Its `collector-inputs/` directory
contains the input documents, final acquisition plans, pending review files and
`collector-preparation.json`. The report identifies the exact export digest and
records implementation availability, configuration state and evidence state for
each current need.

New planning records carry explicit preparation metadata. A `scoped_json` plan
declares `input_preparation` as
`{"kind":"scoped","operation":"grafana_search"}` or selects
`kubernetes` or `grafana_trace`. Preparation uses that operation and the supplied
scope settings. It preserves the literal query, endpoint, datasource and window.
The exporter fixes input paths before computing the final digest, then stamps
that digest into the input documents. Old `case_digest` and `needs` fields in an
operator file are reported as ignored. Their mappings belong to their earlier
argument.

A full-trace operation stays unresolved until the operator supplies actual
`grafana.trace_ids`. Preparation performs no search and chooses no trace IDs.
A historical plan without typed operation metadata also stays unresolved. Use
explicit current-need overrides when preparing that saved analysis, as described
in [private collector inputs](PRIVATE_COLLECTORS.md#prepare-current-case-bindings).

Retained artifacts use stable named references in the operator file:

```json
{
  "artifacts": {
    "trace_capture": {
      "artifacts": [{
        "path": "observations/trace.json",
        "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "observed_at": "2026-01-01T12:00:00Z",
        "source": "Retained trace response with its original provenance"
      }]
    }
  }
}
```

Add this object alongside the Kubernetes and Grafana settings and replace the
illustrative pins with an existing JSON artifact's exact hash and provenance.
Only reference names enter the model request. Paths and private provenance stay
in the operator snapshot. An `artifact_json` plan selects
`{"kind":"artifact","reference":"trace_capture"}` in its
`input_preparation`. Preparation verifies the retained bytes against the supplied
hash and validates the provenance and observation-time fields. It resolves relative paths against the original
operator file's directory. Missing files or references leave the binding
unresolved. `retained_artifacts_verified` records this input check. Independent
acceptance remains a separate review obligation.

The summary retains the preparation report and prints bound and unresolved
counts. A bound configuration has sufficient declared input values. Credentials,
runtime access and collection outcomes are established by their subsequent
operations. An unavailable independent review remains an evidence gap until an
authored decision exists. A genuinely unsupported producing operation selects
`custom` for implementation.

## Implement missing flat-mode collectors

Add `--implement-tools` to generate missing collectors in Rust during analysis:

```sh
bash scripts/run-catalog-sync-authoring.sh --legacy-flat --implement-tools \
  .engarg-local/collector-inputs.json
```

The default is `--tool-language rust` with
`examples/toolchains/rust.json`. Docker must be installed and running. The script
prepares the trusted SDK image from `examples/toolchains/rust`; its pinned,
precompiled Serde, Serde JSON and Reqwest libraries let generated Rust source
compile offline. Contract fixtures run without network access. Frozen contracts,
fixtures and observed passing test results gate registration. See
[collector implementation](TOOL_IMPLEMENTATION.md) for the profile contract and
fixture isolation.

An explicit `--toolchain` selects an operator-owned profile and skips automatic
image preparation. The operator must prepare that profile's image. Other
implementation languages require an explicit matching language and profile:

```sh
bash scripts/run-catalog-sync-authoring.sh --legacy-flat \
  --implement-tools --tool-language python \
  --toolchain examples/toolchains/python.json \
  --tool-store .engarg-local/tool-store \
  --tool-max-calls 16 --tool-timeout-secs 3600 \
  .engarg-local/collector-inputs.json
```

Options precede the input paths. Profile and explicit store paths resolve from
the invocation directory. Command-line options take precedence over their
environment fallbacks:

| Option | Environment fallback | Default |
| --- | --- | --- |
| `--toolchain` | `ENGARG_TOOLCHAIN` | `examples/toolchains/rust.json` in this checkout |
| `--tool-store` | `ENGARG_TOOL_STORE` | `.engarg-local/tool-store` in this checkout |
| `--tool-max-calls` | `ENGARG_TOOL_MAX_CALLS` | 16, range 1–256 |
| `--tool-timeout-secs` | `ENGARG_TOOL_TIMEOUT_SECS` | 3600 seconds, range 1–86400 |

Selecting a profile through `ENGARG_TOOLCHAIN` also skips automatic image
preparation. Live network access and credential forwarding follow the selected
profile's settings. `--check-only --implement-tools` validates and snapshots the
profile, language match and budgets with input-only dependencies; it needs no
Docker, Rust compiler or model credentials. A full run admits the complete profile
and its installed immutable image before any model call.

Implementation has a separate call budget and deadline; argument budgets remain
unchanged. A fresh collector normally needs two model calls. A failed coding
attempt can reuse its frozen contract with one call, while a verified package
needs none. EngArg checks the required budget before starting those calls. Use a
larger explicit budget for cases with many gaps, or continue from a saved analysis
with `engarg eal implement`.

The package store retains Rust source and verification receipts for review and
verified reuse. Use the same store across worktrees to reuse packages whose
identities match. A unique `tooling/` directory within each run retains the
original analysis, implementation audit and combined `collectors.json`. The
script also retains an exact private profile snapshot and its digest.

The coding stage handles absent acquisition plans and `custom` operations.
`required_calls: 0` with `model_calls: 0`, `packages: []` and `overrides: []`
means no implementation was generated or reused. Existing adapters can still
have missing input bindings or unavailable evidence. Tooling coverage reports
those selections separately from the current-case preparation report. An
`artifact_json` reader requires its retained record or an identified producer
for the acquisition operation. Missing independent reviews remain evidence
obligations.

The successful `analysis.json` carries the verified tooling metadata. The normal
export verifies registrations and writes matching EAL, TOML and bundle bindings.
`run-settings.json` and `run-summary.json` record `tool_implementation`; the
summary includes model calls and verified/reused package counts. A coding or
verification failure stops the run before export and preserves the canonical
analysis and available tooling artifacts. Fixture tests do not collect live
evidence or discharge independent review obligations.
Omitting `--implement-tools` retains the argument-only flow; implementation
options without that switch are rejected.

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
bash scripts/run-catalog-sync-authoring.sh --legacy-flat \
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
`run-summary.json` in a unique private directory. Collector preparation also
retains the final input documents and their per-need readiness report. Failures
preserve existing run
artifacts. A passing run means authoring and deterministic planning succeeded;
`evidence_collected` remains `false`.
When tooling is enabled, the summary also records its model calls and verified
and reused package counts. Synthetic fixture results remain separate from live
engineering observations.

The script renders each presentation from the same saved analysis without further
model calls. The shared `presentations` map in `run-summary.json` records their
paths in graph and legacy modes, including replanning runs.

| File | Presentation |
| --- | --- |
| `analysis.json` | Complete canonical argument and run audit |
| `argument.md` | Full Markdown argument records with the duplicate JSON audit appendix omitted |
| `argument.txt` | Indented argument statements with two detailed claim levels |
| `argument.html` | Expandable HTML with two detailed claim levels and glossary help |
| `prose.md` | Connected reading prose with two detailed claim levels |

The Markdown report keeps every argument record. The canonical JSON retains the
audit appendix's source data.

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
