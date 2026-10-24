//! Plain sentence lines retain authored values rather than rewriting punctuation.

use super::{
    Attachment, BoundedText, Condition, PresentationView, RenderError, RenderLimits, claim,
    composition, label,
};
use crate::domain::{ArgumentCase, Request, SupportRelationship};
use std::{
    collections::{HashSet, VecDeque},
    fmt::Write,
};

struct Lines<'a> {
    text: BoundedText,
    view: PresentationView<'a>,
    max_indent: usize,
    display_depth: Option<u8>,
    claims: HashSet<String>,
    expanded: HashSet<String>,
    grounds: HashSet<String>,
    notes: HashSet<&'a str>,
    deferred: VecDeque<String>,
    queued: HashSet<String>,
}

impl Lines<'_> {
    fn line(&mut self, level: usize, text: &str) {
        // Preserve code punctuation and authored words. Only physical line breaks
        // become spaces so a record occupies one inspection line.
        let text = text.replace(['\r', '\n'], " ");
        writeln!(
            self.text,
            "{}{}",
            "  ".repeat(level.min(self.max_indent - 1)),
            text
        )
        .unwrap();
    }

    fn field(&mut self, level: usize, lead: &str, value: &str) {
        self.line(level, &format!("{lead} {value}"));
    }

    fn defer(&mut self, id: &str) {
        if self.view.depths.visible(id, self.display_depth)
            && !self.expanded.contains(id)
            && self.queued.insert(id.to_owned())
        {
            self.deferred.push_back(id.into());
        }
    }

    fn boundary_conditions(&mut self, root: &str, level: usize) {
        let Some(root) = self.view.canonical_id(root) else {
            return;
        };
        for condition in self.view.conditions(root) {
            match condition {
                Condition::Assumption(a) => {
                    self.line(
                        level,
                        &format!("The critical condition {} is {}", a.id, a.statement),
                    );
                    self.line(
                        level,
                        &format!("Its recorded status is {}.", label(&a.status)),
                    );
                }
                Condition::Challenge(d) => {
                    self.line(
                        level,
                        &format!("The outstanding challenge {} is {}", d.id, d.challenge),
                    );
                    self.line(
                        level,
                        &format!("Its recorded status is {}.", label(&d.status)),
                    );
                }
            }
        }
    }

    fn notes(&mut self, id: &str, level: usize) {
        let Some(id) = self.view.canonical_id(id) else {
            return;
        };
        for attachment in self.view.attachment_walk(id, &mut self.notes) {
            match attachment {
                Attachment::Assumption(a) => {
                    self.line(
                        level,
                        &format!("The assumption {} is {}", a.id, a.statement),
                    );
                    self.line(
                        level,
                        &format!("Its recorded status is {}.", label(&a.status)),
                    );
                    if a.critical {
                        self.line(
                            level,
                            "This assumption is critical to the recorded argument.",
                        );
                    }
                    self.field(level, "The proposed validation is", &a.validation);
                }
                Attachment::Challenge(d) => {
                    self.line(level, &format!("The challenge {} is {}", d.id, d.challenge));
                    self.line(level, &format!("Its attack is {}.", label(&d.kind)));
                    self.line(level, &format!("Its basis is {}.", label(&d.basis)));
                    self.line(
                        level,
                        if d.material {
                            "The assessment treats this challenge as material."
                        } else {
                            "The assessment records this challenge as immaterial."
                        },
                    );
                    self.line(
                        level,
                        &format!("Its recorded status is {}.", label(&d.status)),
                    );
                    self.field(level, "The recorded response is", &d.response);
                }
                Attachment::Investigation(need) => {
                    self.line(
                        level,
                        &format!(
                            "The investigation {} addresses {}.",
                            need.id, need.target_id
                        ),
                    );
                    self.field(level + 1, "It asks", &need.question);
                    self.field(level + 1, "The proposed method is", &need.method);
                    self.field(level + 1, "The measurement is", &need.measurement);
                    self.field(level + 1, "The comparator is", &need.comparator);
                    self.field(
                        level + 1,
                        "The acceptance criterion is",
                        &need.acceptance_criterion,
                    );
                    self.field(level + 1, "A failed check means", &need.if_failed);
                    self.line(
                        level + 1,
                        &format!("It could strengthen {}.", need.enables_claim),
                    );
                    self.line(
                        level + 1,
                        &format!("Its proposed priority is {}.", need.priority),
                    );
                    self.field(level + 1, "Its estimated cost is", &need.cost);
                }
            }
        }
    }

    fn claim(&mut self, id: &str, level: usize) {
        let Some((text, qualifier)) = claim(&self.view, id) else {
            return;
        };
        if !self.claims.insert(id.into()) {
            self.line(level, &format!("See {id} for the shared argument."));
            return;
        }
        self.line(level, &format!("{id} — {text}"));
        self.field(level, "Its qualification is", qualifier);
        if let Some(item) = composition(&self.view, id) {
            let relation = match item.relationship {
                SupportRelationship::JointlyRequired => {
                    "These supporting arguments are jointly required."
                }
                SupportRelationship::AlternativeRoutes => {
                    "These supporting arguments provide alternative routes."
                }
                SupportRelationship::Cumulative => {
                    "These supporting arguments contribute cumulative support."
                }
            };
            self.line(level, relation);
            self.field(level, "The composition rationale is", &item.rationale);
        }
        self.boundary_conditions(id, level);
        if !self.view.depths.visible(id, self.display_depth) {
            self.line(
                level,
                "The canonical JSON retains the deeper supporting detail.",
            );
        } else if level + 1 >= self.max_indent {
            self.line(
                level,
                &format!("The supporting argument for {id} follows in a separate record."),
            );
            self.defer(id);
        } else {
            self.support(id, level + 1);
        }
    }

    fn support(&mut self, id: &str, level: usize) {
        if !self.expanded.insert(id.into()) {
            return;
        }
        self.notes(id, level);
        let warrants = self.view.warrants(id).to_vec();
        for warrant in warrants {
            self.line(level, &format!("{} — {}", warrant.id, warrant.rule));
            self.line(
                level,
                "Every following premise is jointly required for this inference.",
            );
            let selected = self.view.selected(warrant).unwrap_or(false);
            self.line(
                level,
                if selected {
                    "The assessment relies on this route."
                } else {
                    "This route remains a candidate."
                },
            );
            self.field(level, "Its applicability requires", &warrant.applicability);
            self.line(
                level,
                &format!("The recorded inference is {}.", label(&warrant.inference)),
            );
            for evidence in &warrant.backing_evidence_ids {
                self.line(
                    level,
                    &format!("Its backing refers to evidence {evidence}."),
                );
            }
            self.notes(&warrant.id, level);
            for premise in &warrant.premise_ids {
                if claim(&self.view, premise).is_some() {
                    self.claim(premise, level + 1);
                } else if let Some(ground) = self.view.ground(premise) {
                    if self.grounds.insert(ground.id.clone()) {
                        self.line(level + 1, &format!("{} — {}", ground.id, ground.statement));
                        self.line(
                            level + 1,
                            &format!("Its recorded status is {}.", label(&ground.status)),
                        );
                        for evidence in &ground.evidence_ids {
                            self.line(
                                level + 1,
                                &format!("It refers to supplied evidence {evidence}."),
                            );
                        }
                        self.notes(&ground.id, level + 1);
                    } else {
                        self.line(level + 1, &format!("See {premise} for the shared ground."));
                    }
                }
            }
        }
    }
}

/// Plain argument lines with each authored record on its own line.
/// Generation controls sentence style. Existing technical expressions and
/// punctuation remain literal because a renderer cannot safely rewrite meaning.
pub fn plain_lines(
    request: &Request,
    case: &ArgumentCase,
    max_indent: usize,
) -> Result<String, RenderError> {
    plain_lines_with_depth(request, case, max_indent, None)
}

/// Limit indentation and detailed claim levels while preserving boundary
/// qualifications and the conditions that constrain reliance on either root.
pub fn plain_lines_with_depth(
    request: &Request,
    case: &ArgumentCase,
    max_indent: usize,
    display_depth: Option<u8>,
) -> Result<String, RenderError> {
    plain_lines_with_limits(
        request,
        case,
        max_indent,
        display_depth,
        RenderLimits::default(),
    )
}

pub fn plain_lines_with_limits(
    request: &Request,
    case: &ArgumentCase,
    max_indent: usize,
    display_depth: Option<u8>,
    limits: RenderLimits,
) -> Result<String, RenderError> {
    let view = PresentationView::new(request, case, limits)?;
    let mut output = Lines {
        text: BoundedText::new(limits.max_output_bytes),
        view,
        max_indent: max_indent.clamp(1, 6),
        display_depth,
        claims: HashSet::new(),
        expanded: HashSet::new(),
        grounds: HashSet::new(),
        notes: HashSet::new(),
        deferred: VecDeque::new(),
        queued: HashSet::new(),
    };
    if let Some(notice) = output.view.inspection_notice() {
        output.line(0, &notice);
    }
    output.line(
        0,
        &format!("The current assessment is {}.", label(&case.status)),
    );
    output.field(0, "The requested claim is", &request.claim);
    output.field(0, "The engineering judgement is", &case.adjudication);
    output.field(0, "The system boundary is", &case.scope.system_boundary);
    output.field(0, "The configuration is", &case.scope.configuration);
    output.field(
        0,
        "The operating conditions are",
        &case.scope.operating_conditions,
    );
    output.field(0, "The time horizon is", &case.scope.time_horizon);
    output.field(0, "The units are", &case.scope.units);
    output.field(0, "The consequences are", &case.scope.consequences);
    output.text.push('\n');
    output.claim("current_claim", 0);
    output.text.push('\n');
    output.claim("stronger_target", 0);
    for subclaim in &case.subclaims {
        if !output.claims.contains(&subclaim.id) {
            output.defer(&subclaim.id);
        }
    }
    while let Some(id) = output.deferred.pop_front() {
        if output.expanded.contains(&id) {
            continue;
        }
        output.text.push('\n');
        if output.claims.contains(&id) {
            output.line(
                0,
                &format!("The following record develops the supporting argument for {id}."),
            );
            output.support(&id, 0);
        } else {
            output.claim(&id, 0);
        }
    }
    output.text.push('\n');
    for alternative in &case.alternatives {
        output.field(0, "An alternative explanation is", &alternative.explanation);
        output.field(
            1,
            "Its distinguishing test is",
            &alternative.distinguishing_test,
        );
        for id in &alternative.evidence_ids {
            output.line(1, &format!("It refers to supplied evidence {id}."));
        }
    }
    for limit in &case.decision_limits {
        output.field(0, "The reliance limit is", limit);
    }
    for risk in &case.residual_risks {
        output.field(0, "The residual risk is", risk);
    }
    for evidence in &request.evidence {
        output.line(
            0,
            &format!("Supplied evidence {} is {}", evidence.id, evidence.title),
        );
        output.field(1, "Its source is", &evidence.source);
        output.field(1, "Its reported finding is", &evidence.excerpt);
    }
    output.view.finish()?;
    output.text.finish()
}
