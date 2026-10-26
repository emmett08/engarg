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

The 25 October 2026 completion review starts from EngArg default-branch revision
`803984920cbfb1ec9a3c5aebb349a8f02016cd38` and EAL default-branch revision
`92b9089`. The original planning PRs are merged. The dependency revision in
`Cargo.toml`, `Cargo.lock` and the producer/consumer CI job identifies the EAL
version required by this checkout. The findings register and verification
results are in [VALIDATION.md](../VALIDATION.md).

`--evidence-planning flat` remains the default. Existing argument-only, flat acquisition and v1/v2/v3 export paths retain their interfaces. Graph reports add `evidence_planning` and use a separate support-export wrapper containing the original authored v3 case. The generated support gates use the existing EAL/3 grammar; this work does not introduce a new language syntax.

An old prose evidence plan does not contain the measurement semantics required for graph execution. Keep using flat mode, or author a graph report with explicit support contracts and review its requirements. Selecting an old adapter, supplying an old argument digest, or attaching old need IDs does not migrate those semantics or establish current source bindings. Flat catalogue and per-need overrides cannot override a retained support graph.

## Five increments and acceptance evidence

| Increment | Integrated behaviour | Decisive regression coverage |
| --- | --- | --- |
| Tool-independent requirements | Synthesis emits support obligations before matching. Requirements retain authored targets, original acceptance clauses, method, units, scope, coverage, time and provenance. Revisions require the prior requirement digest and a reason; retained targets cannot silently drop requirements, and changing an original acceptance clause is rejected. | `src/support_requirements/tests.rs`: eighteen tests covering semantic flow, exact unit scaling, percentile/mean distinction, identity membership, scope/time lineage, revision, retirement and acceptance clauses. Four engine regressions include `six_calls_extract_without_catalogues_then_critic_sees_routes_and_final_recompiles` in `src/engine/support.rs`. |
| Qualified capabilities and deterministic selection | Hard compatibility precedes explicit lexicographic cost ranking. An interface description alone supplies no measurement qualification. Provider type has no hidden preference. Unknown costs remain unknown. | EAL `crates/eal-collector-contracts/tests/capabilities.rs`: seventeen tests covering method/subject mismatch, actual input scope, witness versus population absence, completeness, historical arguments, exact JSON identity and cost ranking. |
| Bounded discovery and existing tools | Only configured MCP connections and explicitly approved catalogues are examined. Persistent SDK sessions discover and invoke existing tools through a generic normalisation bridge. Changed schemas, missing authentication, forged scope and invalid time are rejected. | EAL `crates/eal-evidence/tests/mcp_bridge.rs`: thirteen SDK/loopback integration tests, including schema drift, credential-scope changes, bounded JSON text, catalogue redirect refusal, statistical coverage, negative measurements and admission limits. Native transports have eight scenario groups in `tests/native_providers.rs`. |
| Shared execution and typed evaluation | Source operations form a validated dependency graph. One acquisition may feed several separate criteria. Fan-out, concurrency, request and byte limits are bounded. Reuse retains original observation time and provenance; immutable observation snapshots survive refresh. | EAL `crates/eal-evidence/tests/execution.rs`: twenty-four integration tests for criteria sharing, physical source identity, time, fan-out, exact quantities and bounded execution. Four `tests/observations.rs` tests cover immutable retention; five `tests/retained_judgement.rs` tests cover scoped reviewer records. EngArg `tests/support_export.rs` has fifteen tests for dependency gates, projections, tamper rejection, CLI companion files, exact source freshness and actual EAL host collection. |
| Capability implementation fallback | Only genuine operation/transformation gaps can request implementation after bounded discovery. New code receives frozen synthetic contract fixtures, observed test results and a rehashed package before registration. Parameterised verified packages can be reused across arguments. | `src/tooling/capability_tests.rs` covers two-call generation, one-call frozen retry, zero-call cache reuse, distinct caller IDs/criteria/aliases, exact algorithm/unit/schema transport, budget admission, qualification boundaries and source tampering. Shared completion tests reject oversized content before audit retention while retaining paid-call token usage. Verification/package tests reject corrupted opaque metadata and receipts predating mandatory probes. CLI tests retain non-implementation gaps without Docker/model calls. `tests/tooling_docker.rs` includes real Rust generation/reuse and the generated → pending qualification → explicit registration → zero-model-call replan workflow in Docker CI. |

The ordinary successful hybrid argument uses six logical model calls: Constructor, Sceptic, EvidenceAuditor, Synthesiser, FinalCritic and Adjudicator. Matching after synthesis and after adjudication is deterministic. The FinalCritic sees the candidate routes and their limitations. Other argument authors do not receive the capability catalogue. Argument repairs use the existing argument budget; implementation has its own call and time budgets. Single mode remains a one-call baseline.

## Incomplete designs and saved-run repair

Argument validation, acquisition design and source collection have separate
completion states. A contract or dependency error is a design defect rather than
evidence that the source does not exist. Historical designs removed during
adjudication remain available in `evidence_planning.design_review` as explicitly
non-executable recovery inputs. A pending design produces an incomplete-planning
warning and leaves completed responses and artefacts available.

Repair a saved support design with its own bounded model-call allowance:

```sh
engarg eal repair --input analysis.json --evidence-config planning.json \
  --model gpt-6.1-sol --max-repairs 1 --progress \
  --audit repaired-support.audit.jsonl --output repaired-analysis.json
```

The repair retains the final argument and paid transcript, checks the support
requirements against their original acceptance criteria, and checkpoints each
new completion. It can use earlier acquisition blueprints without promoting
invalid drafts into executable routes. Existing `engarg eal replan` refreshes
capability selection for an unchanged support elaboration and makes no authoring
model calls. It cannot reconstruct a removed blueprint. New implementations
still require explicit operation specifications, function-contract verification
and separate source or method qualification.

The catalog-sync summary reports authored blueprints separately from compiled
route entries. `source_collection.ready_acquisition_ids` lists configured routes
whose dependencies, including dependencies through derivations, are available.
This can identify a usable source subset while independent reviews or other
sources remain pending. The script reports the complete emitted evidence budget
separately. Its evidence-declaration count includes reviews and does not predict
physical acquisition calls. The script retains artefacts and reports incomplete
planning without claiming collected evidence or verified inference.

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

Selection compares bounded combinations using the configured cost order. Equivalent symbolic request groups are charged once even when requirements consume different quantities. Unresolved references can converge after binding, and freshness can require later refreshes, so these estimates do not predict exact runtime requests. Configuration and implementation effort are charged once per capability. Fan-out execution costs remain unknown because per-invocation estimates cannot predict the returned identifier count or witness position; setup costs stay explicit. `estimated_distinct_acquisitions` is null when selected inputs depend on runtime outputs. `estimated_distinct_acquisitions_upper_bound` sums per-step source-operation invocation bounds without a reuse discount, using each fan-out maximum. It excludes transport admission and any provider-internal requests. Runtime metrics report actual requests and transferred bytes.

The deterministic search retains at most 32 partial plans and considers at most 64 ranked candidates per source. Backpointers retain selected choices, and only surviving states copy compact integer source sets. `selection.search_complete` reports pruning. Complete enumeration establishes a minimum only within the admitted candidates and declared objective.

Each cost dimension compares `(unknown estimate count, known-value sum)`. Fewer
unknown estimates take precedence over the known sum. This ordering assigns no
numeric value to a missing estimate. Configuration and implementation effort
are charged per capability, even when two capabilities use the same package.
Other dimensions are charged per symbolic request group. A sum of latency
estimates represents accumulated service time. Concurrent elapsed time also
depends on scheduling and downstream limits.

Omitting `prefer_configured`, or setting it to `false`, selects by cost and
retains configuration gaps. This replaces the former implicit preference for
configured routes. Setting it to `true` restricts each source to executable
candidates when any exist, then falls back to ranked candidates with explicit
gaps when none exist.

## Comparing qualified acquisition graphs

An operator can supply `batch_correspondences` in the planning configuration.
Each entry pins a qualified item interface and batch interface by their
`measurement_identity()` digests. Its qualification reference warrants that
each returned batch quantity is the ordered array of the corresponding item
quantities, including duplicate inputs. The scopes, methods, units, observation
times, coverage and provenance must satisfy the original requirements.

```json
{
  "id": "trace_identity_batch",
  "qualification_reference": "reviewed-trace-batch-equivalence/1",
  "item_capability_id": "trace_identity",
  "item_measurement_sha256": "<item measurement_identity()>",
  "batch_capability_id": "trace_identities",
  "batch_measurement_sha256": "<batch measurement_identity()>",
  "logical_argument_pointer": "/trace_id",
  "item_argument_pointer": "/trace_id",
  "batch_argument_pointer": "/trace_ids",
  "maximum_items": 64
}
```

Replace the digest placeholders with the reviewed descriptor identities. The
batch capability's operator binding maps `/trace_ids` to `/trace_id`. Its input
schema must declare an array whose item schema matches the scalar input and
whose `maxItems` equals the authored fan-out bound. Each output quantity must
declare an array of the corresponding item quantity schema. This finite schema
profile requires resolved schemas for references and unions.

The selected upstream source must also qualify an array whose `maxItems` fits
the fan-out bound and whose element schema matches the item interface. An
unbounded list keeps its authored fan-out route. Runtime checks require one
value per input for every batch quantity before any dependent requirement can
pass. Dynamic comparison controls retain their original rejection of null and
array operands, together with their freshness and provenance.

The planner compares the authored fan-out with admitted batch alternatives.
Equivalent request groups can still supply several independently evaluated
quantities. It retains at most 32 graph variants and checks at most 256
correspondences, then runs the existing bounded selector within each variant.
The report retains the selected graph, rejected alternatives, costs and pruning
status. Export replays the selected correspondence against the original
requirements and pinned descriptors.

Graph comparison first minimises unresolved acquisitions. With
`prefer_configured: true`, it then uses fully configured graphs at that level
when available. Otherwise it applies the declared cost ordering with gaps
retained. Determinism applies to the identified input snapshots and work
limits. Live discovery can produce a different snapshot on a later replan.

Dynamic dependency and fan-out slots follow those same physical mappings. Their eventual values undergo full runtime schema admission. Statically known inputs are checked during planning; dynamic slots cannot replace a fixed selector or historical interval. Missing object parents can be prepared along declared bindings, while operator arrays are never resized.

For example, two requirements can compare one measured `latency_p99` quantity in milliseconds against different thresholds. The measurement operation accepts the service and interval as parameters. Each requirement retains its own comparator and threshold. A calculation of a mean cannot substitute for the percentile, and a bounded search cannot establish population-wide absence.

Built-in calculation transfers have closed method identities: `engarg.sum/1`, `engarg.mean/1`, `engarg.count/1`, `engarg.minimum/1`, `engarg.maximum/1`, `engarg.scale/1`, `engarg.identity-equal/1` and `engarg.any-identity-equal/1`. Their result contracts use calculation support and exact-calculation strength; measurement source contracts retain their original methods, scope and provenance. Sample summaries identify their source operations in a lexically ordered `sample_sources` selector while preserving the input selectors. Population totals additionally require complete lineage for the same population and selectors. Identity comparisons use subject kind `identity_relation`, an `operands` selector mapping source IDs to their exact original subject scopes, and `operand_times` mapping direct measurement and selector-ancestor source IDs to their exact original time requirements. This permits current observations to be compared with an older selection without pretending that their windows match. Lifecycle, temporal causation and applicability remain separate obligations.

`time.historical_interval` constrains the original observation clock. A current query over historical events instead places its event window in subject selectors and physical input bindings. An immutable-ID detail lookup may use its own current-observation clock after a historical selector, while retaining that selector's original interval in lineage. If the detail lookup explicitly declares a historical interval, it must equal the selector's interval.

`IdentityEqual` requires matching scalar/collection shapes. `AnyIdentityEqual` accepts a scalar identity followed by a collection, uses exact equality without coercion, and returns `any_identity_equal` with witness coverage. A false result over incomplete input remains pending. Every source retains its own strict age bound, while the consumer declares the accepted age of its oldest lineage. A fan-out quantity reference such as `/latency` denotes an array of that logical quantity, rather than an array of unprojected response records.

EAL/3 requires a freshness bound on machine evidence. Graph requirements and evaluations therefore need an explicit positive `max_age_seconds` from 1 to 315360000; missing bounds fail with `SUPPORT_FRESHNESS_CLARIFICATION`. The EAL gate uses that exact requirement bound. It does not silently tighten it through the legacy flat export-age policy. Unavailable/judgement stubs with no age use `max_age 0` and never produce observations. Independent review freshness retains its separate policy.

## Alternative obligations and operation definitions

New model runs use `engarg.support-elaboration/2`. Every authored case alternative has a positional ID (`alternative_1`, `alternative_2`, and so on) and an `alternative_support` record containing its exact distinguishing test, linked support requirement IDs and both root claim IDs. Each linked requirement retains that exact test in `discriminating_evidence`; its normal evaluation and pending status gate the claims. The source case does not establish a narrower alternative scope, so mapping an alternative only to an unused subclaim is rejected. Changing the hypothesis or its claim bindings requires an explicit prior-digest requirement revision. Version 1 remains importable, but uncovered alternatives become explicit pending legacy gates rather than satisfied facts.

Calculation lineage is typed and survives representation changes. Scaling a calculated mean, or collecting it after raw inputs, cannot turn it into a qualified statistical measurement. Population calculations require actual complete-enumeration coverage at evaluation, including selector ancestors and truncation checks; an exact count of a retained sample remains a separate contract. A witness can compare a declared logical quantity with either a literal `equals` value or an upstream scalar `equals_output` reference with the same unit. Comparison controls retain age and provenance without relabelling the selected source's scope.

A missing operation requires `operation_spec` before coding: schema `engarg.operation-specification/1`, a parameterised `definition`, explicit `input_conditions` and `output_guarantees`, and fixed `input_schema`/`output_schema` objects. Dynamic quantity inputs retain known units and shapes; an unknown provider scalar/array shape is not guessed. Input unit annotations use `x-engarg-unit`; the logical output schema requires each declared quantity. The collector transport adds its separate `collected` bit. Algorithm changes alter the reusable package identity, while consumer scope, source IDs and freshness do not define the algorithm. Existing qualified operations need no new specification. Missing definitions stay clarification gaps, not guessed implementations.

An operator can pin an independently supplied definition through the planning
configuration's optional `operation_specifications` object. Each key is one exact
versioned method identifier and each value is a complete operation specification
with the fields above. A single matching method selects the operator's definition
for implementation. The authored input and output validation constraints must
still match, and conflicting units remain clarification gaps. The registry holds
at most 64 entries. Replanning retains it with the route configuration while
preserving the authored argument, support elaboration and transcript. Changing a
pinned definition changes the implementation identity and requires a matching
verified package.

`ProjectValue` projects JSON Pointers from one whole response, including a root array. `Project` retains the older per-record projection for fan-out results. Both preserve logical quantity identities. Existing batch operations receive a whole discovered collection through a dependency binding; they are not automatically expanded into individual reads.

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

When packages are generated or reused, the stage also writes `operation-contracts.json` and `generated-native-providers.json` in that attempt directory. The former retains the exact parameterised algorithm, schemas, quantity units and declared limits; the latter contains discoverable native command configurations, pinned package manifests and `semantics: null`. The selected `eal-collector prepare-native-provider` command computes native configuration identities without calling a source or admitting credentials. It preserves every other field. A matching installed collector is required for this preparation.

A new reusable contract takes two model calls: draft and freeze synthetic fixtures, then implement code without the fixture oracle. A failed implementation retry reuses the frozen contract and needs one coding call. A verified, unchanged package needs zero model calls. The key contains the canonical parameterised support contract, input/output schemas, operation purpose and declared limits, together with language and admitted toolchain/runner identity. Consumer claim IDs, need IDs and acceptance thresholds are excluded. Runtime selectors remain parameters.

New `engarg.capability-implementation/2` records leave runtime parameter admission to the frozen input schema instead of requiring a second operator confirmation for each supplied argument. Credential prerequisites remain separate. Saved version-1 records retain their exact original reconstruction and validation; reusing the same verified package produces version-2 metadata with zero coding calls. The parameterised request/package identity remains version 1, so this metadata change does not invalidate the source cache or silently rewrite archived reports.

Mandatory protocol probes also preserve huge integers, precise decimals, escaped text and literal objects keyed `$serde_json::private::Number` or `$serde_json::private::RawValue`. The Rust SDK enables `raw_value`; its reference collector streams opaque input/context through `RawValue` instead of a `Value` intermediate. Receipt version `engarg.tool-tests/2` requires these checks. Older receipts cannot authorize registration or reuse; rerun implementation with the current runner rather than editing receipt metadata.

Configuration, missing credentials, unavailable source records, absent historical coverage, clarification, judgement and missing scientific qualification do not trigger coding. Passing contract fixtures permits registration of the tested function interface only: its generated `CapabilityDescriptor.semantics` remains absent. A separate operator/domain qualification is required before it can become an adequate measurement route. The retained receipt states these limits explicitly. Editing saved metadata to broaden that qualification is rejected by package revalidation.

Copy `tooling-attempt/generated-native-providers.json` to `qualified-native-providers.json` for operator review. Supply explicit measurement semantics with a qualification reference, methods, scope and units, coverage, provenance and limitations, then review the scope and observation-time mappings against the implementation's actual output. The generated `/observed_at` mapping is a pending declaration: it does not create an original source timestamp. Collection time is suitable only when the reviewed semantics and original requirement permit it. Required environment names remain required even when their values are absent. Keep the executable, package pins, input/output schemas and EAL protocol fixed.

A bounded `max_age_seconds` requires a qualified original observation time and a response timestamp pointer. Registering a collection-time-only provider does not make it adequate for that requirement. Scope pointers refer to the normalized measurement value, so required identities must appear in the frozen output schema and returned value. Omitted optional capability and mapping fields are normalized before preparation; supplied values, schemas, execution settings and package pins remain checked.

Register the reviewed configuration and deterministically rematch the unchanged support requirements:

```sh
engarg eal register-tools \
  --input tooling-attempt/analysis.with-tools.json \
  --qualifications qualified-native-providers.json \
  --evidence-config operator-planning.json \
  --output registered-planning.json

engarg eal replan \
  --input tooling-attempt/analysis.with-tools.json \
  --evidence-config registered-planning.json \
  --output analysis.replanned.json
```

Registration rehashes each verified package, validates that qualification preserves its execution and interface, and writes a new planning file plus `registered-planning.connections.json`. The original files remain available. It retains a registration record under `registered-planning.registration`; source calls and model calls are both zero. The command accepts packages retained in earlier replanning provenance as well as active tooling records. It rejects conflicting provider identities and qualifications without reviewed semantics. A domain qualification is an explicit operator assertion; registration does not certify its scientific truth.

Replanning performs bounded discovery and deterministic selection with no model calls, preserves the exact support elaboration, and archives prior planning and tooling under `replanning`. The active `tooling` block is removed because its consumer bindings refer to the earlier routes; the archived package receipts remain validated and available for registration. Replanning can select the newly qualified capability or retain configuration, adequacy and source gaps. It does not acquire evidence or assert that the authored claim is true.

A report can retain 16 replanning transitions. Existing reports at that limit
remain readable and exportable. A further transition fails before discovery
or output creation. Inputs remain bounded at 16 MiB. Use the exported tools
registry for both collection and assessment so their retained bindings agree.

A related qualified capability must share the subject kind, support type and an acceptable method. Its insufficient scope, units, coverage, time or provenance stays a capability mismatch. An unrelated tool does not suppress a genuine missing operation. Missing calculation and proof-check operations are classified as `transformation_implementation`; other unsupported operations use `operation_implementation`. A schema-compatible interface without reviewed semantics remains a qualification gap.

## Source reuse and metric boundaries

Source identity includes provider/version, physical arguments, schema and qualification identities, configuration/authentication fingerprints, scope and historical interval. Reuse retains the original observation time and lineage. Requirement IDs, acceptance prose and thresholds are evaluated separately. Shared use of one source is reported as one source, rather than independent corroboration. A stale source or failed acquisition is pending; a successfully observed false value can fail a criterion. Neither becomes a fabricated positive observation.

Whole-plan execution continues a witness-producing fan-out when another
consumer requires complete enumeration or statistical coverage. A positive
witness remains available if subsequent reads fail or exhaust the budget.
Its population consumer remains pending until coverage is complete. A
requirement-only request can still stop at the first sufficient witness.
`execution_complete` and `budget_exhausted` describe the invocation separately
from `criterion_satisfied`.

Physical source IDs remain stable when an instrument produces a new observation. Each distinct observation is retained as a signed, hashed immutable `observations/<id>.json` snapshot. Reports and collector envelopes expose observation IDs, value hashes, original timestamps and relative paths; `SourceStore::load_observation` verifies retained snapshots. Refresh never silently overwrites the earlier raw record, including when two age policies use different observations from one source. Capacity exhaustion is explicit and does not evict earlier evidence.

An interrupted acquisition with an uncertain outcome retains a pending attempt instead of silently issuing the source call again. `eal-collector evidence-reconcile` provides explicit reconciliation after the previous read has ended; active reads cannot be reconciled. Separately configured scoped judgement records can be acquired through an identified reviewer and evaluated, while absent or unscoped independent judgement remains pending.

A retained judgement uses `eal.retained-judgement/1` with reviewer identity, exact criterion and subject scope, original review time, verdict, reasons and nonempty provenance. Supported/rejected/inconclusive records map to satisfied/not-satisfied/pending only after correspondence to a separately qualified judgement source is checked. The retained `judgement_record_correspondence_only` label expressly leaves the review's truth and reasoning quality unverified.

MCP discovery reports successful initialise/list/call requests and explicitly approved catalogue GETs, logical JSON request/result bytes, elapsed time and persistent owner starts. These bytes exclude HTTP/TLS framing. Execution reports actual source attempts, reused records, fresh normalised value bytes and provider-reported request/byte counts, including interface admission when a cached source is reused. Unknown failure bytes are not invented. `generated_code_count` in execution remains zero because execution does not generate code; implementation counts belong to the tooling stage.

MCP/native concurrency and rolling-minute acquisition-attempt limits apply to one shared bridge/provider instance and its clones. Separate collector processes have separate admission budgets; any quota spanning clients/processes must also be enforced by the remote service. Failed acquisitions consume their admission and do not trigger an automatic replay.

These counters describe their measured boundaries, not monetary cost or a controlled comparison with a different topology. Provider token counts remain provider-reported. The implementation supplies deterministic ranking and observable reuse; it does not claim a measured universal token, latency or price optimum.

`scripts/check-support-domains.py` retains paired live flat and graph analyses for deployment identity, performance, statistical reliability, API conformance, numerical results and independent design judgement:

```sh
python3 scripts/check-support-domains.py \
  --binary target/release/engarg --evidence-config planning.json \
  --model gpt-6.1-sol --repetitions 3 --output-dir domain-experiment
```

The experiment requires `OPENAI_API_KEY` and a new output directory. It alternates mode order, retains each report, audit and failure log, and records the binary and configuration hashes. Provider-reported usage includes completed repair calls. Missing usage remains unknown and unsuccessful attempts stay in the result set. Paired ratios describe structurally accepted runs only. An expected support-type check does not establish domain adequacy: inspect the retained scope, methods, criteria and gaps independently before using the measurements to choose an agent topology. Discovery caching and provider billing classes are not controlled by this experiment.

The [complete-workflow study protocol](../experiments/support-domains/README.md)
adds frozen obligation references and paired recipes through implementation,
qualification, registration, replanning, export, collection and assessment.
It records phase-level usage and failures, and binds independent reviews to
the exact output. Live semantic adequacy and cost results require executing
and reviewing that study. The deterministic harness tests verify its
accounting and failure handling.

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
