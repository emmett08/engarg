//! Construct, investigate, challenge and repair bounded alternative arguments.
mod calls;
mod candidates;
mod prompts;
mod types;
pub use types::*;

use crate::{
    Error,
    engine::RunReport,
    provider::LanguageModel,
    support_requirements::{self, SupportedArgument},
};
use calls::{CallContext, Ledger};
use candidates::{apply_criticism, candidate, compare};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn digest(value: &impl serde::Serialize) -> Result<String, Error> {
    eal_collector_contracts::execution::canonical_digest(value).map_err(Error::InvalidInput)
}

fn config_valid(config: DevelopmentConfig) -> Result<(), Error> {
    if !(1..=8).contains(&config.rounds)
        || !(1..=64).contains(&config.max_calls)
        || !(1..=64).contains(&config.beam_width)
        || !(1..=16 * 1_048_576).contains(&config.max_input_bytes)
        || !(1..=16 * 1_048_576).contains(&config.max_response_bytes)
    {
        return Err(Error::InvalidInput("argument development requires rounds 1..8, calls/beam 1..64 and positive byte limits through 16 MiB".into()));
    }
    Ok(())
}

fn record_failure<O: Fn(&Value) -> Result<(), Error> + Sync>(
    failures: &mut Vec<DevelopmentFailure>,
    failure: DevelopmentFailure,
    observer: &O,
) -> Result<(), Error> {
    observer(&json!({"event":"development_failure","failure":failure}))?;
    failures.push(failure);
    Ok(())
}

/// Develop alternatives even when the original graph declares only one route.
///
/// The caller supplies a mandatory independent investigator for target fidelity,
/// premise/formalisation/applicability checks, revision and native execution.
/// A sceptic's unresolved critical findings additionally veto generated winners;
/// sceptic agreement never discharges those external checks. All proposal and
/// sceptic exchanges are observed before parsing and remain separate from the
/// original author's immutable transcript.
pub async fn develop<
    M: LanguageModel,
    I: CandidateInvestigator,
    O: Fn(&Value) -> Result<(), Error> + Sync,
>(
    model: &M,
    original: &RunReport,
    config: DevelopmentConfig,
    tool_context: &Value,
    investigator: &I,
    observer: &O,
) -> Result<DevelopmentResult, Error> {
    config_valid(config)?;
    crate::report::validate(original)?;
    let baseline = SupportedArgument {
        argument: original.argument.clone(),
        support_elaboration: original.support_elaboration.clone().ok_or_else(|| {
            Error::InvalidInput("argument development requires complete support_elaboration".into())
        })?,
    };
    observer(
        &json!({"event":"development_start","schema":"engarg.argument-development/1",
        "config":config,"original_report_sha256":digest(original)?}),
    )?;
    let baseline_outcome = investigator.assess(original, baseline, 0).await?;
    let mut candidates = vec![candidate(0, None, 0, baseline_outcome)];
    observer(&json!({"event":"candidate_assessed","candidate":candidates[0]}))?;
    if candidates[0].outcome.report.is_none() {
        // Source admission is needed to construct any safe subsequent prompt.
        return Err(Error::InvalidInput("baseline investigator did not return an admitted report; retained baseline outcome identifies the missing prerequisite".into()));
    }
    let mut frontier = vec![0usize];
    let mut ledger = Ledger {
        attempted: 0,
        calls: Vec::new(),
    };
    let mut failures = Vec::new();
    let mut rounds_completed = 0;
    let mut budget_exhausted = false;
    let mut pruned_states = 0usize;
    'rounds: for round in 1..=config.rounds {
        let mut next: BTreeSet<_> = frontier.iter().copied().collect();
        for parent_index in &frontier {
            let parent_index = *parent_index;
            if ledger.attempted == config.max_calls {
                budget_exhausted = true;
                break 'rounds;
            }
            let request = prompts::proposal(
                original,
                &candidates[parent_index],
                &candidates,
                tool_context,
                &failures,
                round,
            )?;
            let call_index = ledger.attempted;
            let completion = match calls::complete(
                model,
                request,
                &config,
                CallContext {
                    round,
                    parent_index,
                    phase: "proposal",
                },
                &mut ledger,
                observer,
            )
            .await?
            {
                Ok(completion) => completion,
                Err(failure) => {
                    record_failure(&mut failures, failure, observer)?;
                    continue;
                }
            };
            let proposal = match support_requirements::parse_supported_argument(
                &completion.content,
                config.max_response_bytes,
            ) {
                Ok(proposal) => proposal,
                Err(error) => {
                    record_failure(
                        &mut failures,
                        DevelopmentFailure {
                            round,
                            parent_index,
                            call_index: Some(call_index),
                            phase: "proposal_parse".into(),
                            reason: error.to_string(),
                        },
                        observer,
                    )?;
                    continue;
                }
            };
            let index = candidates.len();
            observer(
                &json!({"event":"candidate_proposed","index":index,"round":round,"parent_index":parent_index,"proposal":proposal}),
            )?;
            let parent = candidates[parent_index]
                .outcome
                .report
                .as_ref()
                .expect("frontier has admitted report");
            let repair_source = proposal.clone();
            let outcome = match investigator.assess(parent, proposal, index).await {
                Ok(outcome) => outcome,
                Err(error) => {
                    record_failure(
                        &mut failures,
                        DevelopmentFailure {
                            round,
                            parent_index,
                            call_index: Some(call_index),
                            phase: "investigation".into(),
                            reason: error.to_string(),
                        },
                        observer,
                    )?;
                    CandidateOutcome {
                        report: None,
                        assessment: super::Assessment {
                            supported: false,
                            status: "investigation_error".into(),
                            artifact: String::new(),
                            reasons: vec![error.to_string()],
                        },
                        verification: json!({"error":error.to_string()}),
                    }
                }
            };
            let mut proposed = candidate(index, Some(parent_index), round, outcome);
            if proposed.outcome.report.is_none() {
                // A rejected formulation is useful repair data, never an
                // admitted report or a source of execution/parent authority.
                proposed.rejected_proposal = Some(repair_source);
            }
            if proposed.outcome.report.is_some() {
                if ledger.attempted < config.max_calls {
                    let request = prompts::critic(original, &proposed)?;
                    let critic_index = ledger.attempted;
                    match calls::complete(
                        model,
                        request,
                        &config,
                        CallContext {
                            round,
                            parent_index,
                            phase: "sceptic",
                        },
                        &mut ledger,
                        observer,
                    )
                    .await?
                    {
                        Ok(completion) => match prompts::parse_criticism(
                            &completion.content,
                            config.max_response_bytes,
                        ) {
                            Ok(criticism) => {
                                for issue in &criticism.critical_issues {
                                    record_failure(
                                        &mut failures,
                                        DevelopmentFailure {
                                            round,
                                            parent_index,
                                            call_index: Some(critic_index),
                                            phase: "sceptic_critical".into(),
                                            reason: issue.clone(),
                                        },
                                        observer,
                                    )?;
                                }
                                let vetoed =
                                    apply_criticism(&mut proposed, criticism, &mut candidates);
                                if !vetoed.is_empty() {
                                    observer(&json!({"event":"semantic_veto",
                                        "proposal_sha256":proposed.proposal_sha256,
                                        "candidate_indices":vetoed,
                                        "pending_critical_issues":proposed.pending_critical_issues}))?;
                                }
                            }
                            Err(error) => record_failure(
                                &mut failures,
                                DevelopmentFailure {
                                    round,
                                    parent_index,
                                    call_index: Some(critic_index),
                                    phase: "sceptic_parse".into(),
                                    reason: error.to_string(),
                                },
                                observer,
                            )?,
                        },
                        Err(failure) => record_failure(&mut failures, failure, observer)?,
                    }
                } else {
                    budget_exhausted = true;
                    proposed.diagnostics.push("No model-call allowance remains for the separate sceptic; candidate is not eligible".into());
                }
                next.insert(index);
            }
            observer(&json!({"event":"candidate_assessed","candidate":proposed}))?;
            candidates.push(proposed);
        }
        frontier = next.into_iter().collect();
        frontier.sort_by(|left, right| compare(&candidates[*left], &candidates[*right]));
        if frontier.len() > config.beam_width {
            pruned_states += frontier.len() - config.beam_width;
            frontier.truncate(config.beam_width);
        }
        rounds_completed = round;
    }
    let best = candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| candidate.eligible)
        .min_by(|(_, left), (_, right)| compare(left, right))
        .map(|(index, _)| index);
    let result = DevelopmentResult {
        schema:"engarg.argument-development/1".into(),config,
        calls_attempted:ledger.attempted,calls:ledger.calls,rounds_completed,budget_exhausted,
        stop_reason:if budget_exhausted {"call_budget"} else {"round_limit"}.into(),
        pruned_states,best,candidates,failures,search_complete:false,
        limitations:vec![
            "The bounded model-generated family is not an exhaustive search of possible arguments; the result is a best checked candidate found.".into(),
            "An LLM sceptic can identify unresolved problems but its agreement is not a proof, observation or independent corroboration.".into(),
            "Mandatory target, premise, formalisation, applicability and execution checks are owned by the supplied investigator; unknown or failed obligations cannot be promoted by model assertions.".into(),
            "Different candidates may collect at different times. Original observations and review clocks are not refreshed by reasoning or reuse.".into(),
        ],
    };
    observer(
        &json!({"event":"development_complete","calls_attempted":result.calls_attempted,
        "best":result.best,"stop_reason":result.stop_reason,"candidate_count":result.candidates.len()}),
    )?;
    Ok(result)
}

#[cfg(test)]
mod tests;
