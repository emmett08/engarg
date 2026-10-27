# Private collector inputs

`eal prepare` turns a saved EngArg analysis into seven private EAL files. Its
optional operator-input preparation adds current-case input documents and a
readiness report. Each evidence need keeps its chosen collector. Preparation retains every
independent review obligation. Collection runs through a separate explicit command.

Run from the repository root with a new output directory:

```sh
cargo run --locked --release -- eal prepare \
  --input path/to/analysis.json \
  --output-dir .engarg-local/example
```

The generated registry records the absolute path of the EngArg executable running
`prepare`. Rebuild at that path or prepare a new directory when moving the
executable. Collector-specific executable paths remain explicit operator settings.
Missing input values stay unresolved. An evidence need with no saved binding
receives a custom adapter stub.

| Generated file | Purpose |
| --- | --- |
| `argument.eal` | EAL/3 source with per-need acquisition bindings and review gates |
| `argument.bundle.json` | Matching source and canonical argument with identity and selected contracts |
| `argument.tools.toml` | Typed collector bindings and the retained-review importer |
| `acquisition-inputs.json` | Final acquisition-plan records with prepared inputs and unresolved parameters |
| `review-inputs.json` | Initially empty independently authored review decisions |
| `review-requests.json` | Exact pending six-field EAL review requests |
| `context.json` | Matching `engarg_export_digest` environment context |

The default planning catalogue exposes the five implemented compatibility adapters.
Planning selects an operation without granting execution or access to credentials.

| Adapter | Implemented acquisition |
| --- | --- |
| `artifact_json` | Explicit digest-checked retained JSON artifacts |
| `scoped_json` | Configured Kubernetes reads and literal Grafana search/full-trace reads |
| `kubectl_get` | Read-only resource JSON through an explicit Kubernetes context |
| `gh_api_get` | Read-only JSON from an explicit GitHub API path |
| `http_json_get` | JSON from an explicit HTTP endpoint |

A fitting implemented collector keeps missing inputs unresolved. The `custom`
stub represents an operation needing an additional implementation. Independent
acceptance remains a separate recorded review; acquiring a review document does
not establish that its judgement is sufficient. Select additional capabilities
with `--collector-catalogue PATH`. For the optional infrastructure package:

```sh
mkdir -p .engarg-local
eal-collector describe --catalogue-version 2 > .engarg-local/infrastructure.collectors.json
engarg eal prepare \
  --input path/to/analysis.json \
  --collector-catalogue .engarg-local/infrastructure.collectors.json \
  --output-dir .engarg-local/example
```

The catalogue option belongs to explicit adapter preparation and export. New
argument authoring uses `engarg analyse --tools tools.toml`; it receives safe
operation names and pinned interfaces from those concrete bindings. A custom
scientific collector can supply its own versioned descriptors for adapter use. Input schemas support JSON arrays and numbers as well as
strings. Named argument values encode non-string inputs as JSON, for example
`{"name":"channels","value":"[0,2]"}`. Templates keep the executable under
operator control and permit parameters only in declared argument positions.

Each need keeps its exact adapter. A method mentioning a full query remains an
HTTP read when its plan selects `http_json_get`. Separate needs retain separate
URLs and selectors. Existing operator-owned artifact files stay at their chosen
paths. Kubernetes and Grafana settings belong to explicitly selected bindings.

V3 export identity binds the projection semantics to the canonical case and
acquisition plan with its selected descriptors. Historical bundles retain their
recorded projection and producer identity during verification. A changed request
or source requires its own matching observation. Copying an old digest into a new input file cannot
establish that correspondence.

`.gitignore` excludes `engarg-runs/`, `.engarg-local/`, `*.local.json` and
`*.local.toml`. Keep internal endpoints and retained observations in those
private directories. Tracked documentation and test fixtures use sanitised
examples. Ignore rules apply to future additions.

## Prepare current-case bindings

Use supplied scope settings and retained artifact references to prepare inputs
for the saved argument:

```sh
engarg eal prepare \
  --input path/to/analysis.json \
  --operator-inputs .engarg-local/operator-inputs.json \
  --output-dir .engarg-local/current-case
```

This operation requires no model call or evidence collection. It preserves the
canonical analysis and verified tooling metadata. `tooling.original_plans`
retains the implementation stage's original proposals. Preparation writes its
current bindings to `acquisition-inputs.json` and the final bundle, with their
configuration and evidence gaps in `collector-preparation.json`.
Acquisition plans declare the
preparation operation through an optional `input_preparation` field. Supported
values are:

| Selected adapter | Preparation declaration |
| --- | --- |
| `scoped_json` | `{"kind":"scoped","operation":"kubernetes"}` |
| `scoped_json` | `{"kind":"scoped","operation":"grafana_search"}` |
| `scoped_json` | `{"kind":"scoped","operation":"grafana_trace"}` |
| `artifact_json` | `{"kind":"artifact","reference":"trace_capture"}` |

The operator input file supplies `kubernetes` and `grafana` scope settings using
the format below. Preparation writes a fresh `needs` map for the current need IDs
and uses the final bundle revision as `case_digest`. It preserves the exact
TraceQL expression and declared window. Full-trace operations require supplied
trace IDs. It reports old `case_digest` and `needs` fields as ignored rather than
reassigning their earlier mappings.

Historical plans can receive explicit preparation declarations through a JSON
acquisition override. Replace these illustrative IDs with IDs from the saved
argument's evidence plan:

```json
[
  {
    "need_id": "E_search",
    "tool_id": "scoped_json",
    "arguments": [],
    "unresolved_arguments": ["inputs"],
    "input_preparation": {"kind":"scoped","operation":"grafana_search"}
  },
  {
    "need_id": "E_capture",
    "tool_id": "artifact_json",
    "arguments": [],
    "unresolved_arguments": ["inputs"],
    "input_preparation": {"kind":"artifact","reference":"trace_capture"}
  }
]
```

```sh
engarg eal prepare \
  --input path/to/analysis.json \
  --acquisition-plan .engarg-local/current-need-overrides.json \
  --operator-inputs .engarg-local/operator-inputs.json \
  --output-dir .engarg-local/current-case
```

An older `scoped_json` plan without an explicit operation remains unresolved.
Operation selection belongs to the acquisition declaration. English method text
and old need IDs provide no deterministic mapping between a search and a full
trace read.

To export source and registry files at separately chosen paths, pair
`--operator-inputs` with a new `--preparation-output-dir`:

```sh
engarg eal export \
  --input path/to/analysis.json \
  --acquisition-plan .engarg-local/current-need-overrides.json \
  --operator-inputs .engarg-local/operator-inputs.json \
  --preparation-output-dir .engarg-local/current-inputs \
  --output .engarg-local/argument.eal \
  --tools-output .engarg-local/argument.tools.toml \
  --manifest .engarg-local/argument.bundle.json
```

The exporter fixes input paths before calculating the revision, then stamps that
revision into the input documents. It writes EAL, TOML and bundle files through
the same checked export path. Use `--operator-input-base DIR` when the input file
is a retained snapshot and relative artifact paths belong to its original
directory. The authoring script supplies this base automatically.

Prepared files include `scoped-inputs.json` or `artifact-inputs.json` when those
adapters are selected. Empty or incomplete entries expose the remaining inputs
while their acquisition plans stay unresolved. `collector-preparation.json`
records these separate properties for every need:

| Property | Meaning |
| --- | --- |
| `implementation` | A selected adapter is available or an implementation is missing |
| `configuration` | Declared inputs are bound or remain unresolved |
| `evidence` | Collection is pending, retained artifact pins were verified, or evidence is unavailable |
| `gaps` | Exact remaining operation, configuration or retained-record requirements |

A bound configuration establishes static input completeness. Runtime access and
observation outcomes remain subsequent checks. `evidence_collected` stays false.
The tooling report separately records code generation and package reuse.
`required_calls: 0` and empty package records establish that no implementation
was generated or reused. They leave configuration and evidence status to the
preparation report.

## Existing scoped adapter profile

Existing saved analyses can gain implemented bindings without another model call.
Use [current-case preparation](#prepare-current-case-bindings) to supply exact
need IDs and typed operations in an acquisition override. Bind a direct Kubernetes read to
`kubectl_get` when its context, resource, namespace and name are supplied. Bind
literal Grafana operations to `scoped_json` with an operator-owned inputs path.
Its `needs` map explicitly selects `grafana_search` or `grafana_trace` for each
current need ID. Current-case preparation obtains those operations from the
typed plan and writes the final digest into the new scoped inputs. Source,
context and tools files share that export identity.

The existing `scoped_json` adapter remains available for explicitly retained
infrastructure bindings. Choose it per evidence need and supply its operator-owned
`inputs` path. Preparation without `--operator-inputs` preserves that path.
Each scoped file contains one
Kubernetes configuration and one Grafana configuration, so needs with different
scopes require different input files. The default preparation path preserves
other adapters directly.

An operator-authored scoped file has this shape. Unknown selectors stay unset:

```json
{
  "schema": "engarg.scoped-inputs/1",
  "case_digest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "needs": {
    "E1": "kubernetes",
    "E2": "grafana_search",
    "E3": "grafana_trace"
  },
  "kubernetes": {
    "executable": "kubectl",
    "context": "dev",
    "resource": null,
    "namespace": null,
    "kind": "Application",
    "name": "catalog-sync-operator"
  },
  "grafana": {
    "origin": "https://grafana.example.invalid",
    "datasource_uid": null,
    "query": "{ span.dpe.rollout_kind = \"Application\" && span.dpe.rollout_name = \"catalog-sync-operator\" }",
    "rollout_kind": "Application",
    "rollout_name": "catalog-sync-operator",
    "start_unix": null,
    "end_unix": null,
    "trace_ids": [],
    "token_env": null,
    "tenant_env": null
  }
}
```

Use the export's case digest and real need IDs. Confirm the API resource and
namespace, Grafana datasource UID, positive ordered Unix-second search interval
and explicit trace IDs. The scoped Kubernetes adapter uses the operator's explicit
context, kind and resource name; use `"namespace": "*"` only when an
all-namespace read is intended. Confirm the fully qualified API resource group
against the returned resource identity. The adapter fixes read-only `kubectl get`
arguments and never changes contexts or authenticates automatically. Null or
missing selectors needed by the selected operation cause failure before any
external command or HTTP request.

The Grafana adapters read the configured datasource proxy's search and trace-by-ID
routes. Search transmits the configured query unchanged and adds no cluster
filter. Its equality to the claim's authored expression is an independent review
obligation. A trace read needs 1–32 explicit nonzero hexadecimal trace IDs. Supply
`rollout_kind` and `rollout_name` for full trace inspection; these optional
configuration fields select the identity to inspect. A search summary alone
cannot establish what attributes occur together on a complete span. Trace inspection identifies candidate
same-span matches; it does not establish resource provenance, current deployment
state, complete trace contents or production fleet coverage.

This differs from the companion `eal-collector tempo` adapter. That native adapter
adds a cluster attribute filter for each entry in an approved inventory. Its
bounded positive result supports existential matches in that declared inventory
and interval. An empty first-N search leaves overall trace absence unavailable.
Choose the adapter whose measured query matches the evidence need; neither query
shape establishes all-production completeness from a single `dev` context.

Set `token_env` to `"TEMPO_TOKEN"` only when a bearer token is required, and
`tenant_env` to `"TEMPO_TENANT"` only when a tenant header is required. A configured
name requires a nonempty environment variable. JSON `null` disables that header;
an unset variable does not silently disable a configured header. The example
leaves both credential selectors `null` until operator configuration.
Credential values stay in the environment. Redirects are disabled.

After connecting to the VPN, authenticate separately when authorized for the
selected cluster:

```sh
osprey user login -g dev -o ~/.osprey/config
```

The collector does not invoke this command. Use the actual kubeconfig for
`KUBECONFIG` when required; the Osprey configuration output path alone does not
establish that it is a kubeconfig. The selected context is always explicit.

## Explicit retained artifact inputs

For an evidence need selecting `artifact_json`, provide the operator-owned file
named by that plan's `inputs` argument. Preparation without `--operator-inputs`
preserves the selected path
and existing bytes. An illustrative `artifact-inputs.json` is:

```json
{
  "schema": "engarg.artifact-inputs/1",
  "case_digest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "needs": {
    "E4": {
      "artifacts": [{
        "path": "observations/inventory.json",
        "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "observed_at": "2026-01-01T12:00:00Z",
        "source": "Approved inventory export and its retained provenance"
      }]
    }
  }
}
```

Replace the illustrative digest with the SHA-256 of the exact artifact bytes.
Each artifact must be a JSON object. Each selected need requires 1–16 artifacts.
Relative paths resolve against the inputs file's directory, parent traversal and
symlink escape fail, and absolute paths are explicit operator grants. Inputs and
individual artifacts are limited to 1 MiB, artifact bytes to 4 MiB in aggregate.
Duplicate JSON keys, missing provenance, changed bytes and invalid timestamps
fail acquisition. `observed_at` must retain the original UTC RFC3339 instant,
with `Z` and optional fractional seconds. The aggregate envelope keeps the oldest
artifact time; importing a file never refreshes its measurement age.

Current-case preparation can construct that input document from stable named
references in the operator file:

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

The plan's artifact reference selects this group. Preparation verifies actual
bytes against the supplied hashes and validates JSON shape, timestamps and
provenance fields against the original input directory. It retains canonical
paths in the current-case document.
Missing references or changed files leave `inputs` unresolved. An identified
artifact producer and an existing retained record are separate requirements.
An unsupported producing operation belongs in a `custom` acquisition plan.
Unavailable independent reviews remain explicit evidence gaps until their
authored decisions exist.

## Retained scoped reviews

Review is a separately recorded judgement. `review-inputs.json` starts with
`{"schema":"eal.review-inputs/2","reviews":[]}`. Each record contains the exact
acquisition `request` and a `value` with these fields:

| Field in `value` | Required value |
| --- | --- |
| `criterion`, `target`, `scope` | Exact values from the pending review request; scope must be explicit and nonempty |
| `reviewer` | Identified reviewer; a name alone does not establish independence or authority |
| `verdict` | `supported`, `rejected` or `inconclusive` |
| `reasons` | Nonempty bounded list of stated reasons |
| `reviewed_at` | Original UTC RFC3339 review time |
| `provenance`, `evidence_refs` | Optional retained provenance; these do not silently confer authority |

The standalone `eal-collector review-json --inputs review-inputs.json` command
imports the record. Preparation selects `EAL_COLLECTOR` when explicitly supplied,
otherwise an installed sibling `eal-collector`, otherwise the executable from
`PATH`. The prepared manifest uses a package-relative input path and requires no
EngArg runtime for review import.

The importer preserves every verdict and uses `reviewed_at` as the observation
clock. It rejects different requests, criteria, targets or scopes, missing
identities/reasons and duplicate matching records. Old `accepted` booleans are
not accepted by the current format; retained historical records remain unchanged.
No successful acquisition or authoring result manufactures a review. Explicit EAL
predicates apply reviewer policies and determine whether a verdict supports a
claim or activates a challenge. A supported challenge can weaken the main claim.

## Explicit collection

Once the selected collector inputs and independent decisions are available,
collect explicitly. This example assumes all three named environment variables
are already set and required:

```sh
cargo run --locked --release -- eal collect \
  --input .engarg-local/example/argument.bundle.json \
  --tools .engarg-local/example/argument.tools.toml \
  --workspace . \
  --collector-env KUBECONFIG \
  --collector-env TEMPO_TOKEN \
  --collector-env TEMPO_TENANT \
  --timeout-secs 3600 \
  --output .engarg-local/example/collection.json
```

Omit each `--collector-env` whose variable is unnecessary. Explicit forwarding of
an unset or empty variable fails. Install `eal-host` and its companion executables
as described in the repository README before using collection. The saved bundle
supplies matching context by default; raw EAL requires the generated
`--context .engarg-local/example/context.json` explicitly. Output files must be new.

The existing scoped adapters bound each Kubernetes or Grafana operation to 60 seconds
and 1 MiB of response bytes. Their registry bindings allow 65 seconds per collector.
Other collectors retain the bounds declared by their selected descriptors. The outer
timeout bounds the complete host request. Acquisition errors, measured negative
results and unavailable conclusions remain distinct. Retain the returned
collection ID and use the same source, context and database when assessing it.
