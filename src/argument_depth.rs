//! Claim levels in the canonical support graph, independent of presentation.

use std::collections::{HashMap, VecDeque};

use crate::domain::ArgumentCase;

/// Roots are level one. A subclaim premise adds one level; grounds, warrants,
/// assumptions and attacks do not add levels. All candidate routes are counted.
/// Construct after structural validation has established an acyclic graph.
pub(crate) struct ClaimDepths<'a> {
    shortest: HashMap<&'a str, usize>,
    longest: HashMap<&'a str, usize>,
}

impl<'a> ClaimDepths<'a> {
    pub(crate) fn new(case: &'a ArgumentCase) -> Self {
        let mut incoming: HashMap<&str, usize> = ["current_claim", "stronger_target"]
            .into_iter()
            .chain(case.subclaims.iter().map(|claim| claim.id.as_str()))
            .map(|id| (id, 0))
            .collect();
        let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
        for warrant in &case.warrants {
            for premise in &warrant.premise_ids {
                if let Some(count) = incoming.get_mut(premise.as_str()) {
                    *count += 1;
                    children
                        .entry(&warrant.conclusion_id)
                        .or_default()
                        .push(premise);
                }
            }
        }
        let mut result = Self {
            shortest: HashMap::from([("current_claim", 1), ("stronger_target", 1)]),
            longest: HashMap::from([("current_claim", 1), ("stronger_target", 1)]),
        };
        let mut ready: VecDeque<_> = incoming
            .iter()
            .filter_map(|(id, count)| (*count == 0).then_some(*id))
            .collect();
        while let Some(parent) = ready.pop_front() {
            for child in children.get(parent).into_iter().flatten() {
                if let Some(depth) = result.shortest.get(parent).copied() {
                    result
                        .shortest
                        .entry(child)
                        .and_modify(|value| *value = (*value).min(depth + 1))
                        .or_insert(depth + 1);
                }
                if let Some(depth) = result.longest.get(parent).copied() {
                    result
                        .longest
                        .entry(child)
                        .and_modify(|value| *value = (*value).max(depth + 1))
                        .or_insert(depth + 1);
                }
                let count = incoming.get_mut(child).expect("declared claim");
                *count -= 1;
                if *count == 0 {
                    ready.push_back(child);
                }
            }
        }
        result
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
