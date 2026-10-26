//! Catalogue-independent case construction followed by qualified route planning.
pub mod authoring;
pub mod recovery;
use super::{
    CallRecord, Mode, RunConfig, RunReport, assessment_schema, assessment_warnings, context,
    debate, execute, limitations, parse_assessment, prompt, sha256_hex,
};
use crate::{
    Error,
    domain::{ArgumentCase, Request, validate_request},
    output_validation::{self, RetainedObjection},
    provider::{CompletionRequest, LanguageModel, Role},
    support_requirements::{self, SupportElaboration, SupportedArgument},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future};

/// Injects capability discovery and compilation without another model call.
/// The resolver must preserve obligations and report unavailable routes explicitly.
pub trait RouteResolver: Send + Sync {
    type Plan: Serialize + Clone + Send;
    fn resolve<'a>(
        &'a self,
        case: &'a ArgumentCase,
        elaboration: &'a SupportElaboration,
    ) -> impl Future<Output = Result<Self::Plan, Error>> + Send + 'a;

    /// A critic-facing projection. Resolvers retaining operator bindings or
    /// credentials must override this to expose only reviewable route metadata.
    fn preview(&self, plan: &Self::Plan) -> Result<Value, Error> {
        serde_json::to_value(plan).map_err(Error::from)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupportPlanning<P> {
    pub schema: String,
    pub elaboration: SupportElaboration,
    pub routes: P,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design_review: Option<support_requirements::AcquisitionDesignReview>,
}

/// The legacy report remains unchanged; graph planning is an explicit extension.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SupportRunReport<P> {
    #[serde(flatten)]
    pub report: RunReport,
    pub evidence_planning: SupportPlanning<P>,
}

fn support_prompt(
    role: Role,
    mut data: Value,
    config: &RunConfig,
) -> Result<CompletionRequest, Error> {
    let scope_identities = method_scope_identities(&data);
    if !scope_identities.is_empty() {
        data["method_scope_identities"] = serde_json::to_value(scope_identities)?;
    }
    let produces_case = matches!(role, Role::Single | Role::Synthesiser | Role::Adjudicator);
    if produces_case {
        data["support_elaboration_schema"] = support_requirements::elaboration_schema();
    }
    let schema = if produces_case {
        support_requirements::envelope_schema()
    } else {
        assessment_schema()
    };
    let mut request = prompt(role, data, schema, config)?;
    if produces_case {
        request.system.push_str("\n\n");
        request
            .system
            .push_str(support_requirements::EXTRACTION_INSTRUCTIONS);
    }
    if matches!(
        role,
        Role::Constructor | Role::Sceptic | Role::EvidenceAuditor
    ) {
        request.system.push_str("\n\n");
        request
            .system
            .push_str(support_requirements::ASSESSMENT_INSTRUCTIONS);
    }
    if role == Role::FinalCritic {
        request.system.push_str("\n\n");
        request
            .system
            .push_str(support_requirements::CRITIC_INSTRUCTIONS);
    }
    Ok(request)
}

/// A model can copy an exact host-computed identity for a retained scope. An
/// altered scope needs a newly computed identity, never a guessed digest.
fn method_scope_identities(data: &Value) -> BTreeMap<String, Value> {
    let mut identities = BTreeMap::new();
    for argument in [&data["draft"], &data["candidate_json"]["argument"]] {
        for warrant in argument["warrants"].as_array().into_iter().flatten() {
            let Some(id) = warrant["id"].as_str() else {
                continue;
            };
            let Ok(scope) = serde_json::from_value::<crate::domain::MethodScope>(
                warrant["reasoning_method"]["conclusion"]["scope"].clone(),
            ) else {
                continue;
            };
            if let Ok(identity) = scope.identity() {
                identities.insert(
                    id.into(),
                    json!({"scope":scope,"eal_scope_identity":identity}),
                );
            }
        }
    }
    identities
}

fn validation_errors(
    request: &Request,
    candidate: &SupportedArgument,
    retained: &[RetainedObjection],
    config: &RunConfig,
    previous: Option<&SupportedArgument>,
) -> Vec<String> {
    let mut errors = output_validation::errors(
        request,
        &candidate.argument,
        retained,
        config.max_argument_depth,
    );
    if candidate.support_elaboration.schema != support_requirements::SUPPORT_ELABORATION_SCHEMA {
        errors.push("new support extraction must use engarg.support-elaboration/2 with explicit alternative support; legacy /1 is import-only".into());
    }
    errors.extend(match previous {
        Some(previous) => support_requirements::validate_revision(
            &previous.argument,
            &previous.support_elaboration,
            &candidate.argument,
            &candidate.support_elaboration,
        ),
        None => {
            let mut errors = support_requirements::validate_elaboration(
                &candidate.argument,
                &candidate.support_elaboration,
            );
            if !candidate.support_elaboration.revisions.is_empty() {
                errors.push(
                    "initial support extraction cannot contain unverifiable prior revisions".into(),
                );
            }
            if !candidate.support_elaboration.acquisition_revisions.is_empty() {
                errors.push("initial support extraction cannot contain unverifiable prior acquisition revisions".into());
            }
            errors
        }
    });
    errors.extend(support_requirements::validate_freshness(
        &candidate.support_elaboration,
    ));
    errors
}

fn step_identities(elaboration: &SupportElaboration) -> Result<BTreeMap<String, String>, Error> {
    elaboration
        .acquisitions
        .iter()
        .map(|step| Ok((step.id.clone(), support_requirements::step_identity(step)?)))
        .chain(
            elaboration
                .derivations
                .iter()
                .map(|step| Ok((step.id.clone(), support_requirements::step_identity(step)?))),
        )
        .collect()
}

fn requirement_identities(
    elaboration: &SupportElaboration,
) -> Result<BTreeMap<String, String>, Error> {
    elaboration
        .requirements
        .iter()
        .map(|requirement| {
            eal_collector_contracts::capability::canonical_identity(requirement)
                .map(|digest| (requirement.id.clone(), digest))
                .map_err(Error::InvalidOutput)
        })
        .collect()
}

/// Hybrid assessment with optional routed debate. Capability matching happens after the
/// synthesiser has extracted source obligations and again after adjudication.
pub async fn run_with_support<M: LanguageModel, O, R: RouteResolver>(
    model: &M,
    request: &Request,
    config: &RunConfig,
    observer: &O,
    resolver: &R,
) -> Result<SupportRunReport<R::Plan>, Error>
where
    O: Fn(&CallRecord) -> Result<(), Error> + Send + Sync,
{
    validate_request(request).map_err(|errors| Error::InvalidInput(errors.join("; ")))?;
    if config.collector_catalogue.is_some() {
        return Err(Error::InvalidInput("support graph planning uses the qualified route resolver; collector_catalogue belongs to flat planning".into()));
    }
    config.validate()?;
    let (mut transcript, mut retained, draft) = match config.mode {
        Mode::Single => {
            let call = execute(
                model,
                support_prompt(Role::Single, json!({"request":request}), config)?,
                config,
                observer,
            )
            .await?;
            (vec![call], Vec::new(), None)
        }
        Mode::Hybrid => hybrid(model, request, config, observer, resolver).await?,
    };
    let candidate = validate_or_repair(
        model,
        request,
        config,
        observer,
        &mut transcript,
        &mut retained,
        draft.as_ref(),
    )
    .await?;
    let design_review = support_requirements::review_acquisition_design(
        &candidate.support_elaboration,
        &transcript,
        config.max_response_bytes,
    )?;
    // A revised case may change IDs, scope or dependencies: never reuse the draft graph.
    // Check retained design provenance before any resolver performs discovery IO.
    let routes = resolver
        .resolve(&candidate.argument, &candidate.support_elaboration)
        .await?;
    let mut run_limitations = limitations(config.mode);
    if let Some(warning) = design_review.warning() {
        run_limitations.push(warning);
    }
    run_limitations.extend(
        transcript
            .iter()
            .flat_map(assessment_warnings)
            .map(|warning| warning.message()),
    );
    run_limitations.push("Evidence requirements were extracted independently of capability discovery. Qualified route previews describe acquisition possibilities; they are not acquired evidence or verified support.".into());
    Ok(SupportRunReport {
        report: RunReport {
            schema_version: "engarg.run.v2".into(),
            execution_mode: "provider_calls".into(),
            input_sha256: sha256_hex(serde_json::to_vec(request)?),
            provider_config: None,
            mode: config.mode,
            run_config: Some(config.clone()),
            request: request.clone(),
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

async fn hybrid<M: LanguageModel, O, R: RouteResolver>(
    model: &M,
    request: &Request,
    config: &RunConfig,
    observer: &O,
    resolver: &R,
) -> Result<
    (
        Vec<CallRecord>,
        Vec<RetainedObjection>,
        Option<SupportedArgument>,
    ),
    Error,
>
where
    O: Fn(&CallRecord) -> Result<(), Error> + Send + Sync,
{
    let debate::Assessments {
        transcript: initial,
        mut retained,
        independent: assessments,
        discussion,
    } = debate::assess(model, request, config, observer, |role, data| {
        support_prompt(role, data, config)
    })
    .await?;
    let synthesis = execute(model, support_prompt(Role::Synthesiser, debate::with_discussion(json!({
        "request":request,"independent_assessments":assessments,"retained_objections":retained,
    }), &discussion), config)?, config, observer).await?;
    let draft = support_requirements::parse_supported_argument(
        &synthesis.response.content,
        config.max_response_bytes,
    )?;
    let draft_errors = validation_errors(request, &draft, &retained, config, None);
    output_validation::retain_objections(&draft.argument, synthesis.role, &mut retained);
    let draft_view = context::argument(request, &draft.argument, &retained)?;
    // Invalid obligations remain visible to the critic but never enter matching.
    let route_preview = if draft_errors.is_empty() {
        resolver.preview(
            &resolver
                .resolve(&draft.argument, &draft.support_elaboration)
                .await?,
        )?
    } else {
        json!({"status":"invalid_requirements","validation_errors":draft_errors})
    };
    let critique = execute(model, support_prompt(Role::FinalCritic, debate::with_discussion(json!({
        "request":request,"independent_assessments":assessments,"draft":draft_view,
        "support_elaboration":draft.support_elaboration,"route_preview":route_preview,
        "route_preview_limitations":"Only qualified matching and configuration checks are represented. The routes have not executed and cannot establish truth. Coverage, availability, historical access and independent review remain explicit obligations.",
        "retained_objections":retained,"draft_validation_errors":draft_errors,
    }), &discussion), config)?, config, observer).await?;
    let final_critique = context::assessment(
        Role::FinalCritic,
        "critic",
        parse_assessment(&critique)?,
        &mut retained,
    );
    let adjudication = execute(
        model,
        support_prompt(
            Role::Adjudicator,
            debate::with_discussion(
                json!({
                    "request":request,"independent_assessments":assessments,"draft":draft_view,
                    "draft_support_elaboration":draft.support_elaboration,
                    "draft_requirement_sha256":requirement_identities(&draft.support_elaboration)?,
                    "draft_step_sha256":step_identities(&draft.support_elaboration)?,
                    "draft_validation_errors":draft_errors,"retained_objections":retained,
                    "final_critique":final_critique["assessment"],
                }),
                &discussion,
            ),
            config,
        )?,
        config,
        observer,
    )
    .await?;
    let mut transcript = initial;
    transcript.extend([synthesis, critique, adjudication]);
    Ok((transcript, retained, Some(draft)))
}

async fn validate_or_repair<M: LanguageModel, O>(
    model: &M,
    request: &Request,
    config: &RunConfig,
    observer: &O,
    transcript: &mut Vec<CallRecord>,
    retained: &mut Vec<RetainedObjection>,
    draft: Option<&SupportedArgument>,
) -> Result<SupportedArgument, Error>
where
    O: Fn(&CallRecord) -> Result<(), Error> + Send + Sync,
{
    let mut repair_scope = None;
    for attempt in 0..=config.max_repairs {
        let mut scope_feedback = Value::Null;
        let record = transcript.last().expect("workflow has a final call");
        // The initial candidate has already passed its producing call's byte
        // admission. A resumed history may use a larger original bound than
        // the allowance for new calls. Later attempts use the current bound.
        let candidate_limit = config.max_response_bytes.max(record.response_content_bytes);
        let (errors, candidate_json) = match support_requirements::parse_supported_argument(
            &record.response.content,
            candidate_limit,
        ) {
            Ok(mut candidate) => {
                let mut errors = validation_errors(request, &candidate, retained, config, draft);
                if errors.is_empty()
                    && let Err(error) = support_requirements::review_acquisition_design(
                        &candidate.support_elaboration,
                        transcript,
                        transcript
                            .iter()
                            .map(|call| call.response_content_bytes)
                            .max()
                            .unwrap_or(config.max_response_bytes)
                            .max(config.max_response_bytes),
                    )
                {
                    errors.push(error.to_string());
                }
                scope_feedback = super::repair_scope::constrain(
                    &mut repair_scope,
                    &candidate.argument,
                    retained,
                    &mut errors,
                    serde_json::to_value(&candidate.support_elaboration)?,
                )?;
                if errors.is_empty() {
                    output_validation::retain_objections(
                        &candidate.argument,
                        record.role,
                        retained,
                    );
                    output_validation::install_history(&mut candidate.argument, retained);
                    return Ok(candidate);
                }
                output_validation::retain_objections(&candidate.argument, record.role, retained);
                (
                    errors,
                    json!({
                        "argument":context::argument(request, &candidate.argument, retained)?,
                        "support_elaboration":candidate.support_elaboration,
                    }),
                )
            }
            Err(error) => (vec![error.to_string()], json!(record.response.content)),
        };
        if attempt == config.max_repairs {
            return Err(Error::InvalidOutput(format!(
                "{}; validation failed after {attempt} repair calls (max-repairs {})",
                errors.join("; "),
                config.max_repairs,
            )));
        }
        let data = json!({
            "request":request,"candidate_json":candidate_json,"validation_errors":errors,
            "retained_objections":retained,"repair_attempt":attempt+1,"repair_limit":config.max_repairs,
            "original_support_elaboration":draft.map(|draft|&draft.support_elaboration),
            "original_requirement_sha256":draft.map(|draft|requirement_identities(&draft.support_elaboration)).transpose()?,
            "original_step_sha256":draft.map(|draft|step_identities(&draft.support_elaboration)).transpose()?,
            "repair_scope":scope_feedback,
        });
        let repair = support_prompt(record.role, data, config)?;
        transcript.push(execute(model, repair, config, observer).await?);
    }
    unreachable!("bounded validation returns or exhausts repairs")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{provider::Completion, support_requirements::tests::pending};
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn scope_identities_are_computed_from_the_retained_structure_for_model_copying() {
        let scope = json!({"kind":"catalogue_snapshot",
            "selectors":[{"name":"snapshot","value":"fixture-v1"}],
            "identity_dimensions":["snapshot"]});
        let argument = json!({"warrants":[{"id":"W1",
            "reasoning_method":{"status":"bound","conclusion":{"scope":scope}}}]});
        let draft = method_scope_identities(&json!({"draft":argument}));
        let candidate = method_scope_identities(&json!({"candidate_json":{"argument":argument}}));
        assert_eq!(draft, candidate);
        let typed: crate::domain::MethodScope = serde_json::from_value(scope).unwrap();
        assert_eq!(draft["W1"]["eal_scope_identity"], typed.identity().unwrap());
        assert!(
            method_scope_identities(&json!({"request":{"claim":"No retained scopes"}})).is_empty()
        );
    }

    fn case() -> ArgumentCase {
        crate::live_output::case(include_str!("../../examples/claim-only-case.json"))
    }

    struct Model {
        calls: Mutex<Vec<CompletionRequest>>,
        omitted_support: bool,
        repair: bool,
        critic_objection: bool,
        collapse_design: bool,
    }
    impl LanguageModel for Model {
        async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
            let data: Value = serde_json::from_str(&request.user).unwrap();
            let role = request.role;
            let repaired = data.get("repair_attempt").is_some();
            self.calls.lock().unwrap().push(request);
            let content = if matches!(role, Role::Single | Role::Synthesiser | Role::Adjudicator) {
                let argument = case();
                let mut elaboration = pending(&argument);
                if self.collapse_design && role == Role::Synthesiser {
                    elaboration = source_design_fixture();
                }
                if self.omitted_support && (!self.repair || !repaired) {
                    let removed = elaboration.requirements.remove(0);
                    elaboration
                        .evaluations
                        .retain(|evaluation| evaluation.requirement_id != removed.id);
                }
                json!({"argument":argument,"support_elaboration_json":serde_json::to_string(&elaboration)?}).to_string()
            } else {
                json!({"findings":[],"evidence_gaps":[],"objections":if self.critic_objection && role==Role::FinalCritic {vec!["The proposed source omits a comparator."]} else {vec![]}}).to_string()
            };
            Ok(Completion {
                content,
                model: "support-test-fixture".into(),
                input_tokens: None,
                output_tokens: None,
                request_id: None,
                service_tier: None,
                reasoning_effort: None,
            })
        }
    }
    #[derive(Default)]
    struct Resolver {
        calls: AtomicUsize,
    }
    impl RouteResolver for Resolver {
        type Plan = Value;
        async fn resolve(
            &self,
            case: &ArgumentCase,
            elaboration: &SupportElaboration,
        ) -> Result<Value, Error> {
            assert!(support_requirements::validate_elaboration(case, elaboration).is_empty());
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(
                json!({"status":"pending_configuration","operator_bindings":{"private":"DO_NOT_SEND_OPERATOR_BINDINGS"},"qualified_capabilities":[{"id":"operator-approved-source","limitations":["Historical completeness remains unverified."]}]}),
            )
        }
        fn preview(&self, plan: &Self::Plan) -> Result<Value, Error> {
            let mut preview = plan.clone();
            preview.as_object_mut().unwrap().remove("operator_bindings");
            Ok(preview)
        }
    }
    fn mock() -> Model {
        Model {
            calls: Mutex::new(vec![]),
            omitted_support: false,
            repair: false,
            critic_objection: false,
            collapse_design: false,
        }
    }
    fn request() -> Request {
        serde_json::from_str(include_str!("../../examples/claim-only-request.json")).unwrap()
    }

    fn source_design_fixture() -> SupportElaboration {
        let mut elaboration = pending(&case());
        let mut source = crate::support_requirements::tests::contract(
            "source_retrieval",
            "existential_witness",
            "fixture.read/1",
            "1",
        );
        source.schema = "engarg.support-contract/1".into();
        source.acceptable_methods.clear();
        elaboration.acquisitions = (1..=5)
            .map(|index| {
                serde_json::from_value(json!({
                    "id":format!("source-{index}"),"contract":source,"arguments":{},
                    "bindings":[],"for_each":null,"operation_spec":null,
                }))
                .unwrap()
            })
            .collect();
        elaboration.derivations = (1..=2).map(|index| serde_json::from_value(json!({
            "id":format!("projection-{index}"),"inputs":[{"step_id":format!("source-{index}"),"pointer":""}],
            "operation":{"kind":"project","fields":{"result":"/result"}},
        })).unwrap()).collect();
        elaboration
    }

    #[tokio::test]
    async fn structurally_valid_graph_collapse_retains_proposals_and_reports_incomplete_design() {
        let mut model = mock();
        model.collapse_design = true;
        let resolver = Resolver::default();
        let report = run_with_support(
            &model,
            &request(),
            &RunConfig::default(),
            &|_| Ok(()),
            &resolver,
        )
        .await
        .unwrap();
        assert_eq!(
            report.report.transcript.len(),
            6,
            "retention does not spend another model call"
        );
        assert_eq!(
            resolver.calls.load(Ordering::SeqCst),
            1,
            "invalid draft never reaches route matching"
        );
        assert!(report.evidence_planning.elaboration.acquisitions.is_empty());
        let review = report.evidence_planning.design_review.as_ref().unwrap();
        assert_eq!(
            review.status,
            support_requirements::AcquisitionDesignStatus::PendingRepair
        );
        assert_eq!(review.pending_designs[0].missing_acquisition_ids.len(), 5);
        assert_eq!(review.pending_designs[0].missing_derivation_ids.len(), 2);
        assert!(
            report
                .report
                .limitations
                .iter()
                .any(|line| line.contains("Acquisition planning remains incomplete"))
        );
        assert!(!review.pending_designs[0].validation_errors.is_empty());
    }

    #[tokio::test]
    async fn six_calls_extract_without_catalogues_then_critic_sees_routes_and_final_recompiles() {
        let model = mock();
        let resolver = Resolver::default();
        let report = run_with_support(
            &model,
            &request(),
            &RunConfig::default(),
            &|_| Ok(()),
            &resolver,
        )
        .await
        .unwrap();
        assert_eq!(report.report.transcript.len(), 6);
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 2);
        assert_eq!(report.evidence_planning.schema, "engarg.support-planning/1");
        assert!(
            report
                .evidence_planning
                .routes
                .get("operator_bindings")
                .is_some()
        );
        let calls = model.calls.lock().unwrap();
        for call in calls.iter().filter(|call| {
            matches!(
                call.role,
                Role::Constructor | Role::Sceptic | Role::EvidenceAuditor | Role::FinalCritic
            )
        }) {
            assert_eq!(call.schema, assessment_schema(), "{}", call.role);
        }
        for call in calls.iter().filter(|call| call.role != Role::FinalCritic) {
            let data: Value = serde_json::from_str(&call.user).unwrap();
            for forbidden in [
                "acquisition_tools",
                "qualified_capabilities",
                "route_preview",
                "collector_catalogue",
            ] {
                assert!(
                    data.get(forbidden).is_none(),
                    "{} received {forbidden}",
                    call.role.as_str()
                );
            }
            assert!(!call.user.contains("operator-approved-source"));
            if matches!(
                call.role,
                Role::Constructor | Role::Sceptic | Role::EvidenceAuditor
            ) {
                assert!(call.system.contains("Return only the supplied findings, objections and evidence_gaps assessment schema"));
                assert!(
                    !call
                        .system
                        .contains("Return the supplied argument plus support_elaboration_json")
                );
            }
        }
        let critic = calls
            .iter()
            .find(|call| call.role == Role::FinalCritic)
            .unwrap();
        assert!(!critic.user.contains("DO_NOT_SEND_OPERATOR_BINDINGS"));
        let data: Value = serde_json::from_str(&critic.user).unwrap();
        assert_eq!(data["route_preview"]["status"], "pending_configuration");
        assert!(
            data["route_preview_limitations"]
                .as_str()
                .unwrap()
                .contains("not executed")
        );
        assert!(critic.system.contains("partial coverage"));
        let adjudicator = calls
            .iter()
            .find(|call| call.role == Role::Adjudicator)
            .unwrap();
        assert!(
            adjudicator
                .system
                .contains("Capability availability cannot justify weakening")
        );
    }

    #[tokio::test]
    async fn missing_support_never_reaches_matching_and_uses_bounded_repairs() {
        let mut model = mock();
        model.omitted_support = true;
        model.repair = true;
        let resolver = Resolver::default();
        let report = run_with_support(
            &model,
            &request(),
            &RunConfig {
                max_calls: 7,
                max_repairs: 1,
                ..RunConfig::default()
            },
            &|_| Ok(()),
            &resolver,
        )
        .await
        .unwrap();
        assert_eq!(report.report.transcript.len(), 7);
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 1);
        let data: Value = serde_json::from_str(&report.report.transcript[4].prompt.user).unwrap();
        assert_eq!(data["route_preview"]["status"], "invalid_requirements");
        assert!(
            data["draft_validation_errors"]
                .as_array()
                .unwrap()
                .iter()
                .any(|error| error.as_str().unwrap().contains("support requirement"))
        );
        let mut invalid = mock();
        invalid.omitted_support = true;
        assert!(
            matches!(run_with_support(&invalid,&request(),&RunConfig::default(),&|_|Ok(()),&Resolver::default()).await,Err(Error::InvalidOutput(message)) if message.contains("support requirement"))
        );
        assert_eq!(invalid.calls.lock().unwrap().len(), 6);
    }

    #[tokio::test]
    async fn dissent_is_preserved_and_call_budget_fails_before_execution() {
        let mut model = mock();
        model.critic_objection = true;
        let resolver = Resolver::default();
        let observed = AtomicUsize::new(0);
        let result = run_with_support(
            &model,
            &request(),
            &RunConfig::default(),
            &|_| {
                observed.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            &resolver,
        )
        .await;
        assert!(
            matches!(result,Err(Error::InvalidOutput(message)) if message.contains("critic-1"))
        );
        assert_eq!(observed.load(Ordering::SeqCst), 6);
        let model = mock();
        assert!(matches!(
            run_with_support(
                &model,
                &request(),
                &RunConfig {
                    max_calls: 5,
                    ..RunConfig::default()
                },
                &|_| Ok(()),
                &Resolver::default()
            )
            .await,
            Err(Error::Budget(_))
        ));
        assert!(model.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn single_mode_is_a_one_call_baseline_and_original_api_keeps_original_format() {
        let model = mock();
        let resolver = Resolver::default();
        let report = run_with_support(
            &model,
            &request(),
            &RunConfig {
                mode: Mode::Single,
                max_calls: 1,
                ..RunConfig::default()
            },
            &|_| Ok(()),
            &resolver,
        )
        .await
        .unwrap();
        assert_eq!(report.report.transcript.len(), 1);
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 1);
        let value = serde_json::to_value(&report).unwrap();
        assert!(value.get("evidence_planning").is_some());
        assert!(value.get("report").is_none());
        let restored: SupportRunReport<Value> = serde_json::from_value(value).unwrap();
        assert_eq!(restored.report.schema_version, "engarg.run.v2");
    }

    #[tokio::test]
    async fn every_debate_topology_keeps_support_extraction_and_private_route_previews() {
        use crate::engine::topology::{DebateConfig, Topology};

        for name in [
            "fully_connected",
            "grouped",
            "chain",
            "ring",
            "star",
            "tree",
            "layered",
            "sparse",
            "bipartite",
        ] {
            let model = mock();
            let resolver = Resolver::default();
            let config = RunConfig {
                debate: Some(DebateConfig {
                    participants: 4,
                    rounds: 1,
                    topology: Topology::preset(name, 4).unwrap(),
                    adaptation: Default::default(),
                }),
                max_calls: 11,
                ..RunConfig::default()
            };
            let report = run_with_support(&model, &request(), &config, &|_| Ok(()), &resolver)
                .await
                .unwrap();
            assert_eq!(report.report.transcript.len(), 11, "{name}");
            assert_eq!(resolver.calls.load(Ordering::SeqCst), 2, "{name}");
            assert!(
                support_requirements::validate_elaboration(
                    &report.report.argument,
                    &report.evidence_planning.elaboration,
                )
                .is_empty()
            );
            let calls = model.calls.lock().unwrap();
            assert_eq!(calls.len(), 11, "{name}");
            assert!(
                calls
                    .iter()
                    .all(|call| !call.user.contains("DO_NOT_SEND_OPERATOR_BINDINGS"))
            );
            let critic: Value = serde_json::from_str(&calls[9].user).unwrap();
            assert_eq!(critic["route_preview"]["status"], "pending_configuration");
            assert_eq!(
                critic["discussion_assessments"].as_array().unwrap().len(),
                4
            );
            let adjudication: Value = serde_json::from_str(&calls[10].user).unwrap();
            assert!(adjudication.get("draft_support_elaboration").is_some());
            assert!(adjudication.get("draft_requirement_sha256").is_some());
            assert!(adjudication.get("draft_step_sha256").is_some());
        }
    }

    #[tokio::test]
    async fn debated_support_rejects_invalid_configuration_before_model_or_resolver_io() {
        use crate::engine::topology::{DebateConfig, Topology};

        let base = RunConfig {
            debate: Some(DebateConfig {
                participants: 4,
                rounds: 1,
                topology: Topology::preset("ring", 4).unwrap(),
                adaptation: Default::default(),
            }),
            max_calls: 11,
            ..RunConfig::default()
        };
        let mut unreserved_repair = base.clone();
        unreserved_repair.max_repairs = 1;
        let mut invalid_graph = base.clone();
        invalid_graph.debate.as_mut().unwrap().topology = Topology::Star { hub: 4 };
        let mut single = base;
        single.mode = Mode::Single;
        for (config, budget_error) in [
            (unreserved_repair, true),
            (invalid_graph, false),
            (single, false),
        ] {
            let model = mock();
            let resolver = Resolver::default();
            let error = run_with_support(&model, &request(), &config, &|_| Ok(()), &resolver)
                .await
                .unwrap_err();
            if budget_error {
                assert!(matches!(error, Error::Budget(_)), "{error}");
            } else {
                assert!(matches!(error, Error::InvalidInput(_)), "{error}");
            }
            assert!(model.calls.lock().unwrap().is_empty());
            assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
        }
    }
}
