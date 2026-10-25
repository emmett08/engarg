# Typed warrant methods

A warrant's `inference` is an authored classification. Its `reasoning_method`
connects an explicit formal problem and acquired evidence to an installed EAL
method. The two are validated together. A successful computation supports only
the declared result predicate under the stated scope and assumptions.

The binding has either `status: "bound"` or `status: "unresolved"`. An unresolved
binding records the missing model or evidence and keeps that route unavailable.
Existing saved cases without this field retain their authored `structured/1`
projection and review obligations. New provider schemas require an explicit
binding for the five computational inference modes. Analogical and practical
warrants retain authored support in this profile.

## Bound inputs and evidence

`method` contains a closed, mode-specific query. `input_evidence_need_id` names a
separate requirement for acquiring a complete `EAL/typed-input/1` envelope.
`formal_premises` bind every relied-upon premise and required formal input to a
named argument node and its evidence need. `evidence_requirements` distinguish
formal-input admission, premise observation, design/model evidence and
applicability. `assumption_ids` retain named model assumptions as dependencies.

`conclusion` identifies the conclusion claim, subject, quantity, unit, scope,
validity interval and exact result predicate. EAL checks the acquired envelope
against these declarations before computing. The source acquisition's observation
age remains a separate constraint from the proposition's interval.

There are two explicit premise correspondences:

- `identity_value` selects an input of the named premise requirement using
  `source_input_index`, then a `source_pointer` within that value. Its canonical
  value must equal the formal payload's `input_pointer`. The native profile also
  requires a compatible declared unit and scope. It performs no implicit unit
  conversion. Different acquired data cannot be substituted just because both
  requirements independently passed their predicates.
- `accepted_premise` is limited to complete deductive `/premises/N` formulae.
  The independently acquired premise requirement must be satisfied before its
  explicitly authored interpretation can license that formal premise. This
  checks the declared dependency, not the meaning of arbitrary prose.

Every formal deductive premise must be mapped. Induction maps successes and
trials. Abduction maps the observation and candidate probability model. Causal
reasoning maps assignment, treatment and control. Counterfactual reasoning maps
the structural model, intervention and outcome. Correspondences and model/design
requirements cannot be discharged by the formal-input admission requirement.

The `method_input` support criterion validates the envelope format against the
installed method contract. It establishes neither premise truth nor a scientific
conclusion. Its requirement targets only the bound warrant's formal-input need.
The `engarg.reasoning-input/1` acquisition selects the exact retained execution
reference, collects the joined premise closure, checks the correspondences and
preserves the source observations, lineage and oldest measurement time.

## Implemented bounded methods

| Mode and EAL method | Input and computation | Required qualification |
| --- | --- | --- |
| Deductive, `deductive/1` | Propositional entailment with consistency and countermodel checks, at most 12 atoms | Each formal premise has acquired support. Inconsistent premises cannot obtain support by vacuous entailment. This is not an arbitrary mathematical proof checker. |
| Inductive, `inductive/1` | Wilson binomial interval from successes, trials and confidence | Sampling design, population correspondence and applicability have separate evidence requirements. The interval calculation does not verify these assumptions. |
| Abductive, `abductive/1` | Bayesian conditioning over 2–128 supplied candidates with priors and likelihoods | The candidate model and applicability need evidence. A posterior ranks only the supplied candidates. Missing probabilities remain unresolved. |
| Causal, `causal/1` | Two-group mean contrast and standard error under declared randomised assignment | Acquired treatment/control data and separate intervention-design evidence. Observational association cannot silently become randomised evidence. |
| Counterfactual, `counterfactual/1` | Intervention in an acyclic affine structural model with at most 32 variables and supplied exogenous context | Model, context and applicability require evidence. The method neither discovers a causal model nor reconstructs unknown noise. The native bridge requires one explicit compatible unit basis. |

The counterfactual binding uses arrays of named variables and coefficients in the
provider schema. Export lowers them to EAL's checked map representation. Duplicate
names, missing variables, cycles and non-finite coefficients are rejected.

## Export and outcomes

Typed cases use authored projection semantics `engarg.eal-projection/4`.
Legacy cases keep version 3 and reconstruct their existing source and identities.
Graph export adds native routes only when the declared method inputs and all
required evidence gates are represented. It retains the original ground and
claim identities so their objections still propagate. Open material challenges
retain their independent resolution gate. A challenge to a model-applicability
assumption leaves the native route pending in this profile. Cumulative
composition retains its independent adequacy review.

Distinguish these results when inspecting an assessment:

- A completed acquisition can contain a measured negative.
- A completed computation can fail the conclusion's result predicate.
- Missing, failed, stale or mismatched evidence keeps an application unavailable
  or `not_run` rather than making the proposition false.
- EAL source validation, completed computation and a supported claim are separate
  results. Prose/formal correspondence remains declared by the argument author.

## Acceptance and example

`tests/reasoning_methods.rs` checks all five kernels through exported EAL with
known-answer test-bench observations. `tests/support_method_export.rs` also
collects and assesses actual exported graphs through the pinned runtime, including
substituted values and unavailable models. `tests/catalogue_workflow_native.rs`
exercises an actual SDK MCP server and an independently authored custom-tool
fixture, measured negatives, source failures and reuse without refreshing age.
The producer/consumer CI job runs the native integration tests.

For the complete model-authoring and generated-tool workflow, see
[the catalogue example](CATALOG_SYNC_AUTHORING.md). The no-argument script uses a
simple local catalogue claim, an additional read-only MCP server and a separately
generated custom audit operation. It automatically collects and assesses the
result, and reports support only after both EAL and the independent observation
check pass. Live tool generation requires provider credentials and Docker.
