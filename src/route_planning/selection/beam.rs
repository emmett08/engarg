//! Bounded route search; expansions retain backpointers instead of copying paths.
use std::collections::BTreeSet;

const FRONTIER: usize = 32;
const OPTIONS_PER_SOURCE: usize = 64;

pub(super) struct OptionCost {
    /// Interned in lexical capability-ID order, preserving deterministic ties.
    pub capability: usize,
    pub source: usize,
    pub maximum_calls: usize,
    pub exact_source_count: bool,
    pub estimates: Vec<Option<u64>>,
}
struct Node {
    parent: Option<usize>,
    choice: Option<usize>,
}
struct State {
    path: Option<usize>,
    lexical_rank: usize,
    sources: BTreeSet<usize>,
    capabilities: BTreeSet<usize>,
    costs: Vec<(usize, u128)>,
    maximum_calls: usize,
    dynamic_sources: usize,
}
struct Expansion {
    parent: usize,
    option: usize,
    costs: Vec<(usize, u128)>,
}
pub(super) struct Search {
    frontier: Vec<State>,
    nodes: Vec<Node>,
    charge_per_capability: Vec<bool>,
    complete: bool,
    considered: usize,
}
pub(super) struct Outcome {
    pub choices: Vec<Option<usize>>,
    pub costs: Vec<(usize, u128)>,
    pub exact_source_count: Option<usize>,
    pub maximum_calls: usize,
    pub dynamic_sources: usize,
    pub complete: bool,
    pub considered: usize,
}
impl Search {
    pub fn new(charge_per_capability: Vec<bool>) -> Self {
        Self {
            frontier: vec![State {
                path: None,
                lexical_rank: 0,
                sources: BTreeSet::new(),
                capabilities: BTreeSet::new(),
                costs: vec![(0, 0); charge_per_capability.len()],
                maximum_calls: 0,
                dynamic_sources: 0,
            }],
            nodes: Vec::new(),
            charge_per_capability,
            complete: true,
            considered: 0,
        }
    }
    pub fn advance(&mut self, options: &[OptionCost]) {
        if options.is_empty() {
            for state in &mut self.frontier {
                let node = self.nodes.len();
                self.nodes.push(Node {
                    parent: state.path,
                    choice: None,
                });
                state.path = Some(node);
            }
            return;
        }
        self.complete &= options.len() <= OPTIONS_PER_SOURCE;
        let options = &options[..options.len().min(OPTIONS_PER_SOURCE)];
        let mut next = Vec::with_capacity(self.frontier.len() * options.len());
        for (parent, state) in self.frontier.iter().enumerate() {
            for (option_index, option) in options.iter().enumerate() {
                self.considered = self.considered.saturating_add(1);
                let new_source = !state.sources.contains(&option.source);
                let new_capability = !state.capabilities.contains(&option.capability);
                let mut costs = state.costs.clone();
                for (index, per_capability) in self.charge_per_capability.iter().enumerate() {
                    if if *per_capability {
                        new_capability
                    } else {
                        new_source
                    } {
                        match option.estimates[index] {
                            Some(value) => {
                                costs[index].1 = costs[index].1.saturating_add(u128::from(value))
                            }
                            None => costs[index].0 += 1,
                        }
                    }
                }
                next.push(Expansion {
                    parent,
                    option: option_index,
                    costs,
                });
            }
        }
        let lexical_key = |expansion: &Expansion| {
            (
                self.frontier[expansion.parent].lexical_rank,
                options[expansion.option].capability,
            )
        };
        next.sort_by(|a, b| {
            a.costs
                .cmp(&b.costs)
                .then_with(|| lexical_key(a).cmp(&lexical_key(b)))
        });
        self.complete &= next.len() <= FRONTIER;
        next.truncate(FRONTIER);
        let mut lexical_order = (0..next.len()).collect::<Vec<_>>();
        lexical_order.sort_by_key(|index| lexical_key(&next[*index]));
        let mut ranks = vec![0; next.len()];
        for (rank, index) in lexical_order.into_iter().enumerate() {
            ranks[index] = rank;
        }
        self.frontier = next
            .into_iter()
            .enumerate()
            .map(|(index, expansion)| {
                let parent = &self.frontier[expansion.parent];
                let option = &options[expansion.option];
                // Only surviving states copy their compact integer sets. Rejected
                // expansions never clone a prefix or allocate source-ID strings.
                let mut sources = parent.sources.clone();
                let new_source = sources.insert(option.source);
                let mut capabilities = parent.capabilities.clone();
                capabilities.insert(option.capability);
                let path = self.nodes.len();
                self.nodes.push(Node {
                    parent: parent.path,
                    choice: Some(option.capability),
                });
                State {
                    path: Some(path),
                    lexical_rank: ranks[index],
                    sources,
                    capabilities,
                    costs: expansion.costs,
                    maximum_calls: parent.maximum_calls.saturating_add(option.maximum_calls),
                    dynamic_sources: parent.dynamic_sources
                        + usize::from(new_source && !option.exact_source_count),
                }
            })
            .collect();
    }
    pub fn finish(self) -> Outcome {
        let best = self
            .frontier
            .into_iter()
            .next()
            .expect("search retains an incumbent");
        let mut path = best.path;
        let mut choices = Vec::new();
        while let Some(index) = path {
            let node = &self.nodes[index];
            choices.push(node.choice);
            path = node.parent;
        }
        choices.reverse();
        Outcome {
            choices,
            costs: best.costs,
            exact_source_count: (best.dynamic_sources == 0).then_some(best.sources.len()),
            maximum_calls: best.maximum_calls,
            dynamic_sources: best.dynamic_sources,
            complete: self.complete,
            considered: self.considered,
        }
    }
}
