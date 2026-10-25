# Argument model ownership

The serialised records in `domain/records.rs` retain their string encodings.
Their schema describes representation. Admission and assessment policy have
separate owners.

| Boundary | Typed owner or reason for text |
| --- | --- |
| Claim, ground, warrant, assumption and defeater IDs | `ArgumentId` and `ArgumentIndex` |
| `conclusion_id`, `premise_ids`, `applies_to`, `target_id` and `enables_claim` | Reference admission in `validation/structure.rs`, then indexed argument IDs |
| Evidence IDs and backing references | `EvidenceId`, request admission and the evidence-reference policy |
| Evidence-need IDs | `NeedId` and the index's need lookup |
| Selected warrant IDs | Validated composition and indexed selected-route membership |
| Narrative `reference_ids` | Deliberate union of argument, evidence and need IDs, admitted against the canonical set |
| Case, ground, assumption, defeater and support states | Existing closed enums in the records module |
| Inference kind, attack kind, defeater basis and paragraph purpose | Existing finite-state enums in the records module |
| Evidence `kind`, methods, qualifiers, rules and prose | Intentional extensible text rather than an artificial closed vocabulary |
| Admission diagnostics | `DiagnosticCode` with location and human-readable message |

`validate_argument` checks request and reference integrity before constructing
one borrowed index. Composition and support-cycle checks then use that index.
Narrative, evidence and status policies run over the admitted graph. Successful
admission returns `ValidatedArgument`, whose source borrow prevents mutation
while its index remains in use. The compatible `validate_case` function formats
the same diagnostics as strings.

Request admission stops before dependent reference checks when it fails.
Canonical declaration IDs admit before diagnostic labels use them per edge.
Reading sections must name `current_claim`, `stronger_target` or a declared
subclaim. Paragraph reference lists admit known distinct canonical IDs before
narrative policy runs. Unknown reading targets and unknown or repeated paragraph
references now report the existing `ReferenceIntegrity` category rather than
`NarrativePolicy`. No diagnostic enum or serialised record changes.
Consumers classifying invalid reading inputs must handle that corrected category.
Presentations reject those unsafe references. Reference-safe policy defects,
such as missing paragraph text or titles, remain eligible for labelled inspection.

The index distinguishes all candidate routes from selected reliance. Selection
records the author's adequacy judgement and supplies no evidence that the
judgement is correct. Warrant-specific dependencies include that inference's
conclusion and attached conditions, while excluding sibling inference routes.
Attachment traversal is iterative and supports cycles. Lists returned for a
target preserve authored order.

Reading sections require an opening answer paragraph that references their own
claim ID. A section can use one paragraph when it carries the complete explanation.
Additional paragraphs declare their dominant reasoning purpose as needed.
Admission checks section coverage, paragraph shape and canonical references.
The critic checks whether the prose preserves the graph's reasoning and conditions.
Paragraph counts and purpose labels alone establish no semantic completeness.

Claim levels use the same adjacency. Longest routes govern generation depth.
Shortest routes govern display visibility. Evidence needs are grouped by their
enabling claim so one dependency traversal serves all needs with the same root.
No index stores every node's transitive closure. Traversal counters allow tests
to inspect work without timing assertions.

`ArgumentIndex::new` is intentionally a query constructor rather than a claim of
admission. Public consumers that require validated input use `validate_argument`.
The standalone `relied_dependency_closure` compatibility function delegates to
the index. Consumers performing multiple queries retain one index or validated
view for the entire operation.
