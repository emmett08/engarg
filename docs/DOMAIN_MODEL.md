# How an argument carries its conditions

[Documentation](index.md) · [Run the workflow](USAGE.md)

Consider the claim that parallel test groups reduce CI duration while preserving
outcomes. Its timing argument may be sound under a particular runner allocation,
yet its outcome argument may still depend on an untested shared-state assumption.
EngArg keeps those dependencies explicit so that support for one part cannot
silently stand in for support for the other.

The canonical argument records claims, grounds, warrants, assumptions and
defeaters. A warrant connects its jointly required premises to one conclusion.
A composition records how the warrants for that conclusion are used together.
Selecting a route records the author's adequacy judgement. The evidence and
assessment must still justify that judgement.

## State the conclusion and the support it needs

For the CI case, distinguish a conditional timing result from a measured result
for a particular repository and runner configuration. Under a fixed-duration
model with enough runners, parallel duration depends on the longest test group
and the added overhead. A shorter modelled duration follows only when the saved
serial work exceeds that overhead. Preserving test outcomes requires a separate
non-interference premise. Timing measurements alone cannot establish it.

The argument makes those relationships inspectable:

| Element | Work it performs in the CI argument |
|---|---|
| Claim and qualifier | State the conclusion and its justified scope or uncertainty. |
| Ground | Supply a premise, such as the observed paired durations or an explicitly assumed non-interference condition. |
| Warrant and backing | Explain why those premises support the conclusion, and what supports that inferential rule. |
| Assumption | Name an applicability condition, such as comparable workloads or the timing model's runner allocation. |
| Defeater | Challenge a premise, the inference's applicability or the conclusion, such as evidence of shared-state interference. |
| Evidence need | Specify an investigation and acceptance criterion that could resolve a material gap. |

The `current_claim` records the conclusion being assessed now. A
`stronger_target` states what additional evidence could justify. A conditional
mechanism can therefore remain useful while production measurements are missing.
The same record preserves the conditions that prevent treating that mechanism
as an established deployment result.

## Admit identities before interpreting the graph

The serialised records in `src/domain/records.rs` retain string identifiers and
closed state enums. Their schema describes representation. Admission then checks
whether the references form a usable argument, and assessment checks what that
argument supports. Keeping those steps separate lets a renderer inspect a
reference-safe policy defect without treating an invalid argument as accepted.

| Boundary | Typed owner or reason for text |
| --- | --- |
| Claim, ground, warrant, assumption and defeater IDs | `ArgumentId` and `ArgumentIndex` |
| `conclusion_id`, `premise_ids`, `applies_to`, `target_id` and `enables_claim` | Reference admission in `validation/structure.rs`, then indexed argument IDs |
| Evidence IDs and backing references | `EvidenceId`, request admission and evidence-reference policy |
| Evidence-need IDs | `NeedId` and the index's need lookup |
| Selected warrant IDs | Validated composition and indexed selected-route membership |
| Narrative `reference_ids` | A union of known argument, evidence and need IDs |
| Claim, ground, assumption, defeater and support states | Closed enums in the records module |
| Inference kind, attack kind, defeater basis and paragraph purpose | Closed enums in the records module |
| Evidence `kind`, methods, qualifiers, rules and prose | Extensible text |
| Admission diagnostics | `DiagnosticCode`, location and human-readable message |

`validate_argument` admits the request and canonical references before building
one borrowed index. Composition and support-cycle checks use that index, followed
by narrative, evidence and status policies. Successful admission returns
`ValidatedArgument`. Its borrow prevents mutation of the source while the index
is in use. `validate_case` formats the same diagnostics as strings for callers
that need that interface.

Invalid request data stops dependent reference checks. Reading sections must
name `current_claim`, `stronger_target` or a declared subclaim. Paragraphs must
use distinct, known references. Unknown targets and repeated or unknown
references produce `ReferenceIntegrity` diagnostics and prevent presentation.
Missing titles or paragraph text are reference-safe policy defects and can be
shown in an explicitly labelled inspection view.

## Follow the route on which a conclusion relies

The index distinguishes candidate routes from selected reliance. A warrant's
dependencies include its own conclusion and attached conditions, while excluding
sibling routes. This matters when a narrow supported claim and a stronger,
still-conditional target share some premises: a gap confined to the stronger
target must not erase independent support for the narrower conclusion.

Attachment traversal is iterative and tolerates attachment cycles. Returned
target lists preserve authored order. The same adjacency supports both depth
policies: the longest route governs generation depth, while the shortest route
governs display visibility. Evidence needs are grouped by their enabling claim,
so needs with the same root share one dependency traversal. The index does not
store a transitive closure for every node.

`ArgumentIndex::new` constructs a query view without certifying admission. Public
consumers requiring admitted input use `validate_argument`. Consumers performing
several queries retain one index or validated view for the operation.
`relied_dependency_closure` delegates to the same index.

## Explain the graph without changing its meaning

Each reading section begins with an answer paragraph referencing its own claim.
One paragraph is sufficient when it carries the explanation. Additional
paragraphs declare their dominant reasoning purpose. Admission checks coverage,
paragraph shape and references. The critic assesses whether the prose preserves
the reasoning and its conditions. Counts and labels alone cannot establish that
the explanation is complete.

The next step is to give the graph's material dependencies obtainable
[support requirements](support-planning.md). [Reasoning methods](REASONING_METHODS.md)
describe which formal relations EAL can execute, while the
[presentation API](PRESENTATION_API.md) describes how the admitted argument is
shown without dropping consequential conditions.

The vocabulary follows [Toulmin, *The Uses of Argument*](https://doi.org/10.1017/CBO9780511840005).
The treatment of defeaters and unsupported outcomes also draws on
[Bloomfield, Netkachova and Rushby's assurance-argument work](https://arxiv.org/html/2405.15800v1).
These sources motivate the representation. The [validation guide](VALIDATION.md)
identifies the implemented checks and the properties they establish.
