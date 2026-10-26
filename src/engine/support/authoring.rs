//! Resume final authoring validation by reconstructing immutable dissent history.
use super::*;
use crate::authoring_checkpoint::Checkpoint;

pub struct ResumeConfig {
    pub additional_repairs: u8,
    pub max_input_bytes: usize,
    pub max_response_bytes: usize,
    pub accept_protocol_migration: bool,
}

pub async fn resume<M: LanguageModel, O, R: RouteResolver>(
    model: &M,
    saved: &Checkpoint,
    options: ResumeConfig,
    observer: &O,
    resolver: &R,
) -> Result<SupportRunReport<R::Plan>, Error>
where
    O: Fn(&CallRecord) -> Result<(), Error> + Send + Sync,
{
    saved.validate()?;
    let ResumeConfig {
        additional_repairs,
        max_input_bytes,
        max_response_bytes,
        accept_protocol_migration,
    } = options;
    if saved.protocol_identity != crate::authoring_checkpoint::protocol_identity()
        && !accept_protocol_migration
    {
        return Err(Error::InvalidInput(
            "authoring resume requires explicit protocol migration acknowledgement".into(),
        ));
    }
    if saved
        .transcript
        .len()
        .saturating_add(usize::from(additional_repairs))
        > crate::authoring_checkpoint::MAX_CALLS
    {
        return Err(Error::Budget(
            "authoring repair allowance exceeds bounded history".into(),
        ));
    }
    if !(1..=3).contains(&additional_repairs) {
        return Err(Error::InvalidInput(
            "authoring resume requires 1–3 separately budgeted repair calls".into(),
        ));
    }
    if saved.inspection()["resumable"] != true {
        return Err(Error::InvalidInput(
            "checkpoint lacks completed authoring stages or contains omitted content".into(),
        ));
    }
    let mut transcript = saved.ordered_transcript()?;
    let ReplayedHistory {
        mut retained,
        draft,
    } = replay_history(&transcript, saved.config.mode)?;
    let config = RunConfig {
        max_calls: u32::from(additional_repairs),
        max_repairs: additional_repairs,
        max_input_bytes,
        max_response_bytes,
        ..saved.config.clone()
    };
    let candidate = validate_or_repair(
        model,
        &saved.request,
        &config,
        observer,
        &mut transcript,
        &mut retained,
        draft.as_ref(),
    )
    .await?;
    let design_review = support_requirements::review_acquisition_design(
        &candidate.support_elaboration,
        &transcript,
        crate::authoring_checkpoint::MAX_BYTES,
    )?;
    let routes = resolver
        .resolve(&candidate.argument, &candidate.support_elaboration)
        .await?;
    let mut run_limitations = limitations(saved.config.mode);
    run_limitations.push("Initial authoring resumed with a separate repair allowance. Previous completed calls were replayed locally without provider requests. Digests establish consistency, not authenticity of imported local history.".into());
    if let Some(warning) = design_review.warning() {
        run_limitations.push(warning);
    }
    Ok(SupportRunReport {
        report: RunReport {
            schema_version: "engarg.run.v2".into(),
            execution_mode: "provider_calls".into(),
            input_sha256: sha256_hex(serde_json::to_vec(&saved.request)?),
            provider_config: None,
            mode: saved.config.mode,
            run_config: Some(saved.config.clone()),
            request: saved.request.clone(),
            argument: candidate.argument,
            transcript,
            tooling: None,
            limitations: run_limitations,
        },
        evidence_planning: SupportPlanning {
            schema: "engarg.support-planning/1".into(),
            elaboration: candidate.support_elaboration,
            routes,
            design_review: Some(design_review),
        },
    })
}

/// Reconstruct first-attachment identities from original workflow order. This
/// reads recorded completions and does not trust a stored dissent ledger.
pub(crate) struct ReplayedHistory {
    pub(crate) retained: Vec<RetainedObjection>,
    pub(crate) draft: Option<SupportedArgument>,
}
pub(crate) fn replay_history(
    transcript: &[CallRecord],
    mode: Mode,
) -> Result<ReplayedHistory, Error> {
    if transcript.is_empty() || (mode == Mode::Hybrid && transcript.len() < 6) {
        return Err(Error::InvalidInput(
            "retained history lacks complete authoring stages".into(),
        ));
    }
    let mut retained = vec![];
    let draft = if mode == Mode::Hybrid {
        for (record, prefix) in transcript[..3]
            .iter()
            .zip(["constructor", "sceptic", "auditor"])
        {
            context::assessment(
                record.role,
                prefix,
                parse_assessment(record)?,
                &mut retained,
            );
        }
        let draft = support_requirements::parse_supported_argument(
            &transcript[3].response.content,
            crate::authoring_checkpoint::MAX_BYTES,
        )?;
        output_validation::retain_objections(&draft.argument, transcript[3].role, &mut retained);
        context::assessment(
            Role::FinalCritic,
            "critic",
            parse_assessment(&transcript[4])?,
            &mut retained,
        );
        Some(draft)
    } else {
        None
    };
    let previous_candidates = if mode == Mode::Hybrid { 5 } else { 0 };
    for record in transcript[previous_candidates..].iter() {
        if let Ok(candidate) = support_requirements::parse_supported_argument(
            &record.response.content,
            crate::authoring_checkpoint::MAX_BYTES,
        ) {
            output_validation::retain_objections(&candidate.argument, record.role, &mut retained);
        }
    }
    Ok(ReplayedHistory { retained, draft })
}
