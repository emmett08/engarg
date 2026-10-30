# Check the meaning and applicability of an argument's inputs

`engarg investigate` accepts `engarg.assurance-plan/2`. In addition to the fixed
native/reference checks, the plan contains `verification`: one
`eal.verification-contract/1` for each selected ground, warrant, assumption,
material defeater and independently evidenced claim. Claims without their own
source obligations compose the checks of their selected warrants and conditions,
so a derived conclusion needs no duplicate observation. EngArg derives the required targets, formulae
and support obligations from the admitted argument. A contract cannot reduce
that coverage by omitting an inconvenient premise.

The contract binds each propositional symbol to a predicate over a named
acquisition's actual value. Each required support obligation also has a source
predicate. The same source must meet explicit identity, unit, scope, validity
interval and maximum-age conditions. EAL evaluates those predicates using its
existing exact values and logical semantics. Missing definitions or records
remain unknown; measured disagreement fails. A collector's `accepted: true`
wrapper does not replace the values that the predicates require.

The observation IDs are original acquisition IDs from the support elaboration.
The investigator reads the corresponding retained native records after
independent replay. It preserves their original clocks and source digests.
Predicates and expected applicability values come from the frozen operator plan,
never from a candidate's proposed replacement verification policy.
The expected identity is the acquisition's canonical `scope.subject`, and the
expected scope is its selector map. Units and maximum age must fit the original
source contract. A bound conclusion retains its identified subject and validity
interval; an unrelated source cannot pass by supplying matching metadata for a
different target.

For deductive empirical warrants, coverage includes the query's premises and
conclusion. Every atom needs a definition. Other empirical native methods retain
their executable `identity_value` input mappings; a prose-only correspondence
does not establish verified input correspondence. Unsupported mappings require
an appropriate executable verifier before the candidate can be selected.

## Mathematical consequences and mixed arguments

A bound method may explicitly declare `claim_mode: mathematical_consequence`.
It must have a `mathematical_model` scope and a permitted closed native query:
a deductive theorem with no external premises, or a finite probability model.
Its warrant has no empirical premise or evidence list. For example, the closed
formula `(p and (p implies q)) implies q` can be proved without observations.
This does not assert `p`, establish its physical interpretation, or prove that
the antecedent describes a possible external system.

Every mathematical target still needs its successful fixed native method check.
Its source-grounding report says empirical grounding is not required and that
inference is checked separately. Empirical warrants in the same argument retain
their own source and applicability obligations. Changing a physical claim to a
mathematical mode does not bypass the fixed claim, scope or meaning guards.

## Review and retained evidence

Assurance receipts use `engarg.assurance-receipt/2`. They contain the ordinary
fixed checks, per-target EAL verification reports, and a lifecycle record linking
the raw proposal, admitted candidate, compiled source, executable, collection and
assessment. Authored `missing` or `untested` labels remain historical statements;
runtime results are recorded separately. Failed execution never acquires a
successful replay label.

Criticism identifies targets, obligations, checks and artifacts. The investigator
can resolve a narrowly typed claim that evidence or a check is absent by citing
the actual matching passing receipt. Checked lifecycle/binding records can also
resolve the corresponding mistaken factual criticism. Resolutions bind the
issue, candidate and receipt digests. Semantic objections remain open until an
appropriate investigation resolves their substance; later reviewer omission,
new timestamps or edited presentation do not erase them.

`engarg.assurance-plan/1` remains readable and replayable for diagnosis. Its
missing semantic coverage is reported as unknown, so it cannot yield a fully
verified candidate. Migration requires specifying the missing contracts. There
is no automatic promotion of the old generic prose mappings.

## Architecture decision and verification boundary

The smallest clean extension reuses EAL's predicate evaluator, logical AST and
exact arithmetic through a public bounded verification contract. EngArg owns
coverage derivation and execution authority; the viewer displays recorded
results. Leaving the old structure unchanged would retain unchecked atom
meanings. A separate EngArg formula/predicate engine would duplicate semantics
and increase the cost of keeping producer, optimizer and viewer consistent.

Verification coverage, lifecycle recording and issue resolution are separate
modules because they change for different reasons. Mathematical admission is
also a separate policy module. Existing acquisition, native replay and argument
revision boundaries remain in use. No new formatter, arithmetic library or
language profile is introduced: the repository's pinned Rust toolchain,
`rustfmt`, Clippy and existing tests remain authoritative.

The strict authoring-schema adapter is separate from canonical records. It
preserves required mode selection while removing provider-incompatible
annotation siblings from references; documentation stays on the referenced
definitions. This boundary is also exercised by a live provider request.

These checks establish conformance to the declared interpretation and acquired
evidence. They do not authenticate an external source, discover omitted facts,
or prove that arbitrary prose has the intended meaning. New domains must supply
suitable sources and executable predicates. Missing domain capability blocks
verification rather than being replaced by LLM consensus.
