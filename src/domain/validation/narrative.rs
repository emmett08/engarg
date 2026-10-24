//! Paragraph shape, section coverage and traceability requirements.

use super::*;

/// Narrative references provide traceability, not semantic equivalence between
/// the graph and the prose. Both the prose and its interpretation need review.
pub(super) fn validate(case: &ArgumentCase, errors: &mut Vec<String>) {
    let mut seen = HashSet::new();
    for section in &case.reading {
        let claim = section.claim_id.as_str();
        if !seen.insert(claim) {
            errors.push(format!("duplicate reading section for claim {claim:?}"));
        }
        nonempty(
            errors,
            &format!("reading section {claim} title"),
            &section.title,
        );
        if section.paragraphs.len() < 2 {
            errors.push(format!(
                "reading section {claim:?} needs at least an answer and conclusion paragraph"
            ));
        }
        if section
            .paragraphs
            .first()
            .is_none_or(|p| p.purpose != ParagraphPurpose::Answer)
        {
            errors.push(format!(
                "reading section {claim:?} must start with an answer paragraph"
            ));
        }
        if section
            .paragraphs
            .last()
            .is_none_or(|p| p.purpose != ParagraphPurpose::Conclusion)
        {
            errors.push(format!(
                "reading section {claim:?} must end with a conclusion paragraph"
            ));
        }
        if section
            .paragraphs
            .first()
            .is_none_or(|p| !p.reference_ids.contains(&section.claim_id))
        {
            errors.push(format!(
                "reading section {claim:?} first answer must reference its own claim ID"
            ));
        }
        if claim == "current_claim" {
            let purposes: HashSet<ParagraphPurpose> =
                section.paragraphs.iter().map(|p| p.purpose).collect();
            if [
                ParagraphPurpose::Answer,
                ParagraphPurpose::Finding,
                ParagraphPurpose::Connection,
                ParagraphPurpose::Complication,
                ParagraphPurpose::Conclusion,
            ]
            .iter()
            .any(|purpose| !purposes.contains(purpose))
            {
                errors.push("current_claim reading must include answer, finding, connection, complication and conclusion paragraphs".into());
            }
        }
        for (index, paragraph) in section.paragraphs.iter().enumerate() {
            let label = format!("reading {claim} paragraph {}", index + 1);
            nonempty(errors, &format!("{label} text"), &paragraph.text);
            let normalised = paragraph.text.replace("\r\n", "\n");
            if normalised.contains("\n\n") || normalised.lines().any(|line| line.trim().is_empty())
            {
                errors.push(format!(
                    "{label} text must contain one paragraph, without blank lines"
                ));
            }
            if paragraph.reference_ids.is_empty() {
                errors.push(format!(
                    "{label} must reference canonical graph or evidence IDs"
                ));
            }
        }
    }
    for claim in
        std::iter::once("current_claim").chain(case.subclaims.iter().map(|s| s.id.as_str()))
    {
        if !seen.contains(claim) {
            errors.push(format!("claim {claim:?} needs exactly one reading section"));
        }
    }
}
