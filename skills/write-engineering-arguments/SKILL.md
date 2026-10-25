---
name: write-engineering-arguments
description: Rewrite every part of an extended engineering argument in the shortest clear British English that preserves its reasoning, scope and evidential limits. Use when constructing, synthesising, critiquing or revising engarg argument records and their reading prose, including verbose claims, warrants, assumptions, challenges and evidence plans.
---

# Write Engineering Arguments

Write for an experienced engineer assessing whether the conclusion follows and what would change it. Prefer the shortest wording that preserves the complete reasoning. Treat clarity and semantic fidelity as acceptance criteria rather than a fixed word count.

## Preserve the argument

Complete the calling role's substantive construction, critique or repair before the editorial pass. Use its evidential rules to govern changes to conclusions, scope, selected routes and assessments. Then preserve that revised argument during rewriting. Original input and explicitly frozen fields remain exact throughout both stages. Apply the preservation rules below to the editorial pass rather than using them to block a required substantive correction.

Use the argument glossary to identify each part's function. Read the complete argument and its supplied evidence before editing individual fields. Keep these properties exact through the rewrite:

- Original input and fields explicitly frozen by the calling workflow, including retained objection identities and challenges.
- IDs, references, support relationships, selected routes, status values and executable tool configuration.
- Quantifiers, negation, units, thresholds, uncertainty, time bounds and applicability conditions.
- Evidential force, including the distinction between supplied reports, assumptions, missing observations, proposed checks and performed checks.
- Material objections, failure consequences and conditions attached to the conclusion.

Preserve an essential negative finding or falsification condition even when it needs negative wording. Treat opaque technical expressions as exact terms until their meaning is established. Leave a substantive ambiguity explicit and retain the associated clarification or evidence obligation. Resolve it only when the supplied context supports one interpretation.

## Rewrite each part for its job

| Part or field | Writing task |
| --- | --- |
| Claim and subclaim | State the system's property and applicable conditions directly. Keep its qualifier adjacent. |
| Scope | Name the actual boundary or operating condition in the relevant field. Reuse established terms consistently. |
| Ground | State the premise or reported finding once. Keep its evidence references and recorded status. |
| Warrant | State how the premises support the conclusion. Name the rule or mechanism and its applicability. |
| Backing | State what supports the warrant and the domain in which that support applies. |
| Composition rationale | Explain why the selected routes supply the required support and how shared dependencies affect it. |
| Assumption | State the condition relied upon. Keep its consequence of failure and proposed validation explicit. |
| Defeater and response | State the challenged premise or inference and its consequence. Preserve the response's actual evidential status. |
| Alternative | State the competing explanation and the check that would distinguish it. |
| Evidence need | Ask one answerable question. Preserve the method, comparison, measurement, criterion and failure interpretation in their own fields. |
| Reading and adjudication | Connect the qualified answer to the decisive reason, consequential limitation and next check. |

Use one sentence for each claim, warrant, assumption and challenge where the output contract requires it. Give every sentence one argumentative task. Use several short sentences within other fields when a single sentence would obscure the relation.

## Revise recursively

1. Replace vague nouns and verbs with the intended property, actor, relation or consequence.
2. Put the actor and action early. Prefer familiar engineering terms and direct verbs.
3. Delete throat-clearing, repeated conclusions and commentary about the writing process.
4. Replace comma-separated inventories with the governing relation or separate sentences. Avoid semicolons and prose colons. Preserve punctuation inside exact code, quoted input and technical expressions.
5. Use “because” for an actual inference and “provided that” for a required condition. Keep shared reasoning in one place and retain explicit references from its uses.
6. After every deletion or merger, reread the surrounding passage. Repair its logical connection and remove repetition exposed by the edit.
7. Compare the revised wording with the source field and its dependent claims. Reject an edit that drops a condition, changes a proposition or upgrades support.
8. Check the complete reading view against the revised canonical graph. Return the required output format without extra editorial commentary.

Retain required schema fields and each section's opening answer with its claim reference. Use one paragraph when it states the complete reasoning clearly. Add paragraphs only for distinct reasoning tasks. State a conclusion again only when the intervening argument changes or qualifies the opening answer. Keep glossary definitions in reader help rather than restating them within each argument part.

## Examples

Ground before: “The supplied benchmark report provides an indication that the observed p95 latency was 42 ms at a workload of 500 requests per second.”

Ground after: “The supplied benchmark report records p95 latency of 42 ms at 500 requests per second.”

Warrant before: “It would be reasonable to infer that the service can be considered to satisfy the latency requirement on the assumption that the test workload is representative of production.”

Warrant after: “The measured latency supports compliance with the latency requirement provided that the test workload represents production.”

Assumption before: “A condition on which this argument depends is that the production workload remains within the workload envelope covered by the benchmark.”

Assumption after: “The production workload stays within the benchmark's workload envelope.”

Evidence need before: “There is a need to undertake an investigation into the question of whether retries could lead to duplicated writes.”

Evidence need after: “Can retries duplicate a write?”

Keep that investigation's configured retry conditions, measurement and acceptance criterion in its remaining fields.
