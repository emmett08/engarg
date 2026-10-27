# From a warrant to an executable method

[Documentation](index.md) · [Support requirements](support-planning.md)

A timing model can calculate the difference between serial and parallel CI
execution. Applying that result to a production pipeline also requires evidence
that the model describes its runner constraints and workload. EngArg records the
calculation and those applicability obligations separately, so a completed
computation cannot silently establish an empirical premise.

A warrant's `inference` classifies the argument. Its `reasoning_method` identifies
an installed EAL computation, the authored query, empirical premise bindings,
applicability assumptions and a result predicate. EAL owns the query, input and
output contracts. EngArg validates against those shared contracts, which keeps
authoring and execution aligned.

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
validation and result-path validation call EAL directly. `entailed` is a method
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
| `deductive/1` | Finite propositional entailment and consistency | Each empirical premise and its interpretation. Inconsistent premises do not provide support by vacuous entailment |
| `inductive/1` | Wilson interval for acquired binomial successes and trials | Sampling design, population correspondence and applicability |
| `abductive/1` | Bayesian conditioning within supplied hypotheses | Observation, priors, likelihood model and adequacy of the candidate set |
| `causal/1` | Two-group mean contrast under declared randomised assignment | Treatment/control observations, intervention design and applicability |
| `counterfactual/1` | Intervention in a supplied acyclic affine model with fixed exogenous context | Structural model correspondence, context and scope |
| `analogical/1` | Declared feature correspondence | Relevance of the mapping, material differences and adequacy of the transfer |
| Structured practical support | Authored option and decision-policy composition | Explicit objectives, constraints, consequences and scoped policy/application reviews |

Native method limits and exact schemas come from EAL descriptors. A successful
calculation only establishes its declared result under those inputs. It does not
validate an external scientific model or make an arbitrary interpretation true.

## Carry the result into the argument

Execution packages put acquisitions, dependencies and argument relationships in
EAL source. TOML binds actual named tools. Objections remain attached to the
applicability assumptions they challenge, including for native methods. Their
resolution requires actual evidence or an accepted scoped review. Several
requirements reusing one acquisition remain dependent on that observation.

Explicit argument revision is separate from acquisition replanning. The revision
API retains the previous argument and parent report digest, requires an explicit
response to retained dissent attached to changed targets, and invalidates old
support elaboration,
route plans, EAL exports, collections and assessments. Historical observations
remain records of their original run.

Assessment distinguishes a usable negative observation, a false result predicate,
a failed acquisition, unavailable or stale evidence, and exhausted computation.
Source validation, completed execution and support for a claim are separate
results. Missing evidence does not establish falsity.

The [compiler mapping](EAL_MAPPING.md) shows how the method, its empirical inputs
and its assumptions become one EAL programme. Follow [USAGE.md](USAGE.md) to build
and execute that programme, then inspect its collection and assessment separately.
The [validation guide](VALIDATION.md) identifies the contract, revision and native
method checks used to verify this path.
