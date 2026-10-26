//! A rooted aggregation tree sends each child's assessment to its parent.
//!
//! Bottom-up direction is an EngArg parameterisation of the survey's tree
//! category. The root is participant 0 and has no outgoing parent edge.

use super::Edge;
use crate::Error;

pub(super) fn edges(count: usize, parents: &[usize]) -> Result<Vec<Edge>, Error> {
    if parents.len() != count - 1 {
        return Err(Error::InvalidInput(format!(
            "tree parents must contain {} entries for participants 1..{count}",
            count - 1
        )));
    }
    for (index, &parent) in parents.iter().enumerate() {
        let child = index + 1;
        if parent >= count || parent == child {
            return Err(Error::InvalidInput(format!(
                "invalid tree parent {parent} for participant {child}"
            )));
        }
        let mut ancestor = child;
        for _ in 0..count {
            if ancestor == 0 {
                break;
            }
            let Some(&next) = parents.get(ancestor - 1) else {
                return Err(Error::InvalidInput(
                    "tree parent is outside the participant set".into(),
                ));
            };
            if next >= count {
                return Err(Error::InvalidInput(
                    "tree parent is outside the participant set".into(),
                ));
            }
            ancestor = next;
        }
        if ancestor != 0 {
            return Err(Error::InvalidInput(
                "tree parents must form an acyclic tree rooted at participant 0".into(),
            ));
        }
    }
    Ok(parents
        .iter()
        .enumerate()
        .map(|(index, &to)| Edge {
            from: index + 1,
            to,
        })
        .collect())
}
