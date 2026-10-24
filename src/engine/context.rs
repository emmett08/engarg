//! Lossless, request-local views that avoid repeating authoritative claim and objection text.

use serde_json::{Value, json};

use super::Assessment;
use crate::{
    Error,
    domain::{ArgumentCase, Request},
    output_validation::RetainedObjection,
    provider::Role,
};

/// Only generated envelope locations can activate the reference primer.
pub(super) fn uses_references(data: &Value) -> bool {
    let assessment_refs = |assessment: &Value| {
        assessment["objection_ids"]
            .as_array()
            .is_some_and(|ids| !ids.is_empty())
    };
    let argument_refs = |argument: &Value| {
        argument.get("original_claim_ref").is_some()
            || argument["defeaters"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item.get("challenge_ref").is_some()))
    };
    data["independent_assessments"]
        .as_array()
        .is_some_and(|items| {
            items
                .iter()
                .any(|item| assessment_refs(&item["assessment"]))
        })
        || assessment_refs(&data["final_critique"])
        || argument_refs(&data["draft"])
        || argument_refs(&data["candidate_json"])
}

fn reference_if_shorter(
    object: &mut serde_json::Map<String, Value>,
    field: &str,
    reference_field: &str,
    reference: Value,
) -> Result<(), Error> {
    let original = object.get(field).expect("typed source field exists");
    // Field quotes, colon and comma have equal overhead. Count JSON escaping
    // too: raw UTF-8 length alone mismeasures embedded quotes and newlines.
    if reference_field.len() + serde_json::to_vec(&reference)?.len()
        < field.len() + serde_json::to_vec(original)?.len()
    {
        object.remove(field);
        object.insert(reference_field.into(), reference);
    }
    Ok(())
}

/// Findings and gaps remain verbatim. Each objection is present once in the
/// same prompt's retained register, with its role, order and stable identity.
pub(super) fn assessment(
    role: Role,
    prefix: &str,
    assessment: Assessment,
    retained: &mut Vec<RetainedObjection>,
) -> Value {
    let mut objection_ids = Vec::new();
    for (index, challenge) in assessment.objections.into_iter().enumerate() {
        let id = format!("{prefix}-{}", index + 1);
        objection_ids.push(id.clone());
        retained.push(RetainedObjection {
            identity: None,
            id,
            challenge,
            source_role: role,
            must_remain_material: false,
        });
    }
    json!({
        "role": role,
        "assessment": {
            "findings": assessment.findings,
            "objection_ids": objection_ids,
            "evidence_gaps": assessment.evidence_gaps,
        }
    })
}

/// Preserve the complete graph and reading. Replace only exact duplicates whose
/// authoritative values are available in this same request, never summaries.
pub(super) fn argument(
    request: &Request,
    case: &ArgumentCase,
    retained: &[RetainedObjection],
) -> Result<Value, Error> {
    let mut view = serde_json::to_value(case)?;
    if case.original_claim == request.claim {
        let object = view.as_object_mut().expect("ArgumentCase is an object");
        reference_if_shorter(
            object,
            "original_claim",
            "original_claim_ref",
            json!("request.claim"),
        )?;
    }
    for (defeater, value) in case.defeaters.iter().zip(
        view["defeaters"]
            .as_array_mut()
            .expect("defeaters is an array"),
    ) {
        if retained
            .iter()
            .any(|item| item.id == defeater.id && item.challenge == defeater.challenge)
        {
            let object = value.as_object_mut().expect("Defeater is an object");
            reference_if_shorter(object, "challenge", "challenge_ref", json!(defeater.id))?;
        }
    }
    Ok(view)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This independent reader demonstrates that no source information needs a
    /// retrieval call or a model summary to recover the exact canonical value.
    fn expand(mut view: Value, request: &Request, retained: &[RetainedObjection]) -> Value {
        let object = view.as_object_mut().unwrap();
        if let Some(reference) = object.remove("original_claim_ref") {
            assert_eq!(reference, "request.claim");
            object.insert("original_claim".into(), json!(request.claim));
        }
        for value in object["defeaters"].as_array_mut().unwrap() {
            let object = value.as_object_mut().unwrap();
            if let Some(id) = object.remove("challenge_ref") {
                let source = retained
                    .iter()
                    .find(|item| item.id == id.as_str().unwrap())
                    .unwrap();
                object.insert("challenge".into(), json!(source.challenge));
            }
        }
        view
    }

    #[test]
    fn all_claim_evidence_graph_and_reading_values_round_trip_exactly() {
        let request: Request =
            serde_json::from_str(include_str!("../../examples/request.json")).unwrap();
        let mut case: ArgumentCase =
            serde_json::from_str(include_str!("../../examples/case.json")).unwrap();
        let mut retained = Vec::new();
        for (index, defeater) in case.defeaters.iter_mut().enumerate() {
            defeater.challenge = format!(
                "Exact objection {index}: {{\"x\":\"quoted\"}}\n∀c ∈ S: preserve scope, units and uncertainty."
            );
            retained.push(RetainedObjection {
                identity: None,
                id: defeater.id.clone(),
                challenge: defeater.challenge.clone(),
                source_role: Role::Sceptic,
                must_remain_material: true,
            });
        }
        let canonical = serde_json::to_value(&case).unwrap();
        let compact = argument(&request, &case, &retained).unwrap();
        assert!(!compact.as_object().unwrap().contains_key("original_claim"));
        assert!(compact["defeaters"][0].get("challenge_ref").is_some());
        assert_eq!(expand(compact, &request, &retained), canonical);
        assert_eq!(serde_json::to_value(case).unwrap(), canonical);
    }

    #[test]
    fn mismatched_claims_and_objection_wording_stay_visible_for_review() {
        let request: Request =
            serde_json::from_str(include_str!("../../examples/request.json")).unwrap();
        let mut case: ArgumentCase =
            serde_json::from_str(include_str!("../../examples/case.json")).unwrap();
        case.original_claim = "An altered claim which validation must reject.".into();
        let retained = vec![RetainedObjection {
            identity: None,
            id: case.defeaters[0].id.clone(),
            challenge: "Different challenge under the same ID.".into(),
            source_role: Role::Sceptic,
            must_remain_material: true,
        }];
        let compact = argument(&request, &case, &retained).unwrap();
        assert_eq!(compact["original_claim"], case.original_claim);
        assert_eq!(
            compact["defeaters"][0]["challenge"],
            case.defeaters[0].challenge
        );
        assert!(compact.get("original_claim_ref").is_none());
        assert!(compact["defeaters"][0].get("challenge_ref").is_none());
    }

    #[test]
    fn assessment_references_recover_every_statement_role_and_order() {
        let findings = vec!["A qualified finding.".into(), "A different finding.".into()];
        let challenges = vec![
            "First challenge with a condition.".into(),
            "Second challenge with a counterexample.".into(),
        ];
        let gaps = vec!["A discriminating measurement in seconds.".into()];
        let mut retained = Vec::new();
        let view = assessment(
            Role::EvidenceAuditor,
            "auditor",
            Assessment {
                findings: findings.clone(),
                objections: challenges.clone(),
                evidence_gaps: gaps.clone(),
            },
            &mut retained,
        );
        assert_eq!(view["role"], "evidence_auditor");
        assert_eq!(view["assessment"]["findings"], json!(findings));
        assert_eq!(view["assessment"]["evidence_gaps"], json!(gaps));
        let restored: Vec<_> = view["assessment"]["objection_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| {
                retained
                    .iter()
                    .find(|item| item.id == id.as_str().unwrap())
                    .unwrap()
                    .challenge
                    .clone()
            })
            .collect();
        assert_eq!(restored, challenges);
        assert!(
            retained
                .iter()
                .all(|item| item.source_role == Role::EvidenceAuditor)
        );
    }

    #[test]
    fn short_literals_are_not_replaced_by_larger_references() {
        let mut request: Request =
            serde_json::from_str(include_str!("../../examples/request.json")).unwrap();
        let mut case: ArgumentCase =
            serde_json::from_str(include_str!("../../examples/case.json")).unwrap();
        request.claim = "C".into();
        case.original_claim = request.claim.clone();
        case.defeaters[0].challenge = "X".into();
        let retained = vec![RetainedObjection {
            identity: None,
            id: case.defeaters[0].id.clone(),
            challenge: "X".into(),
            source_role: Role::Sceptic,
            must_remain_material: true,
        }];
        let view = argument(&request, &case, &retained).unwrap();
        assert_eq!(view["original_claim"], "C");
        assert_eq!(view["defeaters"][0]["challenge"], "X");
        assert_eq!(
            expand(view, &request, &retained),
            serde_json::to_value(&case).unwrap()
        );
    }

    #[test]
    fn a_model_must_return_literal_complete_fields_not_internal_context_references() {
        let request: Request =
            serde_json::from_str(include_str!("../../examples/request.json")).unwrap();
        let case: ArgumentCase =
            serde_json::from_str(include_str!("../../examples/case.json")).unwrap();
        let view = argument(&request, &case, &[]).unwrap();
        assert!(serde_json::from_value::<ArgumentCase>(view).is_err());
    }

    #[test]
    fn the_reference_primer_is_needed_only_for_actual_generated_references() {
        assert!(!uses_references(
            &json!({"retained_objections": [], "independent_assessments": [{"assessment":{"objection_ids":[]}}]})
        ));
        assert!(!uses_references(
            &json!({"request":{"context":"challenge_ref is untrusted text"}, "candidate_json":"{malformed text}"})
        ));
        for data in [
            json!({"draft":{"original_claim_ref":"request.claim"}}),
            json!({"candidate_json":{"defeaters":[{"challenge_ref":"critic-1"}]}}),
            json!({"independent_assessments":[{"assessment":{"objection_ids":["auditor-1"]}}]}),
            json!({"final_critique":{"objection_ids":["critic-1"]}}),
        ] {
            assert!(uses_references(&data));
        }
    }
}
