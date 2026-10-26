//! Typed debate graphs and reproducible changes to their directed edge sets.
//!
//! A graph constrains peer assessment visibility. Execution order and the final
//! synthesis, critique and adjudication stages belong to the debate executor.

use serde::{Deserialize, Serialize};

use crate::Error;

mod bipartite;
mod chain;
mod fully_connected;
mod grouped;
mod layered;
mod ring;
mod sparse;
mod star;
mod tree;
mod validation;

/// One permitted transmission from a participant to a different participant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
}

/// Output from configuration validation, in sender-then-recipient edge order.
/// Graphs are inspected and serialised rather than accepted as run configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Graph {
    pub edges: Vec<Edge>,
}

/// Communication structure, independent of round scheduling and agent roles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Topology {
    FullyConnected {},
    /// Every participant belongs to exactly one nonempty group.
    Grouped {
        groups: Vec<Vec<usize>>,
    },
    Chain {},
    Ring {},
    /// All other participants send to the hub.
    Star {
        hub: usize,
    },
    /// Child i + 1 sends to parents[i], with participant 0 as the root.
    Tree {
        parents: Vec<usize>,
    },
    /// Every participant in a layer sends to every participant in the next layer.
    Layered {
        layers: Vec<Vec<usize>>,
    },
    /// Explicit directed edges, including the empty graph for isolated agents.
    Sparse {
        edges: Vec<Edge>,
    },
    /// Complete bidirectional communication across two disjoint partitions.
    Bipartite {
        left: Vec<usize>,
        right: Vec<usize>,
    },
}

impl Topology {
    /// Deterministic CLI presets. Parameterised graphs can use JSON instead.
    pub fn preset(name: &str, participants: u8) -> Result<Self, Error> {
        let count = validation::participant_count(participants)?;
        let split = count / 2;
        let topology = match name.replace('-', "_").as_str() {
            "fully_connected" => Self::FullyConnected {},
            "grouped" | "groups" | "divided_into_groups" => Self::Grouped {
                groups: vec![(0..split).collect(), (split..count).collect()],
            },
            "chain" => Self::Chain {},
            "ring" => Self::Ring {},
            "star" => Self::Star { hub: 0 },
            "tree" => Self::Tree {
                parents: (1..count).map(|child| (child - 1) / 2).collect(),
            },
            "layered" | "layered_network" => Self::Layered {
                layers: vec![
                    (0..count / 3).collect(),
                    (count / 3..2 * count / 3).collect(),
                    (2 * count / 3..count).collect(),
                ],
            },
            "sparse" => Self::Sparse {
                edges: sparse::preset(count),
            },
            "bipartite" => Self::Bipartite {
                left: (0..split).collect(),
                right: (split..count).collect(),
            },
            _ => {
                return Err(Error::InvalidInput(format!(
                    "unknown topology {name:?}; choose fully_connected, grouped, chain, ring, star, tree, layered, sparse or bipartite"
                )));
            }
        };
        Ok(topology)
    }

    fn graph(&self, participants: usize) -> Result<Graph, Error> {
        let edges = match self {
            Self::FullyConnected {} => fully_connected::edges(participants),
            Self::Grouped { groups } => grouped::edges(participants, groups)?,
            Self::Chain {} => chain::edges(participants),
            Self::Ring {} => ring::edges(participants),
            Self::Star { hub } => star::edges(participants, *hub)?,
            Self::Tree { parents } => tree::edges(participants, parents)?,
            Self::Layered { layers } => layered::edges(participants, layers)?,
            Self::Sparse { edges } => sparse::edges(edges),
            Self::Bipartite { left, right } => bipartite::edges(participants, left, right)?,
        };
        validation::graph(
            edges,
            participants,
            matches!(self, Self::Sparse { .. } | Self::Grouped { .. }),
        )
    }
}

/// Static connectivity or explicit replacement graphs after the first round.
///
/// Scheduled changes are operator-supplied and reproducible. They make no claim
/// to learn which participants or edges improve argument quality.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Adaptation {
    Static {},
    /// Exactly rounds - 1 edge sets. Empty sets isolate every participant.
    Scheduled {
        round_edges: Vec<Vec<Edge>>,
    },
}

impl Default for Adaptation {
    fn default() -> Self {
        Self::Static {}
    }
}

fn default_participants() -> u8 {
    6
}

fn default_rounds() -> u8 {
    1
}

/// Bounded peer debate, following one independent pass by every participant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DebateConfig {
    #[serde(default = "default_participants")]
    pub participants: u8,
    #[serde(default = "default_rounds")]
    pub rounds: u8,
    pub topology: Topology,
    #[serde(default)]
    pub adaptation: Adaptation,
}

impl DebateConfig {
    /// Validate the entire configuration before returning any graph for execution.
    pub fn graphs(&self) -> Result<Vec<Graph>, Error> {
        let participants = validation::participant_count(self.participants)?;
        if !(1..=8).contains(&self.rounds) {
            return Err(Error::InvalidInput("debate rounds must be 1–8".into()));
        }
        let base = self.topology.graph(participants)?;
        match &self.adaptation {
            Adaptation::Static {} => Ok(vec![base; usize::from(self.rounds)]),
            Adaptation::Scheduled { round_edges } => {
                if round_edges.len() != usize::from(self.rounds - 1) {
                    return Err(Error::InvalidInput(format!(
                        "scheduled adaptation requires {} replacement edge sets after the first round, received {}",
                        self.rounds - 1,
                        round_edges.len()
                    )));
                }
                let mut graphs = vec![base];
                for edges in round_edges {
                    graphs.push(validation::graph(edges.clone(), participants, true)?);
                }
                Ok(graphs)
            }
        }
    }

    /// Independent and peer calls plus synthesis, critique and adjudication.
    /// Repair calls must be reserved separately by the caller.
    pub fn required_calls(&self) -> Result<u32, Error> {
        self.graphs()?;
        Ok(u32::from(self.participants) * (u32::from(self.rounds) + 1) + 3)
    }
}

#[cfg(test)]
mod tests;
