//! Extended Toulmin records and structural checks.
//!
//! These checks establish reference integrity and conservative reporting rules.
//! They do not determine whether evidence is authentic or an inference is sound.

use std::collections::{HashMap, HashSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub claim: String,
    /// Optional information supplied by the user; absent context remains unknown.
    #[serde(default)]
    pub context: String,
    /// Optional supplied evidence, never observations invented from a claim.
    #[serde(default)]
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub id: String,
    pub title: String,
    pub source: String,
    pub excerpt: String,
    pub kind: String,
    /// A user assertion about checking, never an assertion made by this tool.
    pub checked_by_user: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArgumentCase {
    pub original_claim: String,
    pub current_claim: ClaimVersion,
    pub stronger_target: ClaimVersion,
    pub status: CaseStatus,
    pub scope: EngineeringScope,
    pub subclaims: Vec<ClaimNode>,
    pub grounds: Vec<Ground>,
    pub warrants: Vec<Warrant>,
    pub compositions: Vec<SupportComposition>,
    pub assumptions: Vec<Assumption>,
    pub defeaters: Vec<Defeater>,
    pub alternatives: Vec<Alternative>,
    pub evidence_plan: Vec<EvidenceNeed>,
    pub reading: Vec<NarrativeSection>,
    pub residual_risks: Vec<String>,
    pub decision_limits: Vec<String>,
    pub adjudication: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClaimVersion {
    pub text: String,
    pub qualifier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClaimNode {
    pub id: String,
    pub text: String,
    pub qualifier: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CaseStatus {
    Unsupported,
    Conditional,
    ProvisionallySupported,
    Refuted,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EngineeringScope {
    pub system_boundary: String,
    pub configuration: String,
    pub operating_conditions: String,
    pub units: String,
    pub time_horizon: String,
    pub consequences: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Ground {
    pub id: String,
    pub statement: String,
    pub evidence_ids: Vec<String>,
    pub status: GroundStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GroundStatus {
    Reported,
    Assumed,
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Warrant {
    pub id: String,
    pub rule: String,
    pub conclusion_id: String,
    pub premise_ids: Vec<String>,
    pub backing_evidence_ids: Vec<String>,
    pub inference: InferenceKind,
    pub applicability: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SupportComposition {
    pub conclusion_id: String,
    pub relationship: SupportRelationship,
    /// An explicit model judgement about which declared routes are relied upon.
    /// A selected alternative is claimed individually adequate under the common
    /// scope; selection itself supplies no certificate of that adequacy.
    pub selected_warrant_ids: Vec<String>,
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SupportRelationship {
    JointlyRequired,
    AlternativeRoutes,
    Cumulative,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NarrativeSection {
    pub claim_id: String,
    pub title: String,
    pub paragraphs: Vec<ReasoningParagraph>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReasoningParagraph {
    pub purpose: ParagraphPurpose,
    pub text: String,
    pub reference_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ParagraphPurpose {
    Answer,
    Finding,
    Connection,
    Complication,
    Conclusion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InferenceKind {
    Deductive,
    Inductive,
    Abductive,
    Causal,
    Analogical,
    Practical,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Assumption {
    pub id: String,
    pub statement: String,
    pub critical: bool,
    pub status: AssumptionStatus,
    pub evidence_ids: Vec<String>,
    pub applies_to: Vec<String>,
    /// A proposed check or an explicitly described existing check.
    pub validation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssumptionStatus {
    Supported,
    Untested,
    Contradicted,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Defeater {
    pub id: String,
    pub kind: DefeaterKind,
    pub target_id: String,
    pub challenge: String,
    pub status: DefeaterStatus,
    pub material: bool,
    pub basis: DefeaterBasis,
    pub response: String,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DefeaterKind {
    Undermining,
    Rebutting,
    Undercutting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DefeaterStatus {
    Open,
    Sustained,
    Refuted,
    InapplicableAfterRevision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DefeaterBasis {
    Observed,
    SupportedPossibility,
    Hypothetical,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Alternative {
    pub explanation: String,
    pub distinguishing_test: String,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvidenceNeed {
    pub id: String,
    pub target_id: String,
    pub question: String,
    pub method: String,
    pub measurement: String,
    pub comparator: String,
    pub acceptance_criterion: String,
    pub if_failed: String,
    /// An existing claim ID whose support would be strengthened by this check.
    pub enables_claim: String,
    /// 1 is highest priority, 5 is lowest.
    pub priority: u8,
    pub cost: String,
}

pub fn case_schema() -> serde_json::Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(ArgumentCase))
        .expect("a statically derived schema is serialisable");
    if let Some(object) = schema.as_object_mut() {
        object.remove("$schema");
    }
    schema
}

fn nonempty(errors: &mut Vec<String>, label: &str, text: &str) {
    if text.trim().is_empty() {
        errors.push(format!("{label} must contain text"));
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn register_id(errors: &mut Vec<String>, ids: &mut HashSet<String>, id: &str) {
    if !valid_id(id) {
        errors.push(format!(
            "invalid identifier {id:?}: use 1–64 ASCII letters, digits, '_' or '-'"
        ));
    }
    if !ids.insert(id.to_owned()) {
        errors.push(format!("duplicate or reserved identifier {id:?}"));
    }
}

fn finish(mut errors: Vec<String>) -> Result<(), Vec<String>> {
    errors.sort_unstable();
    errors.dedup();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn validate_request(request: &Request) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    nonempty(&mut errors, "claim", &request.claim);
    let mut ids = HashSet::from(["current_claim".to_owned(), "stronger_target".to_owned()]);
    for evidence in &request.evidence {
        register_id(&mut errors, &mut ids, &evidence.id);
        for (name, value) in [
            ("title", &evidence.title),
            ("source", &evidence.source),
            ("excerpt", &evidence.excerpt),
            ("kind", &evidence.kind),
        ] {
            nonempty(
                &mut errors,
                &format!("evidence {} {name}", evidence.id),
                value,
            );
        }
    }
    finish(errors)
}

fn evidence_references(
    errors: &mut Vec<String>,
    label: &str,
    references: &[String],
    known: &HashMap<&str, &Evidence>,
) {
    let mut seen = HashSet::new();
    for id in references {
        if !known.contains_key(id.as_str()) {
            errors.push(format!("{label} references unknown evidence {id:?}"));
        }
        if !seen.insert(id) {
            errors.push(format!("{label} repeats evidence {id:?}"));
        }
    }
}

/// Follow declared support from a conclusion to its warrants and premises.
/// Includes every candidate route, including candidates that are not selected.
fn support_closure(case: &ArgumentCase, root: &str) -> HashSet<String> {
    let mut by_conclusion: HashMap<&str, Vec<&Warrant>> = HashMap::new();
    for warrant in &case.warrants {
        by_conclusion
            .entry(&warrant.conclusion_id)
            .or_default()
            .push(warrant);
    }
    let mut closure = HashSet::new();
    let mut pending = vec![root.to_owned()];
    while let Some(id) = pending.pop() {
        if !closure.insert(id.clone()) {
            continue;
        }
        if let Some(warrants) = by_conclusion.get(id.as_str()) {
            for warrant in warrants {
                closure.insert(warrant.id.clone());
                pending.extend(warrant.premise_ids.iter().cloned());
            }
        }
    }
    closure
}

/// Follow only explicitly selected support routes. Selection is an assessment
/// supplied by the model, never a proof of a candidate route's adequacy.
fn relied_support_closure(case: &ArgumentCase, root: &str) -> HashSet<String> {
    let warrants: HashMap<&str, &Warrant> =
        case.warrants.iter().map(|w| (w.id.as_str(), w)).collect();
    let selections: HashMap<&str, &SupportComposition> = case
        .compositions
        .iter()
        .map(|c| (c.conclusion_id.as_str(), c))
        .collect();
    let mut closure = HashSet::new();
    let mut pending = vec![root.to_owned()];
    while let Some(id) = pending.pop() {
        if !closure.insert(id.clone()) {
            continue;
        }
        if let Some(composition) = selections.get(id.as_str()) {
            for warrant_id in &composition.selected_warrant_ids {
                if let Some(warrant) = warrants.get(warrant_id.as_str()) {
                    // Composition validation separately rejects cross-claim selection.
                    if warrant.conclusion_id == id {
                        closure.insert(warrant.id.clone());
                        pending.extend(warrant.premise_ids.iter().cloned());
                    }
                }
            }
        }
    }
    closure
}

/// Attach assumptions and challenges to the support they qualify or attack.
fn extended_closure(case: &ArgumentCase, mut closure: HashSet<String>) -> HashSet<String> {
    loop {
        let before = closure.len();
        for assumption in &case.assumptions {
            if assumption.applies_to.iter().any(|id| closure.contains(id)) {
                closure.insert(assumption.id.clone());
            }
        }
        for defeater in &case.defeaters {
            if closure.contains(&defeater.target_id) {
                closure.insert(defeater.id.clone());
            }
        }
        if closure.len() == before {
            return closure;
        }
    }
}

/// Canonical dependencies for the explicitly relied-upon argument routes,
/// including assumptions and challenges attached to those routes.
pub fn relied_dependency_closure(case: &ArgumentCase, claim_id: &str) -> HashSet<String> {
    extended_closure(case, relied_support_closure(case, claim_id))
}

fn has_support_cycle(case: &ArgumentCase, claims: &HashSet<&str>) -> bool {
    // Kahn's algorithm avoids recursion on model-generated graph depth.
    let mut indegree: HashMap<&str, usize> = claims.iter().map(|&id| (id, 0)).collect();
    let mut edges: HashMap<&str, Vec<&str>> = HashMap::new();
    for warrant in &case.warrants {
        if !claims.contains(warrant.conclusion_id.as_str()) {
            continue;
        }
        for premise in &warrant.premise_ids {
            if claims.contains(premise.as_str()) {
                edges
                    .entry(&warrant.conclusion_id)
                    .or_default()
                    .push(premise);
                *indegree.get_mut(premise.as_str()).expect("known claim") += 1;
            }
        }
    }
    let mut ready: Vec<&str> = indegree
        .iter()
        .filter_map(|(&id, &degree)| (degree == 0).then_some(id))
        .collect();
    let mut removed = 0;
    while let Some(id) = ready.pop() {
        removed += 1;
        if let Some(children) = edges.get(id) {
            for child in children {
                let degree = indegree.get_mut(child).expect("known claim");
                *degree -= 1;
                if *degree == 0 {
                    ready.push(child);
                }
            }
        }
    }
    removed != claims.len()
}

fn validate_compositions(case: &ArgumentCase, claim_ids: &HashSet<&str>, errors: &mut Vec<String>) {
    let mut attached: HashMap<&str, HashSet<&str>> = HashMap::new();
    for warrant in &case.warrants {
        attached
            .entry(&warrant.conclusion_id)
            .or_default()
            .insert(&warrant.id);
    }
    let mut seen = HashSet::new();
    for composition in &case.compositions {
        let conclusion = composition.conclusion_id.as_str();
        if !claim_ids.contains(conclusion) {
            errors.push(format!("composition has unknown conclusion {conclusion:?}"));
        }
        if !seen.insert(conclusion) {
            errors.push(format!("duplicate composition for claim {conclusion:?}"));
        }
        nonempty(
            errors,
            &format!("composition {conclusion} rationale"),
            &composition.rationale,
        );
        let Some(expected) = attached.get(conclusion) else {
            errors.push(format!(
                "composition {conclusion:?} has no attached warrants"
            ));
            continue;
        };
        let mut selected = HashSet::new();
        for warrant_id in &composition.selected_warrant_ids {
            if !selected.insert(warrant_id.as_str()) {
                errors.push(format!(
                    "composition {conclusion:?} repeats selected warrant {warrant_id:?}"
                ));
            }
            if !expected.contains(warrant_id.as_str()) {
                errors.push(format!("composition {conclusion:?} selects an unknown or wrong-claim warrant {warrant_id:?}"));
            }
        }
        match composition.relationship {
            SupportRelationship::JointlyRequired | SupportRelationship::Cumulative => {
                if &selected != expected {
                    errors.push(format!("composition {conclusion:?} must select all attached warrants for jointly_required or cumulative support"));
                }
            }
            SupportRelationship::AlternativeRoutes => {
                if composition.selected_warrant_ids.len() > 1 {
                    errors.push(format!("alternative_routes composition {conclusion:?} must select zero or exactly one warrant"));
                }
            }
        }
    }
    for conclusion in attached.keys() {
        if !seen.contains(conclusion) {
            errors.push(format!(
                "claim {conclusion:?} needs exactly one support composition"
            ));
        }
    }
}

/// Narrative references provide traceability, not semantic equivalence between
/// the graph and the prose. Both the prose and its interpretation need review.
fn validate_reading(
    case: &ArgumentCase,
    claim_ids: &HashSet<&str>,
    canonical_ids: &HashSet<String>,
    errors: &mut Vec<String>,
) {
    let mut seen = HashSet::new();
    for section in &case.reading {
        let claim = section.claim_id.as_str();
        if !claim_ids.contains(claim) {
            errors.push(format!(
                "reading section references unknown claim {claim:?}"
            ));
        }
        if !seen.insert(claim) {
            errors.push(format!("duplicate reading section for claim {claim:?}"));
        }
        nonempty(
            errors,
            &format!("reading section {claim} title"),
            &section.title,
        );
        if section.paragraphs.len() < 2 {
            errors.push(format!(
                "reading section {claim:?} needs at least an answer and conclusion paragraph"
            ));
        }
        if section
            .paragraphs
            .first()
            .is_none_or(|p| p.purpose != ParagraphPurpose::Answer)
        {
            errors.push(format!(
                "reading section {claim:?} must start with an answer paragraph"
            ));
        }
        if section
            .paragraphs
            .last()
            .is_none_or(|p| p.purpose != ParagraphPurpose::Conclusion)
        {
            errors.push(format!(
                "reading section {claim:?} must end with a conclusion paragraph"
            ));
        }
        if section
            .paragraphs
            .first()
            .is_none_or(|p| !p.reference_ids.contains(&section.claim_id))
        {
            errors.push(format!(
                "reading section {claim:?} first answer must reference its own claim ID"
            ));
        }
        if claim == "current_claim" {
            let purposes: HashSet<ParagraphPurpose> =
                section.paragraphs.iter().map(|p| p.purpose).collect();
            if [
                ParagraphPurpose::Answer,
                ParagraphPurpose::Finding,
                ParagraphPurpose::Connection,
                ParagraphPurpose::Complication,
                ParagraphPurpose::Conclusion,
            ]
            .iter()
            .any(|purpose| !purposes.contains(purpose))
            {
                errors.push("current_claim reading must include answer, finding, connection, complication and conclusion paragraphs".into());
            }
        }
        for (index, paragraph) in section.paragraphs.iter().enumerate() {
            let label = format!("reading {claim} paragraph {}", index + 1);
            nonempty(errors, &format!("{label} text"), &paragraph.text);
            let normalised = paragraph.text.replace("\r\n", "\n");
            if normalised.contains("\n\n") || normalised.lines().any(|line| line.trim().is_empty())
            {
                errors.push(format!(
                    "{label} text must contain one paragraph, without blank lines"
                ));
            }
            if paragraph.reference_ids.is_empty() {
                errors.push(format!(
                    "{label} must reference canonical graph or evidence IDs"
                ));
            }
            let mut references = HashSet::new();
            for id in &paragraph.reference_ids {
                if !canonical_ids.contains(id) {
                    errors.push(format!("{label} references unknown canonical ID {id:?}"));
                }
                if !references.insert(id) {
                    errors.push(format!("{label} repeats canonical ID {id:?}"));
                }
            }
        }
    }
    for claim in
        std::iter::once("current_claim").chain(case.subclaims.iter().map(|s| s.id.as_str()))
    {
        if !seen.contains(claim) {
            errors.push(format!("claim {claim:?} needs exactly one reading section"));
        }
    }
}

/// Validate reference integrity and conservative evidence-status rules.
///
/// `status` assesses `current_claim` and its dependency closure. Missing support
/// attached only to `stronger_target` does not defeat a narrower current claim.
/// For alternative routes, provisional checks follow explicit route selections.
/// These checks cannot inspect truth, relevance, sufficiency or claim narrowing.
pub fn validate_case(request: &Request, case: &ArgumentCase) -> Result<(), Vec<String>> {
    let mut errors = validate_request(request).err().unwrap_or_default();
    if case.original_claim != request.claim {
        errors.push("original_claim must exactly preserve the requested claim".into());
    }
    for (name, text) in [
        ("current_claim.text", &case.current_claim.text),
        ("current_claim.qualifier", &case.current_claim.qualifier),
        ("stronger_target.text", &case.stronger_target.text),
        ("stronger_target.qualifier", &case.stronger_target.qualifier),
        ("scope.system_boundary", &case.scope.system_boundary),
        ("scope.configuration", &case.scope.configuration),
        (
            "scope.operating_conditions",
            &case.scope.operating_conditions,
        ),
        ("scope.units", &case.scope.units),
        ("scope.time_horizon", &case.scope.time_horizon),
        ("scope.consequences", &case.scope.consequences),
        ("adjudication", &case.adjudication),
    ] {
        nonempty(&mut errors, name, text);
    }
    if case.decision_limits.is_empty() {
        errors.push("decision_limits must identify at least one limit on reliance".into());
    }
    for text in case.decision_limits.iter().chain(&case.residual_risks) {
        nonempty(&mut errors, "risk or decision limit", text);
    }
    let known_evidence: HashMap<&str, &Evidence> = request
        .evidence
        .iter()
        .map(|e| (e.id.as_str(), e))
        .collect();
    let mut ids = HashSet::from(["current_claim".to_owned(), "stronger_target".to_owned()]);
    for evidence in &request.evidence {
        ids.insert(evidence.id.clone());
    }
    for id in case
        .subclaims
        .iter()
        .map(|s| &s.id)
        .chain(case.grounds.iter().map(|g| &g.id))
        .chain(case.warrants.iter().map(|w| &w.id))
        .chain(case.assumptions.iter().map(|a| &a.id))
        .chain(case.defeaters.iter().map(|d| &d.id))
    {
        register_id(&mut errors, &mut ids, id);
    }
    let claim_ids: HashSet<&str> = ["current_claim", "stronger_target"]
        .into_iter()
        .chain(case.subclaims.iter().map(|s| s.id.as_str()))
        .collect();
    let premise_ids: HashSet<&str> = case
        .grounds
        .iter()
        .map(|g| g.id.as_str())
        .chain(case.subclaims.iter().map(|s| s.id.as_str()))
        .collect();
    let node_ids: HashSet<&str> = claim_ids
        .iter()
        .copied()
        .chain(case.grounds.iter().map(|g| g.id.as_str()))
        .chain(case.warrants.iter().map(|w| w.id.as_str()))
        .chain(case.assumptions.iter().map(|a| a.id.as_str()))
        .chain(case.defeaters.iter().map(|d| d.id.as_str()))
        .collect();
    let mut planned_targets = HashSet::new();
    for need in &case.evidence_plan {
        register_id(&mut errors, &mut ids, &need.id);
        if !node_ids.contains(need.target_id.as_str()) {
            errors.push(format!(
                "evidence plan {} has unknown target {:?}",
                need.id, need.target_id
            ));
        }
        if !claim_ids.contains(need.enables_claim.as_str()) {
            errors.push(format!(
                "evidence plan {} has unknown enables_claim {:?}",
                need.id, need.enables_claim
            ));
        }
        planned_targets.insert(need.target_id.as_str());
        if !(1..=5).contains(&need.priority) {
            errors.push(format!("evidence plan {} priority must be 1–5", need.id));
        }
        for (name, value) in [
            ("question", &need.question),
            ("method", &need.method),
            ("measurement", &need.measurement),
            ("comparator", &need.comparator),
            ("acceptance_criterion", &need.acceptance_criterion),
            ("if_failed", &need.if_failed),
            ("enables_claim", &need.enables_claim),
            ("cost", &need.cost),
        ] {
            nonempty(
                &mut errors,
                &format!("evidence plan {} {name}", need.id),
                value,
            );
        }
    }
    for subclaim in &case.subclaims {
        nonempty(
            &mut errors,
            &format!("subclaim {} text", subclaim.id),
            &subclaim.text,
        );
        nonempty(
            &mut errors,
            &format!("subclaim {} qualifier", subclaim.id),
            &subclaim.qualifier,
        );
        if !case.warrants.iter().any(|w| w.conclusion_id == subclaim.id) {
            errors.push(format!(
                "subclaim {} needs an attached warrant",
                subclaim.id
            ));
        }
    }
    for ground in &case.grounds {
        nonempty(
            &mut errors,
            &format!("ground {} statement", ground.id),
            &ground.statement,
        );
        evidence_references(
            &mut errors,
            &ground.id,
            &ground.evidence_ids,
            &known_evidence,
        );
        if ground.status == GroundStatus::Reported && ground.evidence_ids.is_empty() {
            errors.push(format!(
                "reported ground {} needs supplied evidence",
                ground.id
            ));
        }
        if ground.status != GroundStatus::Reported && !planned_targets.contains(ground.id.as_str())
        {
            errors.push(format!("unmet ground {} needs an evidence plan", ground.id));
        }
    }
    for warrant in &case.warrants {
        nonempty(
            &mut errors,
            &format!("warrant {} rule", warrant.id),
            &warrant.rule,
        );
        nonempty(
            &mut errors,
            &format!("warrant {} applicability", warrant.id),
            &warrant.applicability,
        );
        if !claim_ids.contains(warrant.conclusion_id.as_str()) {
            errors.push(format!(
                "warrant {} has unknown conclusion {:?}",
                warrant.id, warrant.conclusion_id
            ));
        }
        if warrant.premise_ids.is_empty() {
            errors.push(format!(
                "warrant {} needs at least one ground or subclaim",
                warrant.id
            ));
        }
        let mut seen = HashSet::new();
        for premise in &warrant.premise_ids {
            if !premise_ids.contains(premise.as_str()) {
                errors.push(format!(
                    "warrant {} references unknown premise {premise:?}",
                    warrant.id
                ));
            }
            if !seen.insert(premise) {
                errors.push(format!(
                    "warrant {} repeats premise {premise:?}",
                    warrant.id
                ));
            }
        }
        evidence_references(
            &mut errors,
            &warrant.id,
            &warrant.backing_evidence_ids,
            &known_evidence,
        );
    }
    validate_compositions(case, &claim_ids, &mut errors);
    if has_support_cycle(case, &claim_ids) {
        errors.push("argument support graph contains a cycle".into());
    }
    for assumption in &case.assumptions {
        nonempty(
            &mut errors,
            &format!("assumption {} statement", assumption.id),
            &assumption.statement,
        );
        nonempty(
            &mut errors,
            &format!("assumption {} validation", assumption.id),
            &assumption.validation,
        );
        evidence_references(
            &mut errors,
            &assumption.id,
            &assumption.evidence_ids,
            &known_evidence,
        );
        if assumption.applies_to.is_empty() {
            errors.push(format!(
                "assumption {} must identify the nodes it applies to",
                assumption.id
            ));
        }
        let mut seen = HashSet::new();
        for target in &assumption.applies_to {
            if !node_ids.contains(target.as_str()) || *target == assumption.id {
                errors.push(format!(
                    "assumption {} has unknown or self target {target:?}",
                    assumption.id
                ));
            }
            if !seen.insert(target) {
                errors.push(format!(
                    "assumption {} repeats target {target:?}",
                    assumption.id
                ));
            }
        }
        if matches!(
            assumption.status,
            AssumptionStatus::Supported | AssumptionStatus::Contradicted
        ) && assumption.evidence_ids.is_empty()
        {
            errors.push(format!(
                "supported or contradicted assumption {} needs supplied evidence",
                assumption.id
            ));
        }
        if assumption.critical
            && assumption.status != AssumptionStatus::Supported
            && !planned_targets.contains(assumption.id.as_str())
        {
            errors.push(format!(
                "critical assumption {} needs an evidence plan",
                assumption.id
            ));
        }
    }
    for defeater in &case.defeaters {
        if !node_ids.contains(defeater.target_id.as_str()) || defeater.target_id == defeater.id {
            errors.push(format!(
                "defeater {} has unknown or self target {:?}",
                defeater.id, defeater.target_id
            ));
        }
        nonempty(
            &mut errors,
            &format!("defeater {} challenge", defeater.id),
            &defeater.challenge,
        );
        nonempty(
            &mut errors,
            &format!(
                "defeater {} response (including materiality rationale)",
                defeater.id
            ),
            &defeater.response,
        );
        evidence_references(
            &mut errors,
            &defeater.id,
            &defeater.evidence_ids,
            &known_evidence,
        );
        let outcome_needs_evidence = matches!(
            defeater.status,
            DefeaterStatus::Sustained | DefeaterStatus::Refuted
        );
        let basis_needs_evidence = matches!(
            defeater.basis,
            DefeaterBasis::Observed | DefeaterBasis::SupportedPossibility
        ) && defeater.status
            != DefeaterStatus::InapplicableAfterRevision;
        if (outcome_needs_evidence || basis_needs_evidence) && defeater.evidence_ids.is_empty() {
            errors.push(format!(
                "observed, supported, sustained or refuted defeater {} needs supplied evidence",
                defeater.id
            ));
        }
        if defeater.material
            && matches!(
                defeater.status,
                DefeaterStatus::Open | DefeaterStatus::Sustained
            )
            && !planned_targets.contains(defeater.id.as_str())
        {
            errors.push(format!(
                "unresolved material defeater {} needs an evidence plan",
                defeater.id
            ));
        }
    }
    for alternative in &case.alternatives {
        nonempty(
            &mut errors,
            "alternative explanation",
            &alternative.explanation,
        );
        nonempty(
            &mut errors,
            "alternative distinguishing_test",
            &alternative.distinguishing_test,
        );
        evidence_references(
            &mut errors,
            "alternative",
            &alternative.evidence_ids,
            &known_evidence,
        );
    }
    let current_support = support_closure(case, "current_claim");
    let current_closure = relied_dependency_closure(case, "current_claim");
    let mut both_support = support_closure(case, "stronger_target");
    both_support.extend(current_support);
    let all_attached = extended_closure(case, both_support);
    for need in &case.evidence_plan {
        if claim_ids.contains(need.enables_claim.as_str()) {
            let enabled_closure =
                extended_closure(case, support_closure(case, &need.enables_claim));
            if !enabled_closure.contains(&need.target_id) {
                errors.push(format!(
                    "evidence plan {} target is outside enables_claim {:?} dependency closure",
                    need.id, need.enables_claim
                ));
            }
        }
    }
    for node in &node_ids {
        if !all_attached.contains(*node) {
            errors.push(format!(
                "unattached argument node {node:?}: connect it to current_claim or stronger_target"
            ));
        }
    }
    if case.status == CaseStatus::ProvisionallySupported {
        if !case
            .warrants
            .iter()
            .any(|w| w.conclusion_id == "current_claim")
        {
            errors.push("provisional support requires a warrant concluding current_claim".into());
        }
        for claim_id in claim_ids.iter().filter(|id| current_closure.contains(**id)) {
            if !case
                .compositions
                .iter()
                .any(|c| c.conclusion_id == *claim_id && !c.selected_warrant_ids.is_empty())
            {
                errors.push(format!(
                    "provisional support requires a selected warrant for reached claim {claim_id:?}"
                ));
            }
        }
        let grounds: Vec<&Ground> = case
            .grounds
            .iter()
            .filter(|g| current_closure.contains(&g.id))
            .collect();
        if grounds.is_empty() {
            errors.push("provisional support requires ground support in the current claim dependency closure".into());
        }
        if grounds.iter().any(|g| g.status != GroundStatus::Reported) {
            errors.push("provisional support cannot depend on missing or assumed grounds".into());
        }
        if !grounds.iter().flat_map(|g| &g.evidence_ids).any(|id| {
            known_evidence
                .get(id.as_str())
                .is_some_and(|e| e.checked_by_user)
        }) {
            errors.push("provisional support requires at least one current ground backed by user-checked evidence".into());
        }
        if case.assumptions.iter().any(|a| {
            current_closure.contains(&a.id) && a.critical && a.status != AssumptionStatus::Supported
        }) {
            errors.push("provisional support cannot retain untested or contradicted critical assumptions in its dependency closure".into());
        }
        if case.defeaters.iter().any(|d| {
            current_closure.contains(&d.id)
                && d.material
                && matches!(d.status, DefeaterStatus::Open | DefeaterStatus::Sustained)
        }) {
            errors.push("provisional support cannot retain open or sustained material defeaters in its dependency closure, including hypothetical concerns".into());
        }
    }
    if case.status == CaseStatus::Refuted
        && !case.defeaters.iter().any(|d| {
            d.kind == DefeaterKind::Rebutting
                && d.status == DefeaterStatus::Sustained
                && d.target_id == "current_claim"
                && d.material
                && !d.evidence_ids.is_empty()
        })
    {
        errors.push("refuted requires a sustained, evidence-backed rebuttal targeting current_claim exactly; missing support alone is insufficient".into());
    }
    if matches!(
        case.status,
        CaseStatus::Unsupported | CaseStatus::Conditional
    ) && case.evidence_plan.is_empty()
    {
        errors.push("unsupported or conditional cases need an evidence plan".into());
    }
    validate_reading(case, &claim_ids, &ids, &mut errors);
    finish(errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim_only_candidate() -> (Request, ArgumentCase) {
        let request: Request = serde_json::from_str(
            r#"{"claim":"Parallelising the integration tests reduces CI time by at least 40% while preserving test outcomes."}"#,
        )
        .unwrap();
        let case = serde_json::from_value(serde_json::json!({
            "original_claim": request.claim,
            "current_claim": {
                "text": "If test groups are independent and runner resources permit overlap, parallel scheduling can reduce elapsed test time while preserving outcomes.",
                "qualifier": "Conditional candidate mechanism constructed from the claim alone; no observations have been supplied."
            },
            "stronger_target": {
                "text": "For frozen test revisions and runner configurations, parallel scheduling reduces median elapsed CI time by at least 40% while meeting separately specified outcome-preservation criteria.",
                "qualifier": "A proposed empirical target requiring measurements and isolation checks."
            },
            "status": "conditional",
            "scope": {
                "system_boundary": "The integration-test stage, including fixture setup and teardown.",
                "configuration": "Runner capacity, test revisions and fixture ownership are unspecified and must be recorded before the trial.",
                "operating_conditions": "Representative revisions, cache states and contention conditions remain to be selected and recorded.",
                "units": "Elapsed time in seconds, relative reduction in percent and outcome discrepancies as counts.",
                "time_horizon": "The proposed trial and its identified revisions.",
                "consequences": "Reduced feedback delay is useful only while test reliability remains acceptable."
            },
            "subclaims": [],
            "grounds": [
                {"id":"G1", "statement":"At least two groups can execute concurrently without changing shared mutable state that affects their outcomes.", "evidence_ids":[], "status":"assumed"},
                {"id":"G2", "statement":"Paired observations meet the 40% timing target and the independently specified outcome criteria.", "evidence_ids":[], "status":"missing"}
            ],
            "warrants": [
                {"id":"W1", "rule":"Overlapping independent work can shorten a sequential execution path when resource contention and setup costs do not erase the overlap.", "conclusion_id":"current_claim", "premise_ids":["G1"], "backing_evidence_ids":[], "inference":"causal", "applicability":"A proposed mechanism under explicit independence and capacity conditions, rather than an observed improvement."},
                {"id":"W2", "rule":"Paired measurements can support a bounded scheduling comparison when revisions, outcome criteria and confounders are controlled.", "conclusion_id":"stronger_target", "premise_ids":["G1","G2"], "backing_evidence_ids":[], "inference":"inductive", "applicability":"Requires a prospective design, retained observations and adequate isolation checks."}
            ],
            "compositions": [
                {"conclusion_id":"current_claim", "relationship":"jointly_required", "selected_warrant_ids":["W1"], "rationale":"The present conditional argument relies on the scheduling mechanism in W1."},
                {"conclusion_id":"stronger_target", "relationship":"jointly_required", "selected_warrant_ids":["W2"], "rationale":"The empirical target relies on W2 with both independence and comparative measurements required."}
            ],
            "assumptions": [
                {"id":"A1", "statement":"The runner has sufficient capacity for useful execution overlap.", "critical":true, "status":"untested", "evidence_ids":[], "applies_to":["W1","W2"], "validation":"Profile CPU, memory, setup costs and contention in the sequential and parallel configurations."}
            ],
            "defeaters": [
                {"id":"D1", "kind":"undercutting", "target_id":"W1", "challenge":"Runner contention or fixture setup may erase the expected overlap benefit.", "status":"open", "material":true, "basis":"hypothetical", "response":"Investigate the resource and setup profile before relying on the proposed mechanism; this concern is not an observed failure.", "evidence_ids":[]}
            ],
            "alternatives": [
                {"explanation":"Fixture initialisation may dominate the stage, leaving little schedulable work to overlap.", "distinguishing_test":"Measure fixture and test-body durations separately in both configurations.", "evidence_ids":[]}
            ],
            "evidence_plan": [
                {"id":"N1", "target_id":"G1", "question":"Can groups overlap without affecting outcomes?", "method":"Inspect fixture ownership and run repeated isolation and scheduling-perturbation checks on frozen revisions.", "measurement":"Cross-group state interference and outcome discrepancies as counts.", "comparator":"Sequential execution and isolated-fixture parallel execution on the same revisions.", "acceptance_criterion":"Meet prospectively specified isolation and outcome criteria, with finite coverage limits reported.", "if_failed":"Repair isolation or retain sequential scheduling for affected groups.", "enables_claim":"current_claim", "priority":1, "cost":"Fixture investigation and repeated test execution."},
                {"id":"N2", "target_id":"G2", "question":"Does parallel execution meet the 40% target while preserving outcomes?", "method":"Pilot variance, then preregister and execute a counterbalanced paired trial with retained timing and outcome records.", "measurement":"Elapsed time in seconds, paired relative reductions in percent and outcome discrepancies as counts.", "comparator":"The sequential workflow on the same revisions and runner configuration.", "acceptance_criterion":"The preregistered interval must support at least a 40% median reduction; outcome-preservation criteria must pass independently.", "if_failed":"Narrow or reject the quantitative target according to the retained observations.", "enables_claim":"stronger_target", "priority":2, "cost":"Runner time and analysis effort estimated from the pilot."},
                {"id":"N3", "target_id":"A1", "question":"Does runner capacity allow useful overlap?", "method":"Record resource utilisation and setup durations under both configurations, including representative contention conditions.", "measurement":"CPU utilisation in percent, memory in bytes and setup and test durations in seconds.", "comparator":"Sequential scheduling on the same runner capacity and revision.", "acceptance_criterion":"Demonstrate useful overlap within prospectively declared capacity and contention conditions.", "if_failed":"Revise the capacity or scheduling proposal and retain a conditional claim.", "enables_claim":"current_claim", "priority":1, "cost":"Instrumentation and representative runs."},
                {"id":"N4", "target_id":"D1", "question":"Do contention and setup costs erase the mechanism's expected benefit?", "method":"Analyse fixture and test-body timing alongside the resource profile in the paired trial.", "measurement":"Fixture and test-body durations in seconds and resource utilisation in declared units.", "comparator":"Matched sequential and parallel runs across recorded contention conditions.", "acceptance_criterion":"The proposed benefit must survive the preregistered contention and setup checks or be restricted to supported conditions.", "if_failed":"Revise the mechanism or restrict its scope before claiming a benefit.", "enables_claim":"current_claim", "priority":1, "cost":"Analysis shared with the capacity investigation."}
            ],
            "reading": [
                {"claim_id":"current_claim", "title":"The strongest conditional argument from the claim alone", "paragraphs":[
                    {"purpose":"answer", "text":"Parallel scheduling offers a plausible route to shorter CI runs if the groups are independent and the runner permits useful overlap. This is a conditional candidate argument because the claim alone supplies no observations.", "reference_ids":["current_claim","G1","A1"]},
                    {"purpose":"finding", "text":"The decisive premise is that concurrent groups preserve their outcomes despite changes in scheduling. Independence is presently assumed, while the comparative measurements needed for the 40% target remain missing.", "reference_ids":["G1","G2","stronger_target"]},
                    {"purpose":"connection", "text":"W1 connects independence and available capacity to the possibility of useful overlap. W2 adds a separate requirement for a controlled comparison before that mechanism can support the quantitative target.", "reference_ids":["W1","W2","A1"]},
                    {"purpose":"complication", "text":"Fixture setup or resource contention could consume the time expected to be saved. The isolation and capacity investigations therefore precede reliance on the mechanism, and the paired trial tests the quantitative target separately.", "reference_ids":["D1","N1","N2","N3","N4"]},
                    {"purpose":"conclusion", "text":"Begin by freezing the configuration and checking independence and capacity, then run the preregistered paired comparison. Retain the 40% reduction as an empirical target until the observations justify accepting, narrowing or rejecting it.", "reference_ids":["current_claim","stronger_target","N1","N2","N3"]}
                ]}
            ],
            "residual_risks": ["Finite checks may miss rare interference or untested workload conditions."],
            "decision_limits": ["This candidate supports planning an investigation; it supplies no measured performance or production assurance."],
            "adjudication": "The claim admits a complete conditional mechanism and discriminating evidence plan. Its quantitative and outcome assertions require observations before empirical support can be claimed."
        }))
        .unwrap();
        (request, case)
    }

    #[test]
    fn claim_only_json_defaults_optional_context_and_evidence() {
        let request: Request =
            serde_json::from_str(r#"{"claim":"The change reduces latency."}"#).unwrap();
        assert_eq!(request.claim, "The change reduces latency.");
        assert!(request.context.is_empty());
        assert!(request.evidence.is_empty());
        assert_eq!(validate_request(&request), Ok(()));
        assert!(
            serde_json::from_str::<Request>(r#"{"context":"A context without a claim"}"#).is_err()
        );
    }

    #[test]
    fn request_schema_requires_only_the_claim_and_declares_empty_defaults() {
        let schema = serde_json::to_value(schemars::schema_for!(Request)).unwrap();
        assert_eq!(schema["required"], serde_json::json!(["claim"]));
        assert_eq!(schema["properties"]["context"]["default"], "");
        assert_eq!(
            schema["properties"]["evidence"]["default"],
            serde_json::json!([])
        );
        assert_eq!(schema["additionalProperties"], false);
    }

    #[test]
    fn optional_request_fields_preserve_supplied_context_and_source_records() {
        let (request, _) = fixtures();
        let mut input = serde_json::to_value(&request).unwrap();
        let explicit: Request = serde_json::from_value(input.clone()).unwrap();
        assert_eq!(explicit.context, request.context);
        input.as_object_mut().unwrap().remove("context");
        let without_context: Request = serde_json::from_value(input).unwrap();
        assert!(without_context.context.is_empty());
        assert_eq!(
            serde_json::to_value(without_context.evidence).unwrap(),
            serde_json::to_value(request.evidence).unwrap()
        );
    }

    #[test]
    fn a_complete_conditional_candidate_needs_no_supplied_sources() {
        let (request, case) = claim_only_candidate();
        assert!(request.context.is_empty() && request.evidence.is_empty());
        assert_eq!(case.status, CaseStatus::Conditional);
        assert!(
            case.grounds
                .iter()
                .all(|ground| ground.evidence_ids.is_empty()
                    && matches!(ground.status, GroundStatus::Assumed | GroundStatus::Missing))
        );
        assert_eq!(validate_case(&request, &case), Ok(()));
    }

    #[test]
    fn a_claim_only_candidate_cannot_invent_reported_or_provisional_support() {
        let (request, mut case) = claim_only_candidate();
        case.grounds[0].status = GroundStatus::Reported;
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|error| error.contains("reported ground G1 needs supplied evidence"))
        );
        case.grounds[0].status = GroundStatus::Assumed;
        case.status = CaseStatus::ProvisionallySupported;
        let errors = validate_case(&request, &case).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.contains("missing or assumed grounds"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("user-checked evidence"))
        );
    }

    fn fixtures() -> (Request, ArgumentCase) {
        (
            serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
            serde_json::from_str(include_str!("../examples/case.json")).unwrap(),
        )
    }

    fn provisional_narrow_case() -> (Request, ArgumentCase) {
        let (mut request, mut case) = fixtures();
        request.evidence[0].checked_by_user = true;
        case.status = CaseStatus::ProvisionallySupported;
        case.current_claim.text =
            "The supplied synthetic description reports sequential test groups.".into();
        case.current_claim.qualifier = "A statement about the supplied description only.".into();
        case.warrants
            .iter_mut()
            .find(|w| w.conclusion_id == "current_claim")
            .unwrap()
            .premise_ids = vec!["S1".into()];
        case.assumptions[0].applies_to = vec!["stronger_target".into()];
        case.evidence_plan
            .iter_mut()
            .find(|p| p.target_id == "A1")
            .unwrap()
            .enables_claim = "stronger_target".into();
        for defeater in &mut case.defeaters {
            defeater.target_id = "stronger_target".into();
        }
        (request, case)
    }

    #[test]
    fn synthetic_example_is_structurally_valid() {
        let (request, case) = fixtures();
        assert_eq!(validate_request(&request), Ok(()));
        assert_eq!(validate_case(&request, &case), Ok(()));
        assert_ne!(case.status, CaseStatus::ProvisionallySupported);
    }

    #[test]
    fn invented_evidence_is_rejected() {
        let (request, mut case) = fixtures();
        case.grounds[0].evidence_ids.push("invented".into());
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("unknown evidence"))
        );
    }

    #[test]
    fn unsupported_is_not_refuted() {
        let (request, mut case) = fixtures();
        case.status = CaseStatus::Refuted;
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("targeting current_claim exactly"))
        );
    }

    #[test]
    fn model_cannot_upgrade_critical_assumptions_into_provisional_support() {
        let (request, mut case) = fixtures();
        case.status = CaseStatus::ProvisionallySupported;
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("critical assumptions"))
        );
    }

    #[test]
    fn duplicate_ids_and_broken_warrant_targets_are_rejected() {
        let (request, mut case) = fixtures();
        case.warrants[0].id = case.grounds[0].id.clone();
        case.warrants[0].premise_ids = vec!["absent-ground".into()];
        let errors = validate_case(&request, &case).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("duplicate")));
        assert!(errors.iter().any(|e| e.contains("unknown premise")));
    }

    #[test]
    fn every_critical_assumption_needs_a_test() {
        let (request, mut case) = fixtures();
        let target = case.assumptions[0].id.clone();
        case.evidence_plan.retain(|p| p.target_id != target);
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("critical assumption"))
        );
    }

    #[test]
    fn supported_assumptions_need_evidence_references() {
        let (request, mut case) = fixtures();
        case.assumptions[0].status = AssumptionStatus::Supported;
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("supported or contradicted assumption"))
        );
    }

    #[test]
    fn stronger_target_gaps_do_not_defeat_support_for_a_narrow_current_claim() {
        let (request, case) = provisional_narrow_case();
        assert!(
            case.grounds
                .iter()
                .any(|g| g.status == GroundStatus::Missing)
        );
        assert_eq!(validate_case(&request, &case), Ok(()));
    }

    fn case_with_an_unused_missing_alternative() -> (Request, ArgumentCase) {
        let (request, mut case) = provisional_narrow_case();
        let mut ground = case.grounds[1].clone();
        ground.id = "G3".into();
        case.grounds.push(ground);
        let mut warrant = case
            .warrants
            .iter()
            .find(|w| w.conclusion_id == "current_claim")
            .unwrap()
            .clone();
        warrant.id = "W4".into();
        warrant.premise_ids = vec!["G3".into()];
        case.warrants.push(warrant);
        let composition = case
            .compositions
            .iter_mut()
            .find(|c| c.conclusion_id == "current_claim")
            .unwrap();
        composition.relationship = SupportRelationship::AlternativeRoutes;
        composition.selected_warrant_ids = vec!["W2".into()];
        composition.rationale = "W2 is the selected route for the bounded source-report claim. W4 remains an unselected candidate requiring missing measurements.".into();
        let mut plan = case.evidence_plan[0].clone();
        plan.id = "N5".into();
        plan.target_id = "G3".into();
        plan.enables_claim = "current_claim".into();
        case.evidence_plan.push(plan);
        (request, case)
    }

    #[test]
    fn selected_alternative_survives_missing_support_on_an_unused_route() {
        let (request, case) = case_with_an_unused_missing_alternative();
        assert!(support_closure(&case, "current_claim").contains("G3"));
        assert!(!relied_dependency_closure(&case, "current_claim").contains("G3"));
        assert_eq!(validate_case(&request, &case), Ok(()));
    }

    #[test]
    fn cumulative_support_selects_all_routes_and_keeps_their_gaps_material() {
        let (request, mut case) = case_with_an_unused_missing_alternative();
        let composition = case
            .compositions
            .iter_mut()
            .find(|c| c.conclusion_id == "current_claim")
            .unwrap();
        composition.relationship = SupportRelationship::Cumulative;
        composition.selected_warrant_ids = vec!["W2".into(), "W4".into()];
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("missing or assumed grounds"))
        );
    }

    #[test]
    fn unresolved_alternative_selection_cannot_supply_provisional_support() {
        let (request, mut case) = provisional_narrow_case();
        let composition = case
            .compositions
            .iter_mut()
            .find(|c| c.conclusion_id == "S1")
            .unwrap();
        composition.relationship = SupportRelationship::AlternativeRoutes;
        composition.selected_warrant_ids.clear();
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("selected warrant for reached claim"))
        );
    }

    #[test]
    fn compositions_cannot_be_missing_duplicated_or_select_another_claims_warrant() {
        let (request, mut case) = fixtures();
        let duplicate = case.compositions[0].clone();
        case.compositions.push(duplicate);
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("duplicate composition"))
        );
        case.compositions.pop();
        case.compositions
            .iter_mut()
            .find(|c| c.conclusion_id == "current_claim")
            .unwrap()
            .selected_warrant_ids = vec!["W1".into()];
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("wrong-claim warrant"))
        );
        case.compositions
            .retain(|c| c.conclusion_id != "current_claim");
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("needs exactly one support composition"))
        );
    }

    #[test]
    fn jointly_required_composition_cannot_drop_a_route() {
        let (request, mut case) = case_with_an_unused_missing_alternative();
        case.compositions
            .iter_mut()
            .find(|c| c.conclusion_id == "current_claim")
            .unwrap()
            .relationship = SupportRelationship::JointlyRequired;
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("must select all attached warrants"))
        );
    }

    #[test]
    fn reading_rejects_unknown_and_duplicate_references() {
        let (request, mut case) = fixtures();
        case.reading[0].paragraphs[0]
            .reference_ids
            .extend(["invented".into(), "current_claim".into()]);
        let errors = validate_case(&request, &case).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("unknown canonical ID")));
        assert!(errors.iter().any(|e| e.contains("repeats canonical ID")));
    }

    #[test]
    fn reading_requires_distinct_connected_paragraph_roles_and_single_paragraph_text() {
        let (request, mut case) = fixtures();
        case.reading[0].paragraphs[1].purpose = ParagraphPurpose::Answer;
        case.reading[0].paragraphs[0]
            .text
            .push_str("\n\nA second paragraph must be a distinct record.");
        let errors = validate_case(&request, &case).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.contains("must include answer, finding"))
        );
        assert!(
            errors
                .iter()
                .any(|e| e.contains("text must contain one paragraph"))
        );
    }

    #[test]
    fn reading_rejects_crlf_and_whitespace_only_paragraph_separators() {
        for separator in ["\r\n\r\n", "\n \t\n", "\r\n \t\r\n"] {
            let (request, mut case) = fixtures();
            case.reading[0].paragraphs[0].text =
                format!("First paragraph.{separator}Second paragraph.");
            assert!(
                validate_case(&request, &case)
                    .unwrap_err()
                    .iter()
                    .any(|error| error.contains("text must contain one paragraph")),
                "separator {separator:?} must be rejected"
            );
        }
    }

    #[test]
    fn every_subclaim_has_one_reading_section_with_its_own_initial_reference() {
        let (request, mut case) = fixtures();
        let section = case
            .reading
            .iter_mut()
            .find(|s| s.claim_id == "S1")
            .unwrap();
        section.paragraphs[0].reference_ids = vec!["E1".into()];
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("first answer must reference its own claim ID"))
        );
        case.reading.retain(|s| s.claim_id != "S1");
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("needs exactly one reading section"))
        );
    }

    #[test]
    fn test_plan_cannot_enable_an_unrelated_claim() {
        let (request, mut case) = fixtures();
        case.evidence_plan
            .iter_mut()
            .find(|p| p.target_id == "G2")
            .unwrap()
            .enables_claim = "current_claim".into();
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("outside enables_claim"))
        );
    }

    #[test]
    fn rebuttal_of_a_subclaim_cannot_mark_the_current_claim_refuted() {
        let (request, mut case) = fixtures();
        case.status = CaseStatus::Refuted;
        case.defeaters[0].target_id = "S1".into();
        case.defeaters[0].kind = DefeaterKind::Rebutting;
        case.defeaters[0].basis = DefeaterBasis::Observed;
        case.defeaters[0].status = DefeaterStatus::Sustained;
        case.defeaters[0].evidence_ids = vec!["E1".into()];
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("targeting current_claim exactly"))
        );
    }

    #[test]
    fn support_cycles_are_rejected() {
        let (request, mut case) = fixtures();
        case.warrants[0].premise_ids = vec!["S1".into()];
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("cycle"))
        );
    }

    #[test]
    fn unrelated_warrant_cannot_support_current_claim() {
        let (request, mut case) = provisional_narrow_case();
        case.warrants.retain(|w| w.conclusion_id != "current_claim");
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("warrant concluding current_claim"))
        );
    }

    #[test]
    fn material_hypothetical_concerns_still_block_provisional_support() {
        let (request, mut case) = provisional_narrow_case();
        case.defeaters[0].target_id = "current_claim".into();
        case.defeaters[0].basis = DefeaterBasis::Hypothetical;
        case.defeaters[0].status = DefeaterStatus::Open;
        case.defeaters[0].material = true;
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("including hypothetical concerns"))
        );
    }

    #[test]
    fn refuted_defeater_requires_evidence() {
        let (request, mut case) = fixtures();
        case.defeaters[0].status = DefeaterStatus::Refuted;
        case.defeaters[0].evidence_ids.clear();
        assert!(
            validate_case(&request, &case)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("refuted defeater"))
        );
    }

    #[test]
    fn unattached_nodes_are_rejected() {
        let (request, mut case) = fixtures();
        case.subclaims.push(ClaimNode {
            id: "orphan".into(),
            text: "Detached assertion".into(),
            qualifier: "Unsupported".into(),
        });
        let errors = validate_case(&request, &case).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.contains("unattached argument node"))
        );
        assert!(
            errors
                .iter()
                .any(|e| e.contains("needs an attached warrant"))
        );
    }

    #[test]
    fn claim_and_evidence_ids_are_preserved() {
        let (mut request, mut case) = fixtures();
        case.original_claim.push('!');
        request.evidence[0].id = "current_claim".into();
        let errors = validate_case(&request, &case).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("original_claim")));
        assert!(errors.iter().any(|e| e.contains("reserved identifier")));
    }

    #[test]
    fn unknown_fields_fail_instead_of_silently_disappearing() {
        let mut value = serde_json::to_value(fixtures().1).unwrap();
        value["confidence"] = serde_json::json!(0.99);
        assert!(serde_json::from_value::<ArgumentCase>(value).is_err());
    }

    #[test]
    fn schema_has_closed_objects_and_enum_status() {
        let schema = case_schema();
        assert_eq!(schema["additionalProperties"], false);
        assert!(
            schema["$defs"]["CaseStatus"]["enum"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("unsupported"))
        );
    }
}
