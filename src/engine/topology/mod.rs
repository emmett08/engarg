//! Shared directed topology configuration used by EngArg and EAL coordination.
//!
//! Builders and validation live in the dependency-light contract crate so the
//! authoring host and coordinator admit exactly the same graphs.

pub use eal_coordination_contracts::topology::{Adaptation, DebateConfig, Edge, Graph, Topology};
