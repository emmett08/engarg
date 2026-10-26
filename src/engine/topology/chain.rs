//! Each participant transmits to its successor, without wraparound.

use super::Edge;

pub(super) fn edges(count: usize) -> Vec<Edge> {
    (0..count - 1)
        .map(|from| Edge { from, to: from + 1 })
        .collect()
}
