# Support requirements, capability planning and shared evidence

Graph planning separates what would support an argument from the tools currently available. It is opt-in:

```sh
engarg analyse \
  --claim 'The identified service meets the stated latency requirement over the stated interval.' \
  --mode hybrid \
  --evidence-planning graph \
  --evidence-config planning.json \
  --format json \
  --output analysis.json

engarg eal export \
  --input analysis.json \
  --output argument.eal \
  --tools-output argument.tools.toml \
  --manifest argument.bundle.json
```

The export also writes `argument.execution-plan.json`, `argument.evidence-plan.json`, `argument.acquisition-requests.json` and `argument.review-requests.json`. Export and preparation do not collect evidence. Use the existing EAL host `validate`, `plan`, `collect` and `assess` commands with the generated source and registry; missing evidence and independent reviews remain explicit obligations.

## Implementation baseline and compatibility

The implementation was developed against these exact revisions:

| Repository | Baseline |
| --- | --- |
| EngArg authoring work | `fcccdc825e0e385029b78a7c9f59046f35fd0818` |
| EngArg main merged before this work | `f5628778c474b4514e1a3cca19621e75b263ef4d` |
| EngArg resulting local merge | `9802758f40ef2b5e4526dfb86f51bb66f0387ece` |
| EAL | `da9279fd5d2b1107335d218c8bf7c2b685667df6` |

`--evidence-planning flat` remains the default. Existing argument-only, flat acquisition and v1/v2/v3 export paths retain their interfaces. Graph reports add `evidence_planning` and use a separate support-export wrapper containing the original authored v3 case. The generated support gates use the existing EAL/3 grammar; this work does not introduce a new language syntax.

An old prose evidence plan does not contain the measurement semantics required for graph execution. Keep using flat mode, or author a graph report with explicit support contracts and review its requirements. Selecting an old adapter, supplying an old argument digest, or attaching old need IDs does not migrate those semantics or establish current source bindings. Flat catalogue and per-need overrides cannot override a retained support graph.

## Five increments and acceptance evidence

| Increment | Integrated behaviour | Decisive regression coverage |
| --- | --- | --- |
| Tool-independent requirements | Synthesis emits support obligations before matching. Requirements retain authored targets, original acceptance clauses, method, units, scope, coverage, time and provenance. Revisions require the prior requirement digest and a reason; retained targets cannot silently drop requirements, and changing an original acceptance clause is rejected. | `src/support_requirements/tests.rs`: eighteen tests covering semantic flow, exact unit scaling, percentile/mean distinction, identity membership, scope/time lineage, revision, retirement and acceptance clauses. Four engine regressions include `six_calls_extract_without_catalogues_then_critic_sees_routes_and_final_recompiles` in `src/engine/support.rs`. |
| Qualified capabilities and deterministic selection | Hard compatibility precedes explicit lexicographic cost ranking. An interface description alone supplies no measurement qualification. Provider type has no hidden preference. Unknown costs remain unknown. | EAL `crates/eal-collector-contracts/tests/capabilities.rs`: seventeen tests covering method/subject mismatch, actual input scope, witness versus population absence, completeness, historical arguments, exact JSON identity and cost ranking. |
| Bounded discovery and existing tools | Only configured MCP connections and explicitly approved catalogues are examined. Persistent SDK sessions discover and invoke existing tools through a generic normalisation bridge. Changed schemas, missing authentication, forged scope and invalid time are rejected. | EAL `crates/eal-evidence/tests/mcp_bridge.rs`: thirteen SDK/loopback integration tests, including schema drift, credential-scope changes, bounded JSON text, catalogue redirect refusal, statistical coverage, negative measurements and admission limits. Native transports have eight scenario groups in `tests/native_providers.rs`. |
| Shared execution and typed evaluation | Source operations form a validated dependency graph. One acquisition may feed several separate criteria. Fan-out, concurrency, request and byte limits are bounded. Reuse retains original observation time and provenance; immutable observation snapshots survive refresh. | EAL `crates/eal-evidence/tests/execution.rs`: twenty-four integration tests for criteria sharing, physical source identity, time, fan-out, exact quantities and bounded execution. Four `tests/observations.rs` tests cover immutable retention; five `tests/retained_judgement.rs` tests cover scoped reviewer records. EngArg `tests/support_export.rs` has fifteen tests for dependency gates, projections, tamper rejection, CLI companion files, exact source freshness and actual EAL host collection. |
| Capability implementation fallback | Only genuine operation/transformation gaps can request implementation after bounded discovery. New code receives frozen synthetic contract fixtures, observed test results and a rehashed package before registration. Parameterised verified packages can be reused across arguments. | `src/tooling/capability_tests.rs`: twelve tests covering two-call generation, one-call frozen retry, zero-call cache reuse, distinct caller IDs/criteria/aliases, opaque schema/context roundtrips, budget admission and source tampering. Verification/package tests reject corrupted opaque metadata and receipts predating its mandatory probe. `src/tooling_commands/capability.rs` tests non-implementation gaps without Docker/model calls. `tests/tooling_docker.rs` adds a real Rust capability generation/reuse regression to the existing Docker CI job. |

The ordinary successful hybrid argument uses six logical model calls: Constructor, Sceptic, EvidenceAuditor, Synthesiser, FinalCritic and Adjudicator. Matching after synthesis and after adjudication is deterministic. The FinalCritic sees the candidate routes and their limitations. Other argument authors do not receive the capability catalogue. Argument repairs use the existing argument budget; implementation has its own call and time budgets. Single mode remains a one-call baseline.

The catalog-sync authoring script defaults to graph planning and requires its operator configuration:

```sh
bash scripts/run-catalog-sync-authoring.sh \
  --evidence-config operator-planning.json --implement-tools inputs.json
```

`--legacy-flat inputs.json [catalogue.json]` selects the earlier script workflow. The script-only collector value `managed` resolves to the selected runtime's pinned `eal-collector` in its retained planning snapshot. `--check-only` validates and snapshots inputs without probing those binaries.

With graph `--implement-tools`, the script first retains `analysis.original.json` without enabling coding. It inspects the typed route gaps before preparing the default Rust image, then invokes the separately budgeted implementation command. Configuration-only and deferred-discovery runs require no Docker or coding calls. The final `analysis.json` is published only after checking that implementation preserved the complete original analysis apart from its tooling metadata.

These are synthetic, local and loopback acceptance checks. They do not establish the truth of a private deployment claim, qualify a scientific method, or verify private credentials and historical records. The real Docker regressions require the documented preloaded images and are not executed when Docker is unavailable.

The versioned contracts are inspectable without model calls:

```sh
engarg schema support
engarg schema execution-plan
engarg schema capabilities
```

## Operator planning configuration

`planning.json` selects the collector binary, connection configuration, source store, explicit cost order and admitted input bindings. Relative paths resolve against this file. Only named environment variables are forwarded; credential values stay in the operator environment.

```json
{
  "schema": "engarg.capability-planning-config/1",
  "collector": "/opt/eal/bin/eal-collector",
  "connections": "connections.json",
  "store": "observations",
  "environment": ["PATH", "READ_TOKEN"],
  "ranking": {
    "dimensions": ["configuration_effort_seconds", "execution_requests", "execution_latency_ms"]
  },
  "scheduling": {
    "maximum_concurrent_sources": 4,
    "per_provider": {"service-measurements": 2}
  },
  "bindings": [{
    "capability_id": "service_measurement",
    "arguments": {"service": null, "query": null},
    "input_bindings": {"/query": "/query"},
    "configured_prerequisites": ["read_connection"]
  }]
}
```

`input_bindings` maps a physical provider input pointer to the original logical acquisition input pointer. Qualified `scope_bindings` separately bind selectors such as `service` to actual input values. A conflicting operator value stays a configuration gap. Listing a prerequisite acknowledges its configuration; runtime still checks the actual connection, authentication scope and current interface.

Dynamic dependency and fan-out slots follow those same physical mappings. Their eventual values undergo full runtime schema admission. Statically known inputs are checked during planning; dynamic slots cannot replace a fixed selector or historical interval. Missing object parents can be prepared along declared bindings, while operator arrays are never resized.

For example, two requirements can compare one measured `latency_p99` quantity in milliseconds against different thresholds. The measurement operation accepts the service and interval as parameters. Each requirement retains its own comparator and threshold. A calculation of a mean cannot substitute for the percentile, and a bounded search cannot establish population-wide absence.

Built-in calculation transfers have closed method identities: `engarg.sum/1`, `engarg.mean/1`, `engarg.count/1`, `engarg.minimum/1`, `engarg.maximum/1`, `engarg.scale/1`, `engarg.identity-equal/1` and `engarg.any-identity-equal/1`. Their result contracts use calculation support and exact-calculation strength; measurement source contracts retain their original methods, scope and provenance. Sample summaries identify their source operations in a lexically ordered `sample_sources` selector while preserving the input selectors. Population totals additionally require complete lineage for the same population and selectors. Identity comparisons use subject kind `identity_relation`, an `operands` selector mapping source IDs to their exact original subject scopes, and `operand_times` mapping direct measurement and selector-ancestor source IDs to their exact original time requirements. This permits current observations to be compared with an older selection without pretending that their windows match. Lifecycle, temporal causation and applicability remain separate obligations.

`time.historical_interval` constrains the original observation clock. A current query over historical events instead places its event window in subject selectors and physical input bindings. An immutable-ID detail lookup may use its own current-observation clock after a historical selector, while retaining that selector's original interval in lineage. If the detail lookup explicitly declares a historical interval, it must equal the selector's interval.

`IdentityEqual` requires matching scalar/collection shapes. `AnyIdentityEqual` accepts a scalar identity followed by a collection, uses exact equality without coercion, and returns `any_identity_equal` with witness coverage. A false result over incomplete input remains pending. Every source retains its own strict age bound, while the consumer declares the accepted age of its oldest lineage. A fan-out quantity reference such as `/latency` denotes an array of that logical quantity, rather than an array of unprojected response records.

EAL/3 requires a freshness bound on machine evidence. Graph requirements and evaluations therefore need an explicit positive `max_age_seconds` from 1 to 315360000; missing bounds fail with `SUPPORT_FRESHNESS_CLARIFICATION`. The EAL gate uses that exact requirement bound. It does not silently tighten it through the legacy flat export-age policy. Unavailable/judgement stubs with no age use `max_age 0` and never produce observations. Independent review freshness retains its separate policy.

## MCP connections and qualification

This complete connection configuration discovers an existing tool without generating a tool-specific adapter:

```json
{
  "schema": "eal.evidence-config/1",
  "connections": [{
    "id": "measurements",
    "authentication_scope": "service-readers",
    "transport": {
      "kind": "http",
      "url": "https://measurements.example/mcp",
      "token_env": "READ_TOKEN"
    },
    "tools": [{
      "tool": "read_measurement",
      "raw_output_schema_sha256": null,
      "response_format": {"kind": "structured"},
      "value_pointer": "/measurement",
      "observation_time": {"kind": "pointer", "pointer": "/observed_at"},
      "scope_pointers": {"service": "/service"}
    }],
    "timeout_seconds": 30,
    "output_bytes": 1048576
  }],
  "native_providers": [],
  "local_catalogues": [],
  "approved_catalogues": [],
  "discovery": {
    "requests": 64,
    "bytes": 4194304,
    "candidates": 128,
    "pages_per_connection": 16,
    "timeout_seconds": 60
  }
}
```

This example deliberately supplies no qualification catalogue, so discovery alone cannot make the tool an adequate evidence route. `raw_output_schema_sha256: null` requires the raw MCP tool to omit an output schema; if it advertises one, pin its exact schema digest instead. The normalised measurement schema is pinned separately in the qualified capability descriptor. Bounded JSON text can be selected with `{"kind":"json_text","block":0}` when structured content is unavailable.

A stdio connection uses the same mappings with an operator-owned transport:

```json
{
  "kind": "stdio",
  "command": "/opt/measurements/bin/mcp-server",
  "arguments": ["--read-only"],
  "inherit_env": ["READ_TOKEN"],
  "cwd": "/opt/measurements"
}
```

An approved catalogue URL is an explicit `{"url":"https://approved.example/capabilities.json"}` entry. Registry discovery does not authorize an advertised execution endpoint. Redirects are refused. Discovery stops at its configured aggregate limits and reports incompleteness; an incomplete search cannot justify generating a replacement tool.

The CLI first uses `eal-collector evidence-discover --configured-only` to examine configured sources without requesting approved registries. If every required operation has a ready qualified route, no registry request is needed. Otherwise it expands discovery through the explicitly approved catalogues once, retains both phases' metrics, and rematches the unchanged requirements. Both phases share the operator's aggregate budget: conservative `budget_usage.requests_upper_bound`, `bytes_upper_bound` and per-connection page allocations determine the remainder. Failed calls consume their allocations. Observed metrics remain separate from those upper bounds. This process does not rewrite the operator's connection file.

A qualified `CapabilityDescriptor` must identify the provider/version, input and output schema digests, configuration identity, authentication scope and a separately reviewed `MeasurementSemantics`. Those semantics identify support type, subject kinds, versioned methods, input scope bindings, output quantity pointers/units, coverage guarantees, original-time extraction and provenance. Complete enumeration requires a measured completeness field; statistical support requires a measured positive sample count. Unknown source authority, identity or uncertainty remains a gap.

Construct identities through `schema_identity`, `ConnectionConfig::configuration_identity` and `CapabilityDescriptor::measurement_identity`. Recompute them when the operator configuration or reviewed qualification changes. Do not copy a digest from another argument or infer qualification from MCP description text. The SDK client currently negotiates MCP **2025-11-25**; no support for a later protocol version is asserted.

## Native command, HTTP and retained sources

`EvidenceConfig.native_providers` admits existing non-MCP sources through the same execution graph. A `NativeProviderConfig` contains a qualified capability, a fixed transport, an explicit output mapping and optional pinned files. Requests cannot choose the executable, endpoint or retained file.

The following are transport fragments inside that typed configuration:

```json
{
  "kind": "command",
  "program": "/opt/engarg/bin/engarg",
  "supervisor": "/opt/engarg/bin/engarg-process-supervisor",
  "arguments": ["eal", "tool-run", "--package", "/srv/tools/package/manifest.json"],
  "cwd": "/srv/tools",
  "inherit_env": ["PATH", "READ_TOKEN"],
  "protocol": {
    "kind": "eal_acquisition",
    "environment": "development",
    "tool": "service_measurement",
    "tool_version": "1",
    "context": {"operator_scope": "service-readers"}
  }
}
```

```json
{
  "kind": "http_get",
  "url": "https://measurements.example/read",
  "query_bindings": {"service": "/service"},
  "token_env": "READ_TOKEN"
}
```

```json
{
  "kind": "retained_artifact",
  "path": "/srv/evidence/measurement.json",
  "sha256": "<SHA-256 of the existing retained file>"
}
```

A raw JSON command uses `"protocol":{"kind":"raw_json"}`. An EAL command receives the full `AcquisitionRequest` and returns an `ObservationEnvelope`, preserving the supplied input and context. Its evidence ID is the current source operation, rather than a frozen argument need ID. Native mappings use `response_format: {"kind":"structured"}` and the same value, observation-time and scope pointers as MCP mappings. Retained files must already exist, match their digest and retain their original observation time; an artifact reader does not produce absent evidence or an independent review.

Use `NativeProviderConfig::configuration_identity()` for the provider's configuration digest. Its top-level `credential_env` lists secret environment references, such as `["READ_TOKEN"]`, selected from the command's `inherit_env`; their values are inspected before retaining observations. For a generated package, include its manifest path/hash in top-level `pinned_files`; the `engarg eal tool-run` wrapper also reopens and rehashes its verified source and test receipt before execution. Native configuration and domain qualification are separate from the generated function-test receipt.

## Optional implementation and retries

```sh
engarg eal implement \
  --input analysis.json \
  --tool-language rust \
  --toolchain examples/toolchains/rust.json \
  --tool-store /srv/engarg/tool-store \
  --tool-output-dir tooling-attempt \
  --tool-max-calls 4
```

The same tool options are available with `analyse --implement-tools`. Use a new output directory for each attempt. The persistent package store provides reuse. `tooling.capabilities` records capability packages and consumer bindings; it does not replace the original `evidence_planning` requirements or pretend that unresolved configuration is implemented code.

A new reusable contract takes two model calls: draft and freeze synthetic fixtures, then implement code without the fixture oracle. A failed implementation retry reuses the frozen contract and needs one coding call. A verified, unchanged package needs zero model calls. The key contains the canonical parameterised support contract, input/output schemas, operation purpose and declared limits, together with language and admitted toolchain/runner identity. Consumer claim IDs, need IDs and acceptance thresholds are excluded. Runtime selectors remain parameters.

Mandatory protocol probes also preserve huge integers, precise decimals, escaped text and literal objects keyed `$serde_json::private::Number` or `$serde_json::private::RawValue`. The Rust SDK enables `raw_value`; its reference collector streams opaque input/context through `RawValue` instead of a `Value` intermediate. Receipt version `engarg.tool-tests/2` requires these checks. Older receipts cannot authorize registration or reuse; rerun implementation with the current runner rather than editing receipt metadata.

Configuration, missing credentials, unavailable source records, absent historical coverage, clarification, judgement and missing scientific qualification do not trigger coding. Passing contract fixtures permits registration of the tested function interface only: its generated `CapabilityDescriptor.semantics` remains absent. A separate operator/domain qualification is required before it can become an adequate measurement route. The retained receipt states these limits explicitly. Editing saved metadata to broaden that qualification is rejected by package revalidation.

A related qualified capability must share the subject kind, support type and an acceptable method. Its insufficient scope, units, coverage, time or provenance stays a capability mismatch. An unrelated tool does not suppress a genuine missing operation. Missing calculation and proof-check operations are classified as `transformation_implementation`; other unsupported operations use `operation_implementation`. A schema-compatible interface without reviewed semantics remains a qualification gap.

## Source reuse and metric boundaries

Source identity includes provider/version, physical arguments, schema and qualification identities, configuration/authentication fingerprints, scope and historical interval. Reuse retains the original observation time and lineage. Requirement IDs, acceptance prose and thresholds are evaluated separately. Shared use of one source is reported as one source, rather than independent corroboration. A stale source or failed acquisition is pending; a successfully observed false value can fail a criterion. Neither becomes a fabricated positive observation.

Physical source IDs remain stable when an instrument produces a new observation. Each distinct observation is retained as a signed, hashed immutable `observations/<id>.json` snapshot. Reports and collector envelopes expose observation IDs, value hashes, original timestamps and relative paths; `SourceStore::load_observation` verifies retained snapshots. Refresh never silently overwrites the earlier raw record, including when two age policies use different observations from one source. Capacity exhaustion is explicit and does not evict earlier evidence.

An interrupted acquisition with an uncertain outcome retains a pending attempt instead of silently issuing the source call again. `eal-collector evidence-reconcile` provides explicit reconciliation after the previous read has ended; active reads cannot be reconciled. Separately configured scoped judgement records can be acquired through an identified reviewer and evaluated, while absent or unscoped independent judgement remains pending.

A retained judgement uses `eal.retained-judgement/1` with reviewer identity, exact criterion and subject scope, original review time, verdict, reasons and nonempty provenance. Supported/rejected/inconclusive records map to satisfied/not-satisfied/pending only after correspondence to a separately qualified judgement source is checked. The retained `judgement_record_correspondence_only` label expressly leaves the review's truth and reasoning quality unverified.

MCP discovery reports successful initialise/list/call requests and explicitly approved catalogue GETs, logical JSON request/result bytes, elapsed time and persistent owner starts. These bytes exclude HTTP/TLS framing. Execution reports actual source attempts, reused records, fresh normalised value bytes and provider-reported request/byte counts, including interface admission when a cached source is reused. Unknown failure bytes are not invented. `generated_code_count` in execution remains zero because execution does not generate code; implementation counts belong to the tooling stage.

MCP/native concurrency and rolling-minute acquisition-attempt limits apply to one shared bridge/provider instance and its clones. Separate collector processes have separate admission budgets; any quota spanning clients/processes must also be enforced by the remote service. Failed acquisitions consume their admission and do not trigger an automatic replay.

These counters describe their measured boundaries, not monetary cost or a controlled comparison with a different topology. Provider token counts remain provider-reported. The implementation supplies deterministic ranking and observable reuse; it does not claim a measured universal token, latency or price optimum.

Two retained loopback measurements exercise sharing at different boundaries:

| Fixture and measured boundary | Separate acquisitions | Shared source graph |
| --- | --- | --- |
| Three threshold evaluations in one runtime: source requests / protocol requests / logical request bytes / logical response bytes | `3 / 10 / 285 / 2514` | `1 / 5 / 95 / 1082` |
| Two ordinary EAL collections in separate collector subprocesses: source tool calls / list calls / HTTP requests / HTTP request-body bytes / HTTP response-body bytes | `2 / 6 / 12 / 1212 / 2356` | `1 / 4 / 9 / 902 / 1568` |

The EAL repository retains the exact observations and fixture scope in `examples/support-planning/source-sharing-measurement.json` and `examples/support-planning/eal-collection-measurement.json`. Both fixtures generated zero tool implementations. Original source time was retained and each threshold produced its own result. These are individual local synthetic runs, not production throughput or universal topology comparisons; their elapsed times must not be used as a general performance estimate.

Reproduce the focused checks from the appropriate repository:

```sh
# EngArg
cargo test --lib support_requirements
cargo test --lib engine::support
cargo test --lib tooling::capability
cargo test --bin engarg tooling_commands::capability
cargo test --test support_export
# Requires Docker and the preloaded toolchain images:
cargo test --test tooling_docker -- --include-ignored

# EAL
cargo test -p eal-collector-contracts --test capabilities
cargo test -p eal-evidence --test mcp_bridge
cargo test -p eal-evidence --test execution
cargo test -p eal-evidence --test native_providers
cargo test -p eal-evidence --test retained_judgement
cargo test -p eal-evidence --test observations
cargo test -p eal-collectors --test evidence
EAL_EVIDENCE_METRICS_OUTPUT=/tmp/eal-collection-measurement.json \
  cargo test -p eal-collectors --test support_graph_acceptance -- --nocapture
```
