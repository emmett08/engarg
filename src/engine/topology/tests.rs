//! Exact graph semantics, bounded admission and reproducible edge schedules.

use super::*;

fn config(topology: Topology) -> DebateConfig {
    DebateConfig {
        participants: 4,
        rounds: 2,
        topology,
        adaptation: Adaptation::Static {},
    }
}

fn edges(pairs: &[(usize, usize)]) -> Vec<Edge> {
    let mut result: Vec<_> = pairs.iter().map(|&(from, to)| Edge { from, to }).collect();
    result.sort_unstable();
    result
}

#[test]
fn every_structure_has_its_declared_directed_edges() {
    let cases = [
        (
            Topology::FullyConnected {},
            edges(&[
                (0, 1),
                (0, 2),
                (0, 3),
                (1, 0),
                (1, 2),
                (1, 3),
                (2, 0),
                (2, 1),
                (2, 3),
                (3, 0),
                (3, 1),
                (3, 2),
            ]),
        ),
        (
            Topology::Grouped {
                groups: vec![vec![0, 2], vec![1, 3]],
            },
            edges(&[(0, 2), (2, 0), (1, 3), (3, 1)]),
        ),
        (Topology::Chain {}, edges(&[(0, 1), (1, 2), (2, 3)])),
        (Topology::Ring {}, edges(&[(0, 1), (1, 2), (2, 3), (3, 0)])),
        (Topology::Star { hub: 2 }, edges(&[(0, 2), (1, 2), (3, 2)])),
        (
            Topology::Tree {
                parents: vec![0, 0, 1],
            },
            edges(&[(1, 0), (2, 0), (3, 1)]),
        ),
        (
            Topology::Layered {
                layers: vec![vec![0], vec![1, 2], vec![3]],
            },
            edges(&[(0, 1), (0, 2), (1, 3), (2, 3)]),
        ),
        (
            Topology::Sparse {
                edges: edges(&[(0, 3), (3, 2)]),
            },
            edges(&[(0, 3), (3, 2)]),
        ),
        (
            Topology::Bipartite {
                left: vec![0, 2],
                right: vec![1, 3],
            },
            edges(&[
                (0, 1),
                (1, 0),
                (0, 3),
                (3, 0),
                (2, 1),
                (1, 2),
                (2, 3),
                (3, 2),
            ]),
        ),
    ];
    for (topology, expected) in cases {
        let graph = config(topology.clone()).graphs().unwrap();
        assert_eq!(graph.len(), 2);
        assert_eq!(graph[0].edges, expected, "{topology:?}");
        assert_eq!(graph[1], graph[0], "{topology:?}");
    }
}

#[test]
fn malformed_structure_is_rejected_before_graphs_are_returned() {
    let invalid = [
        Topology::Star { hub: 4 },
        Topology::Grouped { groups: vec![] },
        Topology::Grouped {
            groups: vec![vec![0, 1, 2, 3]],
        },
        Topology::Grouped {
            groups: vec![vec![0, 1], vec![], vec![2, 3]],
        },
        Topology::Grouped {
            groups: vec![vec![0, 1], vec![1, 2, 3]],
        },
        Topology::Grouped {
            groups: vec![vec![0, 1], vec![2]],
        },
        Topology::Grouped {
            groups: vec![vec![0, 1], vec![2, 4]],
        },
        Topology::Tree { parents: vec![0] },
        Topology::Tree {
            parents: vec![0, 4, 1],
        },
        Topology::Tree {
            parents: vec![0, 2, 1],
        },
        Topology::Tree {
            parents: vec![2, 1, 0],
        },
        Topology::Layered {
            layers: vec![vec![0, 1], vec![2]],
        },
        Topology::Layered {
            layers: vec![vec![0, 1], vec![2, 1, 3]],
        },
        Topology::Sparse {
            edges: edges(&[(0, 0)]),
        },
        Topology::Sparse {
            edges: edges(&[(0, 1), (0, 1)]),
        },
        Topology::Sparse {
            edges: edges(&[(4, 0)]),
        },
        Topology::Bipartite {
            left: vec![],
            right: vec![0, 1, 2, 3],
        },
        Topology::Bipartite {
            left: vec![0, 1],
            right: vec![1, 2, 3],
        },
    ];
    for topology in invalid {
        let config = config(topology.clone());
        assert!(config.graphs().is_err(), "{topology:?}");
        assert!(config.required_calls().is_err(), "{topology:?}");
    }
}

#[test]
fn root_tree_accepts_arbitrary_acyclic_parent_order() {
    // The higher-index parent of participant 1 is valid and still reaches 0.
    let graph = config(Topology::Tree {
        parents: vec![3, 1, 0],
    })
    .graphs()
    .unwrap();
    assert_eq!(graph[0].edges, edges(&[(1, 3), (2, 1), (3, 0)]));
}

#[test]
fn scheduled_graphs_follow_the_base_and_preserve_isolated_rounds() {
    let mut debate = config(Topology::Ring {});
    debate.rounds = 3;
    debate.adaptation = Adaptation::Scheduled {
        round_edges: vec![edges(&[(3, 0), (2, 0)]), vec![]],
    };
    let graphs = debate.graphs().unwrap();
    assert_eq!(graphs.len(), 3);
    assert_eq!(graphs[0].edges, edges(&[(0, 1), (1, 2), (2, 3), (3, 0)]));
    assert_eq!(graphs[1].edges, edges(&[(2, 0), (3, 0)]));
    assert!(graphs[2].edges.is_empty());
    assert_eq!(debate.required_calls().unwrap(), 19);
}

#[test]
fn all_scheduled_graphs_are_validated_in_advance() {
    let mut debate = config(Topology::Chain {});
    for schedule in [
        vec![],
        vec![vec![], vec![]],
        vec![edges(&[(1, 1)])],
        vec![edges(&[(1, 4)])],
        vec![edges(&[(1, 0), (1, 0)])],
    ] {
        debate.adaptation = Adaptation::Scheduled {
            round_edges: schedule,
        };
        assert!(debate.graphs().is_err());
    }
}

#[test]
fn bounds_and_call_accounting_include_all_peer_rounds() {
    let mut debate = config(Topology::FullyConnected {});
    for participants in [0, 1, 2, 17, u8::MAX] {
        debate.participants = participants;
        assert!(debate.required_calls().is_err());
    }
    debate.participants = 16;
    for rounds in [0, 9, u8::MAX] {
        debate.rounds = rounds;
        assert!(debate.required_calls().is_err());
    }
    debate.rounds = 8;
    assert_eq!(debate.required_calls().unwrap(), 147);
    assert_eq!(debate.graphs().unwrap()[0].edges.len(), 16 * 15);
}

#[test]
fn presets_admit_every_bounded_participant_count() {
    for name in [
        "fully_connected",
        "fully-connected",
        "grouped",
        "divided-into-groups",
        "chain",
        "ring",
        "star",
        "tree",
        "layered",
        "layered-network",
        "sparse",
        "bipartite",
    ] {
        for participants in 3..=16 {
            let mut debate = config(Topology::preset(name, participants).unwrap());
            debate.participants = participants;
            debate.graphs().unwrap();
        }
    }
    assert!(Topology::preset("unknown", 4).is_err());
    assert!(Topology::preset("ring", 2).is_err());
}

#[test]
fn explicit_sparse_graph_admits_disconnected_and_isolated_participants() {
    for pairs in [&[][..], &[(0, 2)][..]] {
        let graph = config(Topology::Sparse {
            edges: edges(pairs),
        })
        .graphs()
        .unwrap();
        assert_eq!(graph[0].edges, edges(pairs));
    }
}

#[test]
fn singleton_groups_form_a_valid_partition_without_peer_transmissions() {
    let debate = config(Topology::Grouped {
        groups: vec![vec![0], vec![1], vec![2], vec![3]],
    });
    let graphs = debate.graphs().unwrap();
    assert!(graphs.iter().all(|graph| graph.edges.is_empty()));
    assert_eq!(debate.required_calls().unwrap(), 15);
}

#[test]
fn configuration_round_trips_and_rejects_unknown_fields() {
    let debate: DebateConfig = serde_json::from_str(r#"{"topology":{"kind":"ring"}}"#).unwrap();
    assert_eq!(debate.participants, 6);
    assert_eq!(debate.rounds, 1);
    assert_eq!(debate.adaptation, Adaptation::Static {});
    assert_eq!(debate.required_calls().unwrap(), 15);
    let encoded = serde_json::to_value(&debate).unwrap();
    assert_eq!(
        serde_json::from_value::<DebateConfig>(encoded).unwrap(),
        debate
    );
    for invalid in [
        r#"{"topology":{"kind":"ring"},"unknown":1}"#,
        r#"{"topology":{"kind":"ring","unknown":1}}"#,
        r#"{"topology":{"kind":"chain"},"adaptation":{"kind":"static","round_edges":[]}}"#,
        r#"{"topology":{"kind":"sparse","edges":[{"from":0,"to":1,"unknown":1}]}}"#,
        r#"{"topology":{"kind":"star"}}"#,
    ] {
        assert!(
            serde_json::from_str::<DebateConfig>(invalid).is_err(),
            "{invalid}"
        );
    }
}
