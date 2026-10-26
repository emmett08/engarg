//! Each participant transmits to its successor, including wraparound.

use super::Edge;

pub(super) fn edges(count: usize) -> Vec<Edge> {
    (0..count)
        .map(|from| Edge {
            from,
            to: (from + 1) % count,
        })
        .collect()
}
