//! Every participant transmits to every other participant.

use super::Edge;

pub(super) fn edges(count: usize) -> Vec<Edge> {
    (0..count)
        .flat_map(|from| {
            (0..count)
                .filter(move |&to| from != to)
                .map(move |to| Edge { from, to })
        })
        .collect()
}
