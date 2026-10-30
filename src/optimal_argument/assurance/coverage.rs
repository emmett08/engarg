//! Derive mandatory verification coverage from the relied-on argument graph.
use super::*;
use crate::domain::{ArgumentIndex, ArgumentNode, FormalInputCorrespondence, MethodClaimMode};
use eal_core::verification::{self, Coverage, Formula, Mode, RequiredObligation, Role, Status};
use std::collections::BTreeMap;

pub(super) fn validate_plan(plan: &AssurancePlan, original: &RunReport) -> Result<(), Error> {
    if plan.verification.len() > 128
        || (plan.schema == PLAN_SCHEMA && !plan.verification.is_empty())
    {
        return Err(invalid(
            "source verification requires assurance-plan/2 and at most 128 target contracts",
        ));
    }
    let index = ArgumentIndex::new(&original.argument);
    let mut targets = BTreeSet::new();
    let sources: BTreeSet<_> = original
        .support_elaboration
        .as_ref()
        .unwrap()
        .acquisitions
        .iter()
        .map(|a| a.id.as_str())
        .collect();
    for contract in &plan.verification {
        if contract.schema != verification::CONTRACT_SCHEMA
            || index.node(&contract.target_id).is_none()
            || !targets.insert(&contract.target_id)
        {
            return Err(invalid(
                "verification contracts need their supported schema and unique original targets",
            ));
        }
        for source in contract
            .atoms
            .iter()
            .map(|a| a.observation_id.as_str())
            .chain(
                contract
                    .obligations
                    .iter()
                    .map(|o| o.observation_id.as_str()),
            )
            .chain(
                contract
                    .applicability
                    .iter()
                    .map(|a| a.observation_id.as_str()),
            )
        {
            if !sources.contains(source) {
                return Err(invalid(format!(
                    "verification source {source} is not an original acquisition"
                )));
            }
        }
        let proposal = SupportedArgument {
            argument: original.argument.clone(),
            support_elaboration: original.support_elaboration.clone().unwrap(),
        };
        super::authority::validate(original, contract)?;
        // Validate bounded predicate syntax before any paid authoring or source
        // access. An empty context deliberately supplies no observation truth.
        verification::evaluate(
            &required(&proposal, &contract.target_id, &[]),
            contract,
            &verification::Context {
                observations: BTreeMap::new(),
                assessed_at: String::new(),
            },
            &eal_core::ExecutionLimits::default(),
        )
        .map_err(invalid)?;
    }
    Ok(())
}

fn mathematical(index: &ArgumentIndex<'_>, target: &str) -> bool {
    if let Some(warrant) = index.warrant(target) {
        return warrant
            .reasoning_method
            .as_ref()
            .and_then(|m| m.bound())
            .is_some_and(|b| b.claim_mode == MethodClaimMode::MathematicalConsequence);
    }
    index.composition_for(target).is_some_and(|c| {
        !c.selected_warrant_ids.is_empty()
            && c.selected_warrant_ids
                .iter()
                .all(|id| mathematical(index, id))
    })
}

fn required(proposal: &SupportedArgument, target: &str, discharged: &[String]) -> Coverage {
    let index = ArgumentIndex::new(&proposal.argument);
    let node = index.node(target).expect("selected graph node");
    let mode = if mathematical(&index, target) {
        Mode::Mathematical
    } else if matches!(node, ArgumentNode::Warrant(w) if w.reasoning_method.as_ref().and_then(|m|m.bound()).is_none())
    {
        Mode::Defeasible
    } else {
        Mode::Empirical
    };
    let role = match node {
        ArgumentNode::Warrant(_) => Role::Warrant,
        ArgumentNode::Assumption(_) | ArgumentNode::Defeater(_) => Role::Applicability,
        _ => Role::Premise,
    };
    let required_obligations = proposal
        .support_elaboration
        .requirements
        .iter()
        .filter(|r| r.target_id == target && !discharged.contains(&r.id))
        .map(|r| RequiredObligation {
            id: r.id.clone(),
            role,
        })
        .collect();
    let mut formulas = Vec::new();
    if mode != Mode::Mathematical
        && let Some(warrant) = index.warrant(target)
        && let Some(bound) = warrant
            .reasoning_method
            .as_ref()
            .and_then(|m| m.bound())
            .filter(|b| b.method.identifier() == "deductive/1")
    {
        if let Some(premises) = bound.method.query["premises"].as_array() {
            formulas.extend(premises.iter().enumerate().map(|(i, f)| Formula {
                id: format!("premise_{i}"),
                formula: f.clone(),
            }));
        }
        formulas.push(Formula {
            id: "conclusion".into(),
            formula: bound.method.query["conclusion"].clone(),
        });
    }
    Coverage {
        target_id: target.into(),
        mode,
        formulas,
        required_obligations,
    }
}

fn selected(proposal: &SupportedArgument) -> Vec<String> {
    let index = ArgumentIndex::new(&proposal.argument);
    let mut targets: Vec<_> = index
        .selected_dependency("current_claim")
        .into_iter()
        .map(str::to_owned)
        .collect();
    targets.sort();
    targets
}

fn composed_targets(proposal: &SupportedArgument, target: &str) -> Vec<String> {
    let index = ArgumentIndex::new(&proposal.argument);
    if index.claim(target).is_none() || index.composition_for(target).is_none() {
        return vec![];
    }
    let mut targets: Vec<_> = index
        .selected_dependency(target)
        .into_iter()
        .filter(|id| *id != target && index.claim(id).is_none())
        .map(str::to_owned)
        .collect();
    targets.sort();
    targets
}

fn result(target: &str, status: CheckStatus, reason: String, details: Value) -> CheckResult {
    CheckResult {
        id: format!("coverage:{target}"),
        role: CheckRole::Formalisation,
        target_id: target.into(),
        status,
        reasons: vec![reason],
        details,
    }
}

pub(super) fn prepare(
    proposal: &SupportedArgument,
    plan: &AssurancePlan,
    receipt: &mut AssuranceReceipt,
) -> Result<(), Error> {
    if plan.schema == PLAN_SCHEMA {
        // Old plans can still be replayed for diagnosis. They never certify the
        // semantic checks introduced in /2, even if all old checks pass.
        receipt.checks.push(result("legacy_plan",CheckStatus::Unknown,"Migrate assurance-plan/1 to /2: every selected target needs explicit source/formula and applicability coverage".into(),json!({"deferred_grounding":true,"legacy_missing_coverage":true})));
        return Ok(());
    }
    for target in selected(proposal) {
        let coverage = required(proposal, &target, &receipt.discharged_requirement_ids);
        let Some(contract) = plan.verification.iter().find(|c| c.target_id == target) else {
            let dependencies = composed_targets(proposal, &target);
            if coverage.required_obligations.is_empty() && !dependencies.is_empty() {
                receipt.checks.push(result(&target, CheckStatus::Unknown,
                    "Claim verification composes the selected warrants and their checked conditions; no duplicate observation is required".into(),
                    json!({"deferred_grounding":true,"required_coverage":coverage,"composed_targets":dependencies})));
                continue;
            }
            receipt.checks.push(result(
                &target,
                CheckStatus::Unknown,
                "Selected material target has no operator verification contract".into(),
                json!({"required_coverage":coverage}),
            ));
            continue;
        };
        let mode_matches = coverage.mode == contract.mode;
        let obligations_present = coverage.required_obligations.iter().all(|r| {
            contract
                .obligations
                .iter()
                .any(|o| o.id == r.id && o.role == r.role)
        });
        let native_covered = coverage.mode != Mode::Mathematical
            || {
                let index = ArgumentIndex::new(&proposal.argument);
                let conclusion = index
                    .warrant(&target)
                    .map_or(target.as_str(), |w| w.conclusion_id.as_str());
                plan.checks.iter().any(|c| matches!(&c.procedure,CheckProcedure::Method {conclusion_id,..} if conclusion_id == conclusion) && receipt.checks.iter().any(|r|r.id == c.id && r.status == CheckStatus::Pass))
            };
        let nonempty = coverage.mode == Mode::Mathematical
            || !coverage.required_obligations.is_empty()
            || !coverage.formulas.is_empty();
        // Other native methods already implement executable acquired-value
        // equality via IdentityValue. Their prose-only legacy mappings cannot
        // acquire a new semantic guarantee from unrelated passing predicates.
        let mappings_executable = ArgumentIndex::new(&proposal.argument)
            .warrant(&target)
            .and_then(|w| w.reasoning_method.as_ref())
            .and_then(|m| m.bound())
            .is_none_or(|b| {
                coverage.mode == Mode::Mathematical
                    || b.method.identifier() == "deductive/1"
                    || b.formal_premises.iter().all(|p| {
                        matches!(
                            p.correspondence,
                            FormalInputCorrespondence::IdentityValue { .. }
                        )
                    })
            });
        let target_preserved = super::authority::preserves_target(proposal, contract);
        let can_execute = target_preserved
            && mode_matches
            && obligations_present
            && native_covered
            && nonempty
            && mappings_executable;
        receipt.checks.push(result(&target,if can_execute {CheckStatus::Unknown}else{CheckStatus::Fail},if can_execute {"Source predicates and applicability await retained native observations".into()}else{"Verification mode, obligation coverage, executable input correspondence or mathematical proof does not cover the selected target".into()},json!({"required_coverage":coverage,"deferred_grounding":can_execute,"mode_matches":mode_matches,"obligations_present":obligations_present,"native_proof_covered":native_covered,"mappings_executable":mappings_executable,"target_preserved":target_preserved})));
    }
    Ok(())
}

pub(super) fn finish(
    proposal: &SupportedArgument,
    execution: &Value,
    plan: &AssurancePlan,
    receipt: &mut AssuranceReceipt,
) -> Result<(), Error> {
    if plan.schema != GROUNDED_PLAN_SCHEMA {
        return Ok(());
    }
    let context = super::observations::context(proposal, execution);
    for target in selected(proposal) {
        let required = required(proposal, &target, &receipt.discharged_requirement_ids);
        let Some(contract) = plan.verification.iter().find(|c| c.target_id == target) else {
            continue;
        };
        let Some(slot) = receipt
            .checks
            .iter_mut()
            .find(|r| r.id == format!("coverage:{target}"))
        else {
            return Err(invalid("receipt omits required target coverage"));
        };
        if slot.details["deferred_grounding"] != true {
            continue;
        }
        match verification::evaluate(
            &required,
            contract,
            &context,
            &eal_core::ExecutionLimits::default(),
        ) {
            Ok(report) => {
                slot.status = match report.status {
                    Status::Pass => CheckStatus::Pass,
                    Status::Fail => CheckStatus::Fail,
                    Status::Unknown | Status::NotRequired => CheckStatus::Unknown,
                };
                slot.reasons = vec!["Evaluated all graph-derived source, formula and applicability obligations against retained native acquisition values".into()];
                slot.details = json!({"required_coverage":required,"verification":report});
                receipt.verification_reports.push(report);
            }
            Err(reason) => {
                slot.status = CheckStatus::Error;
                slot.reasons = vec![reason];
                slot.details = json!({"required_coverage":required});
            }
        }
    }
    for target in selected(proposal) {
        let Some(position) = receipt
            .checks
            .iter()
            .position(|r| r.id == format!("coverage:{target}"))
        else {
            continue;
        };
        let Some(dependencies) = receipt.checks[position].details.get("composed_targets") else {
            continue;
        };
        let dependencies: Vec<String> = serde_json::from_value(dependencies.clone())?;
        let coverage = required(proposal, &target, &receipt.discharged_requirement_ids);
        match verification::compose(
            &target,
            coverage.mode,
            &dependencies,
            &receipt.verification_reports,
            &eal_core::ExecutionLimits::default(),
        ) {
            Ok(report) => {
                receipt.checks[position].status = match report.status {
                    Status::Pass => CheckStatus::Pass,
                    Status::Fail => CheckStatus::Fail,
                    _ => CheckStatus::Unknown,
                };
                receipt.checks[position].details["verification"] = serde_json::to_value(&report)?;
                receipt.verification_reports.push(report);
            }
            Err(reason) => {
                receipt.checks[position].status = CheckStatus::Error;
                receipt.checks[position].reasons = vec![reason];
            }
        }
    }
    Ok(())
}
