//! Claim levels in the canonical support graph, independent of presentation.

use std::collections::HashMap;

#[cfg(test)]
use crate::domain::ArgumentCase;

/// Roots are level one. A subclaim premise adds one level; grounds, warrants,
/// assumptions and attacks do not add levels. All candidate routes are counted.
/// Construct after structural validation has established an acyclic graph.
pub(crate) struct ClaimDepths<'a> {
    shortest: HashMap<&'a str, usize>,
    longest: HashMap<&'a str, usize>,
}

impl<'a> ClaimDepths<'a> {
    #[cfg(test)]
    pub(crate) fn new(case: &'a ArgumentCase) -> Self {
        Self::from_index(&crate::domain::ArgumentIndex::new(case))
    }

    pub(crate) fn from_index(index: &crate::domain::ArgumentIndex<'a>) -> Self {
        let (shortest, longest, _) = index.claim_depth_maps();
        Self { shortest, longest }
    }

    pub(crate) fn maximum(&self) -> usize {
        self.longest.values().copied().max().unwrap_or(1)
    }

    /// A shared claim is visible if any root reaches it within the display limit.
    pub(crate) fn visible(&self, id: &str, limit: Option<u8>) -> bool {
        limit.is_none_or(|limit| {
            self.shortest
                .get(id)
                .is_some_and(|depth| *depth <= usize::from(limit))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case() -> ArgumentCase {
        serde_json::from_str(include_str!("../examples/case.json")).unwrap()
    }

    #[test]
    fn a_direct_inference_has_one_level_and_shared_subclaims_have_two() {
        let mut case = case();
        let depths = ClaimDepths::new(&case);
        assert_eq!(depths.maximum(), 2);
        assert!(!depths.visible("S1", Some(1)));
        assert!(depths.visible("S1", Some(2)));
        assert!(depths.visible("current_claim", Some(1)));
        case.warrants.retain(|w| w.conclusion_id != "S1");
        for warrant in &mut case.warrants {
            warrant.premise_ids = vec!["G1".into()];
        }
        assert_eq!(ClaimDepths::new(&case).maximum(), 1);
    }

    #[test]
    fn longest_route_limits_generation_but_shortest_route_controls_display() {
        let mut case = case();
        let mut child = case.subclaims[0].clone();
        child.id = "S2".into();
        case.subclaims.push(child);
        let mut route = case
            .warrants
            .iter()
            .find(|w| w.conclusion_id == "S1")
            .unwrap()
            .clone();
        route.id = "W2".into();
        route.conclusion_id = "S2".into();
        route.premise_ids = vec!["S1".into()];
        case.warrants.push(route);
        case.warrants
            .iter_mut()
            .find(|w| w.conclusion_id == "current_claim")
            .unwrap()
            .premise_ids = vec!["S2".into()];
        let depths = ClaimDepths::new(&case);
        assert_eq!(depths.maximum(), 3);
        // S1 also has a direct route from stronger_target in this fixture.
        assert!(depths.visible("S1", Some(2)));
        assert!(!depths.visible("S1", Some(1)));
    }
}
