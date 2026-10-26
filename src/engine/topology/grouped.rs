//! Complete peer communication within each disjoint group.

use super::{Edge, validation};
use crate::Error;

pub(super) fn edges(count: usize, groups: &[Vec<usize>]) -> Result<Vec<Edge>, Error> {
    validation::partition(count, groups, "debate groups")?;
    Ok(groups
        .iter()
        .flat_map(|group| {
            group.iter().flat_map(move |&from| {
                group
                    .iter()
                    .filter(move |&&to| from != to)
                    .map(move |&to| Edge { from, to })
            })
        })
        .collect())
}
