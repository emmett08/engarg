//! Populate the supplied engineering extended Toulmin template from canonical records.
//!
//! Rendering preserves recorded assessment status. It labels information that has
//! not been supplied rather than filling evidential gaps with generated findings.

use crate::domain::{ArgumentCase, Assumption, Defeater, EvidenceNeed, Request, Warrant};
use crate::presentations::{BoundedText, PresentationView, RenderError, RenderLimits};
use std::collections::BTreeSet;
use std::fmt::Write;

fn label<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

mod text;
use std::fmt::Display;
use text::{fragment, prose};

// Each cell is evaluated only while output remains admissible. Display fragments
// borrow canonical records and stop expanding as soon as the checked sink fails.
macro_rules! row {
    ($output:expr, &[$($value:expr),* $(,)?] $(,)?) => {{
        let output: &mut BoundedText = &mut *$output;
        if !output.failed() {
            output.push('|');
            $(
                if !output.failed() {
                    output.push(' ');
                    text::cell(output, &$value);
                    if !output.failed() { output.push_str(" |"); }
                }
            )*
            if !output.failed() { output.push('\n'); }
        }
    }};
}

fn table(s: &mut BoundedText, headers: &[&str]) {
    for values in [headers, &vec!["---"; headers.len()]] {
        if s.failed() {
            return;
        }
        s.push('|');
        for value in values {
            if s.failed() {
                return;
            }
            s.push(' ');
            text::cell(s, value);
            if !s.failed() {
                s.push_str(" |");
            }
        }
        s.push('\n');
    }
}

fn joined(values: &[String]) -> String {
    if values.is_empty() {
        "None recorded.".into()
    } else {
        values.join(" ")
    }
}

fn list<I>(values: I, separator: &'static str, empty: &'static str) -> impl Display
where
    I: Iterator + Clone,
    I::Item: Display,
{
    fragment(move |s| {
        let mut values = values.clone().peekable();
        if values.peek().is_none() {
            return s.write_str(empty);
        }
        for (index, value) in values.enumerate() {
            if index > 0 {
                s.write_str(separator)?;
            }
            write!(s, "{value}")?;
        }
        Ok(())
    })
}

fn claim<'a>(view: &PresentationView<'a>, id: &str) -> (&'a str, &'a str) {
    view.claim(id).unwrap_or((
        "Claim reference is absent from this assessment.",
        "Not assessed.",
    ))
}

fn claim_assessment<'a>(case: &'a ArgumentCase, id: &'a str) -> impl Display + 'a {
    fragment(move |s| {
        match id {
        "current_claim" => write!(s, "Current assessment: {}. {}", label(&case.status), case.adjudication),
        "stronger_target" => s.write_str("Conditional target. Its fulfilment has not been established by this report."),
        _ => s.write_str("Separate subclaim verdict not recorded. Its stated qualifier and dependencies govern its use."),
    }
    })
}

fn composition_text<'a>(view: &'a PresentationView<'_>, conclusion: &'a str) -> impl Display + 'a {
    fragment(move |s| match view.composition(conclusion) {
        Some(c) => write!(
            s,
            "Relationship: {}. Selected warrants: {}. Model assessment rationale: {} Selection records a judgement; its adequacy requires the referenced reasoning and evidence.",
            label(&c.relationship),
            list(
                c.selected_warrant_ids.iter(),
                ", ",
                "None; this composition remains unresolved."
            ),
            c.rationale
        ),
        None => s.write_str("No composition assessment recorded for this conclusion."),
    })
}

fn evidence_refs<'a, I>(view: &'a PresentationView<'_>, ids: I) -> impl Display + 'a
where
    I: Iterator<Item = &'a str> + Clone + 'a,
{
    list(
        ids.map(move |id| {
            fragment(move |s| match view.evidence(id) {
                Some(e) => write!(
                    s,
                    "{id}: {}. Source/location: {}. Kind: {}. User asserts checking: {}.",
                    e.title, e.source, e.kind, e.checked_by_user
                ),
                None => write!(s, "{id}: reference absent from supplied evidence."),
            })
        }),
        " ",
        "No supporting evidence reference supplied.",
    )
}

fn evidence_summary<'a>(view: &'a PresentationView<'_>, ids: &'a [String]) -> impl Display + 'a {
    evidence_refs(view, ids.iter().map(String::as_str))
}

fn node_description<'a>(view: &'a PresentationView<'_>, id: &'a str) -> impl Display + 'a {
    fragment(move |s| {
        if let Some(g) = view.ground(id) {
            write!(
                s,
                "{id}: {} Status: {}. {}",
                g.statement,
                label(&g.status),
                evidence_summary(view, &g.evidence_ids)
            )
        } else {
            let (text, qualifier) = claim(view, id);
            write!(s, "{id}: {text} Qualifier: {qualifier}")
        }
    })
}

fn assumption_text<'a>(a: &'a Assumption, view: &'a PresentationView<'_>) -> impl Display + 'a {
    fragment(move |s| {
        write!(
            s,
            "{}: {} Critical: {}. Status: {}. Applies to: {}. Basis: {} Validation/check: {}. If a critical assumption fails, reassess each declared dependent inference.",
            a.id,
            a.statement,
            a.critical,
            label(&a.status),
            list(a.applies_to.iter(), ", ", ""),
            evidence_summary(view, &a.evidence_ids),
            a.validation
        )
    })
}

fn challenge_text<'a>(d: &'a Defeater, view: &'a PresentationView<'_>) -> impl Display + 'a {
    fragment(move |s| {
        write!(
            s,
            "{} targets {} by {}: {} Basis: {}. Material: {}. Disposition: {}. Response and remaining effect: {} Evidence: {}",
            d.id,
            d.target_id,
            label(&d.kind),
            d.challenge,
            label(&d.basis),
            d.material,
            label(&d.status),
            d.response,
            evidence_summary(view, &d.evidence_ids)
        )
    })
}

fn need_text(n: &EvidenceNeed) -> impl Display + '_ {
    fragment(move |s| {
        write!(
            s,
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
    })
}

fn scope_text(case: &ArgumentCase) -> impl Display + '_ {
    fragment(move |s| {
        write!(
            s,
            "System boundary: {} Configuration: {} Operating conditions: {} Measures and units: {} Applicable period: {} Separate term definitions and established property criteria: not supplied unless explicitly stated in the foregoing assessment. Additional transfer justification is required for other configurations or operating conditions.",
            case.scope.system_boundary,
            case.scope.configuration,
            case.scope.operating_conditions,
            case.scope.units,
            case.scope.time_horizon
        )
    })
}

fn alternatives_text<'a>(view: &'a PresentationView<'_>) -> impl Display + 'a {
    list(view.case.alternatives.iter().map(move |a| fragment(move |s| write!(
        s,
        "Alternative: {} Discriminating check: {} Evidence: {} An independently justified common decision criterion is not separately recorded.",
        a.explanation, a.distinguishing_test, evidence_summary(view, &a.evidence_ids)
    ))), " ", "No competing explanation or option recorded. Completeness has not been established.")
}

fn current_summary(case: &ArgumentCase) -> impl Display + '_ {
    fragment(move |s| {
        write!(
            s,
            "current_claim: {} Qualifier: {} Assessment: {}. Decisive reasoning and limitations: {}",
            case.current_claim.text,
            case.current_claim.qualifier,
            label(&case.status),
            case.adjudication
        )
    })
}

fn decision_text(case: &ArgumentCase) -> impl Display + '_ {
    fragment(move |s| {
        write!(
            s,
            "Consequences: {} Reliance limits: {} Residual risks: {} A separate action authorisation, value criterion and action acceptance decision are not recorded. Report revision policy: reassess affected inferences when evidence, assumptions, target configuration or operating conditions change.",
            case.scope.consequences,
            list(case.decision_limits.iter(), " ", "None recorded."),
            list(case.residual_risks.iter(), " ", "None recorded.")
        )
    })
}

fn composition_check<T: Display + ?Sized>(s: &mut BoundedText, name: &str, value: &T) {
    if s.failed() {
        return;
    }
    write!(s, "- **{name}:** ").unwrap();
    text::prose_into(s, value);
    if !s.failed() {
        s.push('\n');
    }
}

fn argument_record(s: &mut BoundedText, view: &PresentationView<'_>, w: &Warrant) {
    if s.failed() {
        return;
    }
    let case = view.case;
    let nodes = view.inference_nodes(w);
    let (conclusion, qualifier) = claim(view, &w.conclusion_id);
    let assumptions = || {
        case.assumptions
            .iter()
            .filter(|a| nodes.contains(a.id.as_str()))
    };
    let challenges = || {
        case.defeaters
            .iter()
            .filter(|d| nodes.contains(d.id.as_str()))
    };
    let needs = || {
        case.evidence_plan
            .iter()
            .filter(|n| nodes.contains(n.target_id.as_str()) || n.enables_claim == w.conclusion_id)
    };
    let mut evidence_ids: BTreeSet<&str> =
        w.backing_evidence_ids.iter().map(String::as_str).collect();
    for g in case
        .grounds
        .iter()
        .filter(|g| nodes.contains(g.id.as_str()))
    {
        evidence_ids.extend(g.evidence_ids.iter().map(String::as_str));
    }
    for a in assumptions() {
        evidence_ids.extend(a.evidence_ids.iter().map(String::as_str));
    }
    for d in challenges() {
        evidence_ids.extend(d.evidence_ids.iter().map(String::as_str));
    }
    writeln!(s, "\n## Argument record — ARG-{}\n", prose(&w.id)).unwrap();
    table(s, &["Component", "Engineering entry"]);
    row!(
        s,
        &[format_args!("1. Claim — {}", w.conclusion_id), conclusion]
    );
    row!(
        s,
        &[
            "2. Grounds — jointly required premises",
            format_args!(
                "{} These premises are jointly required within ARG-{}.",
                list(
                    w.premise_ids.iter().map(|id| node_description(view, id)),
                    " ",
                    ""
                ),
                w.id
            )
        ]
    );
    row!(
        s,
        &[
            format_args!("3. Warrant — {}", w.id),
            format_args!(
                "{} Given premises {} and assumptions {}, this warrant connects them to {}. Inference type: {}. Applicability: {}",
                w.rule,
                list(w.premise_ids.iter(), ", ", ""),
                list(assumptions().map(|a| &a.id), ", ", "none recorded"),
                w.conclusion_id,
                label(&w.inference),
                w.applicability
            )
        ]
    );
    row!(
        s,
        &[
            "4. Backing",
            fragment(|s| {
                if w.backing_evidence_ids.is_empty() {
                    write!(
                        s,
                        "No backing evidence supplied for {}. Its rule and applicability remain recorded reasoning requiring appraisal.",
                        w.id
                    )
                } else {
                    write!(
                        s,
                        "{} Separate adequacy and domain-of-applicability justification for this backing is not recorded beyond the warrant's applicability statement.",
                        evidence_summary(view, &w.backing_evidence_ids)
                    )
                }
            })
        ]
    );
    row!(
        s,
        &[
            "5. Qualifier",
            format_args!(
                "{qualifier} A separately calibrated numerical uncertainty estimate is not recorded."
            )
        ]
    );
    row!(
        s,
        &[
            "6. Rebuttal conditions",
            fragment(|s| {
                if challenges().next().is_none() {
                    write!(
                        s,
                        "No specific rebuttal condition is recorded for {}. Applicability boundary: {} Reassess the inference if evidence or analysis establishes that this boundary fails.",
                        w.id, w.applicability
                    )
                } else {
                    write!(
                        s,
                        "{} Applicability boundary: {} An established exception requires reassessment or qualification of this inference.",
                        list(challenges().map(|d| challenge_text(d, view)), " ", ""),
                        w.applicability
                    )
                }
            })
        ]
    );
    row!(s, &["7. Scope, definitions and criteria", scope_text(case)]);
    row!(
        s,
        &[
            "8. Evidence provenance and adequacy",
            format_args!(
                "{} Full findings and known shared uses are in the evidence register. Separate source dates, acquisition details and adequacy judgements are not inferred from identifiers.",
                evidence_refs(view, evidence_ids.iter().copied())
            )
        ]
    );
    row!(
        s,
        &[
            "9. Critical assumptions and dependencies",
            format_args!(
                "{} Required ground or subclaim identifiers: {}.",
                list(
                    assumptions().map(|a| assumption_text(a, view)),
                    " ",
                    "No assumptions recorded for this inference."
                ),
                list(w.premise_ids.iter(), ", ", "")
            )
        ]
    );
    row!(
        s,
        &[
            "10. Competing explanations or options",
            alternatives_text(view)
        ]
    );
    row!(
        s,
        &[
            "11. Challenges and responses",
            list(
                challenges().map(|d| challenge_text(d, view)),
                " ",
                "No targeted challenge recorded for this inference and its imported support. Completeness remains unassessed."
            )
        ]
    );
    row!(
        s,
        &[
            "12. Stronger target and evidence obligations",
            format_args!(
                "stronger_target: {} Qualifier: {} Relevant obligations: {}",
                case.stronger_target.text,
                case.stronger_target.qualifier,
                list(
                    needs().map(need_text),
                    " ",
                    "No additional obligation is separately recorded for this inference."
                )
            )
        ]
    );
    row!(
        s,
        &[
            "13. Current assessment and defensible claim",
            format_args!(
                "Record conclusion {}: {} Qualifier: {} {} This argument record is {} for its conclusion. {} Overall current formulation: {}",
                w.conclusion_id,
                conclusion,
                qualifier,
                claim_assessment(case, &w.conclusion_id),
                if view.selected(w).unwrap_or(false) {
                    "selected"
                } else {
                    "a retained, unselected candidate"
                },
                composition_text(view, &w.conclusion_id),
                current_summary(case)
            )
        ]
    );
    row!(
        s,
        &[
            "14. Decision basis and revision triggers",
            decision_text(case)
        ]
    );
}

fn evidence_register(s: &mut BoundedText, view: &PresentationView<'_>) {
    let request = view.request;
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
        if s.failed() {
            return;
        }
        let uses = view.evidence_uses(&e.id);
        row!(
            s,
            &[
                &e.id,
                &if uses.is_empty() {
                    "No argument use recorded.".into()
                } else {
                    uses.join("; ")
                },
                &format_args!(
                    "{} Source/location: {}. Version/date: not separately supplied unless included in that provenance.",
                    e.title, e.source
                ),
                &format_args!("Kind: {}. Supplied finding/excerpt: {}", e.kind, e.excerpt),
                &format_args!(
                    "Available as supplied text. User asserts checking: {}. Independent verification by this workflow: not performed.",
                    e.checked_by_user
                ),
                &format_args!(
                    "Relevance, reliability, coverage and uncertainty require appraisal for the intended use. Known uses: {}. Reuse of one evidence identity supplies no independent replication.",
                    uses.len()
                ),
            ],
        );
    }
    if request.evidence.is_empty() {
        row!(
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

fn registers(s: &mut BoundedText, view: &PresentationView<'_>) {
    let case = view.case;
    evidence_register(s, view);
    if s.failed() {
        return;
    }
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
        if s.failed() {
            return;
        }
        row!(
            s,
            &[
                &a.id,
                &format_args!(
                    "{} Critical: {}. Status: {}.",
                    a.statement,
                    a.critical,
                    label(&a.status)
                ),
                &evidence_summary(view, &a.evidence_ids),
                &a.applies_to.join(", "),
                &format_args!(
                    "Reassess the declared dependencies if a critical assumption fails. Check: {}",
                    a.validation
                ),
            ],
        );
    }
    for c in &case.subclaims {
        if s.failed() {
            return;
        }
        let dependencies = view
            .index
            .consumers_for(&c.id)
            .iter()
            .map(|w| format!("ARG-{} → {}", w.id, w.conclusion_id))
            .collect::<Vec<_>>()
            .join("; ");
        let routes = view
            .warrants(&c.id)
            .iter()
            .map(|w| format!("ARG-{}", w.id))
            .collect::<Vec<_>>()
            .join(", ");
        row!(
            s,
            &[
                &c.id,
                &format_args!("{} Qualifier: {}", c.text, c.qualifier),
                &format_args!(
                    "Declared supporting arguments: {routes}. Separate verdict not recorded."
                ),
                &dependencies,
                "Reassess each inference importing this proposition if its support or qualifier changes.",
            ],
        );
    }
    if case.assumptions.is_empty() && case.subclaims.is_empty() {
        row!(
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
    row!(
        s,
        &["Undermining", "Challenges a premise's truth or support."],
    );
    row!(
        s,
        &[
            "Undercutting",
            "Challenges the applicability or reliability of an inference while its premises may hold.",
        ],
    );
    row!(
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
        if s.failed() {
            return;
        }
        row!(
            s,
            &[
                &d.id,
                &format_args!("{}: {}", d.target_id, label(&d.kind)),
                &format_args!(
                    "{} {}",
                    d.challenge,
                    evidence_summary(view, &d.evidence_ids)
                ),
                &label(&d.basis),
                &d.response,
                &format_args!(
                    "Disposition: {}. Material to target: {}.",
                    label(&d.status),
                    d.material
                ),
            ],
        );
    }
    if case.defeaters.is_empty() {
        row!(
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
        if s.failed() {
            return;
        }
        row!(
            s,
            &[
                &format_args!(
                    "{} targets {}; enables {}.",
                    n.id, n.target_id, n.enables_claim
                ),
                &n.question,
                &format_args!(
                    "Method: {} Measurement: {} Comparator: {}",
                    n.method, n.measurement, n.comparator
                ),
                &format_args!(
                    "Proposed criterion: {} Its independent justification must be established before decisive interpretation.",
                    n.acceptance_criterion
                ),
                &format_args!(
                    "Favourable: reassess {} with all other dependencies. Adverse: {} Inconclusive: leave the stronger target unresolved.",
                    n.enables_claim, n.if_failed
                ),
                &format_args!(
                    "Priority {}. Next action: {} Estimated effort: {}",
                    n.priority, n.method, n.cost
                ),
            ],
        );
    }
    if case.evidence_plan.is_empty() {
        row!(
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

fn composition(s: &mut BoundedText, view: &PresentationView<'_>) {
    if s.failed() {
        return;
    }
    let case = view.case;
    let request = view.request;
    writeln!(s,"\n## Composition overview for a complex case\n\n**Case question:** What formulation of the requested claim is justified? Requested claim: {}\n\n**Top-level claim — current_claim:** {}\n\n**Shared context:** {}\n\n**Composition warrant:** Each declared warrant identifies jointly required premises and its conclusion. Explicit composition records identify joint, cumulative or alternative support and the selected routes. Route selection is a model judgement whose evidential adequacy requires appraisal.\n\n**Required argument records:** {}\n",prose(&request.claim),prose(&format_args!("{} Qualifier: {}",case.current_claim.text,case.current_claim.qualifier)),prose(&scope_text(case)),case.warrants.iter().map(|w|format!("ARG-{}",w.id)).collect::<Vec<_>>().join(", ")).unwrap();
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
        if s.failed() {
            return;
        }
        let (text, qualifier) = claim(view, id);
        let routes = view
            .warrants(id)
            .iter()
            .map(|w| format!("ARG-{}", w.id))
            .collect::<Vec<_>>()
            .join(", ");
        let challenges = view
            .index
            .defeaters_for(id)
            .iter()
            .map(|d| d.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        row!(
            s,
            &[
                id,
                &format_args!("{text} Qualifier: {qualifier} Shared engineering scope applies."),
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
        if s.failed() {
            return;
        }
        let nodes = view.inference_nodes(w);
        let assumptions = case
            .assumptions
            .iter()
            .filter(|a| nodes.contains(a.id.as_str()))
            .map(|a| a.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        row!(
            s,
            &[
                &format_args!("ARG-{}", w.id),
                &w.conclusion_id,
                &w.premise_ids.join(", "),
                &w.id,
                &format_args!(
                    "Premises jointly required within this route. {}",
                    composition_text(view, &w.conclusion_id)
                ),
                &format_args!(
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
                .filter(|p| {
                    view.claim(p).is_some()
                        && p.as_str() != "current_claim"
                        && p.as_str() != "stronger_target"
                })
                .map(move |p| format!("{p} → ARG-{} → {}", w.id, w.conclusion_id))
        })
        .collect::<Vec<_>>()
        .join("; ");
    row!(
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
    row!(
        s,
        &[
            "Joint support",
            "Each warrant's premises are jointly required. Missing support leaves that declared route unresolved.",
        ],
    );
    row!(
        s,
        &[
            "Alternative routes",
            list(
                case.compositions
                    .iter()
                    .filter(
                        |c| c.relationship == crate::domain::SupportRelationship::AlternativeRoutes
                    )
                    .map(|c| fragment(move |s| write!(
                        s,
                        "{}: {}",
                        c.conclusion_id,
                        composition_text(view, &c.conclusion_id)
                    ))),
                " ",
                "No alternative-route composition recorded.",
            ),
        ],
    );
    row!(
        s,
        &[
            "Cumulative support",
            "Combination requires an explicit warrant and adequate treatment of shared dependencies. No quantitative score combination is inferred.",
        ],
    );
    row!(
        s,
        &[
            "Shared support or assumptions",
            "Canonical identifiers are reused across branches. Evidence and assumption registers show their declared uses.",
        ],
    );
    row!(
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
        .map(|d| fragment(move |s| write!(s, "{} → {} ({})", d.id, d.target_id, label(&d.status))));
    writeln!(s, "\n### Composition checks\n").unwrap();
    composition_check(
        s,
        "Coverage",
        "The declared graph identifies its premises and conclusions. Completeness of requirements, interfaces and interactions is not separately assessed.",
    );
    composition_check(
        s,
        "Compatibility",
        &format_args!(
            "Shared scope: {} Individual transfer arguments are not separately recorded.",
            scope_text(case)
        ),
    );
    composition_check(
        s,
        "Support conditions",
        &list(
            case.compositions.iter().map(|c| {
                fragment(move |s| {
                    write!(
                        s,
                        "{}: {}",
                        c.conclusion_id,
                        composition_text(view, &c.conclusion_id)
                    )
                })
            }),
            " ",
            "",
        ),
    );
    composition_check(
        s,
        "Shared dependencies",
        "Evidence and assumption registers retain canonical identities and uses. Shared identities supply no independent replication.",
    );
    composition_check(
        s,
        "Conflict assessment",
        &list(
            unresolved,
            " ",
            "No open or sustained challenge recorded. Completeness of challenge discovery remains unassessed.",
        ),
    );
    if s.failed() {
        return;
    }
    composition_check(s, "Circularity check", &view.validation_summary());
    composition_check(
        s,
        "Change impact",
        "Use the composition register to follow an altered premise through each consuming warrant to current_claim or stronger_target. Reassess affected assumptions and challenges as well.",
    );
    composition_check(s, "Whole-case assessment", &current_summary(case));
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
pub fn markdown(request: &Request, case: &ArgumentCase) -> Result<String, RenderError> {
    markdown_with_limits(request, case, RenderLimits::default())
}

pub fn markdown_with_limits(
    request: &Request,
    case: &ArgumentCase,
    limits: RenderLimits,
) -> Result<String, RenderError> {
    let view = PresentationView::new(request, case, limits)?;
    let mut s = BoundedText::new(limits.max_output_bytes);
    s.push_str("# Engineering Extended Toulmin Argument\n\n");
    if let Some(notice) = view.inspection_notice() {
        writeln!(s, "> {}\n", prose(&notice)).unwrap();
    }
    writeln!(s, "## Argument context\n").unwrap();
    table(&mut s, &["Field", "Entry"]);
    row!(
        &mut s,
        &[
            "Argument identity",
            &format_args!(
                "Engineering case; current conclusion current_claim. Argument records: {}. Separate case revision: not supplied.",
                case.warrants
                    .iter()
                    .map(|w| format!("ARG-{}", w.id))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ],
    );
    row!(&mut s, &["Engineering question", &request.claim]);
    row!(
        &mut s,
        &[
            "Intended use",
            &format_args!(
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
    row!(
        &mut s,
        &[
            "Claim type",
            &format_args!(
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
    row!(
        &mut s,
        &[
            "Target identity",
            &format_args!(
                "{} Configuration: {}",
                case.scope.system_boundary, case.scope.configuration
            ),
        ],
    );
    row!(
        &mut s,
        &[
            "Assessment boundary",
            &format_args!(
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
        if s.failed() {
            return s.finish();
        }
        argument_record(&mut s, &view, w);
    }
    registers(&mut s, &view);
    if s.failed() {
        return s.finish();
    }
    writeln!(s,"\n## Assessed argument\n\n> {} Declared grounds: {} Declared warrants: {} Consequential recorded challenges: {} The stronger target is {} Additional support is specified by evidence obligations {}.\n",prose(&current_summary(case)),case.grounds.iter().map(|g|g.id.as_str()).collect::<Vec<_>>().join(", "),case.warrants.iter().map(|w|w.id.as_str()).collect::<Vec<_>>().join(", "),case.defeaters.iter().filter(|d|d.material).map(|d|d.id.as_str()).collect::<Vec<_>>().join(", "),prose(&case.stronger_target.text),case.evidence_plan.iter().map(|n|n.id.as_str()).collect::<Vec<_>>().join(", ")).unwrap();
    for section in &case.reading {
        if s.failed() {
            return s.finish();
        }
        writeln!(
            s,
            "\n### {} — {}\n",
            prose(&section.title),
            prose(&section.claim_id)
        )
        .unwrap();
        for paragraph in &section.paragraphs {
            if s.failed() {
                return s.finish();
            }
            writeln!(
                s,
                "{} (References: {}.)\n",
                prose(&paragraph.text),
                list(paragraph.reference_ids.iter().map(prose), ", ", "")
            )
            .unwrap();
        }
    }
    if s.failed() {
        return s.finish();
    }
    if case.warrants.len() > 1 || !case.subclaims.is_empty() {
        composition(&mut s, &view);
    }
    writeln!(s,"\n## Interpretation notes\n\n- Source identification and structural integrity support traceability. Evidence adequacy and inferential soundness require separate appraisal.\n- Proposed investigation criteria remain proposed until adopted and tested. No investigation is represented as completed merely because it appears in this report.\n- The top-level status assesses current_claim. Subclaims and stronger_target retain their own wording and qualifications; a separate verdict is not inferred.\n- Residual risks: {}\n- Decision limits: {}\n",prose(&joined(&case.residual_risks)),prose(&joined(&case.decision_limits))).unwrap();
    view.finish()?;
    s.finish()
}

/// Include the complete report with bounded output and an injection-safe fence.
pub fn with_audit(markdown: String, report: &serde_json::Value) -> Result<String, crate::Error> {
    with_audit_limits(markdown, report, RenderLimits::default())
}

pub fn with_audit_limits(
    markdown: String,
    report: &serde_json::Value,
    limits: RenderLimits,
) -> Result<String, crate::Error> {
    let budget_error = |e: RenderError| crate::Error::Budget(e.to_string());
    if markdown.len() > limits.max_output_bytes
        || limits.max_output_bytes.saturating_mul(3) > limits.max_working_bytes
    {
        return Err(budget_error(RenderError::ResourceLimit("audit bytes")));
    }
    // Admit JSON iteratively before serde's recursive pretty printer. The audit
    // boundary accepts decoded JSON so its depth and work can be inspected.
    let mut pending = vec![(report, 0usize)];
    let mut visited = 0usize;
    let mut bytes = 0usize;
    let mut edges = 0usize;
    while let Some((value, depth)) = pending.pop() {
        visited = visited.saturating_add(1);
        if depth > 128 || visited > limits.max_visits || visited > limits.max_nodes {
            return Err(budget_error(RenderError::ResourceLimit("audit graph")));
        }
        let children = match value {
            serde_json::Value::Array(v) => v.len(),
            serde_json::Value::Object(v) => v.len(),
            _ => 0,
        };
        edges = edges.saturating_add(children);
        let pending_count = pending.len().saturating_add(children);
        let working = pending_count
            .saturating_mul(std::mem::size_of::<(&serde_json::Value, usize)>())
            .saturating_add(limits.max_output_bytes.saturating_mul(3));
        if edges > limits.max_edges
            || pending_count > limits.max_nodes
            || working > limits.max_working_bytes
        {
            return Err(budget_error(RenderError::ResourceLimit("audit storage")));
        }
        match value {
            serde_json::Value::Array(values) => {
                pending.extend(values.iter().map(|value| (value, depth + 1)))
            }
            serde_json::Value::Object(values) => {
                for (key, value) in values {
                    bytes = bytes.saturating_add(key.len());
                    pending.push((value, depth + 1));
                }
            }
            serde_json::Value::String(text) => bytes = bytes.saturating_add(text.len()),
            _ => {}
        }
        if pending.len() > limits.max_edges || bytes > limits.max_input_bytes {
            return Err(budget_error(RenderError::ResourceLimit("audit input")));
        }
    }
    let mut json = BoundedText::new(limits.max_output_bytes);
    {
        struct JsonWriter<'a>(&'a mut BoundedText);
        impl std::io::Write for JsonWriter<'_> {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                let text = std::str::from_utf8(bytes).map_err(std::io::Error::other)?;
                self.0.push_str(text);
                if self.0.failed() {
                    return Err(std::io::Error::other("audit output budget exceeded"));
                }
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        serde_json::to_writer_pretty(JsonWriter(&mut json), report)?;
    }
    let json = json.finish().map_err(budget_error)?;
    let longest = json
        .split(|c| c != char::from(96))
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence_len = (longest + 1).max(3);
    if markdown
        .len()
        .saturating_add(json.len())
        .saturating_add(fence_len.saturating_mul(2))
        .saturating_add(64)
        > limits.max_output_bytes
    {
        return Err(budget_error(RenderError::ResourceLimit("output bytes")));
    }
    let fence = char::from(96).to_string().repeat(fence_len);
    let mut output = BoundedText::new(limits.max_output_bytes);
    output.push_str(&markdown);
    writeln!(
        output,
        "\n## Complete audit record\n\n{fence}json\n{json}\n{fence}"
    )
    .unwrap();
    output.finish().map_err(budget_error)
}
