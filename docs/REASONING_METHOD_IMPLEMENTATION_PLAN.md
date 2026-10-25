# Typed warrant methods and executable catalogue example

This plan builds on EngArg PR #17 and EAL PR #21. Its producer extension is [EAL PR #22](https://github.com/emmett08/eal-mcp/pull/22). The order below is the implementation priority. EAL/3 remains the language. Existing mathematical kernels are reused through checked bindings rather than through new grammar or mode labels alone.

## 1. Bind a warrant to its reasoning method

Add an optional, versioned reasoning-method binding to each warrant. Old reports remain readable and keep their conservative authored support semantics. Newly generated computational warrants must supply a bound method or an explicit unresolved binding.

A bound method identifies:

- Its versioned EAL method and typed formal query/model.
- Its formal premises, their argument identifiers and input locations, and their evidence-need identifiers.
- The acquired method-input envelope and the other evidence requirements, with their roles and acceptance obligations.
- Named assumptions and the conclusion's subject, quantity, unit, scope and time interval.
- The exact predicate on the computed result that would support that conclusion.

Validate identifiers, inference/method agreement, bounded input shapes, correspondence, units, scope, time, assumptions and result predicates. An explanatory sentence cannot supply a missing observation or prove a formal correspondence. Incomplete bindings retain the missing model or evidence as unresolved.

Derive the EAL scope identity from a closed structured target scope. Match its
identity dimensions and selectors against the formal-input and premise evidence
contracts. Extra sample selectors may narrow the same population. Missing or
conflicting identities require an explicit unresolved scope-transfer obligation.

Export bound warrants using the actual EAL method. Export absent or explicitly unresolved formalisation without claiming computational verification. Ground acquisition and model-applicability obligations remain independent of successful method execution.

Add an acquisition projection that selects the exact declared method-input reference from an already evaluated support requirement. The collector must check the request, requirement, input reference, satisfied criterion and source lineage. It must retain original observation times and provenance and return the acquired typed envelope, never an envelope invented by the exporter.

## 2. Demonstrate mode-specific evidence and computation

Run actual EAL evaluation in automated acceptance tests. Successful tool execution, method computation and support of the conclusion are separate assertions.

| Mode | Required bounded representation | Computed result | Acceptance and unresolved cases |
| --- | --- | --- | --- |
| Deductive | Formal premises and conclusion, with correspondence to the argument | Propositional entailment, consistency and countermodel | Entailed and consistent premises support only the bound proposition; countermodel, missing premises or unavailable correspondence cannot establish support |
| Inductive | Success/trial observations, population and declared sampling assumptions | Wilson binomial interval | Check the specified interval bound; missing observations or sampling justification remain unresolved |
| Abductive | Named candidate hypotheses, justified priors and observation likelihoods | Posterior comparison within the declared candidates | Check the named hypothesis and threshold; missing likelihood/model evidence remains unresolved and candidate completeness is not inferred |
| Causal | Treatment/control observations, design evidence and explicit identification assumptions | The supported two-group contrast and uncertainty | Check the contrast and assignment requirements; association or absent design evidence cannot establish a causal claim |
| Counterfactual | A supplied acyclic affine structural model, exogenous context and intervention | Factual/intervened outcomes under that model | Check the specified result; absent model/context/applicability evidence remains unresolved |

For every mode, include a known-answer success, a measured negative or failing acceptance predicate, absent evidence/model, wrong method input and scope/unit/correspondence rejection where applicable. Exercise actual acquisition of method envelopes and preserve source identities and timestamps. Do not fabricate numerical priors, samples, causal designs or structural models to make a test appear empirically supported.

## 3. Run a small complete catalogue example

Provide a simple claim about a clearly identified demonstration catalogue. A separate, read-only MCP server supplies the catalogue snapshot. A custom tool performs the operation absent from that MCP interface. The generated argument must name both the source acquisition and the custom operation and bind the final reasoning step to its EAL method.

The workflow is:

1. Validate configuration and runtime interfaces before inference.
2. Generate the argument and evidence requirements for the simple claim.
3. Discover the additional MCP server and identify the custom-tool implementation gap.
4. Generate and fixture-test the custom tool in the configured isolated toolchain.
   Require original observation timestamps before registration where specified,
   and repair source/build/fixture failures within the existing call and time
   allowances without changing the frozen contract or test oracle.
5. Integrate the verified tool, replan the retained argument and export EAL/3.
6. Automatically acquire evidence from both sources, run EAL and retain the argument, tool package, registry, observations and assessment.
7. Report the queried claim as supported only if the actual assessment supports it and all required acquisitions and method computations completed. Otherwise retain an explicit unresolved or unsupported outcome with diagnostics.

Keep the existing operator-input workflow available. Remove the script's hardcoded two-level argument depth and expose an optional depth setting. Record token allowances for authoring and tool generation. For the exact default model `gpt-6.1-sol`, use its published 128,000-token maximum, verified on 25 October 2026 at <https://developers.openai.com/api/docs/models/gpt-6.1-sol>. An unknown model requires an explicit output allowance instead of guessing a provider maximum. Output allowance is a ceiling that includes reasoning; it is not a promise of completion or a requirement to consume it.

## Verification and delivery

Run Rust formatting, all-target Clippy and the complete test suite, Python script tests, shell syntax checks and actual subprocess MCP/custom-tool/EAL integration. Preserve existing export and saved-report compatibility. Publish the changes in new pull requests with pinned producer/consumer revisions and exact test results.

Live model-generated tool verification requires provider credentials and the configured container runtime. A fixture-provider test establishes orchestration and deterministic integration, not a live provider result. Record any unavailable live verification explicitly. The demonstration concerns its declared catalogue and does not establish Kubernetes, Grafana or production service claims.

## Implementation map

The typed records and validation are in `src/domain/reasoning.rs` and
`src/domain/validation/reasoning.rs`. Authored method projection is in
`src/eal_export.rs`, and acquired method routes are in
`src/support_export/methods.rs`. Their public bounded semantics are documented in
[REASONING_METHODS.md](REASONING_METHODS.md).

`tests/reasoning_methods.rs` checks the five computations and unresolved
outcomes. `tests/support_method_export.rs` exercises actual exported graphs
through the native collector and host. `tests/catalogue_workflow_native.rs`
exercises the additional MCP server, a custom-tool fixture, negative evidence,
source failure and observation reuse. `tests/legacy_export_identity.rs` preserves
the persisted legacy format.

The complete executable example is in `scripts/run-catalog-sync-authoring.sh`,
`scripts/catalogue_evidence_example.py` and `examples/catalogue_mcp.rs`. See
[CATALOG_SYNC_AUTHORING.md](CATALOG_SYNC_AUTHORING.md) for prerequisites and the
single-command run. The script uses frozen independent custom-tool probes and
checks the actual acquired MCP/custom-tool observations before reporting support.
