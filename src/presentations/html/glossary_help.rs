//! Canonical glossary help for explicitly typed HTML argument parts.
use super::{PresentationView, anchor, escape_html};
use crate::{domain::ArgumentNode, glossary};

pub(super) fn entry_anchor(code: &str) -> String {
    format!("glossary-{}", anchor(code))
}

pub(super) fn definition_anchor(code: &str) -> String {
    format!("glossary-definition-{}", anchor(code))
}

/// The caller supplies renderer-owned markup; authored values must be escaped first.
pub(super) fn markup(content: &str, code: &str) -> String {
    let entry = glossary::find(code).expect("HTML role codes belong to the canonical glossary");
    let tooltip = format!(
        "{} — {}\n{}\n{}",
        entry.code, entry.name, entry.definition, entry.question
    );
    // Some browsers omit closed <details> descendants from descriptions. Keep
    // the same canonical text available without requiring that disclosure open.
    format!(
        "<span class=\"argument-part\" data-glossary-code=\"{}\" title=\"{}\">{}<a class=\"glossary-help\" href=\"#{}\" aria-describedby=\"{}\" aria-description=\"{}\" aria-label=\"Read glossary entry: {}\">?</a></span>",
        escape_html(&entry.code),
        escape_html(&tooltip),
        content,
        entry_anchor(&entry.code),
        definition_anchor(&entry.code),
        escape_html(&format!("{} {}", entry.definition, entry.question)),
        escape_html(&entry.name),
    )
}

pub(super) fn text(content: &str, code: &str) -> String {
    markup(&escape_html(content), code)
}

pub(super) fn claim_role(id: &str) -> &'static str {
    match id {
        "current_claim" => "current_claim",
        "stronger_target" => "stronger_target",
        _ => "S",
    }
}

fn reference_role(view: &PresentationView<'_>, id: &str) -> Option<&'static str> {
    match view.index.node(id) {
        Some(ArgumentNode::Root(_)) => Some(claim_role(id)),
        Some(ArgumentNode::Claim(_)) => Some("S"),
        Some(ArgumentNode::Ground(_)) => Some("G"),
        Some(ArgumentNode::Warrant(_)) => Some("W"),
        Some(ArgumentNode::Assumption(_)) => Some("A"),
        Some(ArgumentNode::Defeater(_)) => Some("D"),
        None if view.index.need(id).is_some() => Some("N"),
        None if view.evidence(id).is_some() => Some("E"),
        None => None,
    }
}

pub(super) fn reference(view: &PresentationView<'_>, id: &str) -> String {
    let reference = format!("<a href=\"#{}\">{}</a>", anchor(id), escape_html(id));
    match reference_role(view, id) {
        Some(code) => markup(&reference, code),
        None => reference,
    }
}
