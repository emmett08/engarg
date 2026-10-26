//! Complete forward communication between consecutive disjoint layers.

use super::{Edge, validation};
use crate::Error;

pub(super) fn edges(count: usize, layers: &[Vec<usize>]) -> Result<Vec<Edge>, Error> {
    validation::partition(count, layers, "debate layers")?;
    Ok(layers
        .windows(2)
        .flat_map(|pair| {
            pair[0]
                .iter()
                .flat_map(|&from| pair[1].iter().map(move |&to| Edge { from, to }))
        })
        .collect())
}
