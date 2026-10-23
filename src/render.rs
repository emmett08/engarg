//! Populate the supplied engineering extended Toulmin template from canonical records.
//!
//! Rendering preserves recorded assessment status. It labels information that has
//! not been supplied rather than filling evidential gaps with generated findings.

use crate::domain::{ArgumentCase, Assumption, Defeater, EvidenceNeed, Request, Warrant};
use std::collections::BTreeSet;
use std::fmt::Write;

fn label<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Escape untrusted content without allowing extra Markdown table columns or rows.
fn cell(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace(char::from(96), "&#96;")
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "<br>")
}

fn prose(value: &str) -> String {
    cell(value).replace("<br>", " ")
}

fn row(s: &mut String, values: &[&str]) {
    s.push('|');
    for value in values {
        write!(s, " {} |", cell(value)).unwrap();
    }
    s.push('\n');
}

fn table(s: &mut String, headers: &[&str]) {
    row(s, headers);
    row(s, &vec!["---"; headers.len()]);
}

fn joined(values: &[String]) -> String {
    if values.is_empty() {
        "None recorded.".into()
    } else {
        values.join(" ")
    }
}

fn claim<'a>(case: &'a ArgumentCase, id: &str) -> (&'a str, &'a str) {
    match id {
        "current_claim" => (&case.current_claim.text, &case.current_claim.qualifier),
        "stronger_target" => (&case.stronger_target.text, &case.stronger_target.qualifier),
        _ => case
            .subclaims
            .iter()
            .find(|c| c.id == id)
            .map(|c| (c.text.as_str(), c.qualifier.as_str()))
            .unwrap_or((
                "Claim reference is absent from this assessment.",
                "Not assessed.",
            )),
    }
}

fn claim_assessment(case: &ArgumentCase, id: &str) -> String {
    match id {
        "current_claim" => format!("Current assessment: {}. {}", label(&case.status), case.adjudication),
        "stronger_target" => "Conditional target. Its fulfilment has not been established by this report.".into(),
        _ => "Separate subclaim verdict not recorded. Its stated qualifier and dependencies govern its use.".into(),
    }
}

fn composition_text(case: &ArgumentCase, conclusion: &str) -> String {
    match case
        .compositions
        .iter()
        .find(|c| c.conclusion_id == conclusion)
    {
        Some(c) => format!(
            "Relationship: {}. Selected warrants: {}. Model assessment rationale: {} Selection records a judgement; its adequacy requires the referenced reasoning and evidence.",
            label(&c.relationship),
            if c.selected_warrant_ids.is_empty() {
                "None; this composition remains unresolved.".into()
            } else {
                c.selected_warrant_ids.join(", ")
            },
            c.rationale
        ),
        None => "No composition assessment recorded for this conclusion.".into(),
    }
}

fn evidence_summary(request: &Request, ids: &[String]) -> String {
    if ids.is_empty() {
        return "No supporting evidence reference supplied.".into();
    }
    ids.iter()
        .map(|id| match request.evidence.iter().find(|e| &e.id == id) {
            Some(e) => format!(
                "{id}: {}. Source/location: {}. Kind: {}. User asserts checking: {}.",
                e.title, e.source, e.kind, e.checked_by_user
            ),
            None => format!("{id}: reference absent from supplied evidence."),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn node_description(case: &ArgumentCase, request: &Request, id: &str) -> String {
    if let Some(g) = case.grounds.iter().find(|g| g.id == id) {
        format!(
            "{id}: {} Status: {}. {}",
            g.statement,
            label(&g.status),
            evidence_summary(request, &g.evidence_ids)
        )
    } else {
        let (text, qualifier) = claim(case, id);
        format!("{id}: {text} Qualifier: {qualifier}")
    }
}

/// Premises imported by one inference, without pulling in sibling warrants.
fn inference_nodes(case: &ArgumentCase, warrant: &Warrant) -> BTreeSet<String> {
    let mut nodes = BTreeSet::from([warrant.id.clone(), warrant.conclusion_id.clone()]);
    let mut pending = warrant.premise_ids.clone();
    while let Some(id) = pending.pop() {
        if !nodes.insert(id.clone()) {
            continue;
        }
        let selection = case.compositions.iter().find(|c| c.conclusion_id == id);
        for prior in case.warrants.iter().filter(|w| {
            w.conclusion_id == id
                && selection.is_none_or(|c| c.selected_warrant_ids.contains(&w.id))
        }) {
            nodes.insert(prior.id.clone());
            pending.extend(prior.premise_ids.iter().cloned());
        }
    }
    loop {
        let before = nodes.len();
        for a in &case.assumptions {
            if a.applies_to.iter().any(|id| nodes.contains(id)) {
                nodes.insert(a.id.clone());
            }
        }
        for d in &case.defeaters {
            if nodes.contains(&d.target_id) {
                nodes.insert(d.id.clone());
            }
        }
        if nodes.len() == before {
            break;
        }
    }
    nodes
}

fn assumption_text(a: &Assumption, request: &Request) -> String {
    format!(
        "{}: {} Critical: {}. Status: {}. Applies to: {}. Basis: {} Validation/check: {}. If a critical assumption fails, reassess each declared dependent inference.",
        a.id,
        a.statement,
        a.critical,
        label(&a.status),
        a.applies_to.join(", "),
        evidence_summary(request, &a.evidence_ids),
        a.validation
    )
}

fn challenge_text(d: &Defeater, request: &Request) -> String {
    format!(
        "{} targets {} by {}: {} Basis: {}. Material: {}. Disposition: {}. Response and remaining effect: {} Evidence: {}",
        d.id,
        d.target_id,
        label(&d.kind),
        d.challenge,
        label(&d.basis),
        d.material,
        label(&d.status),
        d.response,
        evidence_summary(request, &d.evidence_ids)
    )
}

fn need_text(n: &EvidenceNeed) -> String {
    format!(
        "{} targets {} and would strengthen {}. Question: {} Method: {} Measurement: {} Comparator: {} Proposed acceptance criterion: {} Favourable: reassess the enabled claim against that criterion and all its other dependencies. Adverse: {} Inconclusive: leave the stronger target unresolved. Priority: {}. Estimated effort: {}",
        n.id,
        n.target_id,
        n.enables_claim,
        n.question,
        n.method,
        n.measurement,
        n.comparator,
        n.acceptance_criterion,
        n.if_failed,
        n.priority,
        n.cost
    )
}

fn scope_text(case: &ArgumentCase) -> String {
    format!(
        "System boundary: {} Configuration: {} Operating conditions: {} Measures and units: {} Applicable period: {} Separate term definitions and established property criteria: not supplied unless explicitly stated in the foregoing assessment. Additional transfer justification is required for other configurations or operating conditions.",
        case.scope.system_boundary,
        case.scope.configuration,
        case.scope.operating_conditions,
        case.scope.units,
        case.scope.time_horizon
    )
}

fn alternatives_text(case: &ArgumentCase, request: &Request) -> String {
    if case.alternatives.is_empty() {
        return "No competing explanation or option recorded. Completeness has not been established.".into();
    }
    case.alternatives.iter().map(|a|format!(
        "Alternative: {} Discriminating check: {} Evidence: {} An independently justified common decision criterion is not separately recorded.",
        a.explanation, a.distinguishing_test, evidence_summary(request, &a.evidence_ids)
    )).collect::<Vec<_>>().join(" ")
}

fn current_summary(case: &ArgumentCase) -> String {
    format!(
        "current_claim: {} Qualifier: {} Assessment: {}. Decisive reasoning and limitations: {}",
        case.current_claim.text,
        case.current_claim.qualifier,
        label(&case.status),
        case.adjudication
    )
}

fn decision_text(case: &ArgumentCase) -> String {
    format!(
        "Consequences: {} Reliance limits: {} Residual risks: {} A separate action authorisation, value criterion and action acceptance decision are not recorded. Report revision policy: reassess affected inferences when evidence, assumptions, target configuration or operating conditions change.",
        case.scope.consequences,
        joined(&case.decision_limits),
        joined(&case.residual_risks)
    )
}

fn argument_record(s: &mut String, case: &ArgumentCase, request: &Request, w: &Warrant) {
    let nodes = inference_nodes(case, w);
    let (conclusion, qualifier) = claim(case, &w.conclusion_id);
    let premises = w
        .premise_ids
        .iter()
        .map(|id| node_description(case, request, id))
        .collect::<Vec<_>>()
        .join(" ");
    let assumptions = case
        .assumptions
        .iter()
        .filter(|a| nodes.contains(&a.id))
        .map(|a| assumption_text(a, request))
        .collect::<Vec<_>>();
    let challenges = case
        .defeaters
        .iter()
        .filter(|d| nodes.contains(&d.id))
        .map(|d| challenge_text(d, request))
        .collect::<Vec<_>>();
    let needs = case
        .evidence_plan
        .iter()
        .filter(|n| nodes.contains(&n.target_id) || n.enables_claim == w.conclusion_id)
        .map(need_text)
        .collect::<Vec<_>>();
    let backing = if w.backing_evidence_ids.is_empty() {
        format!(
            "No backing evidence supplied for {}. Its rule and applicability remain recorded reasoning requiring appraisal.",
            w.id
        )
    } else {
        format!(
            "{} Separate adequacy and domain-of-applicability justification for this backing is not recorded beyond the warrant's applicability statement.",
            evidence_summary(request, &w.backing_evidence_ids)
        )
    };
    let mut evidence_ids = BTreeSet::new();
    evidence_ids.extend(w.backing_evidence_ids.iter().cloned());
    for g in case.grounds.iter().filter(|g| nodes.contains(&g.id)) {
        evidence_ids.extend(g.evidence_ids.iter().cloned());
    }
    for a in case.assumptions.iter().filter(|a| nodes.contains(&a.id)) {
        evidence_ids.extend(a.evidence_ids.iter().cloned());
    }
    for d in case.defeaters.iter().filter(|d| nodes.contains(&d.id)) {
        evidence_ids.extend(d.evidence_ids.iter().cloned());
    }
    let provenance = format!(
        "{} Full findings and known shared uses are in the evidence register. Separate source dates, acquisition details and adequacy judgements are not inferred from identifiers.",
        evidence_summary(request, &evidence_ids.into_iter().collect::<Vec<_>>())
    );
    let rebuttals = if challenges.is_empty() {
        format!(
            "No specific rebuttal condition is recorded for {}. Applicability boundary: {} Reassess the inference if evidence or analysis establishes that this boundary fails.",
            w.id, w.applicability
        )
    } else {
        format!(
            "{} Applicability boundary: {} An established exception requires reassessment or qualification of this inference.",
            joined(&challenges),
            w.applicability
        )
    };
    writeln!(s, "\n## Argument record — ARG-{}\n", prose(&w.id)).unwrap();
    table(s, &["Component", "Engineering entry"]);
    let entries = [
        (
            format!("1. Claim — {}", w.conclusion_id),
            conclusion.to_owned(),
        ),
        (
            "2. Grounds — jointly required premises".into(),
            format!(
                "{premises} These premises are jointly required within ARG-{}.",
                w.id
            ),
        ),
        (
            format!("3. Warrant — {}", w.id),
            format!(
                "{} Given premises {} and assumptions {}, this warrant connects them to {}. Inference type: {}. Applicability: {}",
                w.rule,
                w.premise_ids.join(", "),
                if assumptions.is_empty() {
                    "none recorded".into()
                } else {
                    case.assumptions
                        .iter()
                        .filter(|a| nodes.contains(&a.id))
                        .map(|a| a.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                },
                w.conclusion_id,
                label(&w.inference),
                w.applicability
            ),
        ),
        ("4. Backing".into(), backing),
        (
            "5. Qualifier".into(),
            format!(
                "{qualifier} A separately calibrated numerical uncertainty estimate is not recorded."
            ),
        ),
        ("6. Rebuttal conditions".into(), rebuttals),
        (
            "7. Scope, definitions and criteria".into(),
            scope_text(case),
        ),
        ("8. Evidence provenance and adequacy".into(), provenance),
        (
            "9. Critical assumptions and dependencies".into(),
            format!(
                "{} Required ground or subclaim identifiers: {}.",
                if assumptions.is_empty() {
                    "No assumptions recorded for this inference.".into()
                } else {
                    joined(&assumptions)
                },
                w.premise_ids.join(", ")
            ),
        ),
        (
            "10. Competing explanations or options".into(),
            alternatives_text(case, request),
        ),
        (
            "11. Challenges and responses".into(),
            if challenges.is_empty() {
                "No targeted challenge recorded for this inference and its imported support. Completeness remains unassessed.".into()
            } else {
                joined(&challenges)
            },
        ),
        (
            "12. Stronger target and evidence obligations".into(),
            format!(
                "stronger_target: {} Qualifier: {} Relevant obligations: {}",
                case.stronger_target.text,
                case.stronger_target.qualifier,
                if needs.is_empty() {
                    "No additional obligation is separately recorded for this inference.".into()
                } else {
                    joined(&needs)
                }
            ),
        ),
        (
            "13. Current assessment and defensible claim".into(),
            format!(
                "Record conclusion {}: {} Qualifier: {} {} This argument record is {} for its conclusion. {} Overall current formulation: {}",
                w.conclusion_id,
                conclusion,
                qualifier,
                claim_assessment(case, &w.conclusion_id),
                if case
                    .compositions
                    .iter()
                    .any(|c| c.conclusion_id == w.conclusion_id
                        && c.selected_warrant_ids.contains(&w.id))
                {
                    "selected"
                } else {
                    "a retained, unselected candidate"
                },
                composition_text(case, &w.conclusion_id),
                current_summary(case)
            ),
        ),
        (
            "14. Decision basis and revision triggers".into(),
            decision_text(case),
        ),
    ];
    for (component, entry) in entries {
        row(s, &[&component, &entry]);
    }
}

fn evidence_register(s: &mut String, request: &Request, case: &ArgumentCase) {
    writeln!(s, "\n## Evidence register\n").unwrap();
    table(
        s,
        &[
            "Evidence ID",
            "Premise or warrant supported",
            "Artefact, location and version/date",
            "Method and relevant finding",
            "Availability and verification performed",
            "Adequacy, limitations and shared dependencies",
        ],
    );
    for e in &request.evidence {
        let mut uses = Vec::new();
        uses.extend(
            case.grounds
                .iter()
                .filter(|g| g.evidence_ids.contains(&e.id))
                .map(|g| format!("{}: premise support", g.id)),
        );
        uses.extend(
            case.warrants
                .iter()
                .filter(|w| w.backing_evidence_ids.contains(&e.id))
                .map(|w| format!("{}: warrant backing", w.id)),
        );
        uses.extend(
            case.assumptions
                .iter()
                .filter(|a| a.evidence_ids.contains(&e.id))
                .map(|a| format!("{}: assumption appraisal", a.id)),
        );
        uses.extend(
            case.defeaters
                .iter()
                .filter(|d| d.evidence_ids.contains(&e.id))
                .map(|d| format!("{}: challenge appraisal", d.id)),
        );
        uses.extend(
            case.alternatives
                .iter()
                .enumerate()
                .filter(|(_, a)| a.evidence_ids.contains(&e.id))
                .map(|(i, _)| format!("alternative {}: comparative support", i + 1)),
        );
        row(
            s,
            &[
                &e.id,
                &if uses.is_empty() {
                    "No argument use recorded.".into()
                } else {
                    uses.join("; ")
                },
                &format!(
                    "{} Source/location: {}. Version/date: not separately supplied unless included in that provenance.",
                    e.title, e.source
                ),
                &format!("Kind: {}. Supplied finding/excerpt: {}", e.kind, e.excerpt),
                &format!(
                    "Available as supplied text. User asserts checking: {}. Independent verification by this workflow: not performed.",
                    e.checked_by_user
                ),
                &format!(
                    "Relevance, reliability, coverage and uncertainty require appraisal for the intended use. Known uses: {}. Reuse of one evidence identity supplies no independent replication.",
                    uses.len()
                ),
            ],
        );
    }
    if request.evidence.is_empty() {
        row(
            s,
            &[
                "None supplied",
                "No empirical support reference",
                "Not supplied",
                "Not supplied",
                "Unavailable",
                "Evidence obligations remain distinct from available evidence.",
            ],
        );
    }
}

fn registers(s: &mut String, case: &ArgumentCase, request: &Request) {
    evidence_register(s, request, case);
    writeln!(s, "\n## Assumptions and dependencies\n").unwrap();
    table(
        s,
        &[
            "ID",
            "Assumption or required subclaim",
            "Basis for reliance",
            "Dependent inference or conclusion",
            "Consequence of failure and relevant check",
        ],
    );
    for a in &case.assumptions {
        row(
            s,
            &[
                &a.id,
                &format!(
                    "{} Critical: {}. Status: {}.",
                    a.statement,
                    a.critical,
                    label(&a.status)
                ),
                &evidence_summary(request, &a.evidence_ids),
                &a.applies_to.join(", "),
                &format!(
                    "Reassess the declared dependencies if a critical assumption fails. Check: {}",
                    a.validation
                ),
            ],
        );
    }
    for c in &case.subclaims {
        let dependencies = case
            .warrants
            .iter()
            .filter(|w| w.premise_ids.contains(&c.id))
            .map(|w| format!("ARG-{} → {}", w.id, w.conclusion_id))
            .collect::<Vec<_>>()
            .join("; ");
        let routes = case
            .warrants
            .iter()
            .filter(|w| w.conclusion_id == c.id)
            .map(|w| format!("ARG-{}", w.id))
            .collect::<Vec<_>>()
            .join(", ");
        row(
            s,
            &[
                &c.id,
                &format!("{} Qualifier: {}", c.text, c.qualifier),
                &format!("Declared supporting arguments: {routes}. Separate verdict not recorded."),
                &dependencies,
                "Reassess each inference importing this proposition if its support or qualifier changes.",
            ],
        );
    }
    if case.assumptions.is_empty() && case.subclaims.is_empty() {
        row(
            s,
            &[
                "None recorded",
                "Assumption completeness remains unassessed.",
                "Not separately assessed.",
                "None recorded",
                "Revisit when the support changes.",
            ],
        );
    }
    writeln!(s, "\n## Challenge register\n").unwrap();
    table(s, &["Attack type", "Target and interpretation"]);
    row(
        s,
        &["Undermining", "Challenges a premise's truth or support."],
    );
    row(
        s,
        &[
            "Undercutting",
            "Challenges the applicability or reliability of an inference while its premises may hold.",
        ],
    );
    row(
        s,
        &[
            "Rebutting",
            "Supports an incompatible conclusion or subclaim.",
        ],
    );
    s.push('\n');
    table(
        s,
        &[
            "Challenge ID",
            "Target ID and attack type",
            "Objection and supporting evidence",
            "Evidential status",
            "Response or discriminating check",
            "Remaining effect",
        ],
    );
    for d in &case.defeaters {
        row(
            s,
            &[
                &d.id,
                &format!("{}: {}", d.target_id, label(&d.kind)),
                &format!(
                    "{} {}",
                    d.challenge,
                    evidence_summary(request, &d.evidence_ids)
                ),
                &label(&d.basis),
                &d.response,
                &format!(
                    "Disposition: {}. Material to target: {}.",
                    label(&d.status),
                    d.material
                ),
            ],
        );
    }
    if case.defeaters.is_empty() {
        row(
            s,
            &[
                "None recorded",
                "Completeness remains unassessed.",
                "No challenge recorded.",
                "Not assessed.",
                "No discriminating check recorded.",
                "Absence of a recorded attack is no guarantee of completeness.",
            ],
        );
    }
    writeln!(s,"\n## Evidence obligations for the stronger target\n\n**Current formulation — current_claim:** {}\n\n**Stronger target formulation — stronger_target:** {}\n\nPlans retain the claim each investigation would enable, including obligations needed for the current formulation.\n",prose(&case.current_claim.text),prose(&case.stronger_target.text)).unwrap();
    table(
        s,
        &[
            "Gap and affected inference",
            "Required evidence or reasoning",
            "Method, scope and comparison",
            "Assessment criterion and its justification",
            "Effect of favourable, adverse or inconclusive results",
            "Priority and next action",
        ],
    );
    let mut needs = case.evidence_plan.iter().collect::<Vec<_>>();
    needs.sort_by_key(|n| n.priority);
    for n in needs {
        row(
            s,
            &[
                &format!(
                    "{} targets {}; enables {}.",
                    n.id, n.target_id, n.enables_claim
                ),
                &n.question,
                &format!(
                    "Method: {} Measurement: {} Comparator: {}",
                    n.method, n.measurement, n.comparator
                ),
                &format!(
                    "Proposed criterion: {} Its independent justification must be established before decisive interpretation.",
                    n.acceptance_criterion
                ),
                &format!(
                    "Favourable: reassess {} with all other dependencies. Adverse: {} Inconclusive: leave the stronger target unresolved.",
                    n.enables_claim, n.if_failed
                ),
                &format!(
                    "Priority {}. Next action: {} Estimated effort: {}",
                    n.priority, n.method, n.cost
                ),
            ],
        );
    }
    if case.evidence_plan.is_empty() {
        row(
            s,
            &[
                "None recorded",
                "Additional evidence obligations were not specified.",
                "Not supplied.",
                "Not supplied.",
                "Reassess when material evidence changes.",
                "No next investigation recorded.",
            ],
        );
    }
}

fn composition(s: &mut String, case: &ArgumentCase, request: &Request) {
    writeln!(s,"\n## Composition overview for a complex case\n\n**Case question:** What formulation of the requested claim is justified? Requested claim: {}\n\n**Top-level claim — current_claim:** {}\n\n**Shared context:** {}\n\n**Composition warrant:** Each declared warrant identifies jointly required premises and its conclusion. Explicit composition records identify joint, cumulative or alternative support and the selected routes. Route selection is a model judgement whose evidential adequacy requires appraisal.\n\n**Required argument records:** {}\n",prose(&request.claim),prose(&format!("{} Qualifier: {}",case.current_claim.text,case.current_claim.qualifier)),prose(&scope_text(case)),case.warrants.iter().map(|w|format!("ARG-{}",w.id)).collect::<Vec<_>>().join(", ")).unwrap();
    writeln!(s, "### Claim register\n").unwrap();
    table(
        s,
        &[
            "Claim ID",
            "Exact proposition and applicable scope",
            "Supporting argument IDs",
            "Relevant challenges",
            "Current assessment",
        ],
    );
    let ids = std::iter::once("current_claim")
        .chain(case.subclaims.iter().map(|c| c.id.as_str()))
        .chain(std::iter::once("stronger_target"));
    for id in ids {
        let (text, qualifier) = claim(case, id);
        let routes = case
            .warrants
            .iter()
            .filter(|w| w.conclusion_id == id)
            .map(|w| format!("ARG-{}", w.id))
            .collect::<Vec<_>>()
            .join(", ");
        let challenges = case
            .defeaters
            .iter()
            .filter(|d| d.target_id == id)
            .map(|d| d.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        row(
            s,
            &[
                id,
                &format!("{text} Qualifier: {qualifier} Shared engineering scope applies."),
                &if routes.is_empty() {
                    "No warrant recorded.".into()
                } else {
                    routes
                },
                &if challenges.is_empty() {
                    "No direct challenge recorded; dependency attacks remain in the challenge register.".into()
                } else {
                    challenges
                },
                &claim_assessment(case, id),
            ],
        );
    }
    writeln!(s, "\n### Composition register\n").unwrap();
    table(
        s,
        &[
            "Argument ID",
            "Conclusion ID",
            "Ground claims or premises",
            "Warrant ID",
            "Required relationship among inputs",
            "Scope and required assumptions",
        ],
    );
    for w in &case.warrants {
        let nodes = inference_nodes(case, w);
        let assumptions = case
            .assumptions
            .iter()
            .filter(|a| nodes.contains(&a.id))
            .map(|a| a.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        row(
            s,
            &[
                &format!("ARG-{}", w.id),
                &w.conclusion_id,
                &w.premise_ids.join(", "),
                &w.id,
                &format!(
                    "Premises jointly required within this route. {}",
                    composition_text(case, &w.conclusion_id)
                ),
                &format!(
                    "Applicability: {} Assumptions: {}.",
                    w.applicability,
                    if assumptions.is_empty() {
                        "None recorded."
                    } else {
                        &assumptions
                    }
                ),
            ],
        );
    }
    writeln!(s, "\n### Relationship meanings\n").unwrap();
    table(s, &["Relationship", "Representation and assessment"]);
    let serial = case
        .warrants
        .iter()
        .flat_map(|w| {
            w.premise_ids
                .iter()
                .filter(|p| case.subclaims.iter().any(|c| &c.id == *p))
                .map(move |p| format!("{p} → ARG-{} → {}", w.id, w.conclusion_id))
        })
        .collect::<Vec<_>>()
        .join("; ");
    row(
        s,
        &[
            "Serial support",
            &if serial.is_empty() {
                "No imported subclaim recorded.".into()
            } else {
                serial
            },
        ],
    );
    row(
        s,
        &[
            "Joint support",
            "Each warrant's premises are jointly required. Missing support leaves that declared route unresolved.",
        ],
    );
    row(
        s,
        &["Alternative routes", &{
            let decisions = case
                .compositions
                .iter()
                .filter(|c| c.relationship == crate::domain::SupportRelationship::AlternativeRoutes)
                .map(|c| {
                    format!(
                        "{}: {}",
                        c.conclusion_id,
                        composition_text(case, &c.conclusion_id)
                    )
                })
                .collect::<Vec<_>>();
            if decisions.is_empty() {
                "No alternative-route composition recorded.".into()
            } else {
                joined(&decisions)
            }
        }],
    );
    row(
        s,
        &[
            "Cumulative support",
            "Combination requires an explicit warrant and adequate treatment of shared dependencies. No quantitative score combination is inferred.",
        ],
    );
    row(
        s,
        &[
            "Shared support or assumptions",
            "Canonical identifiers are reused across branches. Evidence and assumption registers show their declared uses.",
        ],
    );
    row(
        s,
        &[
            "Attack",
            "Challenges retain their premise, inference or conclusion target and recorded disposition.",
        ],
    );
    let unresolved = case
        .defeaters
        .iter()
        .filter(|d| {
            matches!(
                d.status,
                crate::domain::DefeaterStatus::Open | crate::domain::DefeaterStatus::Sustained
            )
        })
        .map(|d| format!("{} → {} ({})", d.id, d.target_id, label(&d.status)))
        .collect::<Vec<_>>();
    writeln!(s, "\n### Composition checks\n").unwrap();
    let checks=[
        ("Coverage","The declared graph identifies its premises and conclusions. Completeness of requirements, interfaces and interactions is not separately assessed.".to_owned()),
        ("Compatibility",format!("Shared scope: {} Individual transfer arguments are not separately recorded.",scope_text(case))),
        ("Support conditions",case.compositions.iter().map(|c|format!("{}: {}",c.conclusion_id,composition_text(case,&c.conclusion_id))).collect::<Vec<_>>().join(" ")),
        ("Shared dependencies","Evidence and assumption registers retain canonical identities and uses. Shared identities supply no independent replication.".into()),
        ("Conflict assessment",if unresolved.is_empty(){"No open or sustained challenge recorded. Completeness of challenge discovery remains unassessed.".into()}else{joined(&unresolved)}),
        ("Circularity check",match crate::domain::validate_case(request,case){Ok(())=>"Structural and reporting checks passed, including the acyclic support graph. This supplies no proof of semantic adequacy.".into(),Err(errors)=>format!("Structural or reporting defects remain: {}",errors.join(" "))}),
        ("Change impact","Use the composition register to follow an altered premise through each consuming warrant to current_claim or stronger_target. Reassess affected assumptions and challenges as well.".into()),
        ("Whole-case assessment",current_summary(case)),
    ];
    for (name, text) in checks {
        writeln!(s, "- **{name}:** {}", prose(&text)).unwrap();
    }
    let next = case
        .evidence_plan
        .iter()
        .min_by_key(|n| n.priority)
        .map(|n| {
            format!(
                "{}: {} The possible results affect {} and the support for {}.",
                n.id, n.question, n.target_id, n.enables_claim
            )
        })
        .unwrap_or_else(|| "No next investigation recorded.".into());
    writeln!(s,"\n### Whole-case conclusion\n\n> {} The stronger target is {} Its recorded qualification is {} The next investigation is {}\n",prose(&current_summary(case)),prose(&case.stronger_target.text),prose(&case.stronger_target.qualifier),prose(&next)).unwrap();
}

/// Render the populated engineering extended Toulmin template.
pub fn markdown(request: &Request, case: &ArgumentCase) -> String {
    let mut s = String::from("# Engineering Extended Toulmin Argument\n\n");
    writeln!(s, "## Argument context\n").unwrap();
    table(&mut s, &["Field", "Entry"]);
    row(
        &mut s,
        &[
            "Argument identity",
            &format!(
                "Engineering case; current conclusion current_claim. Argument records: {}. Separate case revision: not supplied.",
                case.warrants
                    .iter()
                    .map(|w| format!("ARG-{}", w.id))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ],
    );
    row(&mut s, &["Engineering question", &request.claim]);
    row(
        &mut s,
        &[
            "Intended use",
            &format!(
                "Evidence-bounded argument assessment and investigation planning. Supplied context: {} Reliance limits: {}",
                if request.context.trim().is_empty() {
                    "Not supplied."
                } else {
                    &request.context
                },
                joined(&case.decision_limits)
            ),
        ],
    );
    row(
        &mut s,
        &[
            "Claim type",
            &format!(
                "Not separately classified. Declared inference types: {}.",
                case.warrants
                    .iter()
                    .map(|w| label(&w.inference))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ],
    );
    row(
        &mut s,
        &[
            "Target identity",
            &format!(
                "{} Configuration: {}",
                case.scope.system_boundary, case.scope.configuration
            ),
        ],
    );
    row(
        &mut s,
        &[
            "Assessment boundary",
            &format!(
                "Evidence considered: {} Applicable period: {} Operating conditions: {} Exclusions and reliance limits: {}",
                request
                    .evidence
                    .iter()
                    .map(|e| e.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                case.scope.time_horizon,
                case.scope.operating_conditions,
                joined(&case.decision_limits)
            ),
        ],
    );
    if case.warrants.is_empty() {
        writeln!(s,"\n## Argument record\n\nNo inference warrant was supplied. A completed 14-component inference record requires an identified warrant and its conclusion.\n").unwrap();
    }
    for w in &case.warrants {
        argument_record(&mut s, case, request, w);
    }
    registers(&mut s, case, request);
    writeln!(s,"\n## Assessed argument\n\n> {} Declared grounds: {} Declared warrants: {} Consequential recorded challenges: {} The stronger target is {} Additional support is specified by evidence obligations {}.\n",prose(&current_summary(case)),case.grounds.iter().map(|g|g.id.as_str()).collect::<Vec<_>>().join(", "),case.warrants.iter().map(|w|w.id.as_str()).collect::<Vec<_>>().join(", "),case.defeaters.iter().filter(|d|d.material).map(|d|d.id.as_str()).collect::<Vec<_>>().join(", "),prose(&case.stronger_target.text),case.evidence_plan.iter().map(|n|n.id.as_str()).collect::<Vec<_>>().join(", ")).unwrap();
    for section in &case.reading {
        writeln!(
            s,
            "\n### {} — {}\n",
            prose(&section.title),
            prose(&section.claim_id)
        )
        .unwrap();
        for paragraph in &section.paragraphs {
            writeln!(
                s,
                "{} (References: {}.)\n",
                prose(&paragraph.text),
                paragraph
                    .reference_ids
                    .iter()
                    .map(|id| prose(id))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .unwrap();
        }
    }
    if case.warrants.len() > 1 || !case.subclaims.is_empty() {
        composition(&mut s, case, request);
    }
    writeln!(s,"\n## Interpretation notes\n\n- Source identification and structural integrity support traceability. Evidence adequacy and inferential soundness require separate appraisal.\n- Proposed investigation criteria remain proposed until adopted and tested. No investigation is represented as completed merely because it appears in this report.\n- The top-level status assesses current_claim. Subclaims and stronger_target retain their own wording and qualifications; a separate verdict is not inferred.\n- Residual risks: {}\n- Decision limits: {}\n",prose(&joined(&case.residual_risks)),prose(&joined(&case.decision_limits))).unwrap();
    s
}

/// Include the complete report without letting embedded backticks end its fence.
pub fn with_audit(
    mut markdown: String,
    report: &impl serde::Serialize,
) -> Result<String, crate::Error> {
    let json = serde_json::to_string_pretty(report)?;
    let longest = json
        .split(|c| c != char::from(96))
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = char::from(96).to_string().repeat((longest + 1).max(3));
    writeln!(
        markdown,
        "\n## Complete audit record\n\n{fence}json\n{json}\n{fence}"
    )
    .unwrap();
    Ok(markdown)
}
