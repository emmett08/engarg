//! Borrowed argument adjacency and typed lookup, shared by validation and views.

use super::{
    ArgumentCase, ArgumentId, Assumption, ClaimNode, ClaimVersion, Defeater, EvidenceNeed, Ground,
    NeedId, SupportComposition, Warrant,
};
use std::collections::{HashMap, HashSet, VecDeque};

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct WorkCounters {
    indexes: usize,
    traversals: usize,
    edges: usize,
}
#[cfg(test)]
thread_local! { static WORK: std::cell::Cell<WorkCounters> = const { std::cell::Cell::new(WorkCounters { indexes: 0, traversals: 0, edges: 0 }) }; }

#[derive(Debug, Clone, Copy)]
pub enum ArgumentNode<'a> {
    Root(&'a ClaimVersion),
    Claim(&'a ClaimNode),
    Ground(&'a Ground),
    Warrant(&'a Warrant),
    Assumption(&'a Assumption),
    Defeater(&'a Defeater),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TraversalStats {
    pub nodes_visited: usize,
    pub edges_visited: usize,
}

#[derive(Debug)]
pub struct Traversal<'a> {
    pub ids: HashSet<&'a str>,
    pub stats: TraversalStats,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SupportTraversal {
    None,
    Candidate,
    Selected,
}

/// An index is a query structure, not a certificate of structural validity.
/// Use validate_argument when a consumer requires an admitted argument.
#[derive(Debug)]
pub struct ArgumentIndex<'a> {
    case: &'a ArgumentCase,
    nodes: HashMap<ArgumentId<'a>, ArgumentNode<'a>>,
    warrants: HashMap<&'a str, Vec<&'a Warrant>>,
    consumers: HashMap<&'a str, Vec<&'a Warrant>>,
    compositions: HashMap<&'a str, &'a SupportComposition>,
    selected: HashSet<(&'a str, &'a str)>,
    assumptions: HashMap<&'a str, Vec<&'a Assumption>>,
    defeaters: HashMap<&'a str, Vec<&'a Defeater>>,
    needs: HashMap<&'a str, Vec<&'a EvidenceNeed>>,
    enables: HashMap<&'a str, Vec<&'a EvidenceNeed>>,
    need_ids: HashMap<NeedId<'a>, &'a EvidenceNeed>,
    claim_children: HashMap<&'a str, Vec<&'a str>>,
    claim_incoming: HashMap<&'a str, usize>,
}

impl<'a> ArgumentIndex<'a> {
    pub fn new(case: &'a ArgumentCase) -> Self {
        #[cfg(test)]
        WORK.with(|work| {
            let mut count = work.get();
            count.indexes += 1;
            work.set(count);
        });
        let mut result = Self {
            case,
            nodes: HashMap::new(),
            warrants: HashMap::new(),
            consumers: HashMap::new(),
            compositions: HashMap::new(),
            selected: HashSet::new(),
            assumptions: HashMap::new(),
            defeaters: HashMap::new(),
            needs: HashMap::new(),
            enables: HashMap::new(),
            need_ids: HashMap::new(),
            claim_children: HashMap::new(),
            claim_incoming: HashMap::new(),
        };
        result.nodes.insert(
            ArgumentId::reference("current_claim"),
            ArgumentNode::Root(&case.current_claim),
        );
        result.nodes.insert(
            ArgumentId::reference("stronger_target"),
            ArgumentNode::Root(&case.stronger_target),
        );
        for claim in &case.subclaims {
            result
                .nodes
                .insert(ArgumentId::reference(&claim.id), ArgumentNode::Claim(claim));
        }
        for id in std::iter::once("current_claim")
            .chain(std::iter::once("stronger_target"))
            .chain(case.subclaims.iter().map(|s| s.id.as_str()))
        {
            result.claim_incoming.insert(id, 0);
        }
        for ground in &case.grounds {
            result.nodes.insert(
                ArgumentId::reference(&ground.id),
                ArgumentNode::Ground(ground),
            );
        }
        for warrant in &case.warrants {
            result.nodes.insert(
                ArgumentId::reference(&warrant.id),
                ArgumentNode::Warrant(warrant),
            );
            result
                .warrants
                .entry(&warrant.conclusion_id)
                .or_default()
                .push(warrant);
            for premise in &warrant.premise_ids {
                result.consumers.entry(premise).or_default().push(warrant);
                if let Some(incoming) = result.claim_incoming.get_mut(premise.as_str()) {
                    *incoming += 1;
                    result
                        .claim_children
                        .entry(&warrant.conclusion_id)
                        .or_default()
                        .push(premise);
                }
            }
        }
        for composition in &case.compositions {
            result
                .compositions
                .insert(&composition.conclusion_id, composition);
            result.selected.extend(
                composition
                    .selected_warrant_ids
                    .iter()
                    .map(|w| (composition.conclusion_id.as_str(), w.as_str())),
            );
        }
        for assumption in &case.assumptions {
            result.nodes.insert(
                ArgumentId::reference(&assumption.id),
                ArgumentNode::Assumption(assumption),
            );
            for target in &assumption.applies_to {
                result
                    .assumptions
                    .entry(target)
                    .or_default()
                    .push(assumption);
            }
        }
        for defeater in &case.defeaters {
            result.nodes.insert(
                ArgumentId::reference(&defeater.id),
                ArgumentNode::Defeater(defeater),
            );
            result
                .defeaters
                .entry(&defeater.target_id)
                .or_default()
                .push(defeater);
        }
        for need in &case.evidence_plan {
            result.need_ids.insert(NeedId::reference(&need.id), need);
            result.needs.entry(&need.target_id).or_default().push(need);
            result
                .enables
                .entry(&need.enables_claim)
                .or_default()
                .push(need);
        }
        result
    }
    pub fn case(&self) -> &'a ArgumentCase {
        self.case
    }
    pub fn node(&self, id: &str) -> Option<ArgumentNode<'a>> {
        self.nodes.get(id).copied()
    }
    pub fn claim(&self, id: &str) -> Option<(&'a str, &'a str)> {
        match self.node(id)? {
            ArgumentNode::Root(c) => Some((&c.text, &c.qualifier)),
            ArgumentNode::Claim(c) => Some((&c.text, &c.qualifier)),
            _ => None,
        }
    }
    pub fn canonical_id(&self, id: &str) -> Option<&'a str> {
        match self.node(id)? {
            ArgumentNode::Root(_) => match id {
                "current_claim" => Some("current_claim"),
                "stronger_target" => Some("stronger_target"),
                _ => None,
            },
            ArgumentNode::Claim(c) => Some(&c.id),
            ArgumentNode::Ground(g) => Some(&g.id),
            ArgumentNode::Warrant(w) => Some(&w.id),
            ArgumentNode::Assumption(a) => Some(&a.id),
            ArgumentNode::Defeater(d) => Some(&d.id),
        }
    }
    pub fn is_selected(&self, warrant_id: &str) -> Option<bool> {
        let warrant = self.warrant(warrant_id)?;
        self.composition_for(&warrant.conclusion_id)?;
        Some(
            self.selected
                .contains(&(warrant.conclusion_id.as_str(), warrant.id.as_str())),
        )
    }
    pub fn ground(&self, id: &str) -> Option<&'a Ground> {
        match self.node(id)? {
            ArgumentNode::Ground(g) => Some(g),
            _ => None,
        }
    }
    pub fn warrant(&self, id: &str) -> Option<&'a Warrant> {
        match self.node(id)? {
            ArgumentNode::Warrant(w) => Some(w),
            _ => None,
        }
    }
    pub fn need(&self, id: &str) -> Option<&'a EvidenceNeed> {
        self.need_ids.get(id).copied()
    }
    pub fn warrants_for(&self, claim: &str) -> &[&'a Warrant] {
        self.warrants
            .get(claim)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
    pub fn consumers_for(&self, premise: &str) -> &[&'a Warrant] {
        self.consumers
            .get(premise)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
    pub fn composition_for(&self, claim: &str) -> Option<&'a SupportComposition> {
        self.compositions.get(claim).copied()
    }
    pub fn assumptions_for(&self, target: &str) -> &[&'a Assumption] {
        self.assumptions
            .get(target)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
    pub fn defeaters_for(&self, target: &str) -> &[&'a Defeater] {
        self.defeaters
            .get(target)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
    pub fn needs_for(&self, target: &str) -> &[&'a EvidenceNeed] {
        self.needs
            .get(target)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
    pub fn needs_enabling(&self, claim: &str) -> &[&'a EvidenceNeed] {
        self.enables
            .get(claim)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
    pub fn candidate_support(&self, root: &'a str) -> HashSet<&'a str> {
        self.walk([root], SupportTraversal::Candidate, false).ids
    }
    pub fn selected_support(&self, root: &'a str) -> HashSet<&'a str> {
        self.walk([root], SupportTraversal::Selected, false).ids
    }
    pub fn candidate_dependency(&self, root: &'a str) -> HashSet<&'a str> {
        self.walk([root], SupportTraversal::Candidate, true).ids
    }
    pub fn selected_dependency(&self, root: &'a str) -> HashSet<&'a str> {
        self.walk([root], SupportTraversal::Selected, true).ids
    }
    pub fn candidate_dependency_with_stats(&self, root: &'a str) -> Traversal<'a> {
        self.walk([root], SupportTraversal::Candidate, true)
    }
    pub fn selected_dependency_with_stats(&self, root: &'a str) -> Traversal<'a> {
        self.walk([root], SupportTraversal::Selected, true)
    }
    /// The supplied nodes and transitive attached conditions only. Support
    /// routes are deliberately excluded because premises carry their own gates.
    pub fn attachment_dependency(&self, roots: &[&str]) -> HashSet<&'a str> {
        self.walk(
            roots.iter().filter_map(|id| self.canonical_id(id)),
            SupportTraversal::None,
            true,
        )
        .ids
    }
    /// The requested warrant and its premises only, never sibling routes for
    /// its conclusion. The selected flag governs nested claim support.
    pub fn warrant_dependency(&self, warrant_id: &str, selected: bool) -> HashSet<&'a str> {
        let Some(warrant) = self.warrant(warrant_id) else {
            return HashSet::new();
        };
        let mut result = self.walk(
            warrant
                .premise_ids
                .iter()
                .map(String::as_str)
                .chain(std::iter::once(warrant.id.as_str())),
            if selected {
                SupportTraversal::Selected
            } else {
                SupportTraversal::Candidate
            },
            true,
        );
        // Preserve the conclusion's conditions without importing sibling routes.
        result
            .ids
            .extend(self.attachment_dependency(&[&warrant.conclusion_id]));
        result.ids
    }
    fn walk(
        &self,
        roots: impl IntoIterator<Item = &'a str>,
        support: SupportTraversal,
        attachments: bool,
    ) -> Traversal<'a> {
        let mut pending: Vec<_> = roots.into_iter().collect();
        let mut result = Traversal {
            ids: HashSet::new(),
            stats: TraversalStats::default(),
        };
        while let Some(id) = pending.pop() {
            if !result.ids.insert(id) {
                continue;
            }
            result.stats.nodes_visited += 1;
            for warrant in if support == SupportTraversal::None {
                &[][..]
            } else {
                self.warrants_for(id)
            } {
                if support == SupportTraversal::Selected
                    && !self.selected.contains(&(id, warrant.id.as_str()))
                {
                    continue;
                }
                result.stats.edges_visited += 1;
                pending.push(&warrant.id);
                pending.extend(warrant.premise_ids.iter().map(String::as_str));
                result.stats.edges_visited += warrant.premise_ids.len();
            }
            if attachments {
                self.push_attachments(id, &mut pending, &mut result.stats);
            }
        }
        #[cfg(test)]
        WORK.with(|work| {
            let mut count = work.get();
            count.traversals += 1;
            count.edges += result.stats.edges_visited;
            work.set(count);
        });
        result
    }
    fn push_attachments(&self, id: &str, pending: &mut Vec<&'a str>, stats: &mut TraversalStats) {
        for assumption in self.assumptions_for(id) {
            pending.push(&assumption.id);
            stats.edges_visited += 1;
        }
        for defeater in self.defeaters_for(id) {
            pending.push(&defeater.id);
            stats.edges_visited += 1;
        }
    }
    pub(crate) fn claim_depth_maps(
        &self,
    ) -> (HashMap<&'a str, usize>, HashMap<&'a str, usize>, bool) {
        let mut incoming = self.claim_incoming.clone();
        let mut shortest = HashMap::from([("current_claim", 1usize), ("stronger_target", 1)]);
        let mut longest = shortest.clone();
        let mut ready: VecDeque<_> = incoming
            .iter()
            .filter_map(|(id, count)| (*count == 0).then_some(*id))
            .collect();
        let mut removed = 0;
        while let Some(parent) = ready.pop_front() {
            removed += 1;
            for child in self.claim_children.get(parent).into_iter().flatten() {
                if let Some(depth) = shortest.get(parent).copied() {
                    shortest
                        .entry(child)
                        .and_modify(|n| *n = (*n).min(depth + 1))
                        .or_insert(depth + 1);
                }
                if let Some(depth) = longest.get(parent).copied() {
                    longest
                        .entry(child)
                        .and_modify(|n| *n = (*n).max(depth + 1))
                        .or_insert(depth + 1);
                }
                let count = incoming.get_mut(child).expect("indexed claim");
                *count -= 1;
                if *count == 0 {
                    ready.push_back(child);
                }
            }
        }
        (shortest, longest, removed != incoming.len())
    }
    pub fn has_support_cycle(&self) -> bool {
        self.claim_depth_maps().2
    }
}

/// Immutable source borrow plus a single index after every policy has passed.
///
/// ```compile_fail
/// use engarg::domain::{ArgumentCase, Request, validate_argument};
/// fn mutation_is_prevented(request: &Request, case: &mut ArgumentCase) {
///     let admitted = validate_argument(request, case).unwrap();
///     case.grounds.clear();
///     let _ = admitted.index();
/// }
/// ```
#[derive(Debug)]
pub struct ValidatedArgument<'a> {
    pub(crate) case: &'a ArgumentCase,
    pub(crate) index: ArgumentIndex<'a>,
}
impl<'a> ValidatedArgument<'a> {
    pub fn case(&self) -> &'a ArgumentCase {
        self.case
    }
    pub fn index(&self) -> &ArgumentIndex<'a> {
        &self.index
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        AssumptionStatus, DefeaterBasis, DefeaterKind, DefeaterStatus, InferenceKind,
        SupportRelationship,
    };

    fn case() -> ArgumentCase {
        let mut case: ArgumentCase =
            serde_json::from_str(include_str!("../../examples/case.json")).unwrap();
        case.subclaims.clear();
        case.grounds.clear();
        case.warrants.clear();
        case.compositions.clear();
        case.assumptions.clear();
        case.defeaters.clear();
        case.evidence_plan.clear();
        case
    }
    fn ground(case: &mut ArgumentCase, id: &str) {
        case.grounds.push(Ground {
            id: id.into(),
            statement: "An independently supplied premise".into(),
            evidence_ids: vec![],
            status: super::super::GroundStatus::Missing,
        });
    }
    fn claim(case: &mut ArgumentCase, id: &str) {
        case.subclaims.push(ClaimNode {
            id: id.into(),
            text: "A bounded subclaim".into(),
            qualifier: "In scope".into(),
        });
    }
    fn route(
        case: &mut ArgumentCase,
        id: &str,
        conclusion: &str,
        premises: &[&str],
        selected: bool,
    ) {
        case.warrants.push(Warrant {
            id: id.into(),
            conclusion_id: conclusion.into(),
            premise_ids: premises.iter().map(|s| (*s).into()).collect(),
            rule: "Declared inference".into(),
            applicability: "Under stated scope".into(),
            inference: InferenceKind::Deductive,
            backing_evidence_ids: vec![],
            reasoning_method: None,
        });
        if let Some(composition) = case
            .compositions
            .iter_mut()
            .find(|c| c.conclusion_id == conclusion)
        {
            if selected {
                composition.selected_warrant_ids.push(id.into());
            }
        } else {
            case.compositions.push(SupportComposition {
                conclusion_id: conclusion.into(),
                relationship: SupportRelationship::AlternativeRoutes,
                selected_warrant_ids: if selected { vec![id.into()] } else { vec![] },
                rationale: "Explicit selection".into(),
            });
        }
    }
    fn assumption(case: &mut ArgumentCase, id: &str, targets: &[&str]) {
        case.assumptions.push(Assumption {
            id: id.into(),
            statement: "A declared condition".into(),
            critical: false,
            status: AssumptionStatus::Untested,
            evidence_ids: vec![],
            applies_to: targets.iter().map(|s| (*s).into()).collect(),
            validation: "A discriminating check".into(),
        });
    }
    fn attack(case: &mut ArgumentCase, id: &str, target: &str) {
        case.defeaters.push(Defeater {
            id: id.into(),
            kind: DefeaterKind::Undercutting,
            target_id: target.into(),
            challenge: "A material challenge".into(),
            status: DefeaterStatus::Open,
            material: false,
            basis: DefeaterBasis::Hypothetical,
            response: "Retain for review".into(),
            evidence_ids: vec![],
        });
    }
    fn set<'a>(ids: &[&'a str]) -> HashSet<&'a str> {
        ids.iter().copied().collect()
    }

    #[test]
    fn candidate_and_selected_routes_have_independently_specified_membership() {
        let mut case = case();
        ground(&mut case, "G1");
        ground(&mut case, "G2");
        route(&mut case, "W1", "current_claim", &["G1"], true);
        route(&mut case, "W2", "current_claim", &["G2"], false);
        assumption(&mut case, "A1", &["G1"]);
        assumption(&mut case, "A2", &["G2"]);
        attack(&mut case, "D1", "A1");
        let index = ArgumentIndex::new(&case);
        assert_eq!(
            index.candidate_support("current_claim"),
            set(&["current_claim", "W1", "W2", "G1", "G2"])
        );
        assert_eq!(
            index.selected_support("current_claim"),
            set(&["current_claim", "W1", "G1"])
        );
        assert_eq!(
            index.selected_dependency("current_claim"),
            set(&["current_claim", "W1", "G1", "A1", "D1"])
        );
        assert_eq!(
            index.candidate_dependency("current_claim"),
            set(&["current_claim", "W1", "W2", "G1", "G2", "A1", "A2", "D1"])
        );
    }

    #[test]
    fn a_warrant_keeps_conclusion_conditions_without_importing_sibling_routes() {
        let mut case = case();
        ground(&mut case, "G1");
        ground(&mut case, "G2");
        route(&mut case, "W1", "current_claim", &["G1"], true);
        route(&mut case, "W2", "current_claim", &["G2"], false);
        assumption(&mut case, "A1", &["current_claim"]);
        attack(&mut case, "D1", "A1");
        assumption(&mut case, "A2", &["W2"]);
        let index = ArgumentIndex::new(&case);
        assert_eq!(
            index.warrant_dependency("W1", true),
            set(&["current_claim", "W1", "G1", "A1", "D1"])
        );
    }

    #[test]
    fn diamonds_and_attachment_cycles_visit_each_node_once() {
        let mut case = case();
        ground(&mut case, "G");
        claim(&mut case, "S1");
        claim(&mut case, "S2");
        route(&mut case, "W0", "current_claim", &["S1", "S2"], true);
        route(&mut case, "W1", "S1", &["G"], true);
        route(&mut case, "W2", "S2", &["G"], true);
        assumption(&mut case, "A1", &["G", "A2"]);
        assumption(&mut case, "A2", &["A1"]);
        let index = ArgumentIndex::new(&case);
        let traversal = index.candidate_dependency_with_stats("current_claim");
        assert_eq!(
            traversal.ids,
            set(&[
                "current_claim",
                "W0",
                "S1",
                "S2",
                "W1",
                "W2",
                "G",
                "A1",
                "A2"
            ])
        );
        assert_eq!(traversal.stats.nodes_visited, traversal.ids.len());
        assert_eq!(traversal.stats.edges_visited, 10);
        assert!(!index.has_support_cycle());
    }

    #[test]
    fn attachment_order_matches_authored_order_for_shared_targets() {
        let mut case = case();
        assumption(&mut case, "A3", &["current_claim"]);
        assumption(&mut case, "A1", &["current_claim"]);
        attack(&mut case, "D2", "current_claim");
        attack(&mut case, "D1", "current_claim");
        let index = ArgumentIndex::new(&case);
        assert_eq!(
            index
                .assumptions_for("current_claim")
                .iter()
                .map(|a| a.id.as_str())
                .collect::<Vec<_>>(),
            vec!["A3", "A1"]
        );
        assert_eq!(
            index
                .defeaters_for("current_claim")
                .iter()
                .map(|d| d.id.as_str())
                .collect::<Vec<_>>(),
            vec!["D2", "D1"]
        );
    }

    #[test]
    fn a_long_chain_uses_linear_edge_visits_without_recursion() {
        let mut case = case();
        for i in 0..12_000 {
            let target = if i == 0 {
                "current_claim".to_owned()
            } else {
                format!("A{}", i - 1)
            };
            assumption(&mut case, &format!("A{i}"), &[&target]);
        }
        let index = ArgumentIndex::new(&case);
        let traversal = index.candidate_dependency_with_stats("current_claim");
        assert_eq!(traversal.ids.len(), 12_001);
        assert_eq!(traversal.stats.edges_visited, 12_000);
        assert_eq!(traversal.stats.nodes_visited, 12_001);
    }

    #[test]
    fn every_small_attachment_graph_matches_an_independent_reachability_definition() {
        // Exhaust all 2^9 directed edges on three attachment nodes. A root is
        // attached to A0. The reference definition repeatedly applies the
        // mathematical reachability rule, independent of indexed traversal.
        for mask in 0u16..512 {
            let mut case = case();
            for destination in 0..3 {
                let mut targets = if destination == 0 {
                    vec!["current_claim".to_owned()]
                } else {
                    vec![]
                };
                for source in 0..3 {
                    if mask & (1 << (source * 3 + destination)) != 0 {
                        targets.push(format!("A{source}"));
                    }
                }
                assumption(
                    &mut case,
                    &format!("A{destination}"),
                    &targets.iter().map(String::as_str).collect::<Vec<_>>(),
                );
            }
            let mut expected = HashSet::from(["current_claim".to_owned()]);
            loop {
                let before = expected.len();
                for a in &case.assumptions {
                    if a.applies_to.iter().any(|target| expected.contains(target)) {
                        expected.insert(a.id.clone());
                    }
                }
                if before == expected.len() {
                    break;
                }
            }
            let index = ArgumentIndex::new(&case);
            let actual: HashSet<_> = index
                .candidate_dependency("current_claim")
                .into_iter()
                .map(str::to_owned)
                .collect();
            assert_eq!(actual, expected, "mask={mask}");
        }
    }
    #[test]
    fn validation_builds_one_index_and_shares_root_queries_across_many_needs() {
        let request: crate::domain::Request =
            serde_json::from_str(include_str!("../../examples/request.json")).unwrap();
        let mut case: ArgumentCase =
            serde_json::from_str(include_str!("../../examples/case.json")).unwrap();
        WORK.with(|work| work.set(WorkCounters::default()));
        crate::domain::validate_argument(&request, &case).unwrap();
        let small = WORK.with(std::cell::Cell::get);
        let mut need = case
            .evidence_plan
            .iter()
            .find(|n| n.enables_claim == "current_claim")
            .unwrap()
            .clone();
        for n in 0..1000 {
            need.id = format!("N_extra_{n}");
            case.evidence_plan.push(need.clone());
        }
        WORK.with(|work| work.set(WorkCounters::default()));
        let admitted = crate::domain::validate_argument(&request, &case).unwrap();
        assert!(admitted.index().needs_enabling("current_claim").len() >= 1000);
        let large = WORK.with(std::cell::Cell::get);
        assert_eq!(small.indexes, 1);
        assert_eq!(large.indexes, 1);
        assert_eq!(large.traversals, small.traversals);
        assert_eq!(large.edges, small.edges);
    }
    #[test]
    fn reverse_support_preserves_authored_warrant_order_without_scans() {
        let mut case = case();
        ground(&mut case, "G1");
        ground(&mut case, "G2");
        route(&mut case, "W3", "current_claim", &["G1"], true);
        route(&mut case, "W1", "stronger_target", &["G1", "G2"], true);
        let index = ArgumentIndex::new(&case);
        assert_eq!(
            index
                .consumers_for("G1")
                .iter()
                .map(|w| w.id.as_str())
                .collect::<Vec<_>>(),
            vec!["W3", "W1"]
        );
        assert_eq!(
            index
                .consumers_for("G2")
                .iter()
                .map(|w| w.id.as_str())
                .collect::<Vec<_>>(),
            vec!["W1"]
        );
        assert!(index.consumers_for("missing").is_empty());
    }
}
