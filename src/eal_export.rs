//! Deterministic, conservative translation into authored EAL/3 arguments.
//!
//! Export creates review contracts, never observations. Supplying an excerpt or
//! setting `checked_by_user` does not discharge any contract. EAL's structured
//! method preserves an authored argument; it does not prove its prose sufficient.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    Error,
    domain::{
        ArgumentCase, DefeaterStatus, Request, SupportRelationship, validate_case, validate_request,
    },
    eal_acquisition::AcquisitionProjection,
};

const SCHEMA_VERSION: &str = "engarg.eal-export/1";
const REVIEW_MAX_AGE_SECONDS: u64 = 86_400;
const ENVIRONMENT: &str = "engarg_case";
const REVIEW_TOOL: &str = "engarg_review_contract";

/// A complete, loss-preserving bundle around the executable EAL/3 projection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EalExport {
    pub schema_version: String,
    pub exporter_version: String,
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

/// Validate the canonical graph, then project its explicit selected reliance.
///
/// Invalid source graphs are rejected rather than repaired by the exporter.
/// All generated identifiers use category prefixes and injective UTF-8 hex.
/// Missing review observations leave the corresponding EAL support unavailable.
pub fn export(request: &Request, case: &ArgumentCase) -> Result<EalExport, Error> {
    export_projection(request, case, None)
}

pub(crate) fn export_with_acquisitions(
    request: &Request,
    case: &ArgumentCase,
    acquisition: &AcquisitionProjection,
) -> Result<EalExport, Error> {
    export_projection(request, case, Some(acquisition))
}

fn export_projection(
    request: &Request,
    case: &ArgumentCase,
    acquisition: Option<&AcquisitionProjection>,
) -> Result<EalExport, Error> {
    validate_request(request).map_err(|errors| Error::InvalidInput(errors.join("; ")))?;
    validate_case(request, case).map_err(|errors| Error::InvalidOutput(errors.join("; ")))?;
    let mut identity =
        serde_json::to_vec(&(SCHEMA_VERSION, env!("CARGO_PKG_VERSION"), request, case))?;
    if let Some(acquisition) = acquisition {
        identity.extend(serde_json::to_vec(&acquisition.plans)?);
    }
    let revision: String = Sha256::digest(identity)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let mut builder = Exporter::new(&revision, acquisition);
    builder.write(request, case)?;
    let mut bundle = EalExport {
        schema_version: SCHEMA_VERSION.into(),
        exporter_version: env!("CARGO_PKG_VERSION").into(),
        revision: revision.clone(),
        target_language: "EAL/3".into(),
        review_max_age_seconds: REVIEW_MAX_AGE_SECONDS,
        source: builder.source,
        context: json!({"engarg_export_digest": revision}),
        claim_names: builder.claim_names,
        source_map: builder.source_map,
        obligations: builder.obligations,
        limitations: vec![
            "The EAL source is a structural projection of authored reasoning using structured/1; original inference labels are retained, not mechanically proved or promoted to computational methods.".into(),
            "No observation records are generated. Supplied excerpts, model statuses, and checked_by_user never discharge a review obligation. Missing observations mean unavailable support, not false claims.".into(),
            "engarg_review_contract is a declared review interface, not an installed collector. An operator must configure a trusted collector or provide compatible independently retained EAL observations before review evidence can be available.".into(),
            format!("Review observations have an explicit export-policy max_age of {REVIEW_MAX_AGE_SECONDS} seconds; this is not an inferred expiry of the original source evidence."),
            "Jointly required and cumulative routes are combined with AND. Cumulative weighting or statistical sufficiency remains an explicit composition review obligation. Only the selected alternative connects to its original conclusion; unselected route claims remain separately queryable.".into(),
            "Critical assumptions and open or sustained material challenges gate affected support. Noncritical assumptions remain declared and reviewable without adding mandatory dependencies absent from the original reliance policy.".into(),
            "Each challenge is an evidence-gated EAL objection at its original target category. Challenge kind, basis, materiality, response and original status are preserved in the review input and original_case; EAL does not encode these labels as an automatic proof or dismissal.".into(),
            "Free-text scope and qualifiers remain beside claim statements; they are not invented typed quantities, formal contraries, strict rules, rankings or preferences. Plans, prose, alternatives and decision limits are preserved exactly in original_request and original_case.".into(),
            "The context digest binds this projection to the canonical case revision. It identifies that authored scope; it does not establish that the real environment satisfies its conditions or authorize an engineering decision.".into(),
        ],
        original_request: request.clone(),
        original_case: case.clone(),
    };
    if acquisition.is_some() {
        bundle.limitations.push(
            "Acquisition availability follows the selected adapter contract. EngArg adapters use collected while native fleet collectors use cluster_count and native GitHub uses found. The existing independent review obligations still determine interpretation and sufficiency. Custom collectors remain command implementation stubs and unresolved inputs retain explicit command configuration templates. Acquisition observations use an explicit 86400-second export-policy age limit rather than an inferred engineering validity period.".into(),
        );
    }
    Ok(bundle)
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
    source: String,
    source_map: BTreeMap<String, EalSourceMapping>,
    claim_names: BTreeMap<String, String>,
    obligations: Vec<ReviewObligation>,
    acquisition: Option<&'a AcquisitionProjection>,
}

impl<'a> Exporter<'a> {
    fn new(revision: &'a str, acquisition: Option<&'a AcquisitionProjection>) -> Self {
        Self {
            revision,
            source: String::new(),
            source_map: BTreeMap::new(),
            claim_names: BTreeMap::new(),
            obligations: Vec::new(),
            acquisition,
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
            "schema": SCHEMA_VERSION,
            "case_digest": self.revision,
            "obligation": obligation,
            "authored_detail": detail,
            "acquisition": "Explicit independent review required; no observation is supplied by this export."
        });
        if let Some(acquisition) = self.acquisition {
            input["required_acquisitions"] = json!(acquisition.review_inputs(id));
        }
        self.source.push_str(&format!(
            "evidence {declaration} {{\n  tool {REVIEW_TOOL}\n  kind review\n  environment {ENVIRONMENT}\n  max_age {REVIEW_MAX_AGE_SECONDS}\n  input {}\n  require accepted == true\n}}\n\n",
            serde_json::to_string(&input)?
        ));
        self.record(id, "evidence", &declaration, kind);
        self.obligations.push(obligation);
        Ok(declaration)
    }

    fn write(&mut self, request: &Request, case: &ArgumentCase) -> Result<(), Error> {
        let plan_comment = self
            .acquisition
            .map(AcquisitionProjection::plan_comment)
            .unwrap_or_default();
        self.source.push_str(&format!(
            "language \"EAL/3\"\n\n// Authored EngArg projection. Review contracts are not observations.\n// Keep the JSON export bundle for the complete canonical source and source map.\n{plan_comment}\nenvironment {ENVIRONMENT} {{\n  require engarg_export_digest == {}\n}}\n\ntool {REVIEW_TOOL} {{\n  version \"1\"\n}}\n\n",
            quoted(self.revision)
        ));
        if let Some(acquisition) = self.acquisition {
            self.source
                .push_str(&acquisition.declarations(self.revision, ENVIRONMENT)?);
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
            attach_conditions(case, &[&ground.id], &mut support, self.acquisition);
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
            let (text, qualifier) = match warrant.conclusion_id.as_str() {
                "current_claim" => (&case.current_claim.text, &case.current_claim.qualifier),
                "stronger_target" => (&case.stronger_target.text, &case.stronger_target.qualifier),
                id => {
                    let claim = case
                        .subclaims
                        .iter()
                        .find(|claim| claim.id == id)
                        .expect("validated conclusion");
                    (&claim.text, &claim.qualifier)
                }
            };
            self.claim(&warrant.id, &route, &format!("Candidate support for: {text}\nQualifier: {qualifier}\nRule: {}\nOriginal inference: {:?}.\nApplicability: {}\nEngineering scope: {scope}", warrant.rule, warrant.inference, warrant.applicability), "candidate_route");
            let reasoning = name("warrant", &warrant.id);
            let mut backing = evidence_names(&warrant.backing_evidence_ids);
            backing.push(name("warrant_review", &warrant.id));
            self.reasoning(&warrant.id, &reasoning, &format!("{}\nOriginal inference: {:?}; retained as authored structured/1, not a computational certificate.\nApplicability: {}", warrant.rule, warrant.inference, warrant.applicability), &backing, "authored_warrant");
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
            attach_conditions(case, &[&warrant.id], &mut support, self.acquisition);
            self.argument(
                &warrant.id,
                &name("route_argument", &warrant.id),
                &support,
                &reasoning,
                &route,
                "candidate_route_support",
            );
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
                case,
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
            let (target_kind, target) = target(case, &self.claim_names, &defeater.target_id);
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
            attach_conditions(case, &[&defeater.id], &mut support, self.acquisition);
            support.premises.extend(
                support
                    .assumptions
                    .iter()
                    .filter_map(|declaration| {
                        case.assumptions
                            .iter()
                            .find(|assumption| name("assumption", &assumption.id) == *declaration)
                            .map(|assumption| name("assumption_condition", &assumption.id))
                    })
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
    case: &ArgumentCase,
    ids: &[&str],
    support: &mut Support,
    acquisition: Option<&AcquisitionProjection>,
) {
    let mut closure: BTreeSet<&str> = ids.iter().copied().collect();
    loop {
        let before = closure.len();
        for assumption in &case.assumptions {
            if assumption
                .applies_to
                .iter()
                .any(|id| closure.contains(id.as_str()))
            {
                closure.insert(&assumption.id);
                if assumption.critical {
                    support
                        .assumptions
                        .insert(name("assumption", &assumption.id));
                }
            }
        }
        for defeater in &case.defeaters {
            if closure.contains(defeater.target_id.as_str()) {
                closure.insert(&defeater.id);
                if unresolved_material(defeater) {
                    support
                        .evidence
                        .insert(name("resolution_review", &defeater.id));
                }
            }
        }
        if before == closure.len() {
            if let Some(acquisition) = acquisition {
                acquisition.attach(&closure, &mut support.evidence);
            }
            return;
        }
    }
}

fn target(
    case: &ArgumentCase,
    claims: &BTreeMap<String, String>,
    id: &str,
) -> (&'static str, String) {
    if let Some(declaration) = claims.get(id) {
        ("claim", declaration.clone())
    } else if case.grounds.iter().any(|ground| ground.id == id) {
        ("claim", name("ground", id))
    } else if case.warrants.iter().any(|warrant| warrant.id == id) {
        ("reasoning", name("warrant", id))
    } else if case
        .assumptions
        .iter()
        .any(|assumption| assumption.id == id)
    {
        ("assumption", name("assumption", id))
    } else {
        // validate_case checked the complete node category and self-target rule.
        ("objection", name("objection", id))
    }
}
