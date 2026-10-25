use super::*;
use crate::{
    domain::ArgumentIndex,
    eal_export::{EalDeclaration, EalSourceMapping, ExportProjection, ReviewRequest, name, quoted},
    support_requirements::SupportRequirement,
};
use eal_collector_contracts::{
    CommandBinding, CommandRegistry, ToolKind,
    execution::{EvaluationCriterion, OutputReference, SourceAvailability},
};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

const GRAPH_TOOL: &str = "engarg_support_graph";
const SUPPORT_ENVIRONMENT: &str = "engarg_case";
const REVIEW_TOOL: &str = "engarg_review_contract";

pub(super) fn build(
    authored: ExportProjection,
    elaboration: &SupportElaboration,
    execution: &EvidenceExecutionPlan,
    routes: &[AcquisitionRouteProof],
    locations: &SupportExportLocations,
    exporter_version: &str,
) -> Result<SupportExportArtifacts, Error> {
    validate_support(
        &authored.bundle.original_case,
        elaboration,
        execution,
        routes,
    )?;
    let (projected_execution, alternative_projection) =
        super::alternatives::project(&authored.bundle.original_case, elaboration, execution)?;
    let execution = &projected_execution;
    let registry = registry(locations)?;
    let tools_toml = registry.to_toml().map_err(Error::InvalidInput)?;
    let execution_plan_sha256 = canonical_digest(execution).map_err(Error::InvalidInput)?;
    let identity = (
        SUPPORT_EXPORT_SCHEMA,
        SUPPORT_PROJECTION_SEMANTICS,
        exporter_version,
        &authored.bundle,
        elaboration,
        execution,
        routes,
        locations,
    );
    let revision = match &alternative_projection {
        None => canonical_digest(&identity),
        Some(projection) => canonical_digest(&(identity, projection)),
    }
    .map_err(Error::InvalidInput)?;
    let mut context = authored.bundle.context.clone();
    context["engarg_support_export_digest"] = json!(revision);
    context["engarg_execution_plan_sha256"] = json!(execution_plan_sha256);
    let mut source = authored.bundle.source.clone();
    source.push_str(&format!(
        "// Generated support gates. The authored projection above is retained unchanged.\n// Requirement evaluations can share source IDs; repeated projections are not independent evidence.\n// Support export revision: {revision}\n\ntool {GRAPH_TOOL} {{\n  version \"1\"\n}}\n\n",
    ));
    let index = ArgumentIndex::new(&authored.bundle.original_case);
    let mut claim_names = BTreeMap::new();
    let mut source_map = authored.bundle.source_map.clone();
    let mut pending = Vec::new();
    let mut acquisition_requests = Vec::new();
    let mut projections = Vec::new();
    let cones: BTreeMap<_, _> = authored
        .bundle
        .claim_names
        .keys()
        .map(|id| (id.as_str(), index.selected_dependency(id)))
        .collect();
    let alternative_gates = crate::support_requirements::alternative_requirement_gates(elaboration);
    for requirement in &elaboration.requirements {
        let evaluation = execution
            .evaluations
            .iter()
            .find(|evaluation| evaluation.requirement_id == requirement.id)
            .expect("validated one evaluation per requirement");
        let evidence_name = name("support_evaluation", &requirement.id);
        let input = json!({
            "schema": SUPPORT_ACQUISITION_SCHEMA,
            "case_digest": authored.bundle.revision,
            "execution_plan_sha256": execution_plan_sha256,
            "requirement_id": requirement.id,
        });
        let age = requirement_age(
            requirement,
            evaluation.max_age_seconds,
            !evaluation.inputs.is_empty(),
        )?;
        source.push_str(&format!(
            "// Generated evaluation for support requirement {}.\nevidence {evidence_name} {{\n  tool {GRAPH_TOOL}\n  kind acquisition\n  environment {SUPPORT_ENVIRONMENT}\n  max_age {age}\n  input {}\n  require collected == true\n  require criterion_satisfied == true\n  require execution_plan_sha256 == {}\n}}\n\n",
            quoted(&requirement.id), serde_json::to_string(&input)?, quoted(&execution_plan_sha256),
        ));
        record(
            &mut source_map,
            &requirement.target_id,
            "evidence",
            &evidence_name,
            "generated_support_evaluation",
        );
        for need_id in &requirement.evidence_need_ids {
            record(
                &mut source_map,
                need_id,
                "evidence",
                &evidence_name,
                "generated_support_evaluation",
            );
        }
        // A support-requirement namespace avoids collisions with canonical node IDs.
        record(
            &mut source_map,
            &format!("support_requirement:{}", requirement.id),
            "evidence",
            &evidence_name,
            "generated_support_evaluation",
        );
        acquisition_requests.push(ReviewRequest {
            evidence_id: evidence_name.clone(),
            environment: SUPPORT_ENVIRONMENT.into(),
            tool: GRAPH_TOOL.into(),
            tool_version: "1".into(),
            input,
            context: context.clone(),
        });
        let (source_step_ids, derived_step_ids) = source_closure(
            execution,
            evaluation
                .inputs
                .iter()
                .chain(&evaluation.applicability)
                .chain(&evaluation.challenges),
        );
        for gap in &requirement.gaps {
            pending.push(PendingSupport {
                kind: format!(
                    "requirement_{}",
                    serde_json::to_value(gap.kind)?
                        .as_str()
                        .expect("gap kind serialises as a stable enum name")
                ),
                requirement_id: Some(requirement.id.clone()),
                target_id: requirement.target_id.clone(),
                description: gap.description.clone(),
            });
        }
        match &requirement.acceptance {
            EvaluationCriterion::Judgement { criterion, scope }
                if scope.is_none() || evaluation.inputs.is_empty() =>
            {
                pending.push(PendingSupport {
                    kind: "independent_judgement".into(),
                    requirement_id: Some(requirement.id.clone()),
                    target_id: requirement.target_id.clone(),
                    description: criterion.clone(),
                })
            }
            EvaluationCriterion::Unavailable { reason } => pending.push(PendingSupport {
                kind: "support_unavailable".into(),
                requirement_id: Some(requirement.id.clone()),
                target_id: requirement.target_id.clone(),
                description: reason.clone(),
            }),
            _ => {}
        }
        for step in execution
            .acquisitions
            .iter()
            .filter(|step| source_step_ids.contains(&step.id))
        {
            let (kind, description) = match &step.availability {
                SourceAvailability::Configured => continue,
                SourceAvailability::Unavailable { reason } => {
                    ("source_unavailable", reason.clone())
                }
                SourceAvailability::ConfigurationMissing { requirements } => {
                    ("configuration_missing", requirements.join("; "))
                }
                SourceAvailability::HistoricalUnavailable { reason } => {
                    ("historical_unavailable", reason.clone())
                }
            };
            pending.push(PendingSupport {
                kind: kind.into(),
                requirement_id: Some(requirement.id.clone()),
                target_id: requirement.target_id.clone(),
                description,
            });
        }
        projections.push(SupportRequirementProjection {
            requirement_id: requirement.id.clone(),
            target_id: requirement.target_id.clone(),
            evidence_need_ids: requirement.evidence_need_ids.clone(),
            evaluation_id: evaluation.id.clone(),
            eal_evidence_name: evidence_name,
            support_type: requirement
                .contract
                .as_ref()
                .map(|contract| contract.support_type),
            source_step_ids,
            derived_step_ids,
            gated_claim_ids: cones
                .iter()
                .filter(|(id, cone)| {
                    cone.contains(requirement.target_id.as_str())
                        || alternative_gates
                            .get(&requirement.id)
                            .is_some_and(|claims| claims.contains(**id))
                })
                .map(|(id, _)| (*id).to_owned())
                .collect(),
        });
    }
    if let Some(projection) = &alternative_projection {
        for guard in &projection.guards {
            let input = json!({"schema":SUPPORT_ACQUISITION_SCHEMA,"case_digest":authored.bundle.revision,"execution_plan_sha256":execution_plan_sha256,"requirement_id":guard.requirement_id});
            source.push_str(&format!("// Generated unavailable guard for incomplete legacy support extraction.\nevidence {} {{\n  tool {GRAPH_TOOL}\n  kind acquisition\n  environment {SUPPORT_ENVIRONMENT}\n  max_age 0\n  input {}\n  require collected == true\n  require criterion_satisfied == true\n  require execution_plan_sha256 == {}\n}}\n\n",guard.evidence_name,serde_json::to_string(&input)?,quoted(&execution_plan_sha256)));
            record(
                &mut source_map,
                &format!("alternative:{}", guard.alternative_id),
                "evidence",
                &guard.evidence_name,
                "generated_legacy_alternative_gap",
            );
            acquisition_requests.push(ReviewRequest {
                evidence_id: guard.evidence_name.clone(),
                environment: SUPPORT_ENVIRONMENT.into(),
                tool: GRAPH_TOOL.into(),
                tool_version: "1".into(),
                input,
                context: context.clone(),
            });
            for claim in &guard.claim_ids {
                pending.push(PendingSupport {
                    kind: "legacy_alternative_support".into(),
                    requirement_id: None,
                    target_id: claim.clone(),
                    description: guard.description.clone(),
                });
            }
        }
    }
    for (id, original_claim) in &authored.bundle.claim_names {
        let gate = name("support_claim", id);
        let reasoning = name("support_reasoning", id);
        let argument = name("support_argument", id);
        let (text, qualifier) = index.claim(id).expect("canonical claim map");
        let mut evidence: Vec<_> = projections
            .iter()
            .filter(|projection| projection.gated_claim_ids.contains(id))
            .map(|projection| projection.eal_evidence_name.as_str())
            .collect();
        if let Some(projection) = &alternative_projection {
            evidence.extend(
                projection
                    .guards
                    .iter()
                    .filter(|guard| guard.claim_ids.contains(id))
                    .map(|guard| guard.evidence_name.as_str()),
            );
        }
        let support = if evidence.is_empty() {
            format!("[premises {original_claim}]")
        } else {
            format!(
                "[evidence {}, premises {original_claim}]",
                evidence.join(", ")
            )
        };
        source.push_str(&format!(
            "claim {gate} {{\n  statement {}\n  environment {SUPPORT_ENVIRONMENT}\n}}\n\nreasoning {reasoning} {{\n  method \"structured/1\"\n  rationale {}\n}}\n\nargument {argument} = {support} via {reasoning} => {gate}\n\n",
            quoted(&format!("{text}\nQualifier: {qualifier}")),
            quoted("Generated support gate around the canonical authored claim. Every requirement on its selected dependency cone must be satisfied separately. Shared source IDs retain common provenance; these evaluations add no independent observations and do not replace the authored inference or its independent review obligations."),
        ));
        for (kind, declaration, role) in [
            ("claim", &gate, "generated_support_gate"),
            ("reasoning", &reasoning, "generated_support_gate"),
            ("argument", &argument, "generated_support_gate"),
        ] {
            record(&mut source_map, id, kind, declaration, role);
        }
        claim_names.insert(id.clone(), gate);
    }
    let mut review_requests = authored.review_requests;
    for request in &mut review_requests {
        request.context = context.clone();
    }
    if locations.review_binding.is_none() {
        for obligation in &authored.bundle.obligations {
            pending.push(PendingSupport {
                kind: "independent_review_registration".into(),
                requirement_id: None,
                target_id: obligation.target_id.clone(),
                description: format!(
                    "{} remains pending; no independent reviewer is registered. {}",
                    obligation.id, obligation.requirement
                ),
            });
        }
    }
    let mut limitations = authored.bundle.limitations.clone();
    limitations.extend([
        "Support gates are generated projections, distinct from the canonical authored case. Only selected dependency routes gate each canonical claim.".into(),
        "Requirement projections retain original source identifiers and observation times. Reusing a source for different evaluations does not create independent evidence.".into(),
        "A plan, available interface or generated configuration is not an observation. Missing configuration, unavailable historical records and judgements without valid qualified retained records remain pending.".into(),
        "Typed machine predicates, calculations and proof-check outputs remain distinct from authored structured reasoning and retained independent review obligations.".into(),
    ]);
    let mut bundle = SupportExport {
        schema_version: SUPPORT_EXPORT_SCHEMA.into(),
        semantics_version: SUPPORT_PROJECTION_SEMANTICS.into(),
        exporter_version: exporter_version.into(),
        revision,
        target_language: "EAL/3".into(),
        authored: authored.bundle,
        support_elaboration: elaboration.clone(),
        execution_plan: execution.clone(),
        alternative_projection,
        route_proofs: routes.to_vec(),
        execution_plan_sha256,
        locations: locations.clone(),
        source,
        context,
        claim_names,
        source_map,
        requirements: projections,
        registry: registry.clone(),
        pending,
        limitations,
    };
    let budget = super::collection_budget(&bundle);
    if budget.total_emitted_evidence > budget.assumed_default_limit {
        bundle.pending.push(PendingSupport {
            kind: "collection_budget_exceeded".into(),
            requirement_id: None,
            target_id: "current_claim".into(),
            description: format!("Default collect-all selects {} emitted evidence declarations, exceeding the assumed EAL default collection_evidence budget {}. Check the selected host advertised budget or explicitly select a bounded claim; the complete graph is retained.", budget.total_emitted_evidence, budget.assumed_default_limit),
        });
    }
    for (claim, count) in &budget.claim_evidence_counts {
        if *count > budget.assumed_default_limit {
            bundle.pending.push(PendingSupport {
                kind: "collection_budget_exceeded".into(),
                requirement_id: None,
                target_id: claim.clone(),
                description: format!("Selected claim requires {count} evidence declarations, exceeding the assumed EAL default collection_evidence budget {}. Check the selected host advertised budget; the complete graph is retained.", budget.assumed_default_limit),
            });
        }
    }
    Ok(SupportExportArtifacts {
        bundle,
        tools_toml,
        registry,
        review_requests,
        acquisition_requests,
        execution_plan: execution.clone(),
    })
}

fn requirement_age(
    requirement: &SupportRequirement,
    maximum: Option<u64>,
    has_inputs: bool,
) -> Result<u64, Error> {
    match maximum {
        Some(age) if (1..=315_360_000).contains(&age) => Ok(age),
        // EAL/3 requires max_age. These stubs never emit an observation, so a
        // zero-second bound cannot turn an unknown age into usable evidence.
        None if matches!(
            requirement.acceptance,
            EvaluationCriterion::Unavailable { .. }
        ) || matches!(&requirement.acceptance,EvaluationCriterion::Judgement{scope,..} if scope.is_none() || !has_inputs) =>
        {
            Ok(0)
        }
        _ => Err(Error::InvalidInput(format!(
            "SUPPORT_FRESHNESS_CLARIFICATION: requirement {} needs an explicit positive bounded freshness contract before EAL/3 can collect it; this graph has no implicit acquisition-age default",
            requirement.id
        ))),
    }
}

fn registry(locations: &SupportExportLocations) -> Result<CommandRegistry, Error> {
    let paths = [
        &locations.collector,
        &locations.execution_plan,
        &locations.connections,
        &locations.store,
    ];
    for path in paths {
        let value = literal_path(path)?;
        if value.contains("REPLACE_") {
            return Err(Error::InvalidInput(
                "SUPPORT_EXPORT_LOCATION: unresolved paths are not executable bindings".into(),
            ));
        }
    }
    if locations.execution_plan == locations.connections
        || locations.execution_plan == locations.store
        || locations.connections == locations.store
    {
        return Err(Error::InvalidInput("SUPPORT_EXPORT_LOCATION: plan, connections and store must have distinct final locations".into()));
    }
    let mut tools = BTreeMap::from([(
        GRAPH_TOOL.into(),
        CommandBinding {
            kind: ToolKind::Command,
            version: "1".into(),
            argv: vec![
                literal_path(&locations.collector)?,
                "support-graph".into(),
                "--plan".into(),
                literal_path(&locations.execution_plan)?,
                "--config".into(),
                literal_path(&locations.connections)?,
                "--store".into(),
                literal_path(&locations.store)?,
            ],
            timeout_seconds: 1800,
            max_output_bytes: 1_048_576,
            inherit_env: locations.environment.clone(),
            parallel_safe: true,
        },
    )]);
    if let Some(binding) = &locations.review_binding {
        tools.insert(REVIEW_TOOL.into(), binding.clone());
    }
    let registry = CommandRegistry { tools };
    registry.validate().map_err(Error::InvalidInput)?;
    Ok(registry)
}
fn literal_path(path: &Path) -> Result<String, Error> {
    let value = path.to_str().ok_or_else(|| {
        Error::InvalidInput("SUPPORT_EXPORT_LOCATION: paths require UTF-8".into())
    })?;
    if !path.is_absolute()
        || value.len() > 8192
        || value.chars().any(char::is_control)
        || value.contains(['{', '}'])
    {
        return Err(Error::InvalidInput("SUPPORT_EXPORT_LOCATION: collector and artifact paths must be absolute bounded literals".into()));
    }
    Ok(value.into())
}
fn record(
    map: &mut BTreeMap<String, EalSourceMapping>,
    id: &str,
    kind: &str,
    declaration: &str,
    role: &str,
) {
    map.entry(id.into())
        .or_default()
        .declarations
        .push(EalDeclaration {
            kind: kind.into(),
            name: declaration.into(),
            role: role.into(),
        });
}
fn source_closure<'a>(
    plan: &EvidenceExecutionPlan,
    references: impl Iterator<Item = &'a OutputReference>,
) -> (Vec<String>, Vec<String>) {
    let mut pending: Vec<_> = references
        .map(|reference| reference.step_id.clone())
        .collect();
    let mut seen = BTreeSet::new();
    let mut sources = BTreeSet::new();
    let mut derived = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        if let Some(step) = plan.acquisitions.iter().find(|step| step.id == id) {
            sources.insert(id);
            pending.extend(
                step.bindings
                    .iter()
                    .map(|binding| binding.output.step_id.clone())
                    .chain(step.for_each.iter().flat_map(|binding| {
                        std::iter::once(binding.output.step_id.clone()).chain(
                            binding.stop_on_witness.iter().filter_map(|predicate| {
                                predicate
                                    .equals_output
                                    .as_ref()
                                    .map(|output| output.step_id.clone())
                            }),
                        )
                    })),
            );
        } else if let Some(step) = plan.derivations.iter().find(|step| step.id == id) {
            derived.insert(id);
            pending.extend(
                step.inputs
                    .iter()
                    .map(|reference| reference.step_id.clone()),
            );
        }
    }
    (sources.into_iter().collect(), derived.into_iter().collect())
}
