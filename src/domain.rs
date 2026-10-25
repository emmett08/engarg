//! Extended Toulmin records, graph queries and staged validation.
//!
//! Records retain their established string wire encoding. ValidatedArgument
//! borrows the source and owns the index used by downstream operations.

mod diagnostics;
mod identifiers;
mod index;
mod records;
mod validation;

pub use diagnostics::{DiagnosticCode, DomainDiagnostic};
pub use identifiers::{ArgumentId, EvidenceId, NeedId};
pub use index::{ArgumentIndex, ArgumentNode, Traversal, TraversalStats, ValidatedArgument};
pub use records::*;
pub use validation::{validate_argument, validate_case, validate_request};

/// Compatibility query for one operation. Consumers making several queries
/// should retain one ArgumentIndex or ValidatedArgument instead.
pub fn relied_dependency_closure(
    case: &ArgumentCase,
    claim_id: &str,
) -> std::collections::HashSet<String> {
    ArgumentIndex::new(case)
        .selected_dependency(claim_id)
        .into_iter()
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
fn support_closure(case: &ArgumentCase, root: &str) -> std::collections::HashSet<String> {
    ArgumentIndex::new(case)
        .candidate_support(root)
        .into_iter()
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim_only_candidate() -> (Request, ArgumentCase) {
        let request: Request = serde_json::from_str(
            r#"{"claim":"Parallelising the integration tests reduces CI time by at least 40% while preserving test outcomes."}"#,
        )
        .unwrap();
        let case = serde_json::from_value(serde_json::json!({
            "original_claim": request.claim,
            "current_claim": {
                "text": "If test groups are independent and runner resources permit overlap, parallel scheduling can reduce elapsed test time while preserving outcomes.",
                "qualifier": "Conditional candidate mechanism constructed from the claim alone; no observations have been supplied."
            },
            "stronger_target": {
                "text": "For frozen test revisions and runner configurations, parallel scheduling reduces median elapsed CI time by at least 40% while meeting separately specified outcome-preservation criteria.",
                "qualifier": "A proposed empirical target requiring measurements and isolation checks."
            },
            "status": "conditional",
            "scope": {
                "system_boundary": "The integration-test stage, including fixture setup and teardown.",
                "configuration": "Runner capacity, test revisions and fixture ownership are unspecified and must be recorded before the trial.",
                "operating_conditions": "Representative revisions, cache states and contention conditions remain to be selected and recorded.",
                "units": "Elapsed time in seconds, relative reduction in percent and outcome discrepancies as counts.",
                "time_horizon": "The proposed trial and its identified revisions.",
                "consequences": "Reduced feedback delay is useful only while test reliability remains acceptable."
            },
            "subclaims": [],
            "grounds": [
                {"id":"G1", "statement":"At least two groups can execute concurrently without changing shared mutable state that affects their outcomes.", "evidence_ids":[], "status":"assumed"},
                {"id":"G2", "statement":"Paired observations meet the 40% timing target and the independently specified outcome criteria.", "evidence_ids":[], "status":"missing"}
            ],
            "warrants": [
                {"id":"W1", "rule":"Overlapping independent work can shorten a sequential execution path when resource contention and setup costs do not erase the overlap.", "conclusion_id":"current_claim", "premise_ids":["G1"], "backing_evidence_ids":[], "inference":"causal", "applicability":"A proposed mechanism under explicit independence and capacity conditions, rather than an observed improvement."},
                {"id":"W2", "rule":"Paired measurements can support a bounded scheduling comparison when revisions, outcome criteria and confounders are controlled.", "conclusion_id":"stronger_target", "premise_ids":["G1","G2"], "backing_evidence_ids":[], "inference":"inductive", "applicability":"Requires a prospective design, retained observations and adequate isolation checks."}
            ],
            "compositions": [
                {"conclusion_id":"current_claim", "relationship":"jointly_required", "selected_warrant_ids":["W1"], "rationale":"The present conditional argument relies on the scheduling mechanism in W1."},
                {"conclusion_id":"stronger_target", "relationship":"jointly_required", "selected_warrant_ids":["W2"], "rationale":"The empirical target relies on W2 with both independence and comparative measurements required."}
            ],
            "assumptions": [
                {"id":"A1", "statement":"The runner has sufficient capacity for useful execution overlap.", "critical":true, "status":"untested", "evidence_ids":[], "applies_to":["W1","W2"], "validation":"Profile CPU, memory, setup costs and contention in the sequential and parallel configurations."}
            ],
            "defeaters": [
                {"id":"D1", "kind":"undercutting", "target_id":"W1", "challenge":"Runner contention or fixture setup may erase the expected overlap benefit.", "status":"open", "material":true, "basis":"hypothetical", "response":"Investigate the resource and setup profile before relying on the proposed mechanism; this concern is not an observed failure.", "evidence_ids":[]}
            ],
            "alternatives": [
                {"explanation":"Fixture initialisation may dominate the stage, leaving little schedulable work to overlap.", "distinguishing_test":"Measure fixture and test-body durations separately in both configurations.", "evidence_ids":[]}
            ],
            "evidence_plan": [
                {"id":"N1", "target_id":"G1", "question":"Can groups overlap without affecting outcomes?", "method":"Inspect fixture ownership and run repeated isolation and scheduling-perturbation checks on frozen revisions.", "measurement":"Cross-group state interference and outcome discrepancies as counts.", "comparator":"Sequential execution and isolated-fixture parallel execution on the same revisions.", "acceptance_criterion":"Meet prospectively specified isolation and outcome criteria, with finite coverage limits reported.", "if_failed":"Repair isolation or retain sequential scheduling for affected groups.", "enables_claim":"current_claim", "priority":1, "cost":"Fixture investigation and repeated test execution."},
                {"id":"N2", "target_id":"G2", "question":"Does parallel execution meet the 40% target while preserving outcomes?", "method":"Pilot variance, then preregister and execute a counterbalanced paired trial with retained timing and outcome records.", "measurement":"Elapsed time in seconds, paired relative reductions in percent and outcome discrepancies as counts.", "comparator":"The sequential workflow on the same revisions and runner configuration.", "acceptance_criterion":"The preregistered interval must support at least a 40% median reduction; outcome-preservation criteria must pass independently.", "if_failed":"Narrow or reject the quantitative target according to the retained observations.", "enables_claim":"stronger_target", "priority":2, "cost":"Runner time and analysis effort estimated from the pilot."},
                {"id":"N3", "target_id":"A1", "question":"Does runner capacity allow useful overlap?", "method":"Record resource utilisation and setup durations under both configurations, including representative contention conditions.", "measurement":"CPU utilisation in percent, memory in bytes and setup and test durations in seconds.", "comparator":"Sequential scheduling on the same runner capacity and revision.", "acceptance_criterion":"Demonstrate useful overlap within prospectively declared capacity and contention conditions.", "if_failed":"Revise the capacity or scheduling proposal and retain a conditional claim.", "enables_claim":"current_claim", "priority":1, "cost":"Instrumentation and representative runs."},
                {"id":"N4", "target_id":"D1", "question":"Do contention and setup costs erase the mechanism's expected benefit?", "method":"Analyse fixture and test-body timing alongside the resource profile in the paired trial.", "measurement":"Fixture and test-body durations in seconds and resource utilisation in declared units.", "comparator":"Matched sequential and parallel runs across recorded contention conditions.", "acceptance_criterion":"The proposed benefit must survive the preregistered contention and setup checks or be restricted to supported conditions.", "if_failed":"Revise the mechanism or restrict its scope before claiming a benefit.", "enables_claim":"current_claim", "priority":1, "cost":"Analysis shared with the capacity investigation."}
            ],
            "reading": [
                {"claim_id":"current_claim", "title":"The strongest conditional argument from the claim alone", "paragraphs":[
                    {"purpose":"answer", "text":"Parallel scheduling offers a plausible route to shorter CI runs if the groups are independent and the runner permits useful overlap. This is a conditional candidate argument because the claim alone supplies no observations.", "reference_ids":["current_claim","G1","A1"]},
                    {"purpose":"finding", "text":"The decisive premise is that concurrent groups preserve their outcomes despite changes in scheduling. Independence is presently assumed, while the comparative measurements needed for the 40% target remain missing.", "reference_ids":["G1","G2","stronger_target"]},
                    {"purpose":"connection", "text":"W1 connects independence and available capacity to the possibility of useful overlap. W2 adds a separate requirement for a controlled comparison before that mechanism can support the quantitative target.", "reference_ids":["W1","W2","A1"]},
                    {"purpose":"complication", "text":"Fixture setup or resource contention could consume the time expected to be saved. The isolation and capacity investigations therefore precede reliance on the mechanism, and the paired trial tests the quantitative target separately.", "reference_ids":["D1","N1","N2","N3","N4"]},
                    {"purpose":"conclusion", "text":"Begin by freezing the configuration and checking independence and capacity, then run the preregistered paired comparison. Retain the 40% reduction as an empirical target until the observations justify accepting, narrowing or rejecting it.", "reference_ids":["current_claim","stronger_target","N1","N2","N3"]}
                ]}
            ],
            "residual_risks": ["Finite checks may miss rare interference or untested workload conditions."],
            "decision_limits": ["This candidate supports planning an investigation; it supplies no measured performance or production assurance."],
            "adjudication": "The claim admits a complete conditional mechanism and discriminating evidence plan. Its quantitative and outcome assertions require observations before empirical support can be claimed."
        }))
        .unwrap();
        (request, case)
    }

    #[test]
    fn claim_only_json_defaults_optional_context_and_evidence() {
        let request: Request =
            serde_json::from_str(r#"{"claim":"The change reduces latency."}"#).unwrap();
        assert_eq!(request.claim, "The change reduces latency.");
        assert!(request.context.is_empty());
        assert!(request.evidence.is_empty());
        assert_eq!(validate_request(&request), Ok(()));
        assert!(
            serde_json::from_str::<Request>(r#"{"context":"A context without a claim"}"#).is_err()
        );
    }

    #[test]
    fn request_schema_requires_only_the_claim_and_declares_empty_defaults() {
        let schema = serde_json::to_value(schemars::schema_for!(Request)).unwrap();
        assert_eq!(schema["required"], serde_json::json!(["claim"]));
        assert_eq!(schema["properties"]["context"]["default"], "");
        assert_eq!(
            schema["properties"]["evidence"]["default"],
            serde_json::json!([])
        );
        assert_eq!(schema["additionalProperties"], false);
    }

    #[test]
    fn optional_request_fields_preserve_supplied_context_and_source_records() {
        let (request, _) = fixtures();
        let mut input = serde_json::to_value(&request).unwrap();
        let explicit: Request = serde_json::from_value(input.clone()).unwrap();
        assert_eq!(explicit.context, request.context);
        input.as_object_mut().unwrap().remove("context");
        let without_context: Request = serde_json::from_value(input).unwrap();
        assert!(without_context.context.is_empty());
        assert_eq!(
            serde_json::to_value(without_context.evidence).unwrap(),
            serde_json::to_value(request.evidence).unwrap()
        );
    }

    #[test]
    fn a_complete_conditional_candidate_needs_no_supplied_sources() {
        let (request, case) = claim_only_candidate();
        assert!(request.context.is_empty() && request.evidence.is_empty());
        assert_eq!(case.status, CaseStatus::Conditional);
        assert!(
            case.grounds
                .iter()
                .all(|ground| ground.evidence_ids.is_empty()
                    && matches!(ground.status, GroundStatus::Assumed | GroundStatus::Missing))
        );
        assert_eq!(validate_case(&request, &case), Ok(()));
    }

    #[test]
    fn a_claim_only_candidate_cannot_invent_reported_or_provisional_support() {
        let (request, mut case) = claim_only_candidate();
        case.grounds[0].status = GroundStatus::Reported;
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|error| error.contains("reported ground G1 needs supplied evidence"))
        );
        case.grounds[0].status = GroundStatus::Assumed;
        case.status = CaseStatus::ProvisionallySupported;
        let errors = validate_case(&request, &case).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.contains("missing or assumed grounds"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("user-checked evidence"))
        );
    }

    fn fixtures() -> (Request, ArgumentCase) {
        (
            serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
            serde_json::from_str(include_str!("../examples/case.json")).unwrap(),
        )
    }

    fn provisional_narrow_case() -> (Request, ArgumentCase) {
        let (mut request, mut case) = fixtures();
        request.evidence[0].checked_by_user = true;
        case.status = CaseStatus::ProvisionallySupported;
        case.current_claim.text =
            "The supplied synthetic description reports sequential test groups.".into();
        case.current_claim.qualifier = "A statement about the supplied description only.".into();
        case.warrants
            .iter_mut()
            .find(|w| w.conclusion_id == "current_claim")
            .unwrap()
            .premise_ids = vec!["S1".into()];
        case.assumptions[0].applies_to = vec!["stronger_target".into()];
        case.evidence_plan
            .iter_mut()
            .find(|p| p.target_id == "A1")
            .unwrap()
            .enables_claim = "stronger_target".into();
        for defeater in &mut case.defeaters {
            defeater.target_id = "stronger_target".into();
        }
        (request, case)
    }

    #[test]
    fn synthetic_example_is_structurally_valid() {
        let (request, case) = fixtures();
        assert_eq!(validate_request(&request), Ok(()));
        assert_eq!(validate_case(&request, &case), Ok(()));
        assert_ne!(case.status, CaseStatus::ProvisionallySupported);
    }

    #[test]
    fn invented_evidence_is_rejected() {
        let (request, mut case) = fixtures();
        case.grounds[0].evidence_ids.push("invented".into());
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("unknown evidence"))
        );
    }

    #[test]
    fn unsupported_is_not_refuted() {
        let (request, mut case) = fixtures();
        case.status = CaseStatus::Refuted;
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("targeting current_claim exactly"))
        );
    }

    #[test]
    fn model_cannot_upgrade_critical_assumptions_into_provisional_support() {
        let (request, mut case) = fixtures();
        case.status = CaseStatus::ProvisionallySupported;
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("critical assumptions"))
        );
    }

    #[test]
    fn duplicate_ids_and_broken_warrant_targets_are_rejected() {
        let (request, mut case) = fixtures();
        let original_id = case.warrants[0].id.clone();
        case.warrants[0].id = case.grounds[0].id.clone();
        case.warrants[0].premise_ids = vec!["absent-ground".into()];
        let errors = validate_case(&request, &case).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("duplicate")));
        // Dependent reference checks resume only after identity admission.
        case.warrants[0].id = original_id;
        let errors = validate_case(&request, &case).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("unknown premise")));
    }

    #[test]
    fn every_critical_assumption_needs_a_test() {
        let (request, mut case) = fixtures();
        let target = case.assumptions[0].id.clone();
        let removed: std::collections::HashSet<_> = case
            .evidence_plan
            .iter()
            .filter(|p| p.target_id == target)
            .map(|p| p.id.clone())
            .collect();
        case.evidence_plan.retain(|p| p.target_id != target);
        // Keep reference admission valid so this test reaches the independent
        // critical-assumption policy rather than a dangling reading reference.
        for paragraph in case.reading.iter_mut().flat_map(|s| &mut s.paragraphs) {
            paragraph.reference_ids.retain(|id| !removed.contains(id));
        }
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("critical assumption"))
        );
    }

    #[test]
    fn supported_assumptions_need_evidence_references() {
        let (request, mut case) = fixtures();
        case.assumptions[0].status = AssumptionStatus::Supported;
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("supported or contradicted assumption"))
        );
    }

    #[test]
    fn stronger_target_gaps_do_not_defeat_support_for_a_narrow_current_claim() {
        let (request, case) = provisional_narrow_case();
        assert!(
            case.grounds
                .iter()
                .any(|g| g.status == GroundStatus::Missing)
        );
        assert_eq!(validate_case(&request, &case), Ok(()));
    }

    fn case_with_an_unused_missing_alternative() -> (Request, ArgumentCase) {
        let (request, mut case) = provisional_narrow_case();
        let mut ground = case.grounds[1].clone();
        ground.id = "G3".into();
        case.grounds.push(ground);
        let mut warrant = case
            .warrants
            .iter()
            .find(|w| w.conclusion_id == "current_claim")
            .unwrap()
            .clone();
        warrant.id = "W4".into();
        warrant.premise_ids = vec!["G3".into()];
        case.warrants.push(warrant);
        let composition = case
            .compositions
            .iter_mut()
            .find(|c| c.conclusion_id == "current_claim")
            .unwrap();
        composition.relationship = SupportRelationship::AlternativeRoutes;
        composition.selected_warrant_ids = vec!["W2".into()];
        composition.rationale = "W2 is the selected route for the bounded source-report claim. W4 remains an unselected candidate requiring missing measurements.".into();
        let mut plan = case.evidence_plan[0].clone();
        plan.id = "N5".into();
        plan.target_id = "G3".into();
        plan.enables_claim = "current_claim".into();
        case.evidence_plan.push(plan);
        (request, case)
    }

    #[test]
    fn selected_alternative_survives_missing_support_on_an_unused_route() {
        let (request, case) = case_with_an_unused_missing_alternative();
        assert!(support_closure(&case, "current_claim").contains("G3"));
        assert!(!relied_dependency_closure(&case, "current_claim").contains("G3"));
        assert_eq!(validate_case(&request, &case), Ok(()));
    }

    #[test]
    fn cumulative_support_selects_all_routes_and_keeps_their_gaps_material() {
        let (request, mut case) = case_with_an_unused_missing_alternative();
        let composition = case
            .compositions
            .iter_mut()
            .find(|c| c.conclusion_id == "current_claim")
            .unwrap();
        composition.relationship = SupportRelationship::Cumulative;
        composition.selected_warrant_ids = vec!["W2".into(), "W4".into()];
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("missing or assumed grounds"))
        );
    }

    #[test]
    fn unresolved_alternative_selection_cannot_supply_provisional_support() {
        let (request, mut case) = provisional_narrow_case();
        let composition = case
            .compositions
            .iter_mut()
            .find(|c| c.conclusion_id == "S1")
            .unwrap();
        composition.relationship = SupportRelationship::AlternativeRoutes;
        composition.selected_warrant_ids.clear();
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("selected warrant for reached claim"))
        );
    }

    #[test]
    fn compositions_cannot_be_missing_duplicated_or_select_another_claims_warrant() {
        let (request, mut case) = fixtures();
        let duplicate = case.compositions[0].clone();
        case.compositions.push(duplicate);
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("duplicate composition"))
        );
        case.compositions.pop();
        case.compositions
            .iter_mut()
            .find(|c| c.conclusion_id == "current_claim")
            .unwrap()
            .selected_warrant_ids = vec!["W1".into()];
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("wrong-claim warrant"))
        );
        case.compositions
            .retain(|c| c.conclusion_id != "current_claim");
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("needs exactly one support composition"))
        );
    }

    #[test]
    fn jointly_required_composition_cannot_drop_a_route() {
        let (request, mut case) = case_with_an_unused_missing_alternative();
        case.compositions
            .iter_mut()
            .find(|c| c.conclusion_id == "current_claim")
            .unwrap()
            .relationship = SupportRelationship::JointlyRequired;
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("must select all attached warrants"))
        );
    }

    #[test]
    fn reading_rejects_unknown_and_duplicate_references() {
        let (request, mut case) = fixtures();
        case.reading[0].paragraphs[0]
            .reference_ids
            .extend(["invented".into(), "current_claim".into()]);
        let errors = validate_case(&request, &case).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("unknown canonical ID")));
        assert!(errors.iter().any(|e| e.contains("repeats canonical ID")));
    }

    #[test]
    fn reading_accepts_one_answer_paragraph_without_repeated_conclusions() {
        let (request, mut case) = fixtures();
        for section in &mut case.reading {
            let text = section
                .paragraphs
                .iter()
                .map(|paragraph| paragraph.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let mut reference_ids = section
                .paragraphs
                .iter()
                .flat_map(|paragraph| paragraph.reference_ids.iter().cloned())
                .collect::<Vec<_>>();
            reference_ids.sort_unstable();
            reference_ids.dedup();
            section.paragraphs = vec![ReasoningParagraph {
                purpose: ParagraphPurpose::Answer,
                text,
                reference_ids,
            }];
        }
        assert_eq!(validate_case(&request, &case), Ok(()));
    }

    #[test]
    fn reading_accepts_additional_paragraph_purposes_without_requiring_all_five() {
        let (request, mut case) = fixtures();
        case.reading[0].paragraphs[1].purpose = ParagraphPurpose::Connection;
        case.reading[0].paragraphs.last_mut().unwrap().purpose = ParagraphPurpose::Complication;
        assert_eq!(validate_case(&request, &case), Ok(()));
    }

    #[test]
    fn reading_still_requires_an_opening_answer_and_single_paragraph_text() {
        let (request, mut case) = fixtures();
        case.reading[0].paragraphs[0].purpose = ParagraphPurpose::Finding;
        case.reading[0].paragraphs[0]
            .text
            .push_str("\n\nA second paragraph must be a distinct record.");
        let errors = validate_case(&request, &case).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.contains("must start with an answer paragraph"))
        );
        assert!(
            errors
                .iter()
                .any(|e| e.contains("text must contain one paragraph"))
        );
    }

    #[test]
    fn reading_still_rejects_empty_sections_and_blank_answer_text() {
        let (request, mut case) = fixtures();
        case.reading[0].paragraphs.clear();
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("needs at least one answer paragraph"))
        );
        let (request, mut case) = fixtures();
        case.reading[0].paragraphs[0].text = " \t".into();
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("paragraph 1 text must contain text"))
        );
    }

    #[test]
    fn reading_rejects_crlf_and_whitespace_only_paragraph_separators() {
        for separator in ["\r\n\r\n", "\n \t\n", "\r\n \t\r\n"] {
            let (request, mut case) = fixtures();
            case.reading[0].paragraphs[0].text =
                format!("First paragraph.{separator}Second paragraph.");
            assert!(
                validate_case(&request, &case)
                    .unwrap_err()
                    .iter()
                    .any(|error| error.contains("text must contain one paragraph")),
                "separator {separator:?} must be rejected"
            );
        }
    }

    #[test]
    fn every_subclaim_has_one_reading_section_with_its_own_initial_reference() {
        let (request, mut case) = fixtures();
        let section = case
            .reading
            .iter_mut()
            .find(|s| s.claim_id == "S1")
            .unwrap();
        section.paragraphs[0].reference_ids = vec!["E1".into()];
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("first answer must reference its own claim ID"))
        );
        case.reading.retain(|s| s.claim_id != "S1");
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("needs exactly one reading section"))
        );
    }

    #[test]
    fn test_plan_cannot_enable_an_unrelated_claim() {
        let (request, mut case) = fixtures();
        case.evidence_plan
            .iter_mut()
            .find(|p| p.target_id == "G2")
            .unwrap()
            .enables_claim = "current_claim".into();
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("outside enables_claim"))
        );
    }

    #[test]
    fn rebuttal_of_a_subclaim_cannot_mark_the_current_claim_refuted() {
        let (request, mut case) = fixtures();
        case.status = CaseStatus::Refuted;
        case.defeaters[0].target_id = "S1".into();
        case.defeaters[0].kind = DefeaterKind::Rebutting;
        case.defeaters[0].basis = DefeaterBasis::Observed;
        case.defeaters[0].status = DefeaterStatus::Sustained;
        case.defeaters[0].evidence_ids = vec!["E1".into()];
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("targeting current_claim exactly"))
        );
    }

    #[test]
    fn support_cycles_are_rejected() {
        let (request, mut case) = fixtures();
        case.warrants[0].premise_ids = vec!["S1".into()];
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("cycle"))
        );
    }

    #[test]
    fn unrelated_warrant_cannot_support_current_claim() {
        let (request, mut case) = provisional_narrow_case();
        let removed: std::collections::HashSet<_> = case
            .warrants
            .iter()
            .filter(|w| w.conclusion_id == "current_claim")
            .map(|w| w.id.clone())
            .collect();
        case.warrants.retain(|w| w.conclusion_id != "current_claim");
        // Preserve reference safety while testing the missing support route.
        for paragraph in case.reading.iter_mut().flat_map(|s| &mut s.paragraphs) {
            paragraph.reference_ids.retain(|id| !removed.contains(id));
        }
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("warrant concluding current_claim"))
        );
    }

    #[test]
    fn material_hypothetical_concerns_still_block_provisional_support() {
        let (request, mut case) = provisional_narrow_case();
        case.defeaters[0].target_id = "current_claim".into();
        case.defeaters[0].basis = DefeaterBasis::Hypothetical;
        case.defeaters[0].status = DefeaterStatus::Open;
        case.defeaters[0].material = true;
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("including hypothetical concerns"))
        );
    }

    #[test]
    fn refuted_defeater_requires_evidence() {
        let (request, mut case) = fixtures();
        case.defeaters[0].status = DefeaterStatus::Refuted;
        case.defeaters[0].evidence_ids.clear();
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("refuted defeater"))
        );
    }

    #[test]
    fn unattached_nodes_are_rejected() {
        let (request, mut case) = fixtures();
        case.subclaims.push(ClaimNode {
            id: "orphan".into(),
            text: "Detached assertion".into(),
            qualifier: "Unsupported".into(),
        });
        let errors = validate_case(&request, &case).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.contains("unattached argument node"))
        );
        assert!(
            errors
                .iter()
                .any(|e| e.contains("needs an attached warrant"))
        );
    }

    #[test]
    fn claim_and_evidence_ids_are_preserved() {
        let (mut request, mut case) = fixtures();
        case.original_claim.push('!');
        request.evidence[0].id = "current_claim".into();
        let errors = validate_case(&request, &case).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("original_claim")));
        assert!(errors.iter().any(|e| e.contains("reserved identifier")));
    }

    #[test]
    fn unknown_fields_fail_instead_of_silently_disappearing() {
        let mut value = serde_json::to_value(fixtures().1).unwrap();
        value["confidence"] = serde_json::json!(0.99);
        assert!(serde_json::from_value::<ArgumentCase>(value).is_err());
    }

    #[test]
    fn schema_has_closed_objects_and_enum_status() {
        let schema = case_schema();
        assert_eq!(schema["additionalProperties"], false);
        assert!(
            schema["$defs"]["CaseStatus"]["enum"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("unsupported"))
        );
    }
}
