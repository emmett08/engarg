//! Independent protocol fixture: exact proof discharge plus actual scoped reads.
//!
//! This case is deliberately synthetic. Its sources describe a test protocol;
//! neither the proof nor the reads establish facts about a power supply.
use super::executable_case;
use eal_collector_contracts::{
    capability::{LogicalStrength, SupportType},
    execution::{EvaluationCriterion, RequiredCoverage},
};
use engarg::{
    direct_planning::DirectPlan,
    domain::{ArgumentIndex, ReasoningMethodBinding},
    engine::{Mode, RunReport},
    optimal_argument::assurance::{
        self, AssurancePlan, CheckProcedure, CheckRole, Discharge, VerificationCheck,
    },
    support_requirements::SupportedArgument,
};
use serde_json::json;
use std::{fs, path::Path};

pub const REDUNDANT_REQUIREMENT: &str = "support_G2";

/// A single declared route has an explicit redundant *formal* proof condition.
/// The operator's frozen complete premise mapping identifies its intended repair.
pub fn fixture(root: &Path) -> (RunReport, SupportedArgument, AssurancePlan) {
    let (request, mut argument, mut support) = executable_case::fixture();
    let Some(ReasoningMethodBinding::Bound(bound)) = &mut argument.warrants[0].reasoning_method
    else {
        unreachable!()
    };
    bound.conclusion.valid_from = "2026-01-01T00:00:00Z".into();
    bound.conclusion.valid_until = "2100-01-01T00:00:00Z".into();
    let intended_method = bound.clone();
    // The extra node is a mathematical condition on immutable programme data,
    // not an observation of the source supplying G1.
    argument.grounds.push(engarg::domain::Ground {
        id: "G2".into(),
        statement: "Redundant proof condition: the finite query with premises p and p implies q entails q under classical propositional semantics.".into(),
        ..argument.grounds[0].clone()
    });
    let mut need = argument
        .evidence_plan
        .iter()
        .find(|need| need.target_id == "G1")
        .unwrap()
        .clone();
    need.id = "N8".into();
    need.target_id = "G2".into();
    need.question = "Does the exact bounded propositional query entail q?".into();
    need.method = "Recompute deductive/1 for the independently fixed finite query.".into();
    need.measurement =
        "A checked finite mathematical consequence; no empirical measurement.".into();
    need.comparator = "The exact operator-owned query and result predicate.".into();
    need.acceptance_criterion =
        "deductive/1 returns entailed=true for premises p and p implies q and conclusion q.".into();
    // Retain the rejected stronger-target branch when this redundant premise
    // leaves the selected current argument. Canonical nodes must stay attached.
    need.enables_claim = "stronger_target".into();
    argument.evidence_plan.push(need.clone());
    argument.warrants[0].premise_ids.insert(0, "G2".into());
    let Some(ReasoningMethodBinding::Bound(bound)) = &mut argument.warrants[0].reasoning_method
    else {
        unreachable!()
    };
    bound.formal_premises[0].premise_id = "G2".into();
    bound.formal_premises[0].evidence_need_id = "N8".into();
    let mut method_requirement = bound.evidence_requirements[0].clone();
    method_requirement.evidence_need_id = "N8".into();
    method_requirement.requirement = "Check the separately declared redundant finite proof condition; do not treat it as an empirical observation.".into();
    bound.evidence_requirements.push(method_requirement);

    let mut requirement = support
        .requirements
        .iter()
        .find(|requirement| requirement.target_id == "G1")
        .unwrap()
        .clone();
    requirement.id = REDUNDANT_REQUIREMENT.into();
    requirement.target_id = "G2".into();
    requirement.evidence_need_ids = vec![need.id.clone()];
    requirement.original_acceptance_criteria = vec![need.acceptance_criterion.clone()];
    let contract = requirement.contract.as_mut().unwrap();
    contract.support_type = SupportType::ProofCheck;
    contract.logical_strength = LogicalStrength::CheckedProof;
    contract.acceptable_methods = vec!["deductive/1".into()];
    contract.quantities[0].name = "proof_result".into();
    contract.quantities[0].unit = "1".into();
    contract.time.max_age_seconds = None;
    contract.time.require_original_observation_time = false;
    contract.provenance.required_authorities = vec!["eal-native".into()];
    requirement.acceptance = EvaluationCriterion::Unavailable {
        reason: "A source read cannot establish this pure proof condition; the exact operator-bound native proof must discharge it.".into(),
    };
    requirement.acceptance_rationale = "This obligation concerns immutable finite programme data, with exact query identity and no claim of external premise truth.".into();
    requirement.gaps.clear();
    let mut evaluation = support
        .evaluations
        .iter()
        .find(|evaluation| evaluation.requirement_id == "requirement_G1")
        .unwrap()
        .clone();
    evaluation.id = "proof_G2".into();
    evaluation.requirement_id = requirement.id.clone();
    evaluation.inputs.clear();
    evaluation.criterion = requirement.acceptance.clone();
    evaluation.max_age_seconds = None;
    evaluation.required_coverage = RequiredCoverage::Unspecified;
    support.requirements.push(requirement.clone());
    support.evaluations.push(evaluation.clone());
    let mut unproved = argument.warrants[0].clone();
    unproved.id = "W2".into();
    unproved.conclusion_id = "stronger_target".into();
    unproved.premise_ids = vec!["G2".into()];
    unproved.rule = "A finite propositional proof alone does not justify the unmeasured physical deployment; this proposed extension remains unresolved.".into();
    unproved.reasoning_method = Some(serde_json::from_value(json!({
        "status":"unresolved","method":"deductive",
        "reasons":["The supplied finite proof does not establish the stronger physical-deployment target."],
        "evidence_need_ids":[]
    })).unwrap());
    argument.warrants.push(unproved);
    argument.compositions.push(serde_json::from_value(json!({
        "conclusion_id":"stronger_target","relationship":"jointly_required",
        "selected_warrant_ids":["W2"],
        "rationale":"Retain the explicitly unproved extension separately from the single current-claim route."
    })).unwrap());
    let mut unproved_requirement = requirement.clone();
    unproved_requirement.id = "unproved_physical_extension".into();
    unproved_requirement.target_id = "W2".into();
    unproved_requirement.evidence_need_ids.clear();
    unproved_requirement.original_acceptance_criteria.clear();
    unproved_requirement.contract = None;
    unproved_requirement.acceptance = EvaluationCriterion::Unavailable {
        reason: "The synthetic protocol supplies no physical-deployment evidence.".into(),
    };
    unproved_requirement.gaps =
        vec![serde_json::from_value(json!({
        "kind":"source_unavailable","description":"No physical deployment has been observed."
    })).unwrap()];
    evaluation.id = "unproved_physical_extension_check".into();
    evaluation.requirement_id = unproved_requirement.id.clone();
    evaluation.criterion = unproved_requirement.acceptance.clone();
    support.requirements.push(unproved_requirement);
    support.evaluations.push(evaluation);

    let mut proposal = SupportedArgument {
        argument: argument.clone(),
        support_elaboration: support.clone(),
    };
    proposal.argument.warrants[0].premise_ids = vec!["G1".into()];
    proposal.argument.warrants[0].reasoning_method =
        Some(ReasoningMethodBinding::Bound(intended_method.clone()));
    let original = RunReport {
        schema_version: "engarg.run.v3".into(),
        execution_mode: "offline_synthetic_fixture".into(),
        input_sha256: assurance::digest(&request).unwrap(),
        provider_config: None,
        mode: Mode::Single,
        run_config: None,
        request,
        argument,
        transcript: vec![],
        tooling: None,
        direct_bindings: Some(DirectPlan::unbound(&support)),
        support_elaboration: Some(support),
        design_review: None,
        argument_revisions: vec![],
        target_runtime: None,
        authoring_recovery: None,
        evidence_planning: None,
        replanning: None,
        support_recovery: None,
        limitations: vec!["Synthetic protocol fixture; not the archived power-supply argument and not evidence about any physical power supply.".into()],
    };
    engarg::report::validate(&original).unwrap();
    engarg::domain::validate_case(&original.request, &proposal.argument).unwrap();
    let errors = engarg::support_requirements::validate_elaboration(
        &proposal.argument,
        &proposal.support_elaboration,
    );
    assert!(errors.is_empty(), "{}", errors.join("; "));
    assert_eq!(
        ArgumentIndex::new(&original.argument)
            .selected_dependency("current_claim")
            .len(),
        5
    );
    assert_eq!(
        ArgumentIndex::new(&proposal.argument)
            .selected_dependency("current_claim")
            .len(),
        4
    );
    let original_selected =
        ArgumentIndex::new(&original.argument).selected_dependency("current_claim");
    let mut checks = original
        .support_elaboration
        .as_ref()
        .unwrap()
        .requirements
        .iter()
        .filter(|requirement| {
            requirement.id != REDUNDANT_REQUIREMENT
                && original_selected.contains(requirement.target_id.as_str())
        })
        .map(|requirement| VerificationCheck {
            id: format!("source_{}", requirement.id),
            role: if requirement.target_id == "A1" {
                CheckRole::Applicability
            } else {
                CheckRole::Premise
            },
            target_id: requirement.target_id.clone(),
            procedure: CheckProcedure::Requirement {
                requirement_id: requirement.id.clone(),
            },
            discharges: vec![],
        })
        .collect::<Vec<_>>();
    checks.push(VerificationCheck {
        id: "exact_finite_proof".into(),
        role: CheckRole::Formalisation,
        target_id: "current_claim".into(),
        procedure: CheckProcedure::Method {
            conclusion_id: "current_claim".into(),
            identifier: intended_method.method.identifier().into(),
            reference_query: intended_method.method.query.clone(),
            result: intended_method.conclusion.result.clone(),
        },
        discharges: vec![Discharge {
            requirement_id: requirement.id.clone(),
            requirement_sha256: assurance::digest(&requirement).unwrap(),
        }],
    });
    checks.push(VerificationCheck {
        id: "complete_intended_premise_mapping".into(),
        role: CheckRole::Formalisation,
        target_id: "current_claim".into(),
        procedure: CheckProcedure::Value {
            pointer: "/argument/warrants/0/reasoning_method/formal_premises".into(),
            expected: serde_json::to_value(&intended_method.formal_premises).unwrap(),
        },
        discharges: vec![],
    });
    let plan = AssurancePlan {
        schema: assurance::PLAN_SCHEMA.into(),
        checks,
    };
    plan.validate(&original).unwrap();
    write_sources(root);
    for (name, value) in [
        ("input.json", serde_json::to_value(&original).unwrap()),
        ("proposal.json", serde_json::to_value(&proposal).unwrap()),
        ("verification.json", serde_json::to_value(&plan).unwrap()),
    ] {
        fs::write(root.join(name), serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    }
    (original, proposal, plan)
}

fn write_sources(root: &Path) {
    fs::create_dir(root.join("tools")).unwrap();
    fs::write(root.join("tools/read.py"), r#"import datetime,json,sys
request=json.load(sys.stdin)
assert request['input']['fixture']=='frozen-fixture-v1'
operation=request['input']['operation']
assert operation in ('premise_source','applicability_source')
value={'measurement':{'count':3,'distinct':True}} if operation=='premise_source' else {'applies':True}
print(json.dumps({'value':value,'observed_at':datetime.datetime.now(datetime.timezone.utc).isoformat().replace('+00:00','Z')}))
"#).unwrap();
    fs::write(root.join("tools.toml"), "[tools.premise_source]\nkind='command'\nversion='1'\nargv=['python3','tools/read.py']\ninherit_env=['PATH']\n[tools.applicability_source]\nkind='command'\nversion='1'\nargv=['python3','tools/read.py']\ninherit_env=['PATH']\n").unwrap();
}
