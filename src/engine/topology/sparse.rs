//! Explicit bounded peer edges, with a bidirectional neighbour-ring preset.

use super::{Edge, ring};

pub(super) fn edges(edges: &[Edge]) -> Vec<Edge> {
    edges.to_vec()
}

pub(super) fn preset(count: usize) -> Vec<Edge> {
    ring::edges(count)
        .into_iter()
        .flat_map(|edge| {
            [
                edge,
                Edge {
                    from: edge.to,
                    to: edge.from,
                },
            ]
        })
        .collect()
}
