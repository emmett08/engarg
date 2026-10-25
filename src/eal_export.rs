//! Deterministic, conservative translation into authored EAL/3 arguments.
//!
//! Export creates review contracts, never observations. Supplying an excerpt or
//! setting `checked_by_user` does not discharge any contract. EAL's structured
//! method preserves an authored argument. Explicit typed warrant bindings select
//! bounded computational methods, without proving correspondence to arbitrary prose.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    Error,
    domain::{
        ArgumentCase, ArgumentIndex, ArgumentNode, BoundReasoningMethod, DefeaterStatus,
        ReasoningMethodBinding, ReasoningMethodKind, Request, SupportRelationship,
        validate_argument, validate_request,
    },
    eal_acquisition::AcquisitionProjection,
};

#[cfg(test)]
const LEGACY_SCHEMA_VERSION: &str = "engarg.eal-export/1";
const SCHEMA_VERSION: &str = "engarg.eal-export/3";
const SEMANTICS_VERSION: &str = "engarg.eal-projection/3";
const TYPED_SEMANTICS_VERSION: &str = "engarg.eal-projection/4";
use crate::export_policy::ExportPolicy;
const ENVIRONMENT: &str = "engarg_case";
const REVIEW_TOOL: &str = "engarg_review_contract";
pub(crate) const REASONING_TOOL: &str = "engarg_reasoning_input";

fn semantics_version(case: &ArgumentCase) -> &'static str {
    if case
        .warrants
        .iter()
        .any(|warrant| warrant.reasoning_method.is_some())
    {
        TYPED_SEMANTICS_VERSION
    } else {
        SEMANTICS_VERSION
    }
}

pub(crate) fn reasoning_input_selector(
    revision: &str,
    warrant_id: &str,
    evidence_need_id: &str,
) -> Value {
    json!({"schema":"engarg.reasoning-input-pending/1", "case_digest":revision,
        "warrant_id":warrant_id, "evidence_need_id":evidence_need_id})
}

/// A complete, loss-preserving bundle around the executable EAL/3 projection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EalExport {
    pub schema_version: String,
    pub exporter_version: String,
    /// Projection semantics are versioned independently of the producer package.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantics_version: Option<String>,
    /// Selected, versioned collector contracts retained for deterministic reconstruction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collector_catalogue: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acquisition_plan: Option<ExportPlan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export_policy: Option<ExportPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authoring_profile: Option<String>,
    pub revision: String,
    pub target_language: String,
    pub review_max_age_seconds: u64,
    pub source: String,
    pub context: Value,
    /// Original claim identifiers to queryable EAL claim identifiers.
    pub claim_names: BTreeMap<String, String>,
    pub source_map: BTreeMap<String, EalSourceMapping>,
    pub obligations: Vec<ReviewObligation>,
    pub limitations: Vec<String>,
    pub original_request: Request,
    pub original_case: ArgumentCase,
}

/// Structural authority for prospective acquisition. Disabled plans have no entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportPlan {
    pub enabled: bool,
    pub entries: Vec<crate::acquisition::AcquisitionPlan>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EalSourceMapping {
    pub declarations: Vec<EalDeclaration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EalDeclaration {
    pub kind: String,
    pub name: String,
    pub role: String,
}

/// An unfulfilled acquisition/review requirement, not an accepted observation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewObligation {
    pub id: String,
    pub eal_evidence_name: String,
    pub kind: String,
    pub target_id: String,
    pub requirement: String,
    pub source_evidence_ids: Vec<String>,
}

/// Select the identity rule declared by the persisted bundle schema.
/// Producer versions remain provenance under the current semantics.
#[derive(Clone, Copy)]
pub(crate) enum ExportIdentity<'a> {
    Current {
        exporter_version: &'a str,
    },
    #[cfg(test)]
    Legacy {
        exporter_version: &'a str,
    },
}

impl<'a> ExportIdentity<'a> {
    pub(crate) fn current() -> Self {
        Self::Current {
            exporter_version: env!("CARGO_PKG_VERSION"),
        }
    }

    pub(crate) fn for_bundle(bundle: &'a EalExport) -> Result<Self, Error> {
        if bundle.exporter_version.is_empty()
            || bundle.exporter_version.len() > 128
            || bundle.exporter_version.chars().any(char::is_control)
        {
            return Err(Error::InvalidInput(
                "invalid export producer version".into(),
            ));
        }
        if bundle.schema_version != SCHEMA_VERSION
            || bundle.semantics_version.as_deref() != Some(semantics_version(&bundle.original_case))
            || bundle.authoring_profile.as_deref()
                != Some(eal_collector_contracts::RUNTIME_EXPORTABLE_PROFILE)
            || bundle.acquisition_plan.is_none()
            || bundle.export_policy.is_none()
            || bundle
                .source
                .lines()
                .any(|line| line.trim_start().starts_with("// engarg_acquisition_plan"))
        {
            return Err(Error::InvalidInput(
                "unsupported export schema, structural plan or projection semantics".into(),
            ));
        }
        let plan = bundle.acquisition_plan.as_ref().expect("checked");
        if !plan.enabled && !plan.entries.is_empty() {
            return Err(Error::InvalidInput(
                "disabled export plan contains entries".into(),
            ));
        }
        Ok(Self::Current {
            exporter_version: &bundle.exporter_version,
        })
    }

    fn schema_version(self) -> &'static str {
        SCHEMA_VERSION
    }

    fn exporter_version(self) -> &'a str {
        match self {
            Self::Current { exporter_version } => exporter_version,
            #[cfg(test)]
            Self::Legacy { exporter_version } => exporter_version,
        }
    }

    fn digest_version(self) -> &'a str {
        SEMANTICS_VERSION
    }
}

/// Reviews use the authoritative acquisition request without another wire model.
pub use eal_collector_contracts::AcquisitionRequest as ReviewRequest;

/// Structured export products share one construction pass and one identity.
pub struct ExportProjection {
    pub bundle: EalExport,
    pub review_requests: Vec<ReviewRequest>,
}

/// Validate the canonical graph, then project its explicit selected reliance.
///
/// Invalid source graphs are rejected rather than repaired by the exporter.
/// All generated identifiers use category prefixes and injective UTF-8 hex.
/// Missing review observations leave the corresponding EAL support unavailable.
pub fn export(request: &Request, case: &ArgumentCase) -> Result<EalExport, Error> {
    Ok(export_projection(request, case)?.bundle)
}

/// Construct source and exact review requests from the same authored records.
pub fn export_projection(
    request: &Request,
    case: &ArgumentCase,
) -> Result<ExportProjection, Error> {
    export_projection_for_identity(request, case, None, ExportIdentity::current())
}

/// Regenerate a supported bundle under its original identity and producer metadata.
pub(crate) fn reconstruct_projection(
    bundle: &EalExport,
    acquisition: Option<&AcquisitionProjection>,
) -> Result<ExportProjection, Error> {
    if bundle.schema_version != SCHEMA_VERSION {
        let legacy: crate::legacy_export::EalExport =
            serde_json::from_value(serde_json::to_value(bundle)?)?;
        let rebuilt = crate::legacy_acquisition::reconstruct_bundle(&legacy)?;
        return Ok(ExportProjection {
            bundle: serde_json::from_value(serde_json::to_value(rebuilt)?)?,
            review_requests: Vec::new(),
        });
    }
    let identity = ExportIdentity::for_bundle(bundle)?;
    let policy = bundle.export_policy.as_ref().expect("checked");
    if policy.resolve(&bundle.original_case)? != *policy {
        return Err(Error::InvalidInput("export policy is not resolved".into()));
    }
    export_projection_with_policy_identity(
        &bundle.original_request,
        &bundle.original_case,
        acquisition,
        identity,
        policy,
    )
}

pub(crate) fn export_projection_for_identity(
    request: &Request,
    case: &ArgumentCase,
    acquisition: Option<&AcquisitionProjection>,
    identity: ExportIdentity<'_>,
) -> Result<ExportProjection, Error> {
    #[cfg(test)]
    if let ExportIdentity::Legacy { exporter_version } = identity {
        let projected = crate::legacy_export::export_projection_for_identity(
            request,
            case,
            None,
            crate::legacy_export::ExportIdentity::Legacy { exporter_version },
        )?;
        return Ok(ExportProjection {
            bundle: serde_json::from_value(serde_json::to_value(projected.bundle)?)?,
            review_requests: serde_json::from_value(serde_json::to_value(
                projected.review_requests,
            )?)?,
        });
    }
    export_projection_with_policy_identity(
        request,
        case,
        acquisition,
        identity,
        &ExportPolicy::default().resolve(case)?,
    )
}

pub(crate) fn export_projection_with_policy_identity(
    request: &Request,
    case: &ArgumentCase,
    acquisition: Option<&AcquisitionProjection>,
    identity: ExportIdentity<'_>,
    policy: &ExportPolicy,
) -> Result<ExportProjection, Error> {
    validate_request(request).map_err(|errors| Error::InvalidInput(errors.join("; ")))?;
    let admitted = validate_argument(request, case).map_err(|errors| {
        Error::InvalidOutput(
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    let mut identity_bytes = serde_json::to_vec(&(
        identity.schema_version(),
        if semantics_version(case) == TYPED_SEMANTICS_VERSION {
            TYPED_SEMANTICS_VERSION
        } else {
            identity.digest_version()
        },
        request,
        case,
    ))?;
    let plan = ExportPlan {
        enabled: acquisition.is_some(),
        entries: acquisition
            .map(|projection| projection.plans.clone())
            .unwrap_or_default(),
    };
    let catalogue = acquisition.and_then(AcquisitionProjection::catalogue_value);
    identity_bytes.extend(serde_json::to_vec(&(
        &plan,
        policy,
        &catalogue,
        eal_collector_contracts::RUNTIME_EXPORTABLE_PROFILE,
    ))?);
    let revision: String = Sha256::digest(identity_bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let mut builder = Exporter::new(&revision, identity.schema_version(), acquisition, policy);
    builder.write(request, admitted.index())?;
    let mut bundle = EalExport {
        schema_version: identity.schema_version().into(),
        exporter_version: identity.exporter_version().into(),
        semantics_version: Some(semantics_version(case).into()),
        collector_catalogue: catalogue,
        acquisition_plan: Some(plan),
        export_policy: Some(policy.clone()),
        authoring_profile: Some(eal_collector_contracts::RUNTIME_EXPORTABLE_PROFILE.into()),
        revision: revision.clone(),
        target_language: "EAL/3".into(),
        review_max_age_seconds: policy.review_max_age_seconds,
        source: builder.source,
        context: json!({"engarg_export_digest": revision}),
        claim_names: builder.claim_names,
        source_map: builder.source_map,
        obligations: builder.obligations,
        limitations: vec![
            "The EAL source is a structural projection of authored reasoning using structured/1; original inference labels are retained, not mechanically proved or promoted to computational methods.".into(),
            "No observation records are generated. Supplied excerpts, model statuses, and checked_by_user never discharge a review obligation. Missing observations mean unavailable support, not false claims.".into(),
            "engarg_review_contract is a declared review interface, not an installed collector. An operator must configure a trusted collector or provide compatible independently retained EAL observations before review evidence can be available.".into(),
            format!("Review observations have an explicit export-policy max_age of {} seconds; this is not an inferred expiry of the original source evidence.",policy.review_max_age_seconds),
            "Jointly required and cumulative routes are combined with AND. Cumulative weighting or statistical sufficiency remains an explicit composition review obligation. Only the selected alternative connects to its original conclusion; unselected route claims remain separately queryable.".into(),
            "Critical assumptions and open or sustained material challenges gate affected support. Noncritical assumptions remain declared and reviewable without adding mandatory dependencies absent from the original reliance policy.".into(),
            "Each challenge is an evidence-gated EAL objection at its original target category. Challenge kind, basis, materiality, response and original status are preserved in the review input and original_case; EAL does not encode these labels as an automatic proof or dismissal.".into(),
            "Free-text scope and qualifiers remain beside claim statements; they are not invented typed quantities, formal contraries, strict rules, rankings or preferences. Plans, prose, alternatives and decision limits are preserved exactly in original_request and original_case.".into(),
            "The context digest binds this projection to the canonical case revision. It identifies that authored scope; it does not establish that the real environment satisfies its conditions or authorize an engineering decision.".into(),
        ],
        original_request: request.clone(),
        original_case: case.clone(),
    };
    if case
        .warrants
        .iter()
        .any(|warrant| warrant.reasoning_method.is_some())
    {
        bundle.limitations[0] = "Explicit bound warrants project to their versioned EAL computational methods with a typed result predicate. Legacy warrants retain authored structured/1 support, and unresolved methods retain an unavailable gate.".into();
        bundle.limitations.extend([
            "Typed warrant bindings execute the selected bounded EAL method and test its declared result. Formal-to-prose correspondence remains authored; a successful computation does not establish empirical premise truth or scientific model adequacy.".into(),
            "Reasoning input, premise observations and sampling, hypothesis, intervention or structural-model evidence are separate obligations. Missing models, applicability records or matching acquired inputs leave the route unavailable.".into(),
        ]);
    }
    if acquisition.is_some() {
        bundle.limitations.push(
            "Acquisition availability follows the selected adapter contract. EngArg adapters use collected while native fleet collectors use cluster_count and native GitHub uses found. The existing independent review obligations still determine interpretation and sufficiency. Custom collectors remain command implementation stubs and unresolved inputs retain explicit command configuration templates. Acquisition observations use the persisted resolved per-need export-policy ages.".into(),
        );
    }
    Ok(ExportProjection {
        bundle,
        review_requests: builder.review_requests,
    })
}

pub(crate) fn name(prefix: &str, id: &str) -> String {
    let mut result = format!("{prefix}_");
    for byte in id.as_bytes() {
        use std::fmt::Write as _;
        write!(&mut result, "{byte:02x}").expect("writing into a String cannot fail");
    }
    result
}

pub(crate) fn quoted(text: &str) -> String {
    serde_json::to_string(text).expect("strings are serialisable")
}

fn evidence_names(ids: &[String]) -> Vec<String> {
    ids.iter().map(|id| name("source_review", id)).collect()
}

#[derive(Default)]
struct Support {
    evidence: BTreeSet<String>,
    assumptions: BTreeSet<String>,
    premises: BTreeSet<String>,
}

impl Support {
    fn render(&self) -> String {
        let mut groups = Vec::new();
        for (kind, values) in [
            ("evidence", &self.evidence),
            ("assumptions", &self.assumptions),
            ("premises", &self.premises),
        ] {
            if !values.is_empty() {
                groups.push(format!(
                    "{kind} {}",
                    values.iter().cloned().collect::<Vec<_>>().join(", ")
                ));
            }
        }
        format!("[{}]", groups.join(", "))
    }
}

struct Exporter<'a> {
    revision: &'a str,
    schema_version: &'static str,
    source: String,
    source_map: BTreeMap<String, EalSourceMapping>,
    claim_names: BTreeMap<String, String>,
    obligations: Vec<ReviewObligation>,
    review_requests: Vec<ReviewRequest>,
    acquisition: Option<&'a AcquisitionProjection>,
    policy: &'a ExportPolicy,
}

impl<'a> Exporter<'a> {
    fn new(
        revision: &'a str,
        schema_version: &'static str,
        acquisition: Option<&'a AcquisitionProjection>,
        policy: &'a ExportPolicy,
    ) -> Self {
        Self {
            revision,
            schema_version,
            source: String::new(),
            source_map: BTreeMap::new(),
            claim_names: BTreeMap::new(),
            obligations: Vec::new(),
            review_requests: Vec::new(),
            acquisition,
            policy,
        }
    }

    fn record(&mut self, id: &str, kind: &str, declaration: &str, role: &str) {
        self.source_map
            .entry(id.into())
            .or_default()
            .declarations
            .push(EalDeclaration {
                kind: kind.into(),
                name: declaration.into(),
                role: role.into(),
            });
    }

    fn claim(&mut self, id: &str, declaration: &str, statement: &str, role: &str) {
        self.source.push_str(&format!(
            "claim {declaration} {{\n  statement {}\n  environment {ENVIRONMENT}\n}}\n\n",
            quoted(statement)
        ));
        self.record(id, "claim", declaration, role);
    }

    fn typed_claim(
        &mut self,
        id: &str,
        declaration: &str,
        statement: &str,
        bound: &BoundReasoningMethod,
    ) -> Result<(), Error> {
        let conclusion = &bound.conclusion;
        self.source.push_str(&format!(
            "claim {declaration} {{\n  statement {}\n  environment {ENVIRONMENT}\n  proposition {{\n    subject {}\n    quantity {}\n    unit {}\n    scope {}\n    valid_from {}\n    valid_until {}\n    query {}\n    result {} {} {}\n  }}\n}}\n\n",
            quoted(statement), quoted(&conclusion.subject), quoted(&conclusion.quantity),
            quoted(&conclusion.unit), quoted(&conclusion.scope.identity().map_err(Error::InvalidInput)?), quoted(&conclusion.valid_from),
            quoted(&conclusion.valid_until), serde_json::to_string(&bound.method.query())?,
            conclusion.result.path, conclusion.result.operator.as_str(),
            serde_json::to_string(&conclusion.result.expected)?,
        ));
        self.record(id, "claim", declaration, "typed_candidate_route");
        Ok(())
    }

    fn method_input(
        &mut self,
        warrant_id: &str,
        bound: &BoundReasoningMethod,
    ) -> Result<String, Error> {
        let declaration = name("reasoning_input", warrant_id);
        let input =
            reasoning_input_selector(self.revision, warrant_id, &bound.input_evidence_need_id);
        self.source.push_str(&format!(
            "evidence {declaration} {{\n  tool {REASONING_TOOL}\n  kind {}\n  environment {ENVIRONMENT}\n  max_age {}\n  input {}\n  require schema == \"EAL/typed-input/1\"\n}}\n\n",
            bound.method.evidence_kind(), self.policy.acquisition_age(&bound.input_evidence_need_id),
            serde_json::to_string(&input)?,
        ));
        self.review_requests.push(ReviewRequest {
            evidence_id: declaration.clone(),
            environment: ENVIRONMENT.into(),
            tool: REASONING_TOOL.into(),
            tool_version: "1".into(),
            input,
            context: json!({"engarg_export_digest":self.revision}),
        });
        self.record(
            warrant_id,
            "evidence",
            &declaration,
            "reasoning_method_input",
        );
        Ok(declaration)
    }

    fn method_reasoning(
        &mut self,
        warrant_id: &str,
        declaration: &str,
        rationale: &str,
        bound: &BoundReasoningMethod,
    ) {
        self.source.push_str(&format!(
            "reasoning {declaration} {{\n  method {}\n  rationale {}\n",
            quoted(bound.method.identifier()),
            quoted(rationale),
        ));
        if bound.method.kind() == ReasoningMethodKind::Deductive {
            self.source
                .push_str("  require consistent_premises == true\n");
        }
        self.source.push_str("}\n\n");
        self.record(warrant_id, "reasoning", declaration, "typed_warrant_method");
    }

    fn unresolved_method(
        &mut self,
        warrant_id: &str,
        binding: &ReasoningMethodBinding,
    ) -> Result<String, Error> {
        let declaration = name("reasoning_unresolved", warrant_id);
        let input = json!({"schema":"engarg.reasoning-unresolved/1","case_digest":self.revision,
            "warrant_id":warrant_id,"binding":binding});
        self.source.push_str(&format!(
            "// Missing formal model or evidence keeps this route unavailable.\nevidence {declaration} {{\n  tool {REASONING_TOOL}\n  kind unavailable_method\n  environment {ENVIRONMENT}\n  max_age 0\n  input {}\n  require method_available == true\n}}\n\n",
            serde_json::to_string(&input)?,
        ));
        self.record(
            warrant_id,
            "evidence",
            &declaration,
            "unresolved_reasoning_method",
        );
        Ok(declaration)
    }

    fn reasoning(
        &mut self,
        id: &str,
        declaration: &str,
        rationale: &str,
        backing: &[String],
        role: &str,
    ) {
        self.source.push_str(&format!(
            "reasoning {declaration} {{\n  method \"structured/1\"\n  rationale {}\n",
            quoted(rationale)
        ));
        if !backing.is_empty() {
            self.source
                .push_str(&format!("  backing [{}]\n", backing.join(", ")));
        }
        self.source.push_str("}\n\n");
        self.record(id, "reasoning", declaration, role);
    }

    fn argument(
        &mut self,
        id: &str,
        declaration: &str,
        support: &Support,
        reasoning: &str,
        conclusion: &str,
        role: &str,
    ) {
        self.source.push_str(&format!(
            "argument {declaration} = {} via {reasoning} => {conclusion}\n\n",
            support.render()
        ));
        self.record(id, "argument", declaration, role);
    }

    fn obligation(
        &mut self,
        prefix: &str,
        id: &str,
        kind: &str,
        requirement: String,
        source_evidence_ids: &[String],
        detail: Value,
    ) -> Result<String, Error> {
        let declaration = name(prefix, id);
        let obligation = ReviewObligation {
            id: format!("{kind}:{id}"),
            eal_evidence_name: declaration.clone(),
            kind: kind.into(),
            target_id: id.into(),
            requirement,
            source_evidence_ids: source_evidence_ids.to_vec(),
        };
        let mut input = json!({
            "schema": self.schema_version,
            "case_digest": self.revision,
            "obligation": obligation,
            "authored_detail": detail,
            "acquisition": "Explicit independent review required; no observation is supplied by this export."
        });
        if let Some(acquisition) = self.acquisition {
            input["required_acquisitions"] = json!(acquisition.review_inputs(id));
        }
        self.source.push_str(&format!(
            "evidence {declaration} {{\n  tool {REVIEW_TOOL}\n  kind review\n  environment {ENVIRONMENT}\n  max_age {review_age}\n  input {}\n  require accepted == true\n}}\n\n",
            serde_json::to_string(&input)?,review_age=self.policy.review_max_age_seconds
        ));
        self.review_requests.push(ReviewRequest {
            evidence_id: declaration.clone(),
            environment: ENVIRONMENT.into(),
            tool: REVIEW_TOOL.into(),
            tool_version: "1".into(),
            input,
            context: json!({"engarg_export_digest": self.revision}),
        });
        self.record(id, "evidence", &declaration, kind);
        self.obligations.push(obligation);
        Ok(declaration)
    }

    fn write(&mut self, request: &Request, index: &ArgumentIndex<'_>) -> Result<(), Error> {
        let case = index.case();
        let assumption_conditions: BTreeMap<_, _> = case
            .assumptions
            .iter()
            .map(|a| {
                (
                    name("assumption", &a.id),
                    name("assumption_condition", &a.id),
                )
            })
            .collect();
        self.source.push_str(&format!(
            "language \"EAL/3\"\n\n// Authored EngArg projection. Review contracts are not observations.\n// Keep the JSON export bundle for the complete canonical source and source map.\n\nenvironment {ENVIRONMENT} {{\n  require engarg_export_digest == {}\n}}\n\ntool {REVIEW_TOOL} {{\n  version \"1\"\n}}\n\n",
            quoted(self.revision)
        ));
        if case
            .warrants
            .iter()
            .any(|warrant| warrant.reasoning_method.is_some())
        {
            self.source.push_str(&format!(
                "tool {REASONING_TOOL} {{\n  version \"1\"\n}}\n\n"
            ));
        }
        if let Some(acquisition) = self.acquisition {
            self.source.push_str(&acquisition.declarations(
                self.revision,
                ENVIRONMENT,
                self.policy,
            )?);
            for (id, tool, evidence) in acquisition.mappings() {
                self.record(id, "tool", tool, "planned_acquisition_tool");
                self.record(id, "evidence", evidence, "planned_acquisition");
            }
        }
        for evidence in &request.evidence {
            self.obligation(
                "source_review", &evidence.id, "source_review",
                "Independently establish this source's identity, integrity, provenance, relevant contents and fitness for the specified case. The supplied checked_by_user flag alone is insufficient.".into(),
                std::slice::from_ref(&evidence.id), serde_json::to_value(evidence)?,
            )?;
        }
        for ground in &case.grounds {
            self.obligation(
                "ground_review", &ground.id, "ground_interpretation",
                format!("Establish the bounded ground from admissible observations and its cited sources: {}. Original status {:?} is authored metadata, not an observation.", ground.statement, ground.status),
                &ground.evidence_ids, serde_json::to_value(ground)?,
            )?;
        }
        for warrant in &case.warrants {
            self.obligation(
                "warrant_review", &warrant.id, "warrant_adequacy",
                format!("Review whether the declared premises and backing adequately support this route's qualified conclusion in the declared scope, including applicability: {}", warrant.applicability),
                &warrant.backing_evidence_ids, serde_json::to_value(warrant)?,
            )?;
        }
        for assumption in &case.assumptions {
            let gate = self.obligation(
                "assumption_review", &assumption.id, "assumption_validation",
                format!("Independently validate the assumption within this case's scope; proposed validation: {}. Original status {:?} is not a validation record.", assumption.validation, assumption.status),
                &assumption.evidence_ids, serde_json::to_value(assumption)?,
            )?;
            let declaration = name("assumption", &assumption.id);
            self.source.push_str(&format!(
                "assumption {declaration} {{\n  statement {}\n  environment {ENVIRONMENT}\n  validate {gate}\n}}\n\n",
                quoted(&assumption.statement)
            ));
            self.record(
                &assumption.id,
                "assumption",
                &declaration,
                "authored_assumption",
            );
        }
        for defeater in &case.defeaters {
            self.obligation(
                "challenge_review", &defeater.id, "challenge_activation",
                format!("Determine from admissible support whether this challenge is active, material where relevant, and applicable to the exact target: {}. Do not activate it solely from its authored kind, basis or status; review its response: {}", defeater.challenge, defeater.response),
                &defeater.evidence_ids, serde_json::to_value(defeater)?,
            )?;
            if unresolved_material(defeater) {
                self.obligation(
                    "resolution_review", &defeater.id, "material_challenge_resolution",
                    format!("Resolve this material challenge before relying on the affected support, with admissible observations and an explicit explanation: {}. Neither the existence nor the hypothetical status of this concern establishes a failure.", defeater.challenge),
                    &defeater.evidence_ids, serde_json::to_value(defeater)?,
                )?;
            }
        }
        for composition in &case.compositions {
            self.obligation(
                "composition_review", &composition.conclusion_id, "composition_adequacy",
                format!("Review the sufficiency of this declared {:?} composition under its explicit selected reliance; selection and agent agreement do not establish sufficiency. {}", composition.relationship, composition.rationale),
                &[], serde_json::to_value(composition)?,
            )?;
        }
        let scope = serde_json::to_string(&case.scope)?;
        for (id, text, qualifier) in [
            (
                "current_claim",
                &case.current_claim.text,
                &case.current_claim.qualifier,
            ),
            (
                "stronger_target",
                &case.stronger_target.text,
                &case.stronger_target.qualifier,
            ),
        ]
        .into_iter()
        .chain(
            case.subclaims
                .iter()
                .map(|claim| (claim.id.as_str(), &claim.text, &claim.qualifier)),
        ) {
            let declaration = name("claim", id);
            self.claim_names.insert(id.into(), declaration.clone());
            self.claim(
                id,
                &declaration,
                &format!("{text}\nQualifier: {qualifier}\nEngineering scope: {scope}"),
                "qualified_canonical_claim",
            );
        }
        for ground in &case.grounds {
            let declaration = name("ground", &ground.id);
            self.claim(
                &ground.id,
                &declaration,
                &format!(
                    "{}\nOriginal ground status: {:?}.\nEngineering scope: {scope}",
                    ground.statement, ground.status
                ),
                "bounded_ground",
            );
            let reasoning = name("ground_reasoning", &ground.id);
            self.reasoning(&ground.id, &reasoning, "Source provenance and a separate reviewed interpretation jointly support only the stated bounded ground. Authored interpretation is not mechanically proved.", &[], "ground_interpretation");
            let mut support = Support::default();
            support
                .evidence
                .extend(evidence_names(&ground.evidence_ids));
            support.evidence.insert(name("ground_review", &ground.id));
            attach_conditions(index, &[&ground.id], &mut support, self.acquisition);
            self.argument(
                &ground.id,
                &name("ground_argument", &ground.id),
                &support,
                &reasoning,
                &declaration,
                "ground_support",
            );
        }
        for warrant in &case.warrants {
            let route = name("route", &warrant.id);
            let (text, qualifier) = index
                .claim(&warrant.conclusion_id)
                .expect("validated conclusion");
            let statement = format!(
                "Candidate support for: {text}\nQualifier: {qualifier}\nRule: {}\nOriginal inference: {:?}.\nApplicability: {}\nEngineering scope: {scope}",
                warrant.rule, warrant.inference, warrant.applicability
            );
            let reasoning = name("warrant", &warrant.id);
            let mut backing = evidence_names(&warrant.backing_evidence_ids);
            backing.push(name("warrant_review", &warrant.id));
            let bound = warrant
                .reasoning_method
                .as_ref()
                .and_then(ReasoningMethodBinding::bound);
            let method_input = if let Some(bound) = bound {
                self.typed_claim(&warrant.id, &route, &statement, bound)?;
                self.method_reasoning(&warrant.id, &reasoning, &format!("{}\nThe declared bounded method tests only the typed formal result. Prose correspondence and scientific applicability remain authored obligations.\nApplicability: {}", warrant.rule, warrant.applicability), bound);
                Some(self.method_input(&warrant.id, bound)?)
            } else {
                self.claim(&warrant.id, &route, &statement, "candidate_route");
                self.reasoning(&warrant.id, &reasoning, &format!("{}\nOriginal inference: {:?}; retained as authored structured/1, not a computational certificate.\nApplicability: {}", warrant.rule, warrant.inference, warrant.applicability), &backing, "authored_warrant");
                None
            };
            let mut support = Support::default();
            for premise in &warrant.premise_ids {
                support.premises.insert(
                    self.claim_names
                        .get(premise)
                        .cloned()
                        .unwrap_or_else(|| name("ground", premise)),
                );
            }
            // An adequacy gate is also explicit source evidence: even a future
            // warrant without premises cannot become an empty EAL argument.
            support.evidence.insert(name("warrant_review", &warrant.id));
            if let Some(bound) = bound {
                support.evidence.extend(backing);
                support.evidence.extend(method_input.iter().cloned());
                support
                    .assumptions
                    .extend(bound.assumption_ids.iter().map(|id| name("assumption", id)));
            } else if let Some(binding @ ReasoningMethodBinding::Unresolved { .. }) =
                &warrant.reasoning_method
            {
                support
                    .evidence
                    .insert(self.unresolved_method(&warrant.id, binding)?);
            }
            attach_conditions(index, &[&warrant.id], &mut support, self.acquisition);
            if let Some(input) = method_input {
                let declaration = name("route_argument", &warrant.id);
                self.source.push_str(&format!(
                    "argument {declaration} = {} via {reasoning} => {route} binding {input}\n\n",
                    support.render()
                ));
                self.record(
                    &warrant.id,
                    "argument",
                    &declaration,
                    "typed_candidate_route_support",
                );
            } else {
                self.argument(
                    &warrant.id,
                    &name("route_argument", &warrant.id),
                    &support,
                    &reasoning,
                    &route,
                    "candidate_route_support",
                );
            }
        }
        for composition in &case.compositions {
            let reasoning = name("composition_reasoning", &composition.conclusion_id);
            self.reasoning(&composition.conclusion_id, &reasoning, &format!("Declared {:?} relationship. {}\nFor alternatives only the selected route is relied upon; other routes remain separately queryable. Cumulative combination sufficiency requires review.", composition.relationship, composition.rationale), &[], "support_composition");
            if composition.relationship == SupportRelationship::AlternativeRoutes
                && composition.selected_warrant_ids.is_empty()
            {
                continue;
            }
            let mut support = Support::default();
            support.premises.extend(
                composition
                    .selected_warrant_ids
                    .iter()
                    .map(|id| name("route", id)),
            );
            support
                .evidence
                .insert(name("composition_review", &composition.conclusion_id));
            attach_conditions(
                index,
                &[&composition.conclusion_id],
                &mut support,
                self.acquisition,
            );
            let conclusion = self.claim_names[&composition.conclusion_id].clone();
            self.argument(
                &composition.conclusion_id,
                &name("composition_argument", &composition.conclusion_id),
                &support,
                &reasoning,
                &conclusion,
                "selected_composition",
            );
        }
        // EAL objections have premises but no native assumption slot. Bridge
        // each critical assumption through an ordinary EAL application so that
        // an objection attacking that assumption also attacks this derivation.
        // These bridge applications have no claim premises, so even cyclic
        // authored condition attachments cannot introduce premise cycles.
        for assumption in case
            .assumptions
            .iter()
            .filter(|assumption| assumption.critical)
        {
            let claim = name("assumption_condition", &assumption.id);
            self.claim(
                &assumption.id,
                &claim,
                &format!("A usable native EAL validation supports this assumption: {}\nEngineering scope: {scope}\nOther authored conditions remain attached to the original applications.", assumption.statement),
                "assumption_condition_claim",
            );
            let reasoning = name("assumption_condition_reasoning", &assumption.id);
            self.reasoning(
                &assumption.id,
                &reasoning,
                "The explicitly reviewed validation and the native EAL assumption are jointly required. This bridge retains assumption attacks when an objection relies on the assumption; it does not establish truth independently.",
                &[],
                "assumption_condition_reasoning",
            );
            let mut support = Support::default();
            support
                .evidence
                .insert(name("assumption_review", &assumption.id));
            support
                .assumptions
                .insert(name("assumption", &assumption.id));
            if let Some(acquisition) = self.acquisition {
                acquisition.attach(
                    &BTreeSet::from([assumption.id.as_str()]),
                    &mut support.evidence,
                );
            }
            self.argument(
                &assumption.id,
                &name("assumption_condition_argument", &assumption.id),
                &support,
                &reasoning,
                &claim,
                "assumption_condition_support",
            );
        }
        for defeater in &case.defeaters {
            let (target_kind, target) = target(index, &self.claim_names, &defeater.target_id);
            let mut support = Support::default();
            support
                .evidence
                .extend(evidence_names(&defeater.evidence_ids));
            support
                .evidence
                .insert(name("challenge_review", &defeater.id));
            // Preserve every attached condition. Native assumptions become
            // premise claims whose derivations retain the assumption attack
            // edges; evidence gates stay evidence gates on the objection.
            attach_conditions(index, &[&defeater.id], &mut support, self.acquisition);
            support.premises.extend(
                support
                    .assumptions
                    .iter()
                    .filter_map(|declaration| assumption_conditions.get(declaration).cloned())
                    .collect::<Vec<_>>(),
            );
            support.assumptions.clear();
            let declaration = name("objection", &defeater.id);
            self.source.push_str(&format!(
                "objection {declaration} = {} -x> {target_kind} {target}\n\n",
                support.render()
            ));
            self.record(
                &defeater.id,
                "objection",
                &declaration,
                "evidence_gated_challenge",
            );
        }
        Ok(())
    }
}

fn unresolved_material(defeater: &crate::domain::Defeater) -> bool {
    defeater.material
        && matches!(
            defeater.status,
            DefeaterStatus::Open | DefeaterStatus::Sustained
        )
}

/// Attach conditions to the exact authored node, including conditions on other
/// attached assumptions or challenges. Premise conditions travel with premises.
fn attach_conditions(
    index: &ArgumentIndex<'_>,
    ids: &[&str],
    support: &mut Support,
    acquisition: Option<&AcquisitionProjection>,
) {
    let closure = index.attachment_dependency(ids);
    for id in &closure {
        for assumption in index.assumptions_for(id) {
            if assumption.critical {
                support
                    .assumptions
                    .insert(name("assumption", &assumption.id));
            }
        }
        for defeater in index.defeaters_for(id) {
            if unresolved_material(defeater) {
                support
                    .evidence
                    .insert(name("resolution_review", &defeater.id));
            }
        }
    }
    if let Some(acquisition) = acquisition {
        acquisition.attach(&closure.into_iter().collect(), &mut support.evidence);
    }
}

fn target(
    index: &ArgumentIndex<'_>,
    claims: &BTreeMap<String, String>,
    id: &str,
) -> (&'static str, String) {
    if let Some(declaration) = claims.get(id) {
        return ("claim", declaration.clone());
    }
    match index.node(id).expect("admitted objection target") {
        ArgumentNode::Root(_) | ArgumentNode::Claim(_) | ArgumentNode::Ground(_) => {
            ("claim", name("ground", id))
        }
        ArgumentNode::Warrant(_) => ("reasoning", name("warrant", id)),
        ArgumentNode::Assumption(_) => ("assumption", name("assumption", id)),
        ArgumentNode::Defeater(_) => ("objection", name("objection", id)),
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;

    fn fixtures() -> (Request, ArgumentCase) {
        (
            serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
            serde_json::from_str(include_str!("../examples/case.json")).unwrap(),
        )
    }

    #[test]
    fn current_identity_survives_unrelated_producer_version_changes() {
        let (request, case) = fixtures();
        let first = export_projection_for_identity(
            &request,
            &case,
            None,
            ExportIdentity::Current {
                exporter_version: "0.8.0",
            },
        )
        .unwrap();
        let second = export_projection_for_identity(
            &request,
            &case,
            None,
            ExportIdentity::Current {
                exporter_version: "9.9.9",
            },
        )
        .unwrap();
        assert_ne!(
            first.bundle.exporter_version,
            second.bundle.exporter_version
        );
        assert_eq!(first.bundle.revision, second.bundle.revision);
        assert_eq!(first.bundle.source, second.bundle.source);
        assert_eq!(first.bundle.context, second.bundle.context);
        assert_eq!(
            serde_json::to_value(&first.bundle).unwrap(),
            serde_json::to_value(reconstruct_projection(&first.bundle, None).unwrap().bundle)
                .unwrap()
        );
        assert_eq!(first.bundle.schema_version, SCHEMA_VERSION);
        assert_eq!(
            first.bundle.semantics_version.as_deref(),
            Some(SEMANTICS_VERSION)
        );
        assert!(first.review_requests.iter().all(|request| {
            request.input["schema"] == SCHEMA_VERSION
                && request.input["case_digest"] == first.bundle.revision
        }));
    }

    #[test]
    fn legacy_reconstruction_preserves_original_package_bound_identity_and_bytes() {
        let (request, case) = fixtures();
        let projection = export_projection_for_identity(
            &request,
            &case,
            None,
            ExportIdentity::Legacy {
                exporter_version: "0.7.1",
            },
        )
        .unwrap();
        // The legacy digest independently follows the original public encoding.
        let identity =
            serde_json::to_vec(&("engarg.eal-export/1", "0.7.1", &request, &case)).unwrap();
        let expected_digest: String = Sha256::digest(identity)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(projection.bundle.revision, expected_digest);
        assert!(projection.bundle.semantics_version.is_none());
        assert!(projection.bundle.collector_catalogue.is_none());
        let value = serde_json::to_value(&projection.bundle).unwrap();
        assert!(value.get("semantics_version").is_none());
        assert!(value.get("collector_catalogue").is_none());
        let reconstructed = reconstruct_projection(&projection.bundle, None).unwrap();
        assert_eq!(value, serde_json::to_value(&reconstructed.bundle).unwrap());
        assert_eq!(projection.bundle.source, reconstructed.bundle.source);
        assert!(projection.review_requests.iter().all(|request| {
            request.input["schema"] == LEGACY_SCHEMA_VERSION
                && request.input["case_digest"] == expected_digest
        }));
    }

    #[test]
    fn unknown_or_mixed_identity_contracts_are_rejected() {
        let (request, case) = fixtures();
        let mut bundle = export(&request, &case).unwrap();
        bundle.semantics_version = Some("engarg.eal-projection/999".into());
        assert!(reconstruct_projection(&bundle, None).is_err());
        bundle.schema_version = LEGACY_SCHEMA_VERSION.into();
        assert!(reconstruct_projection(&bundle, None).is_err());
        bundle.semantics_version = None;
        bundle.collector_catalogue = Some(json!({"schema_version":"unknown"}));
        assert!(reconstruct_projection(&bundle, None).is_err());
        bundle.collector_catalogue = None;
        bundle.schema_version = "engarg.eal-export/999".into();
        assert!(reconstruct_projection(&bundle, None).is_err());
    }
}
