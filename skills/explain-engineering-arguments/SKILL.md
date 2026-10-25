---
name: explain-engineering-arguments
description: Explain and apply engarg's extended Toulmin argument roles, including N evidence needs, D defeaters, S subclaims, W warrants and G grounds. Use when drafting, rewriting or reviewing an argument, interpreting its identifiers, answering a terminology question, or maintaining reader help for the argument model.
---

# Explain engineering arguments

Read [roles.json](references/roles.json) as the shared role dictionary.
Use its definitions when classifying or rewriting each argument part.
Use the declared record type and reference relationships to identify a role.
Treat letter prefixes as naming conventions rather than a type system.
An identifier such as `D-cache` names a particular record.
The glossary explains the role that record performs.

## Explain only what the reader needs

For a terminology question, give the role's name and function.
Add its guiding question and one example when these clarify the answer.
For a question about an existing argument, explain the selected record using its actual premises and target.
Keep invented examples clearly labelled as examples.
Offer the full glossary only when requested or through a separate reader-help view.
Keep repeated definitions outside the argument prose.

During drafting and revision, use the dictionary internally to check each part's purpose.
Check that grounds state premises and warrants explain the inference.
Check that backing supports a warrant and that evidence needs specify proposed investigations.
Preserve the recorded distinction between supplied evidence, assumptions and missing support.
Explain scope as the cases covered and a qualifier as the justified force of a claim.

## Preserve the argument's meaning

Retain identifiers, links, statuses, quantities, qualifiers and evidence references.
Explain a suspected misclassification before proposing a change to the graph.
Assess the content and its relationships separately from the label chosen for it.
Treat an available tool or a proposed check as a way to acquire evidence.
Attribute observations to the retained source that records them.
Retain uncertainty and unresolved challenges when explaining an assessment.

## Maintain one vocabulary

Update `roles.json` when a model role changes.
Check its `field` mappings against `src/domain/records.rs` and its role labels against `templates/engineering-toulmin.md`.
Review runtime glossary output and generation prompts after changing the dictionary.
Preserve the distinction between Toulmin's core roles and engarg's engineering extensions.

The six core roles are claim, grounds, warrant, backing, qualifier and rebuttal.
The definitions here are original engineering explanations informed by [Purdue OWL's account of the Toulmin model](https://owl.purdue.edu/owl/general_writing/academic_writing/historical_perspectives_on_argumentation/toulmin_argument.html).
Engarg's record types and template govern its extensions and storage mappings.
In particular, its rebuttal-conditions entry describes an exception to an inference, while a rebutting defeater challenges a conclusion.
