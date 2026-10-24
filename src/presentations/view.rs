//! Borrowed, unescaped presentation semantics shared by all formatting backends.
use super::budget::{RenderError, RenderLimits};
use crate::{
    argument_depth::ClaimDepths,
    domain::{
        ArgumentCase, Assumption, Defeater, DefeaterStatus, Evidence, EvidenceNeed, Ground,
        Request, SupportComposition, Warrant,
    },
    domain::{ArgumentIndex, ValidatedArgument},
};
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
};

#[derive(Clone, Copy)]
pub(crate) enum Condition<'a> {
    Assumption(&'a Assumption),
    Challenge(&'a Defeater),
}
#[derive(Clone, Copy)]
pub(crate) enum Attachment<'a> {
    Assumption(&'a Assumption),
    Challenge(&'a Defeater),
    Investigation(&'a EvidenceNeed),
}

/// One index and depth projection serve one bounded presentation operation.
/// The source is borrowed immutably. Formatting never assigns evidential status.
pub(crate) struct PresentationView<'a> {
    pub(crate) case: &'a ArgumentCase,
    pub(crate) request: &'a Request,
    pub(crate) index: ArgumentIndex<'a>,
    pub(crate) depths: ClaimDepths<'a>,
    remaining: Cell<usize>,
    failed: Cell<bool>,
    query_charge: usize,
    evidence: HashMap<&'a str, &'a Evidence>,
    uses: HashMap<&'a str, Vec<String>>,
    diagnostics: Vec<String>,
}
impl<'a> PresentationView<'a> {
    pub(crate) fn new(
        request: &'a Request,
        case: &'a ArgumentCase,
        limits: RenderLimits,
    ) -> Result<Self, RenderError> {
        limits.admit(request, case)?;
        match crate::domain::validate_argument(request, case) {
            Ok(validated) => Self::from_validated(request, validated, limits),
            Err(diagnostics) => {
                if diagnostics.iter().any(|d| {
                    matches!(
                        d.code,
                        crate::domain::DiagnosticCode::ReferenceIntegrity
                            | crate::domain::DiagnosticCode::SupportCycle
                            | crate::domain::DiagnosticCode::InvalidRequest
                    )
                }) {
                    return Err(RenderError::InvalidStructure);
                }
                Self::from_index(
                    request,
                    case,
                    ArgumentIndex::new(case),
                    limits,
                    diagnostics.into_iter().map(|d| d.to_string()).collect(),
                )
            }
        }
    }
    /// Canonical rendering consumes the admitted view and reuses its index.
    fn from_validated(
        request: &'a Request,
        validated: ValidatedArgument<'a>,
        limits: RenderLimits,
    ) -> Result<Self, RenderError> {
        Self::from_index(request, validated.case, validated.index, limits, Vec::new())
    }
    /// Explicit inspection compatibility retains malformed records and reports
    /// their diagnostics. It receives the same resource ceilings as admitted data.
    fn from_index(
        request: &'a Request,
        case: &'a ArgumentCase,
        index: ArgumentIndex<'a>,
        limits: RenderLimits,
        diagnostics: Vec<String>,
    ) -> Result<Self, RenderError> {
        let depths = ClaimDepths::from_index(&index);
        let query_charge = 2
            + case.subclaims.len()
            + case.grounds.len()
            + case.warrants.len()
            + case.assumptions.len()
            + case.defeaters.len()
            + case.evidence_plan.len()
            + case
                .warrants
                .iter()
                .map(|w| w.premise_ids.len())
                .sum::<usize>()
            + case
                .assumptions
                .iter()
                .map(|a| a.applies_to.len())
                .sum::<usize>();
        // Each query may perform bounded formatting passes over its result.
        let query_charge = query_charge.saturating_mul(16);
        let evidence = request
            .evidence
            .iter()
            .map(|e| (e.id.as_str(), e))
            .collect();
        let mut uses: HashMap<&str, Vec<String>> = HashMap::new();
        for g in &case.grounds {
            for id in &g.evidence_ids {
                uses.entry(id)
                    .or_default()
                    .push(format!("{}: premise support", g.id));
            }
        }
        for w in &case.warrants {
            for id in &w.backing_evidence_ids {
                uses.entry(id)
                    .or_default()
                    .push(format!("{}: warrant backing", w.id));
            }
        }
        for a in &case.assumptions {
            for id in &a.evidence_ids {
                uses.entry(id)
                    .or_default()
                    .push(format!("{}: assumption appraisal", a.id));
            }
        }
        for d in &case.defeaters {
            for id in &d.evidence_ids {
                uses.entry(id)
                    .or_default()
                    .push(format!("{}: challenge appraisal", d.id));
            }
        }
        for (i, a) in case.alternatives.iter().enumerate() {
            for id in &a.evidence_ids {
                uses.entry(id)
                    .or_default()
                    .push(format!("alternative {}: comparative support", i + 1));
            }
        }
        if limits.max_visits < query_charge {
            return Err(RenderError::ResourceLimit("visits"));
        }
        Ok(Self {
            case,
            request,
            index,
            depths,
            remaining: Cell::new(limits.max_visits - query_charge),
            failed: Cell::new(false),
            query_charge,
            evidence,
            uses,
            diagnostics,
        })
    }
    pub(crate) fn charge(&self, count: usize) -> bool {
        if let Some(remaining) = self.remaining.get().checked_sub(count) {
            self.remaining.set(remaining);
            !self.failed.get()
        } else {
            self.failed.set(true);
            false
        }
    }
    pub(crate) fn finish(&self) -> Result<(), RenderError> {
        if self.failed.get() {
            Err(RenderError::ResourceLimit("visits"))
        } else {
            Ok(())
        }
    }
    /// Inspection compatibility preserves recorded values while making failed
    /// policy admission visible in every representation.
    pub(crate) fn inspection_notice(&self) -> Option<String> {
        (!self.diagnostics.is_empty()).then(|| format!(
            "Inspection view. Domain validation failed. The recorded assessments require repair. {}",
            self.diagnostics.join(" ")
        ))
    }
    pub(crate) fn validation_summary(&self) -> String {
        if self.diagnostics.is_empty() {
            "Structural and reporting checks passed, including the acyclic support graph. This supplies no proof of semantic adequacy.".into()
        } else {
            format!(
                "Structural or reporting defects remain: {}",
                self.diagnostics.join(" ")
            )
        }
    }
    pub(crate) fn claim(&self, id: &str) -> Option<(&'a str, &'a str)> {
        self.index.claim(id)
    }
    pub(crate) fn evidence(&self, id: &str) -> Option<&'a Evidence> {
        self.evidence.get(id).copied()
    }
    pub(crate) fn evidence_uses(&self, id: &str) -> &[String] {
        self.uses.get(id).map(Vec::as_slice).unwrap_or_default()
    }
    pub(crate) fn canonical_id(&self, id: &str) -> Option<&'a str> {
        self.index.canonical_id(id)
    }
    pub(crate) fn ground(&self, id: &str) -> Option<&'a Ground> {
        self.index.ground(id)
    }
    pub(crate) fn composition(&self, id: &str) -> Option<&'a SupportComposition> {
        self.index.composition_for(id)
    }
    pub(crate) fn warrants(&self, id: &str) -> &[&'a Warrant] {
        self.index.warrants_for(id)
    }
    pub(crate) fn selected(&self, warrant: &Warrant) -> Option<bool> {
        self.index.is_selected(&warrant.id)
    }
    pub(crate) fn conditions(&self, root: &'a str) -> Vec<Condition<'a>> {
        if !self.charge(self.query_charge) {
            return Vec::new();
        }
        let closure = self.index.selected_dependency(root);
        self.case
            .assumptions
            .iter()
            .filter(|a| a.critical && closure.contains(a.id.as_str()))
            .map(Condition::Assumption)
            .chain(
                self.case
                    .defeaters
                    .iter()
                    .filter(|d| {
                        d.material
                            && closure.contains(d.id.as_str())
                            && matches!(d.status, DefeaterStatus::Open | DefeaterStatus::Sustained)
                    })
                    .map(Condition::Challenge),
            )
            .collect()
    }
    pub(crate) fn inference_nodes(&self, warrant: &Warrant) -> HashSet<&'a str> {
        if !self.charge(self.query_charge) {
            return HashSet::new();
        }
        self.index.warrant_dependency(&warrant.id, true)
    }
    pub(crate) fn direct_attachments(&self, id: &str) -> Vec<Attachment<'a>> {
        let assumptions = self.index.assumptions_for(id);
        let challenges = self.index.defeaters_for(id);
        let needs = self.index.needs_for(id);
        if !self.charge(assumptions.len() + challenges.len() + needs.len() + 1) {
            return Vec::new();
        }
        assumptions
            .iter()
            .copied()
            .map(Attachment::Assumption)
            .chain(challenges.iter().copied().map(Attachment::Challenge))
            .chain(needs.iter().copied().map(Attachment::Investigation))
            .collect()
    }
    /// Preorder preserves authored assumptions, nested attachments, challenges,
    /// and investigations without consuming the native call stack.
    pub(crate) fn attachment_walk(
        &self,
        root: &'a str,
        seen: &mut HashSet<&'a str>,
    ) -> Vec<Attachment<'a>> {
        enum Frame<'a> {
            Target(&'a str),
            Emit(Attachment<'a>),
        }
        let mut stack = vec![Frame::Target(root)];
        let mut output = Vec::new();
        while let Some(frame) = stack.pop() {
            if !self.charge(1) {
                break;
            }
            match frame {
                Frame::Target(id) => {
                    if !seen.insert(id) {
                        continue;
                    }
                    for attachment in self.direct_attachments(id).into_iter().rev() {
                        match attachment {
                            Attachment::Assumption(a) => stack.push(Frame::Target(&a.id)),
                            Attachment::Challenge(d) => stack.push(Frame::Target(&d.id)),
                            Attachment::Investigation(_) => {}
                        }
                        stack.push(Frame::Emit(attachment));
                    }
                }
                Frame::Emit(attachment) => output.push(attachment),
            }
        }
        output
    }
}
