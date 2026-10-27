//! Compile resolved acquisitions and semantic obligations into executable EAL.
//!
//! The execution plan is a compiler input. No plan, catalogue or connection JSON
//! participates in execution of the emitted source.
mod conditions;
mod methods;
mod requirements;
mod values;

use crate::{
    Error,
    domain::{ArgumentCase, Request},
    eal_export::{self, ExportProjection, name, quoted},
    export_policy::ExportPolicy,
    support_requirements::SupportElaboration,
};
use eal_collector_contracts::execution::{
    DependencyBinding, DerivationStep, EvidenceExecutionPlan, ForEachBinding, OutputReference,
    RequirementEvaluation, SourceAvailability,
};
use serde_json::Value;
use std::collections::BTreeMap;

const ENVIRONMENT: &str = "engarg_case";

pub(super) struct ProgramPlan {
    pub acquisitions: Vec<SourceOperation>,
    pub derivations: Vec<DerivationStep>,
    pub evaluations: Vec<RequirementEvaluation>,
}
pub(super) struct SourceOperation {
    pub id: String,
    pub version: String,
    pub arguments: Value,
    pub bindings: Vec<DependencyBinding>,
    pub for_each: Option<ForEachBinding>,
    pub max_age_seconds: Option<u64>,
    pub availability: SourceAvailability,
}
impl From<&EvidenceExecutionPlan> for ProgramPlan {
    fn from(plan: &EvidenceExecutionPlan) -> Self {
        Self {
            acquisitions: plan
                .acquisitions
                .iter()
                .map(|step| SourceOperation {
                    id: step.id.clone(),
                    version: step.source.capability_version.clone(),
                    arguments: step.arguments.clone(),
                    bindings: step.bindings.clone(),
                    for_each: step.for_each.clone(),
                    max_age_seconds: step.max_age_seconds,
                    availability: step.availability.clone(),
                })
                .collect(),
            derivations: plan.derivations.clone(),
            evaluations: plan.evaluations.clone(),
        }
    }
}

/// Compile logical operations bound explicitly by the operator. No capability
/// registry, qualification proof or fabricated source identity is involved.
pub fn compile_direct(
    request: &Request,
    case: &ArgumentCase,
    elaboration: &SupportElaboration,
    versions: &BTreeMap<String, String>,
    policy: &ExportPolicy,
) -> Result<ExportProjection, Error> {
    let diagnostics = crate::support_requirements::validate_elaboration(case, elaboration);
    if !diagnostics.is_empty() {
        return Err(Error::InvalidInput(diagnostics.join("; ")));
    }
    let plan = ProgramPlan {
        acquisitions: elaboration
            .acquisitions
            .iter()
            .map(|step| {
                Ok(SourceOperation {
                    id: step.id.clone(),
                    version: versions
                        .get(&step.id)
                        .ok_or_else(|| {
                            invalid(format!("No direct TOML binding for operation {}", step.id))
                        })?
                        .clone(),
                    arguments: step.arguments.clone(),
                    bindings: step.bindings.clone(),
                    for_each: step.for_each.clone(),
                    max_age_seconds: step.contract.time.max_age_seconds,
                    availability: SourceAvailability::Configured,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?,
        derivations: elaboration.derivations.clone(),
        evaluations: elaboration.evaluations.clone(),
    };
    let resolved = policy.resolve(case)?;
    let evidence = compile_evidence(case, elaboration, &plan, &resolved)?;
    eal_export::export_executable(request, case, &resolved, &evidence)
}

/// Tool names are local program symbols, not capability catalogue lookups.
pub fn operation_tool_name(step_id: &str) -> String {
    name("source", step_id)
}

/// Construction result consumed by the single argument exporter.
#[derive(Default)]
pub(crate) struct CompiledEvidence {
    pub declarations: String,
    pub targets: BTreeMap<String, Vec<String>>,
    pub challenge_activation: BTreeMap<String, Vec<String>>,
    pub challenge_resolution: BTreeMap<String, Vec<String>>,
    pub method_inputs: BTreeMap<String, String>,
    pub requirements: BTreeMap<String, String>,
    pub pending: Vec<String>,
    pub review_requests: Vec<crate::eal_export::ReviewRequest>,
}

/// Compile the argument once, including the evidence that supports each node.
pub fn compile(
    request: &Request,
    case: &ArgumentCase,
    elaboration: &SupportElaboration,
    execution: &EvidenceExecutionPlan,
    policy: &ExportPolicy,
) -> Result<ExportProjection, Error> {
    let diagnostics = crate::support_requirements::validate_elaboration(case, elaboration);
    if !diagnostics.is_empty() {
        return Err(Error::InvalidInput(diagnostics.join("; ")));
    }
    execution.validate().map_err(Error::InvalidInput)?;
    let resolved = policy.resolve(case)?;
    let evidence = compile_evidence(case, elaboration, &ProgramPlan::from(execution), &resolved)?;
    eal_export::export_executable(request, case, &resolved, &evidence)
}

fn compile_evidence(
    case: &ArgumentCase,
    elaboration: &SupportElaboration,
    plan: &ProgramPlan,
    policy: &ExportPolicy,
) -> Result<CompiledEvidence, Error> {
    let mut compiled = CompiledEvidence::default();
    for step in &plan.acquisitions {
        if step.for_each.is_some() {
            return Err(invalid(format!(
                "Acquisition {} uses plan-only fan-out. Bind an explicit bounded batch operation before executable export",
                step.id
            )));
        }
        let kind = if elaboration.acquisitions.iter().any(|blueprint| {
            blueprint.id == step.id
                && blueprint.contract.support_type
                    == eal_collector_contracts::capability::SupportType::Judgement
        }) {
            "review"
        } else {
            "observation"
        };
        let tool = operation_tool_name(&step.id);
        let evidence = name("acquisition", &step.id);
        compiled.declarations.push_str(&format!(
            "tool {tool} {{\n  version {}\n}}\n\nevidence {evidence} {{\n  tool {tool}\n  kind {kind}\n  environment {ENVIRONMENT}\n  max_age {}\n  input {}\n",
            quoted(&step.version), step.max_age_seconds.unwrap_or(policy.acquisition_max_age_seconds), serde_json::to_string(&step.arguments)?,
        ));
        for binding in &step.bindings {
            write_bind(
                &mut compiled.declarations,
                &binding.argument_pointer,
                &binding.output,
                plan,
            );
        }
        if !matches!(step.availability, SourceAvailability::Configured) {
            // A missing binding must not turn a source-availability assertion into
            // a positive observation. The configuration is left explicitly absent.
            compiled
                .pending
                .push(format!("{}: {:?}", step.id, step.availability));
        }
        compiled.declarations.push_str("  require true\n}\n\n");
    }
    values::compile(plan, policy, &mut compiled.declarations)?;

    requirements::compile(elaboration, plan, policy, &mut compiled)?;
    conditions::complete(case, policy, &mut compiled)?;
    methods::compile(case, elaboration, plan, policy, &mut compiled)?;
    Ok(compiled)
}

pub(super) fn write_bind(
    source: &mut String,
    destination: &str,
    reference: &OutputReference,
    plan: &ProgramPlan,
) {
    let prefix = if plan
        .acquisitions
        .iter()
        .any(|step| step.id == reference.step_id)
    {
        "acquisition"
    } else {
        "derivation"
    };
    let pointer = if values::wrapped(plan, &reference.step_id) {
        format!("/value{}", reference.pointer)
    } else {
        reference.pointer.clone()
    };
    source.push_str(&format!(
        "  bind {} from {} {}\n",
        quoted(destination),
        name(prefix, &reference.step_id),
        quoted(&pointer)
    ));
}

/// One pure evidence declaration, including its input, provenance bindings and
/// acceptance predicates. Keeping these together makes emission atomic.
pub(super) struct DerivedSpec<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub max_age: u64,
    pub input: &'a Value,
    pub bindings: &'a str,
    pub expression: &'a str,
    pub predicates: &'a [String],
}

pub(super) fn write_derived(source: &mut String, spec: DerivedSpec<'_>) -> Result<(), Error> {
    let DerivedSpec {
        name: declaration,
        kind,
        max_age: age,
        input,
        bindings: binds,
        expression,
        predicates,
    } = spec;
    source.push_str(&format!("evidence {declaration} {{\n  kind {kind}\n  environment {ENVIRONMENT}\n  max_age {age}\n  input {}\n{binds}  derive {expression}\n", serde_json::to_string(input)?));
    if predicates.is_empty() {
        source.push_str("  require true\n");
    }
    for predicate in predicates {
        source.push_str(&format!("  require {predicate}\n"));
    }
    source.push_str("}\n\n");
    Ok(())
}

pub(super) fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(format!("EXECUTABLE_EAL: {}", message.into()))
}
