//! Inspection and reading views of the same canonical argument graph.
//!
//! These views preserve recorded assessments. Formatting does not establish the
//! truth of a claim or the semantic fidelity of a model-authored paragraph.

use crate::domain::{ArgumentCase, Request, SupportComposition, SupportRelationship, Warrant};
use std::{
    collections::{HashSet, VecDeque},
    fmt::Write,
};

mod budget;
mod html;
mod plain;
mod view;
pub(crate) use budget::BoundedText;
pub use budget::{RenderError, RenderLimits};
pub use html::{html, html_with_depth, html_with_limits};
pub use plain::{plain_lines, plain_lines_with_depth, plain_lines_with_limits};
pub(crate) use view::{Attachment, Condition, PresentationView};

fn label<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
        .replace('_', " ")
}

fn inline(value: &str) -> String {
    let mut result = String::new();
    for character in value.chars() {
        match character {
            '\n' | '\r' => result.push(' '),
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '\\' | '`' | '*' | '_' | '[' | ']' | '#' | '|' => {
                result.push('\\');
                result.push(character);
            }
            _ => result.push(character),
        }
    }
    result
}

/// Generated anchors avoid assigning source text to an HTML attribute or URL.
fn anchor(id: &str) -> String {
    let mut result = String::from("node-");
    for byte in id.as_bytes() {
        write!(result, "{byte:02x}").unwrap();
    }
    result
}

fn md_reference(id: &str) -> String {
    format!("[{}](#{})", inline(id), anchor(id))
}

fn claim<'a>(view: &PresentationView<'a>, id: &str) -> Option<(&'a str, &'a str)> {
    view.claim(id)
}

fn composition<'a>(view: &PresentationView<'a>, id: &str) -> Option<&'a SupportComposition> {
    view.composition(id)
}

fn relationship(value: SupportRelationship) -> &'static str {
    match value {
        SupportRelationship::JointlyRequired => "Jointly required supporting arguments",
        SupportRelationship::AlternativeRoutes => "Alternative supporting arguments",
        SupportRelationship::Cumulative => "Cumulative evidence and arguments",
    }
}

fn selection(view: &PresentationView<'_>, warrant: &Warrant) -> &'static str {
    match view.selected(warrant) {
        Some(true) => "relied-upon route",
        Some(false) => "candidate route, not selected for reliance",
        None => "reliance relationship not declared",
    }
}

/// Consequential conditions use the shared selected-dependency semantics.
fn conditions<'a>(view: &PresentationView<'a>, root: &'a str) -> Vec<String> {
    view.conditions(root)
        .into_iter()
        .map(|condition| match condition {
            Condition::Assumption(a) => format!(
                "Critical condition {} ({}): {}",
                a.id,
                label(&a.status),
                a.statement
            ),
            Condition::Challenge(d) => format!(
                "Outstanding challenge {} ({}): {}",
                d.id,
                label(&d.status),
                d.challenge
            ),
        })
        .collect()
}

fn direct_notes(view: &PresentationView<'_>, id: &str) -> Vec<String> {
    let Some(id) = view.canonical_id(id) else {
        return Vec::new();
    };
    view.attachment_walk(id, &mut HashSet::new()).into_iter().map(|attachment| match attachment {
        Attachment::Assumption(a) => format!("{}Assumption {} ({}): {} Check: {}", if a.critical { "Critical " } else { "" }, a.id, label(&a.status), a.statement, a.validation),
        Attachment::Challenge(d) => format!("{}Challenge {} — {} ({}; {}): {} Response: {}", if d.material { "Material " } else { "" }, d.id, label(&d.kind), label(&d.basis), label(&d.status), d.challenge, d.response),
        Attachment::Investigation(n) => format!("Investigation {} targets {} and could strengthen {}. Question: {} Method: {} Measurement: {} Comparator: {} Acceptance criterion: {} Adverse result: {} Priority: {}. Cost: {}", n.id, n.target_id, n.enables_claim, n.question, n.method, n.measurement, n.comparator, n.acceptance_criterion, n.if_failed, n.priority, n.cost),
    }).collect()
}

struct Outline<'a> {
    text: BoundedText,
    view: PresentationView<'a>,
    max_depth: usize,
    display_depth: Option<u8>,
    printed_claims: HashSet<String>,
    expanded_claims: HashSet<String>,
    printed_grounds: HashSet<String>,
    deferred: VecDeque<String>,
    queued: HashSet<String>,
}

impl Outline<'_> {
    fn line(&mut self, level: usize, value: &str) {
        writeln!(self.text, "{}- {value}", "  ".repeat(level)).unwrap();
    }

    fn defer(&mut self, id: &str) {
        if self.view.depths.visible(id, self.display_depth)
            && !self.expanded_claims.contains(id)
            && self.queued.insert(id.to_owned())
        {
            self.deferred.push_back(id.to_owned());
        }
    }

    fn claim(&mut self, id: &'_ str, level: usize) {
        let Some((text, qualifier)) = claim(&self.view, id) else {
            return;
        };
        if !self.printed_claims.insert(id.to_owned()) {
            self.line(
                level,
                &format!(
                    "See {} for the shared claim and its supporting argument.",
                    md_reference(id)
                ),
            );
            return;
        }
        self.line(
            level,
            &format!(
                "<a id=\"{}\"></a>**{} — {}** Qualification: {}",
                anchor(id),
                inline(id),
                inline(text),
                inline(qualifier)
            ),
        );
        let notes = conditions(
            &self.view,
            self.view.canonical_id(id).unwrap_or("current_claim"),
        );
        if !self.view.depths.visible(id, self.display_depth) {
            for note in notes {
                self.line(level, &inline(&note));
            }
            self.line(
                level,
                "Supporting detail collapsed by display-depth; retained in the canonical JSON.",
            );
            return;
        }
        if level + 1 >= self.max_depth {
            // Keep consequential conditions adjacent even when the display depth
            // places the underlying argument in a separate shallow record.
            for note in notes {
                self.line(level, &inline(&note));
            }
            self.line(
                level,
                &format!(
                    "Supporting argument: see the detail record for {} below.",
                    inline(id)
                ),
            );
            self.defer(id);
            return;
        }
        for note in notes {
            self.line(level + 1, &inline(&note));
        }
        self.support(id, level + 1, false);
    }

    fn support(&mut self, id: &str, level: usize, detached: bool) {
        if !self.expanded_claims.insert(id.to_owned()) {
            return;
        }
        if let Some(item) = composition(&self.view, id) {
            self.line(
                level,
                &format!(
                    "**Relationship: {}.** {}",
                    relationship(item.relationship),
                    inline(&item.rationale)
                ),
            );
        } else {
            self.line(level, "Supporting-argument relationship: not declared.");
        }
        for note in direct_notes(&self.view, id) {
            self.line(level, &inline(&note));
        }
        let warrants = self.view.warrants(id).to_vec();
        if warrants.is_empty() {
            self.line(level, "No attached warrant is recorded.");
        }
        for warrant in warrants {
            self.line(level, &format!("<a id=\"{}\"></a>**ARG-{} — Warrant {}:** {} **Premises jointly required:** {}. Route: {}. Applicability: {}", anchor(&warrant.id), inline(&warrant.id), inline(&warrant.id), inline(&warrant.rule), warrant.premise_ids.iter().map(|id| md_reference(id)).collect::<Vec<_>>().join(", "), selection(&self.view, warrant), inline(&warrant.applicability)));
            let note_level = if level + 1 < self.max_depth {
                level + 1
            } else {
                level
            };
            for note in direct_notes(&self.view, &warrant.id) {
                self.line(note_level, &inline(&note));
            }
            if !warrant.backing_evidence_ids.is_empty() {
                self.line(
                    note_level,
                    &format!(
                        "Warrant backing: {}.",
                        warrant
                            .backing_evidence_ids
                            .iter()
                            .map(|id| md_reference(id))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                );
            }
            for premise in &warrant.premise_ids {
                if claim(&self.view, premise).is_some() {
                    if !self.view.depths.visible(premise, self.display_depth) {
                        self.claim(premise, note_level);
                    } else if detached || level + 1 >= self.max_depth {
                        self.line(
                            note_level,
                            &format!("Shared or deeper premise: see {}.", md_reference(premise)),
                        );
                        self.defer(premise);
                    } else {
                        self.claim(premise, level + 1);
                    }
                } else if let Some(ground) = self.view.ground(premise) {
                    if self.printed_grounds.insert(ground.id.clone()) {
                        self.line(
                            note_level,
                            &format!(
                                "<a id=\"{}\"></a>**{} — Ground ({}):** {} Evidence: {}.",
                                anchor(&ground.id),
                                inline(&ground.id),
                                label(&ground.status),
                                inline(&ground.statement),
                                if ground.evidence_ids.is_empty() {
                                    "none supplied".to_owned()
                                } else {
                                    ground
                                        .evidence_ids
                                        .iter()
                                        .map(|id| md_reference(id))
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                }
                            ),
                        );
                        for note in direct_notes(&self.view, &ground.id) {
                            self.line(note_level, &inline(&note));
                        }
                    } else {
                        self.line(
                            note_level,
                            &format!("Shared ground: see {}.", md_reference(&ground.id)),
                        );
                    }
                }
            }
        }
    }
}

/// One claim per line, with stated support relationships and bounded indentation.
/// Deeper support is retained in separate shallow records rather than discarded.
pub fn outline(
    request: &Request,
    case: &ArgumentCase,
    max_depth: usize,
) -> Result<String, RenderError> {
    outline_with_depth(request, case, max_depth, None)
}

/// Limit detailed claim levels without altering the graph or its assessments.
/// Boundary claims keep their qualification and consequential conditions.
pub fn outline_with_depth(
    request: &Request,
    case: &ArgumentCase,
    max_depth: usize,
    display_depth: Option<u8>,
) -> Result<String, RenderError> {
    outline_with_limits(
        request,
        case,
        max_depth,
        display_depth,
        RenderLimits::default(),
    )
}

pub fn outline_with_limits(
    request: &Request,
    case: &ArgumentCase,
    max_depth: usize,
    display_depth: Option<u8>,
    limits: RenderLimits,
) -> Result<String, RenderError> {
    let view = PresentationView::new(request, case, limits)?;
    let mut output = Outline {
        text: BoundedText::new(limits.max_output_bytes),
        view,
        max_depth: max_depth.clamp(1, 6),
        display_depth,
        printed_claims: HashSet::new(),
        expanded_claims: HashSet::new(),
        printed_grounds: HashSet::new(),
        deferred: VecDeque::new(),
        queued: HashSet::new(),
    };
    writeln!(output.text, "# Working argument outline\n\nRequested claim: {}\n\nCurrent assessment: **{}**. {}\n\nScope: {} Configuration: {} Operating conditions: {} Time horizon: {}\n", inline(&request.claim), label(&case.status), inline(&case.adjudication), inline(&case.scope.system_boundary), inline(&case.scope.configuration), inline(&case.scope.operating_conditions), inline(&case.scope.time_horizon)).unwrap();
    if let Some(notice) = output.view.inspection_notice() {
        writeln!(output.text, "> {}\n", inline(&notice)).unwrap();
    }
    if let Some(depth) = display_depth {
        writeln!(output.text, "Detailed claim levels: {depth}. Deeper supporting records are collapsed in this view. The canonical JSON retains the complete graph.\n").unwrap();
    }
    output.claim("current_claim", 0);
    output.text.push_str("\nStronger conditional target:\n\n");
    output.claim("stronger_target", 0);
    for subclaim in &case.subclaims {
        if !output.printed_claims.contains(&subclaim.id) {
            output.defer(&subclaim.id);
        }
    }
    if !output.deferred.is_empty() {
        output.text.push_str("\n## Separate argument records\n\n");
    }
    while let Some(id) = output.deferred.pop_front() {
        if output.expanded_claims.contains(&id) {
            continue;
        }
        if let Some((text, qualifier)) = claim(&output.view, &id) {
            if output.printed_claims.insert(id.clone()) {
                writeln!(
                    output.text,
                    "### <a id=\"{}\"></a>{} — {} Qualification: {}\n",
                    anchor(&id),
                    inline(&id),
                    inline(text),
                    inline(qualifier)
                )
                .unwrap();
                for note in conditions(
                    &output.view,
                    output.view.canonical_id(&id).unwrap_or("current_claim"),
                ) {
                    output.line(0, &inline(&note));
                }
            } else {
                writeln!(
                    output.text,
                    "### Supporting argument for {}\n",
                    md_reference(&id)
                )
                .unwrap();
            }
            output.support(&id, 0, true);
            output.text.push('\n');
        }
    }
    output.text.push_str("\n## Evidence sources\n\n");
    for evidence in &request.evidence {
        writeln!(output.text, "- <a id=\"{}\"></a>**{}:** {} Source: {}. Kind: {}. Checked by user: {}. Finding supplied: {}", anchor(&evidence.id), inline(&evidence.id), inline(&evidence.title), inline(&evidence.source), inline(&evidence.kind), evidence.checked_by_user, inline(&evidence.excerpt)).unwrap();
    }
    output.view.finish()?;
    output.text.finish()
}

/// Connected, model-authored paragraphs with canonical references. Each section
/// appears once; the renderer adds no measurements or inferred support status.
pub fn prose(request: &Request, case: &ArgumentCase) -> Result<String, RenderError> {
    prose_with_depth(request, case, None)
}

/// A reading view with a bounded set of detailed claim sections. Parent prose
/// is never rewritten, and outstanding relied-upon conditions remain explicit.
pub fn prose_with_depth(
    request: &Request,
    case: &ArgumentCase,
    display_depth: Option<u8>,
) -> Result<String, RenderError> {
    prose_with_limits(request, case, display_depth, RenderLimits::default())
}

pub fn prose_with_limits(
    request: &Request,
    case: &ArgumentCase,
    display_depth: Option<u8>,
    limits: RenderLimits,
) -> Result<String, RenderError> {
    let view = PresentationView::new(request, case, limits)?;
    let depths = &view.depths;
    let mut output = BoundedText::new(limits.max_output_bytes);
    write!(output,
        "# Engineering assessment\n\n> Canonical current claim: {}\n>\n> Qualification: {}\n>\n> Current assessment: **{}**.\n",
        inline(&case.current_claim.text),
        inline(&case.current_claim.qualifier),
        label(&case.status)
    ).unwrap();
    if let Some(notice) = view.inspection_notice() {
        writeln!(output, "\n> {}\n", inline(&notice)).unwrap();
    }
    if let Some(depth) = display_depth {
        writeln!(output, "\nDetailed claim levels: {depth}. Deeper supporting sections are omitted from this view and retained in the canonical JSON.\n").unwrap();
    }
    for condition in conditions(&view, "current_claim") {
        writeln!(output, "{}\n", inline(&condition)).unwrap();
    }
    let mut sections: Vec<_> = case.reading.iter().collect();
    sections.sort_by_key(|section| usize::from(section.claim_id != "current_claim"));
    let mut seen = HashSet::new();
    for section in sections {
        if !depths.visible(&section.claim_id, display_depth) {
            continue;
        }
        if !seen.insert(section.claim_id.as_str()) {
            continue;
        }
        writeln!(
            output,
            "\n## <a id=\"{}\"></a>{}\n",
            anchor(&section.claim_id),
            inline(&section.title)
        )
        .unwrap();
        if section.claim_id == "stronger_target" {
            writeln!(
                output,
                "> Canonical stronger target: {}\n>\n> Qualification: {}\n",
                inline(&case.stronger_target.text),
                inline(&case.stronger_target.qualifier)
            )
            .unwrap();
            for condition in conditions(&view, "stronger_target") {
                writeln!(output, "{}\n", inline(&condition)).unwrap();
            }
        } else if section.claim_id != "current_claim"
            && let Some((_, qualifier)) = claim(&view, &section.claim_id)
        {
            writeln!(output, "Qualification: {}\n", inline(qualifier)).unwrap();
        }
        for paragraph in &section.paragraphs {
            writeln!(
                output,
                "{}\n\n(References: {}.)\n",
                inline(&paragraph.text),
                paragraph
                    .reference_ids
                    .iter()
                    .map(|id| inline(id))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .unwrap();
        }
    }
    if !seen.contains("stronger_target") {
        writeln!(
            output,
            "\n## Stronger conditional target\n\n{} Qualification: {}\n",
            inline(&case.stronger_target.text),
            inline(&case.stronger_target.qualifier)
        )
        .unwrap();
        for condition in conditions(&view, "stronger_target") {
            writeln!(output, "{}\n", inline(&condition)).unwrap();
        }
    }
    writeln!(output, "## Limits on reliance\n").unwrap();
    for limit in case.decision_limits.iter().chain(&case.residual_risks) {
        writeln!(output, "{}\n", inline(limit)).unwrap();
    }
    writeln!(output, "## Sources\n").unwrap();
    for evidence in &request.evidence {
        writeln!(
            output,
            "**{} — {}.** Source: {}. Kind: {}. Checked by user: {}. {}\n",
            inline(&evidence.id),
            inline(&evidence.title),
            inline(&evidence.source),
            inline(&evidence.kind),
            evidence.checked_by_user,
            inline(&evidence.excerpt)
        )
        .unwrap();
    }
    view.finish()?;
    output.finish()
}
