# Catalogue evidence example and operator workflow

With no arguments, [`scripts/run-catalog-sync-authoring.sh`](../scripts/run-catalog-sync-authoring.sh)
runs a bounded local example with this claim:

> The demonstration-v1 catalogue snapshot contains three entries with distinct identifiers.

```sh
bash scripts/run-catalog-sync-authoring.sh --simple-example --check-only

# A clean checkout, configured OPENAI_API_KEY, Rustup and running Docker are required.
bash scripts/run-catalog-sync-authoring.sh --simple-example
```

The read-only additional MCP server uses the official Rust MCP SDK and reads a
retained local catalogue file. Authoring receives the simple claim and explicit
source and operation contracts, with no observations supplied. A separately
generated custom tool counts the retrieved items and checks exact identifier
distinctness. Its formal-input output preserves the specified deductive query.
Separate acquired predicates establish the empirical premises before EAL's
`deductive/1` method checks their conjunction.

The workflow generates and verifies the custom tool in the isolated runner,
checks its frozen examples against an independently specified reference, registers
only the fixed demonstration operation, replans without repeating authoring,
exports EAL/3, collects through both providers and assesses the current claim.
Qualification runs independently authored empty, singleton, three-item, duplicate,
case-sensitive and incomplete-input probes through the verified package's actual
Docker-backed launcher. Their plan is frozen before authoring, retained with a
digest at input preflight, and accompanied by an observed audit and receipt.
These probes require no further model calls. After collection, a separate reference check
recomputes the audit from the actual retained MCP snapshot and checks the custom
tool's exact inputs and outputs. These checks concern a local demonstration
snapshot and make no claim about a private deployment or a general scientific
model.

The default custom-tool profile is Python. Use `--tool-language rust` or
`--tool-language javascript`, optionally with `--toolchain PROFILE.json`, to
select another isolated profile. Runtime preparation is shared with the operator
workflow. Default images are prepared before paid inference. An operator-supplied
profile requires its image to be installed locally. The demonstration never
replaces Docker verification with host execution.

`execution-result.json` reports `supported` only when the EAL assessment is valid,
complete, the current claim has status `supported`, and the independent check of
the actual observations passes. Missing evidence, an unavailable provider, a
failed method or an unmatched formal input remains unresolved. Collection,
assessment, generated code, audits and rendered arguments remain available.
An unresolved completed assessment returns status 3. A failed stage returns a
nonzero status and identifies the retained run directory.
Rendered arguments retain the authored case and its status. The result after
collection is recorded in `execution-result.json` and `eal-assessment.json`.

The positional-input workflow below retains the existing Kubernetes and Grafana
example. Add `--collect-and-assess` to execute its selected current claim after
planning. Its independent judgements and unavailable observations remain visible
and can leave that claim unresolved.

[`scripts/run-catalog-sync-authoring.sh`](../scripts/run-catalog-sync-authoring.sh)
authors a claim about one Kubernetes resource in context `dev` and one literal
Grafana TraceQL query. It renders the argument presentations, exports an EAL bundle, validates
the domain case and EAL source, and obtains an evidence plan. Collection and
assessment are requested through `--collect-and-assess`.

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
Before each validation-guided model repair, progress reports the repair number,
remaining allowance and diagnostic count. Diagnostic text and argument contents
remain in the private audit.

The provider schema and role instructions request at most 32 entries separately
for `findings`, `objections` and `evidence_gaps` in every specialist and final-critic
assessment. If an otherwise valid list exceeds that request, processing continues
with all entries retained and passed to the next stage. A warning reports the
role, field and received count on stderr, in the private audit and in the report's
limitations. No entries are silently dropped and no extra model call is spent
repairing the list count. Invalid JSON, blank entries, byte budgets and final
argument validation remain enforced. This continuation handles an oversized list
in the active run; it does not add automatic resume from an older failed audit.

With `--implement-tools`, the script prepares the default Rust image once and
checks the selected toolchain before authoring. The check runs a trusted probe
through the actual restricted container and temporary source and fixture mounts.
It retains `toolchain-preflight.json` and stops before model calls if isolated
execution fails. Operator-supplied profiles use their already installed images.
Input-only `--check-only` retains its build-free and inference-free behaviour.
Omit `--implement-tools` when a run needs only configured collectors and no
generated code.

Graph implementation retains `analysis.original.json` and inspects its typed
gaps before coding. Configuration-only runs consume no coding calls. Graph
evidence plans, native/MCP configuration, scoped reviews and the function-test
qualification boundary are documented in the support-planning guide.

The simple example pins its independently specified identifier audit in the
planning configuration before tool generation. Model wording remains in the
retained analysis. The implementation uses the operator's definition, and
qualification checks the frozen package against that definition, its exact
interface and its independent probes. `tooling-operation-admission.json` records
the binding before coding calls. Known unit annotations are checked separately
from JSON Schema validation constraints. Cardinality units remain those of the
admitted contract, and both source and audit qualifications declare the returned
snapshot identifier. Qualifications retain their own source authority. Upstream
authority requirements and unsupported scope selectors remain route gaps.

`--replan-analysis` can apply this current operator configuration to an earlier
argument without repeating authoring. A package generated from the earlier
model-authored definition has a different identity and requires verification
under the pinned definition. Pending judgement evaluations and unresolved
reasoning-method bindings remain unresolved after tool qualification.

Machine-evaluated graph requirements need an explicit freshness bound in both
`requirement.contract.time.max_age_seconds` and the matching evaluation's
`max_age_seconds`. The bound is from 1 through 315360000 seconds. Authoring
validation checks these fields before collector implementation, and support
recovery applies the same check. A supplied request bound must be carried into
each applicable requirement and acquisition while preserving stricter source
bounds. The legacy flat export-age policy does not supply missing graph ages.

If an older analysis omitted the bound, use `engarg eal repair` with its saved
`analysis.original.json` and `evidence-config.json`, then pass the repaired report
to `--replan-analysis`. Repair retains the original argument and authoring calls
and records the changed requirement and acquisition identities. It uses a
separate model-call allowance. Unavailable evaluations and pending unscoped
judgements can retain absent ages because they emit no usable observations.

Authoring uses `ENGARG_MODEL`, `ENGARG_REASONING_EFFORT` and
`ENGARG_MAX_OUTPUT_TOKENS`. Graph tool generation inherits those settings.
Override individual settings with `ENGARG_TOOL_MODEL`,
`ENGARG_TOOL_REASONING_EFFORT` and `ENGARG_TOOL_MAX_OUTPUT_TOKENS`:

```sh
ENGARG_REASONING_EFFORT=high \
ENGARG_TOOL_REASONING_EFFORT=max \
ENGARG_TOOL_MAX_OUTPUT_TOKENS=64000 \
  bash scripts/run-catalog-sync-authoring.sh --simple-example
```

A different tool model uses its own supported output allowance. The exact
`gpt-6.1-sol` model has a built-in maximum of 128000 tokens per request. Switching
to other model identifiers or deployment aliases requires an explicit allowance.
Both stages retain their selected settings in `run-settings.json`, with the
tool settings under `tool_implementation`. The legacy flat workflow shares one
provider and rejects differing tool-stage settings. Reasoning and model changes
affect generation. Container execution retains the selected toolchain bounds.

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

`run-summary.json` reports argument validation and artefact generation separately
from acquisition design, capability selection, tool implementation and collection.
An incomplete acquisition design produces a terminal warning while retaining all
completed model responses and exported artefacts. A successful export therefore
has an explicit `pending_repair` or `pending_design` status when work remains.
Historical designs retained under `evidence_planning.design_review` are recovery
inputs and cannot execute through the current plan.

`graph-readiness.json` counts `authored_logical_acquisitions` separately from
`compiled_acquisition_routes`. Its older `logical_acquisitions` field remains a
compatibility alias for the compiled-route count. `source_collection` identifies
configured source routes whose complete dependency closure is available. Its
status can be `partial` when other routes remain blocked. Independent judgement
gaps remain visible alongside those routes. `ready_for_collection` additionally
checks the complete emitted selection and its collection budget. The budget
counts emitted evidence declarations, including review obligations, rather than
physical collector calls. No status in this report establishes evidence truth or
inferential soundness.

Initial graph authoring writes `authoring.checkpoint.json` before interpreting
completed model responses. On failure, the script inspects that checkpoint
locally and prints its inspection command. When `resumable` is `true`, it also
prints a final-validation resume command with the run's model settings and
resource bounds. Inspection uses no model calls:

```sh
engarg authoring inspect --checkpoint engarg-runs/PREVIOUS-RUN/authoring.checkpoint.json
```

Final-validation recovery requires a completed initial candidate and all
preceding authoring stages, with their response content retained. In default
hybrid mode this means six completed calls, including adjudication. A provider
failure before that point leaves `resumable` as `false`. Retain the checkpoint
for diagnosis and start a fresh authoring run after the provider recovers.
An unavailable or invalid inspection produces inspection-only guidance.

Older runs retain their completed calls in `analysis.audit.jsonl`. Inspect one
with `engarg authoring inspect --run-dir engarg-runs/PREVIOUS-RUN`. A legacy
resume requires `--accept-protocol-migration` because its reasoning protocol was
unrecorded. That flag acknowledges revalidation under the current protocol.
The inspector reports whether the retained exchanges can reconstruct the final
validation stage. Missing or omitted responses remain explicit recovery limits.

Resume gives only the outstanding final repair a new allowance. Use new output,
audit and checkpoint paths, and the same retained evidence configuration:

```sh
engarg authoring resume \
  --run-dir engarg-runs/PREVIOUS-RUN --accept-protocol-migration \
  --evidence-config engarg-runs/PREVIOUS-RUN/evidence-config.json \
  --model gpt-6.1-sol --api-key-env OPENAI_API_KEY \
  --max-repairs 1 --request-timeout-secs 1800 --timeout-secs 10800 \
  --max-input-bytes 1048576 --max-response-bytes 16777216 \
  --max-output-tokens 128000 --progress \
  --output resumed-analysis.json --audit resumed-authoring.audit.jsonl \
  --next-checkpoint resumed-authoring.checkpoint.json
```

For a current checkpoint, replace `--run-dir` and the migration flag with
`--checkpoint PATH`. Completed assessments, synthesis and critique are replayed
locally. Digests establish consistency of the retained records. Argument
acceptance still requires complete validation, including every retained
objection and its application to any revised target.

Continue the demonstration from an accepted resumed report:

```sh
bash scripts/run-catalog-sync-authoring.sh --simple-example \
  --replan-analysis resumed-analysis.json
```

This reuses the argument and replans acquisition routes before tool generation,
collection and assessment. The saved report must match the demonstration's
exact request. New collection preserves the distinction between empirical
premise support and the formal method result.

For a saved run whose adjudication removed an earlier acquisition design, repair
the support graph before route-only replanning:

```sh
engarg eal repair \
  --input engarg-runs/PREVIOUS-RUN/analysis.json \
  --evidence-config operator-planning.json \
  --model gpt-6.1-sol --max-repairs 1 --timeout-secs 3600 --progress \
  --request-timeout-secs 1800 --max-output-tokens 65536 \
  --audit repaired-support.audit.jsonl --output repaired-analysis.json
```

This command gives the retained final argument, original acceptance criteria,
material objections and earlier designs to a separately bounded support repair.
It preserves the original transcript and records new usage and source-report
identity. Completed decoded responses within the admitted byte budget are
retained in the private audit before argument validation. Provider truncation
and an interrupted request are not resumable workflow checkpoints. An invalid
historical blueprint remains outside execution until its repaired form passes
validation. Repair does not generate tools or collect evidence. Review the result,
then use the implementation and registration workflow for genuine missing
operations. Existing capabilities can be selected through ordinary replanning.
Support recovery defaults to a 1,800-second request deadline and 65,536 output
tokens so the complete frozen argument and repaired graph can be returned.
Both settings can be overridden for the selected provider.

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
It also preserves the existing support elaboration, so it cannot restore a design
that was omitted or removed during authoring. Use `engarg eal repair` for that
case, then pass its saved report to `--replan-analysis`.

Graph export requires an explicit positive evidence age for each collectable
requirement. The graph freshness preflight reports missing bounds before tool
implementation. The legacy `ENGARG_ACQUISITION_MAX_AGE_SECONDS` setting does
not supply graph bounds.

For a retained analysis, `--freshness-clarifications` accepts an operator's
explicit additions. Select exact requirement IDs and supply the
age in seconds. The rationale records why those bounds are appropriate. For
example, use the following structure after checking the IDs and scope in the
retained report:

```json
{
  "schema": "engarg.freshness-clarification/1",
  "rationale": "The operator requires observations no older than one hour.",
  "requirements": {"R-count": 3600}
}
```

```sh
bash scripts/run-catalog-sync-authoring.sh --simple-example \
  --replan-analysis path/to/analysis.original.json \
  --freshness-clarifications path/to/freshness.json
```

The revision cannot replace an existing age. It retains the earlier planning
record and does not resolve outstanding judgements. A fresh calculation does
not refresh an older upstream observation. Choosing an age can restrict a claim,
so inspect the affected requirements and acquisition lineage before applying it.
Existing acquisition contracts remain unchanged. Each clarified requirement
still limits the age of the original observations in its complete lineage.

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
invocation directory. `--check-only` needs Bash 3.2 or later, Python 3.7 or later and Git.
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
`ENGARG_MAX_CONCURRENT_REQUESTS`. The default `gpt-6.1-sol` uses its published
128,000 maximum output-token allowance, checked against the
[official model specification](https://developers.openai.com/api/docs/models/gpt-6.1-sol)
on 25 October 2026. This is an upper allowance, not a requested response length.
An explicit override above that known limit fails before inference. Other model
names and deployment aliases require an explicit `ENGARG_MAX_OUTPUT_TOKENS`
matching their provider limit. The same allowance reaches authoring and tool
implementation unless a graph tool-stage override selects a separate allowance.
The default decoded response allowance is 16 MiB.

Argument depth is automatic by default. Set `ENGARG_ARGUMENT_DEPTH` from 1
through 16 to impose an explicit bound. This replaces the previous hardcoded
two-level authoring limit. Rendering's two-level initial display does not remove
deeper nodes from the saved argument.

Freshness policy overrides are
`ENGARG_REVIEW_MAX_AGE_SECONDS` and `ENGARG_ACQUISITION_MAX_AGE_SECONDS`.
`ENGARG_EAL_TIMEOUT_SECS` sets the separate host deadline for startup, validation
and planning, from 1 to 3600 seconds with a default of 30 seconds.
`ENGARG_EAL_COLLECTION_TIMEOUT_SECS` sets the collection and assessment host
deadline, from 1 to 3600 seconds with a default of 600 seconds. Provider and
overall analysis deadlines remain independently configurable.

Each run retains request and input identity, build and binary provenance, the
analysis audit, all renders, the EAL bundle, validation result, evidence plan and
`run-summary.json` in a unique private directory. Collector preparation also
retains the final input documents and their per-need readiness report. Failures
preserve existing run
artifacts. Without `--collect-and-assess`, a passing operator run means authoring
and deterministic planning succeeded and `evidence_collected` remains `false`.
With execution requested, the summary records the actual collection identifier,
assessment, current-claim status and explicit supported or unresolved outcome.
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

## Maintaining the workflow

The public entry point remains `scripts/run-catalog-sync-authoring.sh`. Bash owns
option parsing, external command order, stage progress and failure recovery. One
shell owner keeps those phases and the exit trap's state visible together.

Structured processing lives in the repository-local `engarg_catalog_sync` Python
package under `scripts`. The shell invokes its dispatcher through
`scripts/catalog_sync.py`. It uses the Python standard library and runs directly
from the checkout. Keep the dispatcher and package alongside the shell script
when copying the workflow. Python 3.7 or later is required.

| Module | Responsibility |
| --- | --- |
| `json_io.py` | Bounded input admission and exact decimal serialisation |
| `preflight.py` | Scope validation and private input snapshots |
| `runtime.py` | EAL runtime admission and recorded executable identities |
| `tooling.py` | Implementation-gap admission and analysis-preserving enrichment |
| `assessment.py` | Collection identifiers and retained assessment results |
| `readiness.py` | Dependency availability and collection-budget admission |
| `summary.py` | Graph and flat reports with terminal outcome messages |

The shell uses `python3 -B` for package calls so imports leave the checkout clean.
The package CLI lists its internal commands with
`python3 -B scripts/catalog_sync.py --help`. Imported policy functions can be
tested directly. The existing CI command discovers both these focused tests and
the shell integration tests:

```sh
bash -n scripts/run-catalog-sync-authoring.sh
python3 -m unittest discover -s tests -p 'test_*.py'
```

Refactoring preserves the command options, environment defaults, snapshot bytes,
report schemas and failure statuses. An unresolved completed assessment continues
to return status 3. Model calls and Docker execution retain their existing bounds.
