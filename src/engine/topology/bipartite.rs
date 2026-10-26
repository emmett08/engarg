//! Complete bidirectional communication across two disjoint partitions.

use super::{Edge, validation};
use crate::Error;

pub(super) fn edges(count: usize, left: &[usize], right: &[usize]) -> Result<Vec<Edge>, Error> {
    validation::partition(count, &[left.to_vec(), right.to_vec()], "bipartite sets")?;
    Ok(left
        .iter()
        .flat_map(|&from| {
            right
                .iter()
                .flat_map(move |&to| [Edge { from, to }, Edge { from: to, to: from }])
        })
        .collect())
}
