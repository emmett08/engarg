# Private collector inputs

`eal prepare` turns a saved EngArg analysis into a private EAL case with working
Rust collector commands and separate operator input files. It preserves the
argument graph and every independent review obligation. Preparation makes no
model calls, runs no login commands and collects no evidence.

Run from the repository root with a new output directory:

```sh
cargo run --locked --release -- eal prepare \
  --input path/to/analysis.json \
  --output-dir .engarg-local/example \
  --kube-context dev \
  --grafana-origin https://grafana.example.invalid
```

Use the actual approved origin only in your private invocation. The generated
registry contains the absolute path of the executable running `prepare`.
Rebuild at that path or prepare a new directory when changing the executable's
location. Preparation proposes scoped Kubernetes or Grafana reads where the saved
acquisition plan identifies those operations; remaining needs use retained JSON
artifacts. Review the proposed operation for every need before collecting.

| Generated file | Purpose |
| --- | --- |
| `argument.eal` | EAL/3 source with acquisition bindings and original review gates |
| `argument.bundle.json` | Matching source, canonical argument, identifier maps and export context |
| `argument.tools.toml` | Executable `scoped_json`, `artifact_json` and `review_json` command bindings |
| `acquisition-inputs.json` | Acquisition-plan records, each bound to its exact evidence need |
| `collector-inputs.json` | Operator-selected Kubernetes and Grafana operations and selectors |
| `artifact-inputs.json` | Pinned retained artifacts for the remaining evidence needs |
| `review-inputs.json` | Initially empty independently authored review decisions |
| `review-requests.json` | Exact pending six-field EAL review requests |
| `context.json` | Matching `engarg_export_digest` environment context |

Keep these files together. Preparation creates a new export digest because it
changes acquisition bindings. Inputs from an older export do not become valid by
copying their digest into the new files. Recheck each acquisition and review
against the new exact request.

`.gitignore` excludes `engarg-runs/`, `.engarg-local/`, `*.local.json` and
`*.local.toml`. Keep actual internal endpoints, inventories, observations and
operator inputs there. Tracked documentation and test fixtures use sanitized
examples. Ignore rules do not remove files already tracked by Git.

The scoped input file has this shape. The example deliberately leaves unknown
selectors unset:

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

Use the generated case digest and real need IDs. Confirm the API resource and
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
an unset variable does not silently disable a configured header. The generated
file leaves both credential selectors `null` until operator configuration.
Credential values stay in the environment. Redirects are disabled.

After connecting to the VPN, authenticate separately when authorized for the
selected cluster:

```sh
osprey user login -g dev -o ~/.osprey/config
```

The collector does not invoke this command. Use the actual kubeconfig for
`KUBECONFIG` when required; the Osprey configuration output path alone does not
establish that it is a kubeconfig. The selected context is always explicit.

For investigations requiring inventories, controller configuration, lifecycle
history or other retained records, populate `artifact-inputs.json`:

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

Once the needed selectors, artifacts and independent decisions are available,
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

The scoped adapters bound each Kubernetes or Grafana operation to 60 seconds and
1 MiB of response bytes; the registry allows 65 seconds per collector. The outer
timeout bounds the complete host request. Acquisition errors, measured negative
results and unavailable conclusions remain distinct. Retain the returned
collection ID and use the same source, context and database when assessing it.
