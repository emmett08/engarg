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
argument to use EngArg's built-in `artifact_json`, `http_json_get` and `custom`
planning tools. These describe prospective acquisition; reading settings into a
claim does not create executable bindings. Kubernetes evidence still needs
retained artifacts or an explicitly configured compatible collector. Review each
generated binding and unresolved parameter before collection.

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

Full runs require a clean tracked checkout, the repository's pinned Rust toolchain
and a compatible installed `eal-host` with its sibling `eal-mcp`. Build the EAL
host from the revision pinned in `Cargo.toml`, following the
[installation instructions](../README.md#eal3-export-and-mcp-assessment).
The host can be on `PATH` or selected explicitly:

```sh
export ENGARG_EAL_HOST="/absolute/path/to/eal-host"
# Export OPENAI_API_KEY through your normal secret-management mechanism.
bash scripts/run-catalog-sync-authoring.sh \
  .engarg-local/catalog-sync-operator-20261024T120522Z/collector-inputs.json
```

The script builds the EngArg binary and process supervisor, then uses their
absolute paths. The default model is `gpt-6.1-sol`, with high reasoning effort,
the Responses API and the default service tier. Six planned model calls plus
one repair permit seven logical calls. Three HTTP retries allow up to four HTTP
attempts per logical call. No inference occurs during `--check-only`.

Environment overrides retain the downloaded script's `ENGARG_MODEL`,
`ENGARG_SERVICE_TIER`, `ENGARG_REASONING_EFFORT`, `ENGARG_MAX_REPAIRS` (0–3),
`ENGARG_REQUEST_TIMEOUT_SECS`, `ENGARG_TIMEOUT_SECS`, `ENGARG_MAX_INPUT_BYTES`,
`ENGARG_MAX_RESPONSE_BYTES`, `ENGARG_MAX_OUTPUT_TOKENS` and
`ENGARG_MAX_CONCURRENT_REQUESTS`. Freshness policy overrides are
`ENGARG_REVIEW_MAX_AGE_SECONDS` and `ENGARG_ACQUISITION_MAX_AGE_SECONDS`.

Each run retains request and input identity, build and binary provenance, the
analysis audit, all renders, the EAL bundle, validation result, evidence plan and
`run-summary.json` in a unique private directory. Failures preserve existing run
artifacts. A passing run means authoring and deterministic planning succeeded;
`evidence_collected` remains `false`.
