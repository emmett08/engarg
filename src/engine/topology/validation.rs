//! Shared admission checks for bounded directed graphs and exact partitions.

use std::collections::BTreeSet;

use super::{Edge, Graph};
use crate::Error;

pub(super) fn participant_count(count: u8) -> Result<usize, Error> {
    if !(3..=16).contains(&count) {
        return Err(Error::InvalidInput(
            "debate participants must be 3–16".into(),
        ));
    }
    Ok(usize::from(count))
}

pub(super) fn partition(count: usize, groups: &[Vec<usize>], name: &str) -> Result<(), Error> {
    if groups.len() < 2 || groups.iter().any(Vec::is_empty) {
        return Err(Error::InvalidInput(format!(
            "{name} must contain at least two nonempty sets"
        )));
    }
    let mut seen = BTreeSet::new();
    for &participant in groups.iter().flatten() {
        if participant >= count || !seen.insert(participant) {
            return Err(Error::InvalidInput(format!(
                "{name} must contain each participant exactly once with indices in 0..{count}"
            )));
        }
    }
    if seen.len() != count {
        return Err(Error::InvalidInput(format!(
            "{name} must cover all {count} participants"
        )));
    }
    Ok(())
}

pub(super) fn graph(mut edges: Vec<Edge>, count: usize, allow_empty: bool) -> Result<Graph, Error> {
    if edges.len() > count * (count - 1) {
        return Err(Error::InvalidInput(
            "debate graph exceeds the number of distinct directed peer edges".into(),
        ));
    }
    if edges.is_empty() && !allow_empty {
        return Err(Error::InvalidInput(
            "this debate topology must permit at least one peer transmission".into(),
        ));
    }
    for edge in &edges {
        if edge.from >= count || edge.to >= count || edge.from == edge.to {
            return Err(Error::InvalidInput(format!(
                "invalid debate edge {} -> {}; endpoints must differ and belong to 0..{count}",
                edge.from, edge.to
            )));
        }
    }
    edges.sort_unstable();
    if edges.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(Error::InvalidInput(
            "debate graph contains duplicate directed edges".into(),
        ));
    }
    Ok(Graph { edges })
}
