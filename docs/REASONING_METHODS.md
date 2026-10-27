# Executable warrant methods

A warrant's `inference` classifies the argument. Its `reasoning_method` identifies
an installed EAL computation, the authored query, empirical premise bindings,
applicability assumptions and a result predicate. EAL owns the query, input and
output contracts. EngArg does not maintain parallel formula or model schemas.

A bound method uses this canonical representation:

```json
{
  "identifier": "deductive/1",
  "query": {
    "premises": ["count_matches", "identifiers_distinct"],
    "conclusion": {"and": ["count_matches", "identifiers_distinct"]}
  }
}
```

Text-model responses carry the same query as `query_json`, a strict JSON string.
This transport accommodates provider schemas that cannot express EAL's arbitrary
mathematical JSON values. EngArg decodes it once, rejects duplicate keys and
trailing documents, and retains exact numeric values in the canonical `query`.
The pinned EAL method descriptors accompany authoring instructions. Query
validation and result-path validation call EAL directly; `entailed` is a method
output path, while `/premises/0` is a JSON pointer into an input.

An unresolved binding states the missing model or evidence and keeps the route
unavailable. Authoring requires an explicit bound or unresolved choice for the
six computational classifications. Practical reasoning uses an explicit decision
policy and scoped structured review. It does not invent objectives or weights.

## Authored queries and acquired evidence

A formula or fixed query is programme data. It needs no collector to echo it and
no separate `formal_input` evidence obligation. `formal_premises` connect every
warrant premise and required measured field to an actual evidence need.
`evidence_requirements` retain empirical observations, sampling/intervention
design, model adequacy and applicability as separate obligations.

Two correspondence forms are supported:

- `identity_value` selects an acquired value using `source_input_index` and a
  `source_pointer`, then binds it to the method payload's `input_pointer`.
  Scope, unit and source identity must remain explicit. Passing another predicate
  does not authorise substitution of different data.
- `accepted_premise` applies to complete deductive `/premises/N` formulae. The
  premise's empirical acceptance predicate must be satisfied before its authored
  interpretation licenses the formula. This dependency does not prove that prose
  and formula have the same meaning.

The conclusion names the claim, subject, quantity, unit, structured scope,
validity interval and result predicate. Scope selectors and identity dimensions
identify the represented population or system. They do not prove physical
continuity. Source observation time remains distinct from acquisition time and
from the conclusion's interval. Calculations and reuse cannot refresh source age.

## Bounded computations

| Method | Computation | Separate evidential obligations |
| --- | --- | --- |
| `deductive/1` | Finite propositional entailment and consistency | Each empirical premise and its interpretation; inconsistent premises do not provide support by vacuous entailment |
| `inductive/1` | Wilson interval for acquired binomial successes and trials | Sampling design, population correspondence and applicability |
| `abductive/1` | Bayesian conditioning within supplied hypotheses | Observation, priors, likelihood model and adequacy of the candidate set |
| `causal/1` | Two-group mean contrast under declared randomised assignment | Treatment/control observations, intervention design and applicability |
| `counterfactual/1` | Intervention in a supplied acyclic affine model with fixed exogenous context | Structural model correspondence, context and scope |
| `analogical/1` | Declared feature correspondence | Relevance of the mapping, material differences and adequacy of the transfer |
| Structured practical support | Authored option and decision-policy composition | Explicit objectives, constraints, consequences and scoped policy/application reviews |

Native method limits and exact schemas come from EAL descriptors. A successful
calculation only establishes its declared result under those inputs. It does not
validate an external scientific model or make an arbitrary interpretation true.

## Compilation, revision and outcomes

Execution packages put acquisitions, dependencies and argument relationships in
EAL source. TOML binds actual named tools. Assumption objections remain attached
to assumptions in the argument; a native method does not discard them or invent
an accepted review. Reusing an acquisition for several requirements does not
produce independent observations.

Explicit argument revision is separate from acquisition replanning. The revision
API retains the previous argument and parent report digest, requires applications
for dissent attached to changed targets, and invalidates old support elaboration,
route plans, EAL exports, collections and assessments. Historical observations
remain records of their original run.

Assessment distinguishes a usable negative observation, a false result predicate,
a failed acquisition, unavailable or stale evidence, and exhausted computation.
Source validation, completed execution and support for a claim are separate
results. Missing evidence does not establish falsity.

Current tests cover shared descriptor validation, exact authoring-query decoding,
revision/dissent preservation and executable EAL compilation. Native EAL tests
check the numerical and logical methods against independently specified cases.
