//! All leaves transmit to one aggregation hub.

use super::Edge;
use crate::Error;

pub(super) fn edges(count: usize, hub: usize) -> Result<Vec<Edge>, Error> {
    if hub >= count {
        return Err(Error::InvalidInput(format!(
            "star hub {hub} must belong to 0..{count}"
        )));
    }
    Ok((0..count)
        .filter(|&from| from != hub)
        .map(|from| Edge { from, to: hub })
        .collect())
}
