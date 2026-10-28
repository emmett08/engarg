//! Self-contained, script-free HTML inspection with native disclosure controls.
use super::{
    Attachment, BoundedText, Condition, PresentationView, RenderError, RenderLimits, anchor, claim,
    composition, label, relationship, selection,
};
use crate::{
    domain::{ArgumentCase, Request},
    glossary,
};
use std::{collections::HashSet, fmt::Write};

mod glossary_help;
use glossary_help::{claim_role, markup as help_markup, text as help_text};

const DOCUMENT_START: &str = r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'">
<title>Engineering argument</title>
<style>
:root{color-scheme:light;--ink:#1c2939;--muted:#46576a;--accent:#174c8c;--line:#ccd6e2;--paper:#fff;--page:#f2f5f9;--condition:#fff7e8}
*{box-sizing:border-box}
html{scroll-padding-top:5rem}
body{margin:0;background:var(--page);color:var(--ink);font:16px/1.65 system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;overflow-wrap:anywhere}
main{max-width:82ch;margin:0 auto;padding:2.5rem 1.25rem 5rem}
h1,h2,h3{line-height:1.25;letter-spacing:-.015em}
h1{font-size:clamp(2rem,5vw,2.7rem);margin:0 0 1rem}
h2{font-size:1.6rem;margin:2.25rem 0 1rem}
h3{font-size:1.1rem}
p{margin:.75rem 0 1rem}
a{color:var(--accent);text-decoration-thickness:.08em;text-underline-offset:.15em}
a:hover{text-decoration-thickness:.15em}
a:focus-visible,summary:focus-visible{outline:3px solid var(--accent);outline-offset:5px;border-radius:.2rem}
.argument-part{cursor:help}
.glossary-help{display:inline-block;vertical-align:super;font-size:.72rem;font-weight:700;line-height:1.2;margin-inline-start:.3em;padding:.1em .3em;border:1px solid var(--line);border-radius:50%;text-decoration:none;white-space:nowrap}
.glossary-help:hover{background:#e2ebf7}
#glossary dt{scroll-margin-top:2rem}
nav{display:flex;gap:.5rem;flex-wrap:wrap;padding:.75rem 0;margin:1.5rem 0;border-top:1px solid var(--line);border-bottom:1px solid var(--line)}
nav a{padding:.35rem .65rem;border-radius:.3rem;background:var(--paper);text-decoration:none;font-weight:600}
nav a:hover{background:#e2ebf7}
section{margin:2rem 0;scroll-margin-top:2rem}
.assessment,.requested-claim,.scope-grid{background:var(--paper);border:1px solid var(--line);border-radius:.65rem;padding:1.2rem 1.35rem;margin:1.1rem 0}
.assessment strong{display:block;font-size:1.1rem;color:var(--accent);margin-bottom:.6rem}
.requested-claim{white-space:pre-wrap;border-left:4px solid var(--accent)}
.scope-grid{display:grid;grid-template-columns:1fr;gap:.4rem}
.scope-grid dt{font-weight:700;color:var(--muted);margin-top:.5rem}
.scope-grid dt:first-child{margin-top:0}
.scope-grid dd{margin:0 0 .4rem}
.browser-note,.disclosure-help{font-size:.94rem;color:var(--muted)}
details{background:var(--paper);border:1px solid var(--line);border-radius:.65rem;padding:1.15rem 1.3rem;margin:1rem 0}
summary{cursor:pointer;font-weight:650;line-height:1.5}
summary::marker{color:var(--accent)}
summary:hover .claim-title,summary:hover .record-title{text-decoration:underline;text-underline-offset:.18em}
.claim-title,.record-title{font-size:1.08rem}
summary>.record-question,summary>.qualifier,summary>.condition,summary>.toggle-label{display:block}
summary>.qualifier{margin:.8rem 0}
.qualifier,.condition{font-weight:400;color:var(--muted)}
.condition{background:var(--condition);border-left:3px solid #a76713;border-radius:0 .3rem .3rem 0;padding:.6rem .85rem;margin:.65rem 0;font-size:.95rem}
.toggle-label{font-size:.9rem;font-weight:700;color:var(--accent);margin-top:.9rem}
.toggle-label::before{content:"Show "}
details[open]>summary>.toggle-label::before{content:"Hide "}
details[open]>summary{padding-bottom:1rem;border-bottom:1px solid var(--line);margin-bottom:1rem}
.record summary>.record-question{font-weight:400;margin-top:.4rem}
.record summary>.toggle-label{margin-top:.5rem}
.relation{background:#edf3fb;border-radius:.3rem;padding:.8rem 1rem}
.references{font-size:.9rem;color:var(--muted)}
article{border-top:1px solid var(--line);padding:1rem 0}
#reading-view article p{max-width:72ch}
.notes{padding-left:1.2rem;color:var(--muted);font-size:.94rem}
.notes li{margin:.7rem 0}
li{margin:.5rem 0}
.claim-summary{border-left:3px solid var(--accent);padding-left:1rem}
@media(max-width:40rem){main{padding:1.5rem .85rem 3rem}details,.assessment,.requested-claim,.scope-grid{padding:1rem}nav{gap:.4rem}}
@media print{body{background:white;color:black;font-size:11pt}main{max-width:none;padding:0}nav,.browser-note,.toggle-label,.glossary-help{display:none}details{break-inside:avoid;box-shadow:none}a{color:inherit}}
</style></head><body><main><h1>Engineering argument</h1>
"##;

fn escape_html(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#39;"),
            _ => result.push(character),
        }
    }
    result
}

fn claim_ids(case: &ArgumentCase) -> Vec<&str> {
    ["current_claim", "stronger_target"]
        .into_iter()
        .chain(case.subclaims.iter().map(|item| item.id.as_str()))
        .collect()
}

fn html_reference(view: &PresentationView<'_>, id: &str) -> String {
    glossary_help::reference(view, id)
}

fn html_glossary(output: &mut BoundedText) {
    output.push_str("<details id=\"glossary\"><summary>Argument glossary — role codes and definitions</summary><p>Codes describe argument roles. A record's type and references establish its role. The model extends Toulmin's claim, grounds, warrant, backing, qualifier and rebuttal with engineering records.</p><dl>");
    for entry in glossary::entries() {
        write!(
            output,
            "<dt id=\"{}\"><strong>{} — {}</strong></dt><dd id=\"{}\"><p>{}</p><p><em>{}</em></p></dd>",
            glossary_help::entry_anchor(&entry.code),
            escape_html(&entry.code),
            escape_html(&entry.name),
            glossary_help::definition_anchor(&entry.code),
            escape_html(&entry.definition),
            escape_html(&entry.question)
        )
        .unwrap();
    }
    output.push_str("</dl></details>");
}

fn html_notes(output: &mut BoundedText, view: &PresentationView<'_>, id: &str) {
    let Some(id) = view.canonical_id(id) else {
        return;
    };
    let notes = view.attachment_walk(id, &mut HashSet::new());
    if notes.is_empty() {
        return;
    }
    output.push_str("<ul class=\"notes\">");
    for note in notes {
        match note {
            Attachment::Assumption(a) => write!(output,
                "<li>{}Assumption {} ({}). {} <a href=\"#{}\">Validation record</a>.</li>",
                if a.critical { "Critical " } else { "" }, html_reference(view, &a.id),
                escape_html(&label(&a.status)), help_text(&a.statement, "A"), anchor(&a.id)),
            Attachment::Challenge(d) => write!(output,
                "<li>{}Challenge {} ({}). {} <a href=\"#{}\">Response and investigation record</a>.</li>",
                if d.material { "Material " } else { "" }, html_reference(view, &d.id),
                escape_html(&label(&d.status)), help_text(&d.challenge, "D"), anchor(&d.id)),
            Attachment::Investigation(n) => write!(output,
                "<li>Investigation {}. {} <a href=\"#{}\">Method and acceptance criterion</a>.</li>",
                html_reference(view, &n.id), help_text(&n.question, "N"), anchor(&n.id)),
        }.unwrap();
    }
    output.push_str("</ul>");
}

/// Self-contained expandable graph. Dynamic strings are text, never markup,
/// scripts, style, source URLs or attribute values supplied by an agent.
pub fn html(request: &Request, case: &ArgumentCase) -> Result<String, RenderError> {
    html_with_depth(request, case, None)
}

/// Omit deeper supporting arguments and reading sections while keeping summaries,
/// canonical anchors, qualifications and disclosable reliance conditions.
pub fn html_with_depth(
    request: &Request,
    case: &ArgumentCase,
    display_depth: Option<u8>,
) -> Result<String, RenderError> {
    html_with_limits(request, case, display_depth, RenderLimits::default())
}

pub fn html_with_limits(
    request: &Request,
    case: &ArgumentCase,
    display_depth: Option<u8>,
    limits: RenderLimits,
) -> Result<String, RenderError> {
    let view = PresentationView::new(request, case, limits)?;
    let depths = &view.depths;
    let mut output = BoundedText::new(limits.max_output_bytes);
    output.push_str(DOCUMENT_START);
    if let Some(notice) = view.inspection_notice() {
        write!(
            output,
            "<aside role=\"note\" class=\"condition\">{}</aside>",
            escape_html(&notice)
        )
        .unwrap();
    }
    write!(output, "<div class=\"assessment\"><strong>Current assessment: {}.</strong><p>{}</p></div><h2>Requested claim</h2><p class=\"requested-claim\">{}</p><dl class=\"scope-grid\"><dt>Scope</dt><dd>{}</dd><dt>Configuration</dt><dd>{}</dd><dt>Operating conditions</dt><dd>{}</dd><dt>Time horizon</dt><dd>{}</dd><dt>Units</dt><dd>{}</dd><dt>Consequences for the decision</dt><dd>{}</dd></dl><nav aria-label=\"Report sections\"><a href=\"#argument-view\">Argument</a><a href=\"#reading-view\">Connected reading</a><a href=\"#sources\">Sources</a><a href=\"#investigations\">Investigation register</a><a href=\"#glossary\">Glossary</a></nav><p class=\"browser-note\">Open this file in a web browser for its styling and disclosure controls. Document previews may simplify the page.</p><p class=\"disclosure-help\">Hover over an argument part for its glossary definition. Select its ? link with a pointer, touch, or keyboard to read the full glossary entry.</p><section id=\"argument-view\"><h2>Expandable argument view</h2><p class=\"disclosure-help\">Select a heading or focus it and press Enter or Space to show supporting detail and full reliance conditions. Qualifications and a count of reliance conditions stay visible when a claim is closed.</p>", help_text(&label(&case.status), "assessment"), help_text(&case.adjudication, "assessment"), help_text(&request.claim, "C"), help_text(&case.scope.system_boundary, "scope"), help_text(&case.scope.configuration, "scope"), help_text(&case.scope.operating_conditions, "scope"), help_text(&case.scope.time_horizon, "scope"), help_text(&case.scope.units, "scope"), help_text(&case.scope.consequences, "decision_basis")).unwrap();
    if let Some(depth) = display_depth {
        write!(output, "<p>Detailed claim levels: {depth}. Deeper supporting arguments and reading sections are omitted from this view. Claim summaries and complete investigation registers remain available. Reliance conditions remain expandable. The canonical JSON retains the complete graph.</p>").unwrap();
    }
    for id in claim_ids(case) {
        let Some((text, qualifier)) = claim(&view, id) else {
            continue;
        };
        write!(output, "<details id=\"{}\"><summary><span class=\"claim-title\">{} — {}</span><span class=\"qualifier\">Qualification: {}</span>", anchor(id), help_text(id, claim_role(id)), help_text(text, claim_role(id)), help_text(qualifier, "Q")).unwrap();
        let claim_conditions = view.conditions(id);
        if !claim_conditions.is_empty() {
            write!(
                output,
                "<span class=\"condition\">Reliance conditions to inspect: {}.</span>",
                claim_conditions.len()
            )
            .unwrap();
        }
        // Only the compact heading belongs in the always-visible summary.
        // Long condition statements must be siblings so native disclosure hides them.
        output.push_str("<span class=\"toggle-label\" aria-hidden=\"true\">supporting argument and conditions</span></summary>");
        if let Some(item) = composition(&view, id) {
            write!(
                output,
                "<p class=\"qualifier\">{}. Selected route(s): {}.</p>",
                help_text(relationship(item.relationship), "composition"),
                if item.selected_warrant_ids.is_empty() {
                    "none".to_owned()
                } else {
                    escape_html(&item.selected_warrant_ids.join(", "))
                }
            )
            .unwrap();
        }
        for condition in claim_conditions {
            let (text, code) = match condition {
                Condition::Assumption(a) => (
                    format!(
                        "Critical condition {} ({}): {}",
                        a.id,
                        label(&a.status),
                        a.statement
                    ),
                    "A",
                ),
                Condition::Challenge(d) => (
                    format!(
                        "Outstanding challenge {} ({}): {}",
                        d.id,
                        label(&d.status),
                        d.challenge
                    ),
                    "D",
                ),
            };
            write!(
                output,
                "<p class=\"condition\">{}</p>",
                help_text(&text, code)
            )
            .unwrap();
        }
        if !depths.visible(id, display_depth) {
            output.push_str("<p>Supporting detail collapsed by display-depth; retained in the canonical JSON.</p></details>");
            continue;
        }
        if let Some(item) = composition(&view, id) {
            write!(
                output,
                "<p class=\"relation\"><strong>{}.</strong> {}</p>",
                help_text(relationship(item.relationship), "composition"),
                help_text(&item.rationale, "composition")
            )
            .unwrap();
        } else {
            output.push_str("<p>Supporting-argument relationship: not declared.</p>");
        }
        html_notes(&mut output, &view, id);
        for warrant in view.warrants(id).iter().copied() {
            write!(output, "<article id=\"{}\"><h3>{} — {}</h3><p>{}</p><p>Route: {}. Inference: {}.</p><p>Applicability: {}</p><p><strong>Premises jointly required:</strong></p><ul>", anchor(&warrant.id), help_text(&format!("ARG-{}", warrant.id), "ARG"), help_text(&format!("Warrant {}", warrant.id), "W"), help_text(&warrant.rule, "W"), help_text(selection(&view, warrant), "composition"), help_text(&label(&warrant.inference), "W"), help_text(&warrant.applicability, "R")).unwrap();
            for premise in &warrant.premise_ids {
                if let Some((text, qualifier)) = claim(&view, premise) {
                    write!(
                        output,
                        "<li>{} — {} Qualification: {}</li>",
                        html_reference(&view, premise),
                        help_text(text, claim_role(premise)),
                        help_text(qualifier, "Q")
                    )
                    .unwrap();
                } else if let Some(ground) = view.ground(premise) {
                    write!(
                        output,
                        "<li>{} — {} ({})</li>",
                        html_reference(&view, premise),
                        help_text(&ground.statement, "G"),
                        escape_html(&label(&ground.status))
                    )
                    .unwrap();
                }
            }
            output.push_str("</ul>");
            if !warrant.backing_evidence_ids.is_empty() {
                write!(
                    output,
                    "<p>{}</p>",
                    help_markup(
                        &format!(
                            "Backing: {}.",
                            warrant
                                .backing_evidence_ids
                                .iter()
                                .map(|id| html_reference(&view, id))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        "B"
                    )
                )
                .unwrap();
            }
            html_notes(&mut output, &view, &warrant.id);
            output.push_str("</article>");
        }
        output.push_str("</details>");
    }
    output.push_str("<h2>Ground findings</h2>");
    for ground in &case.grounds {
        write!(
            output,
            "<article id=\"{}\"><h3>{} — {}</h3><p>{}</p><p>Evidence: {}.</p>",
            anchor(&ground.id),
            help_text(&ground.id, "G"),
            escape_html(&label(&ground.status)),
            help_text(&ground.statement, "G"),
            if ground.evidence_ids.is_empty() {
                "none supplied".to_owned()
            } else {
                ground
                    .evidence_ids
                    .iter()
                    .map(|id| html_reference(&view, id))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        )
        .unwrap();
        html_notes(&mut output, &view, &ground.id);
        output.push_str("</article>");
    }
    output.push_str("</section><section id=\"reading-view\"><h2>Connected reading</h2>");
    write!(
        output,
        "<p>{} Qualification: {}</p>",
        help_text(&case.current_claim.text, "current_claim"),
        help_text(&case.current_claim.qualifier, "Q")
    )
    .unwrap();
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
        write!(
            output,
            "<article><h3>{}</h3>",
            help_text(&section.title, claim_role(&section.claim_id))
        )
        .unwrap();
        if section.claim_id == "stronger_target" {
            write!(output, "<aside class=\"claim-summary\"><p><strong>Canonical stronger target:</strong> {}</p><p class=\"qualifier\">Qualification: {}</p></aside>", help_text(&case.stronger_target.text, "stronger_target"), help_text(&case.stronger_target.qualifier, "Q")).unwrap();
        } else if section.claim_id != "current_claim"
            && let Some((_, qualifier)) = claim(&view, &section.claim_id)
        {
            write!(
                output,
                "<p class=\"qualifier\">Qualification: {}</p>",
                help_text(qualifier, "Q")
            )
            .unwrap();
        }
        for paragraph in &section.paragraphs {
            write!(
                output,
                "<p>{}</p><p class=\"references\">References: {}.</p>",
                help_text(&paragraph.text, claim_role(&section.claim_id)),
                paragraph
                    .reference_ids
                    .iter()
                    .map(|id| html_reference(&view, id))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .unwrap();
        }
        output.push_str("</article>");
    }
    if !seen.contains("stronger_target") {
        write!(output, "<article><h3>Stronger conditional target</h3><p>{}</p><p class=\"qualifier\">Qualification: {}</p></article>", help_text(&case.stronger_target.text, "stronger_target"), help_text(&case.stronger_target.qualifier, "Q")).unwrap();
    }
    output.push_str("<h3>Limits on reliance</h3>");
    for limit in &case.decision_limits {
        write!(output, "<p>{}</p>", help_text(limit, "decision_limit")).unwrap();
    }
    for risk in &case.residual_risks {
        write!(output, "<p>{}</p>", help_text(risk, "residual_risk")).unwrap();
    }
    if !case.alternatives.is_empty() {
        output.push_str("<h3>Competing explanations and options</h3>");
        for (position, alternative) in case.alternatives.iter().enumerate() {
            write!(output, "<article><h3>{}</h3><p>{}</p><p>Distinguishing check: {}</p><p>Evidence: {}.</p></article>",
                help_text(&format!("Alternative {}", position + 1), "alternative"),
                help_text(&alternative.explanation, "alternative"),
                help_text(&alternative.distinguishing_test, "alternative"),
                if alternative.evidence_ids.is_empty() { "none supplied".to_owned() } else {
                    alternative.evidence_ids.iter().map(|id| html_reference(&view, id)).collect::<Vec<_>>().join(", ")
                }).unwrap();
        }
    }
    output.push_str("</section><section id=\"sources\"><h2>Sources</h2>");
    for evidence in &request.evidence {
        write!(output, "<details class=\"record\" id=\"{}\"><summary><span class=\"record-title\">{} — {}</span><span class=\"toggle-label\" aria-hidden=\"true\">source record</span></summary><p>Source: {}</p><p>Kind: {}. Checked by user: {}.</p><p>{}</p></details>", anchor(&evidence.id), help_text(&evidence.id, "E"), help_text(&evidence.title, "E"), help_text(&evidence.source, "E"), escape_html(&evidence.kind), evidence.checked_by_user, help_text(&evidence.excerpt, "E")).unwrap();
    }
    output.push_str("</section><section id=\"investigations\"><h2>Assumptions and investigations</h2><p class=\"disclosure-help\">Each heading opens its complete retained record. The links beside an argument lead to these records.</p>");
    for assumption in &case.assumptions {
        write!(output, "<details class=\"record\" id=\"{}\"><summary><span class=\"record-title\">{} — {}</span><span class=\"record-question\">{}</span><span class=\"toggle-label\" aria-hidden=\"true\">assumption record</span></summary><p>Applies to: {}.</p><p>Check: {}</p></details>", anchor(&assumption.id), help_text(&assumption.id, "A"), escape_html(&label(&assumption.status)), help_text(&assumption.statement, "A"), assumption.applies_to.iter().map(|id| html_reference(&view, id)).collect::<Vec<_>>().join(", "), help_text(&assumption.validation, "revision_trigger")).unwrap();
    }
    for defeater in &case.defeaters {
        write!(output, "<details class=\"record\" id=\"{}\"><summary><span class=\"record-title\">{} — {} ({})</span><span class=\"record-question\">{}</span><span class=\"toggle-label\" aria-hidden=\"true\">challenge record</span></summary><p>Target: {}.</p><p>Basis: {}.</p><p>{}</p></details>", anchor(&defeater.id), help_text(&defeater.id, "D"), escape_html(&label(&defeater.kind)), escape_html(&label(&defeater.status)), help_text(&defeater.challenge, "D"), html_reference(&view, &defeater.target_id), escape_html(&label(&defeater.basis)), help_text(&defeater.response, "D")).unwrap();
    }
    for need in &case.evidence_plan {
        write!(output,"<details class=\"record\" id=\"{}\"><summary><span class=\"record-title\">{} — Priority {}</span><span class=\"record-question\">{}</span><span class=\"toggle-label\" aria-hidden=\"true\">investigation record</span></summary><p>Target: {}. Enables claim: {}.</p><p>Method: {}</p><p>Measurement: {}</p><p>Comparator: {}</p><p>Acceptance criterion: {}</p><p>If it fails: {}</p><p>Effort: {}</p></details>",anchor(&need.id),help_text(&need.id,"N"),need.priority,help_text(&need.question,"N"),html_reference(&view, &need.target_id),html_reference(&view, &need.enables_claim),help_text(&need.method,"N"),help_text(&need.measurement,"N"),help_text(&need.comparator,"N"),help_text(&need.acceptance_criterion,"criterion"),help_text(&need.if_failed,"revision_trigger"),help_text(&need.cost,"N")).unwrap();
    }
    output.push_str("</section>");
    html_glossary(&mut output);
    output.push_str("</main></body></html>");
    view.finish()?;
    output.finish()
}
