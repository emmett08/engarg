//! Deterministic, conservative translation into authored EAL/4 arguments.
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
};

const SCHEMA_VERSION: &str = "engarg.eal-export/4";
const SEMANTICS_VERSION: &str = "engarg.eal-projection/4";
use crate::export_policy::ExportPolicy;
const ENVIRONMENT: &str = "engarg_case";
const REVIEW_TOOL: &str = "engarg_review_contract";

/// A complete, loss-preserving bundle around the executable EAL/4 projection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EalExport {
    pub schema_version: String,
    pub exporter_version: String,
    /// Projection semantics are versioned independently of the producer package.
    pub semantics_version: String,
    pub export_policy: ExportPolicy,
    pub revision: String,
    pub target_language: String,
    pub review_max_age_seconds: u64,
    pub source: String,
    pub context: Value,
    /// Original claim identifiers to queryable EAL claim identifiers.
    pub claim_names: BTreeMap<String, String>,
    pub source_map: BTreeMap<String, EalSourceMapping>,
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

/// Reviews use the authoritative acquisition request without another wire model.
pub use eal_collector_contracts::AcquisitionRequest as ReviewRequest;

/// Structured export products share one construction pass and one identity.
pub struct ExportProjection {
    pub bundle: EalExport,
    pub review_requests: Vec<ReviewRequest>,
}

/// Emit the argument over already compiled canonical obligations. There is no
/// authored-only compiler or automatic second layer of review contracts.
pub(crate) fn export_executable(
    request: &Request,
    case: &ArgumentCase,
    policy: &ExportPolicy,
    evidence: &crate::support_export::compiler::CompiledEvidence,
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
        SCHEMA_VERSION,
        SEMANTICS_VERSION,
        request,
        case,
        policy,
        "EAL/4",
    ))?;
    identity_bytes.extend(evidence.declarations.as_bytes());
    let revision: String = Sha256::digest(identity_bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let mut builder = Exporter::new(&revision, evidence);
    builder.write(admitted.index())?;
    let mut limitations = vec![
        "The EAL/4 source contains all acquisition, derivation, acceptance and reasoning semantics. TOML selects actual tools; no execution-plan sidecar is needed.".into(),
        "Authored method queries are programme data. Empirical premise values and independent judgements are acquired observations; interpretation and scope remain explicit.".into(),
        "Shared source observations retain common provenance. An unsupported claim does not establish falsity.".into(),
    ];
    limitations.extend(evidence.pending.clone());
    let context = json!({"engarg_export_digest": revision});
    let review_requests = evidence
        .review_requests
        .iter()
        .cloned()
        .map(|mut request| {
            request.context = context.clone();
            request
        })
        .collect();
    Ok(ExportProjection {
        bundle: EalExport {
            schema_version: SCHEMA_VERSION.into(),
            exporter_version: env!("CARGO_PKG_VERSION").into(),
            semantics_version: SEMANTICS_VERSION.into(),
            export_policy: policy.clone(),
            revision: revision.clone(),
            target_language: "EAL/4".into(),
            review_max_age_seconds: policy.review_max_age_seconds,
            source: builder.source,
            context,
            claim_names: builder.claim_names,
            source_map: builder.source_map,
            limitations,
            original_request: request.clone(),
            original_case: case.clone(),
        },
        review_requests,
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
    compiled: &'a crate::support_export::compiler::CompiledEvidence,
}

impl<'a> Exporter<'a> {
    fn new(
        revision: &'a str,
        compiled: &'a crate::support_export::compiler::CompiledEvidence,
    ) -> Self {
        Self {
            revision,
            source: String::new(),
            source_map: BTreeMap::new(),
            claim_names: BTreeMap::new(),
            compiled,
        }
    }

    fn gates(&self, prefix: &str, id: &str) -> Vec<String> {
        let targets = match prefix {
            "challenge_review" => &self.compiled.challenge_activation,
            "resolution_review" => &self.compiled.challenge_resolution,
            _ => &self.compiled.targets,
        };
        targets.get(id).cloned().unwrap_or_default()
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

    fn method_input(&self, warrant_id: &str) -> Option<String> {
        self.compiled.method_inputs.get(warrant_id).cloned()
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
        if bound.method.kind() == Some(ReasoningMethodKind::Deductive) {
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
            "// Missing formal model or evidence keeps this route unavailable.\nevidence {declaration} {{\n  kind unavailable_method\n  environment {ENVIRONMENT}\n  max_age 0\n  input {}\n  derive object(\"method_available\", pending.method_available)\n  require method_available == true\n}}\n\n",
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

    fn write(&mut self, index: &ArgumentIndex<'_>) -> Result<(), Error> {
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
            "language \"EAL/4\"\n\n// Authored EngArg projection. Review contracts are not observations.\n// Keep the JSON export bundle for the complete canonical source and source map.\n\nenvironment {ENVIRONMENT} {{\n  require engarg_export_digest == {}\n}}\n\n",
            quoted(self.revision)
        ));
        if !self.compiled.review_requests.is_empty() {
            self.source
                .push_str(&format!("tool {REVIEW_TOOL} {{\n  version \"1\"\n}}\n\n"));
        }
        {
            let compiled = self.compiled;
            self.source.push_str(&compiled.declarations);
            for (target, declarations) in compiled
                .targets
                .iter()
                .chain(&compiled.challenge_activation)
                .chain(&compiled.challenge_resolution)
            {
                for declaration in declarations {
                    self.record(
                        target,
                        "evidence",
                        declaration,
                        "canonical_obligation_support",
                    );
                }
            }
            for (requirement, declaration) in &compiled.requirements {
                self.record(
                    &format!("requirement:{requirement}"),
                    "evidence",
                    declaration,
                    "canonical_obligation",
                );
            }
        }
        for assumption in &case.assumptions {
            let gate = self
                .gates("assumption_review", &assumption.id)
                .first()
                .cloned()
                .ok_or_else(|| {
                    Error::InvalidInput(format!(
                        "Missing compiled assumption condition {}",
                        assumption.id
                    ))
                })?;
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
            self.reasoning(&ground.id, &reasoning, "Canonical acquisition and acceptance conditions support only the stated bounded ground. Authored interpretation is not mechanically proved.", &[], "ground_interpretation");
            let mut support = Support::default();
            support
                .evidence
                .extend(self.gates("ground_review", &ground.id));
            attach_conditions(index, &[&ground.id], &mut support, self.compiled);
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
            let backing = self.gates("warrant_review", &warrant.id);
            let bound = warrant
                .reasoning_method
                .as_ref()
                .and_then(ReasoningMethodBinding::bound);
            let method_input = if let Some(bound) = bound {
                self.typed_claim(&warrant.id, &route, &statement, bound)?;
                self.method_reasoning(&warrant.id, &reasoning, &format!("{}\nThe declared bounded method tests only the typed formal result. Prose correspondence and scientific applicability remain authored obligations.\nApplicability: {}", warrant.rule, warrant.applicability), bound);
                self.method_input(&warrant.id)
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
            support
                .evidence
                .extend(self.gates("warrant_review", &warrant.id));
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
            attach_conditions(index, &[&warrant.id], &mut support, self.compiled);
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
                .extend(self.gates("composition_review", &composition.conclusion_id));
            attach_conditions(
                index,
                &[&composition.conclusion_id],
                &mut support,
                self.compiled,
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
                "The explicit canonical validation and the native EAL assumption are jointly required. This bridge retains assumption attacks when an objection relies on the assumption; it does not establish truth independently.",
                &[],
                "assumption_condition_reasoning",
            );
            let mut support = Support::default();
            support
                .evidence
                .extend(self.gates("assumption_review", &assumption.id));
            support
                .assumptions
                .insert(name("assumption", &assumption.id));
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
                .extend(self.gates("challenge_review", &defeater.id));
            // Preserve every attached condition. Native assumptions become
            // premise claims whose derivations retain the assumption attack
            // edges; evidence gates stay evidence gates on the objection.
            attach_conditions(index, &[&defeater.id], &mut support, self.compiled);
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
    compiled: &crate::support_export::compiler::CompiledEvidence,
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
                support.evidence.extend(
                    compiled
                        .challenge_resolution
                        .get(&defeater.id)
                        .cloned()
                        .unwrap_or_default(),
                );
            }
        }
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
