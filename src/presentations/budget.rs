//! Enforce a single allocation and output policy at every presentation boundary.
use crate::domain::{ArgumentCase, Request};
use std::{fmt, io};

/// Logical resource ceilings shared by every presentation backend.
#[derive(Clone, Copy, Debug)]
pub struct RenderLimits {
    pub max_nodes: usize,
    pub max_edges: usize,
    pub max_visits: usize,
    pub max_input_bytes: usize,
    pub max_working_bytes: usize,
    pub max_output_bytes: usize,
}
impl Default for RenderLimits {
    fn default() -> Self {
        Self {
            max_nodes: 100_000,
            max_edges: 500_000,
            max_visits: 8_000_000,
            max_input_bytes: 16 * 1024 * 1024,
            max_working_bytes: 192 * 1024 * 1024,
            max_output_bytes: 16 * 1024 * 1024,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RenderError {
    #[error(
        "render.invalid_structure: argument references or identities require repair before presentation"
    )]
    InvalidStructure,
    #[error("render.resource_limit: {0} budget exceeded")]
    ResourceLimit(&'static str),
}

struct Counter {
    bytes: usize,
    limit: usize,
}
impl io::Write for Counter {
    fn write(&mut self, value: &[u8]) -> io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(value.len())
            .ok_or_else(|| io::Error::other("input budget"))?;
        if self.bytes > self.limit {
            return Err(io::Error::other("input budget"));
        }
        Ok(value.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl RenderLimits {
    /// Check input and graph storage before building indices or formatting text.
    /// Working storage is conservatively charged for owned strings, indexing,
    /// traversal frames, escaping and the bounded output buffer.
    pub(crate) fn admit(self, request: &Request, case: &ArgumentCase) -> Result<(), RenderError> {
        let nodes = 2usize
            .saturating_add(case.subclaims.len())
            .saturating_add(case.grounds.len())
            .saturating_add(case.warrants.len())
            .saturating_add(case.assumptions.len())
            .saturating_add(case.defeaters.len())
            .saturating_add(case.evidence_plan.len())
            .saturating_add(request.evidence.len())
            .saturating_add(case.reading.len())
            .saturating_add(case.compositions.len())
            .saturating_add(case.alternatives.len())
            .saturating_add(
                case.reading
                    .iter()
                    .map(|s| s.paragraphs.len())
                    .fold(0usize, usize::saturating_add),
            );
        let mut edges = case
            .warrants
            .iter()
            .map(|w| {
                w.premise_ids
                    .len()
                    .saturating_add(w.backing_evidence_ids.len())
                    .saturating_add(1)
            })
            .fold(0usize, usize::saturating_add);
        edges = edges.saturating_add(
            case.assumptions
                .iter()
                .map(|a| a.applies_to.len().saturating_add(a.evidence_ids.len()))
                .fold(0usize, usize::saturating_add),
        );
        edges = edges.saturating_add(
            case.defeaters
                .iter()
                .map(|d| d.evidence_ids.len().saturating_add(1))
                .fold(0usize, usize::saturating_add),
        );
        edges = edges.saturating_add(
            case.compositions
                .iter()
                .map(|c| c.selected_warrant_ids.len())
                .fold(0usize, usize::saturating_add),
        );
        edges = edges.saturating_add(
            case.grounds
                .iter()
                .map(|g| g.evidence_ids.len())
                .fold(0usize, usize::saturating_add),
        );
        edges = edges.saturating_add(case.evidence_plan.len().saturating_mul(2));
        edges = edges.saturating_add(
            case.alternatives
                .iter()
                .map(|a| a.evidence_ids.len())
                .fold(0usize, usize::saturating_add),
        );
        edges = edges.saturating_add(case.reading.len());
        edges = edges.saturating_add(
            case.reading
                .iter()
                .flat_map(|s| &s.paragraphs)
                .map(|p| p.reference_ids.len())
                .fold(0usize, usize::saturating_add),
        );
        if nodes > self.max_nodes {
            return Err(RenderError::ResourceLimit("nodes"));
        }
        if edges > self.max_edges {
            return Err(RenderError::ResourceLimit("edges"));
        }
        let mut counter = Counter {
            bytes: 0,
            limit: self.max_input_bytes,
        };
        serde_json::to_writer(&mut counter, &(request, case))
            .map_err(|_| RenderError::ResourceLimit("input bytes"))?;
        let storage = counter
            .bytes
            .saturating_mul(32)
            .saturating_add(nodes.saturating_mul(512))
            .saturating_add(edges.saturating_mul(128))
            .saturating_add(self.max_output_bytes);
        if storage > self.max_working_bytes {
            return Err(RenderError::ResourceLimit("working bytes"));
        }
        let roots = case
            .evidence_plan
            .iter()
            .map(|n| n.enables_claim.as_str())
            .collect::<std::collections::HashSet<_>>()
            .len()
            .saturating_add(3);
        let validation_work = nodes
            .saturating_add(edges)
            .saturating_mul(roots)
            .saturating_mul(16);
        if validation_work > self.max_visits {
            return Err(RenderError::ResourceLimit("visits"));
        }
        Ok(())
    }
}

/// A sticky failure sink permits existing formatting code to finish safely, but
/// never returns its partial buffer as a successful result. Allocation stops at
/// the first limit violation. Semantic traversal has its own work allowance.
pub(crate) struct BoundedText {
    text: String,
    limit: usize,
    failed: bool,
}
impl BoundedText {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            text: String::new(),
            limit,
            failed: false,
        }
    }
    pub(crate) fn push_str(&mut self, value: &str) {
        let _ = fmt::Write::write_str(self, value);
    }
    pub(crate) fn push(&mut self, value: char) {
        let mut bytes = [0u8; 4];
        self.push_str(value.encode_utf8(&mut bytes));
    }
    pub(crate) fn failed(&self) -> bool {
        self.failed
    }
    pub(crate) fn finish(self) -> Result<String, RenderError> {
        if self.failed {
            Err(RenderError::ResourceLimit("output bytes"))
        } else {
            Ok(self.text)
        }
    }
}
impl fmt::Write for BoundedText {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if !self.failed {
            if self.text.len().saturating_add(value.len()) > self.limit {
                self.failed = true;
            } else {
                let required = self.text.len() + value.len();
                if required > self.text.capacity() {
                    let capacity = self
                        .text
                        .capacity()
                        .saturating_mul(2)
                        .max(required)
                        .min(self.limit);
                    if self
                        .text
                        .try_reserve_exact(capacity - self.text.len())
                        .is_err()
                    {
                        self.failed = true;
                        return Ok(());
                    }
                }
                self.text.push_str(value);
            }
        }
        Ok(())
    }
}
