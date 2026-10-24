# Private collector inputs

`eal prepare` turns a saved EngArg analysis into seven private EAL files. Each
evidence need keeps its chosen collector and inputs. Preparation retains every
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
| `acquisition-inputs.json` | Saved acquisition-plan records with original inputs and unresolved parameters |
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

Use the same option during `engarg analyse` to make selected collectors available
while the argument is being constructed. The run retains that catalogue for
export and preparation. A custom scientific collector can supply its own
versioned descriptors. Input schemas support JSON arrays and numbers as well as
strings. Named argument values encode non-string inputs as JSON, for example
`{"name":"channels","value":"[0,2]"}`. Templates keep the executable under
operator control and permit parameters only in declared argument positions.

Each need keeps its exact adapter. A method mentioning a full query remains an
HTTP read when its plan selects `http_json_get`. Separate needs retain separate
URLs and selectors. Existing operator-owned artifact files stay at their chosen
paths. Kubernetes and Grafana settings belong to explicitly selected bindings.

V2 export identity binds the projection semantics to the canonical case and
acquisition plan with its selected descriptors. Producer package updates preserve
that identity when semantics remain compatible. V1 bundles use their recorded
producer identity during verification. A changed request or source requires its
own matching observation. Copying an old digest into a new input file cannot
establish that correspondence.

`.gitignore` excludes `engarg-runs/`, `.engarg-local/`, `*.local.json` and
`*.local.toml`. Keep internal endpoints and retained observations in those
private directories. Tracked documentation and test fixtures use sanitised
examples. Ignore rules apply to future additions.

## Existing scoped adapter profile

Existing saved analyses can gain implemented bindings without another model call.
Supply exact need IDs in an acquisition override file and re-export the retained
`analysis.json` using `--acquisition-plan`. Bind a direct Kubernetes read to
`kubectl_get` when its context, resource, namespace and name are supplied. Bind
literal Grafana operations to `scoped_json` with an operator-owned inputs path.
Its `needs` map must explicitly select `grafana_search` or `grafana_trace` for each
new need ID. The exporter preserves that path and does not infer operation
assignments from prose. After re-export, use the new bundle's case digest in the
scoped inputs. Existing source, context and tools files must retain matching
export identity; editing only a registry cannot establish that correspondence.

The existing `scoped_json` adapter remains available for explicitly retained
infrastructure bindings. Choose it per evidence need and supply its operator-owned
`inputs` path. Preparation preserves that path. Each scoped file contains one
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
named by that plan's `inputs` argument. Preparation preserves the selected path
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

## Independently retained reviews

Independent review is a separate recorded decision. `review-inputs.json` starts
with `{"schema":"engarg.review-inputs/1","reviews":[]}`. An empty file cannot
discharge a review gate. Each authored record has these fields:

| Field | Required value |
| --- | --- |
| `request` | Exact `tool`, `tool_version`, `input` and `context` copied from one pending request in `review-requests.json` |
| `observed_at` | Original UTC RFC3339 review time |
| `value` | JSON object containing an explicit boolean `accepted`, with reviewer provenance and reasoning as appropriate |

For example, a reviewer may retain
`{"accepted":false,"reviewer":"independent reviewer","reason":"inventory coverage remains unresolved"}`
as the record's `value`. The importer preserves negative decisions. It accepts
exactly one matching correspondence record and rejects changed obligations,
changed scoped context, stale export digests, missing decisions and duplicate
matches. The program never derives `accepted` from successful acquisition or an
argument generator's verdict. EAL applies each evidence declaration's age limit
to the retained observation time.

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
